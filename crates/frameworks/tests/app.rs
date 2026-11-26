//! The whole window, drawn off screen (egui_kittest with wgpu) and saved as pictures, to look at
//! what the views draw without a person at the keyboard. Run by hand (it needs a GPU, or a
//! software Vulkan such as lavapipe):
//! `cargo test -p codelibre_text --test app -- --ignored --nocapture`; the pictures land in
//! `target/tmp/gui-checks`. The tests take turns (`TURN`): each points `CODELIBRE_TEXT_DATA`, which
//! the whole process shares, at its own profile.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use codelibre_text::app::App;
use codelibre_text::compose;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

const SOURCE: &str = "def outer():\n    if True:\n        a = 1\n        b = 2\n    return 3\n\nclass Thing:\n    def method(self):\n        pass\n\nprint('end')\n";

const PLUGIN: &str = r##"import sublime
import sublime_plugin


def plugin_loaded():
    print("gui check plugin loaded")


class GuiCheckMenuCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        def chosen(index):
            self.view.window().status_message("popup chose %d" % index)
        self.view.show_popup_menu(["First choice", "Second choice", "Third choice"], chosen)


class GuiCheckHtmlCommand(sublime_plugin.WindowCommand):
    def run(self):
        self.window.new_html_sheet("Html sheet", "<h1>Heading</h1><p>Some <b>bold</b> text and <a href='x'>a link</a>.</p>")


class GuiCheckAnnotateCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        def navigate(href):
            self.view.window().status_message("annotation link %s" % href)
        line = self.view.line(self.view.text_point(4, 0))
        far = self.view.line(self.view.text_point(150, 0))
        self.view.add_regions(
            "gui_check", [line, far], "invalid", "dot",
            annotations=["<a href='first'>Go to first</a>", "far away"],
            annotation_color="#e05050", on_navigate=navigate)
"##;

const KEYMAP: &str = r#"[
    { "keys": ["f9"], "command": "gui_check_menu" },
    { "keys": ["f8"], "command": "gui_check_html" },
    { "keys": ["f7"], "command": "gui_check_annotate" },
    { "keys": ["f6"], "command": "toggle_minimap" },
    { "keys": ["f5"], "command": "show_panel", "args": { "panel": "find_in_files", "where": "<open files>" } },
]"#;

/// Held by a test's window from its start to its end, so the tests run one at a time.
static TURN: Mutex<()> = Mutex::new(());

struct Checks {
    harness: Harness<'static, App>,
    out: PathBuf,
    _turn: MutexGuard<'static, ()>,
}

impl Checks {
    fn shot(&mut self, name: &str) {
        self.settle(5);
        let image = self.harness.render().expect("the window could be drawn");
        let path = self.out.join(format!("{name}.png"));
        image.save(&path).expect("the picture was saved");
        println!("saved {}", path.display());
    }

    /// A frame, with the input the app adds before eframe runs it (`raw_input_hook`: the widget
    /// edits it replays), which the harness does not call.
    fn step(&mut self) {
        let ctx = self.harness.ctx.clone();
        let mut input = std::mem::take(self.harness.input_mut());
        eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut input);
        *self.harness.input_mut() = input;
        self.harness.step();
    }

    fn settle(&mut self, frames: usize) {
        for _ in 0..frames {
            self.step();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn key(&mut self, modifiers: Modifiers, key: Key) {
        self.harness.press_key_modifiers(modifiers, key);
        self.settle(3);
    }

    /// A key as a keyboard sends it: the character it typed (`{` for Shift+[) and the physical key.
    fn typed_key(&mut self, modifiers: Modifiers, key: Key, physical: Key) {
        for pressed in [true, false] {
            self.harness.input_mut().modifiers = if pressed { modifiers } else { Modifiers::NONE };
            self.harness.input_mut().events.push(Event::Key {
                key,
                physical_key: Some(physical),
                pressed,
                repeat: false,
                modifiers,
            });
            self.step();
        }
        self.settle(3);
    }

    /// Text typed into the widget that has the focus.
    fn type_text(&mut self, text: &str) {
        self.harness
            .input_mut()
            .events
            .push(Event::Text(text.to_string()));
        self.settle(3);
    }

    fn click(&mut self, x: f32, y: f32) {
        self.click_with(Modifiers::NONE, PointerButton::Primary, x, y);
    }

    /// A click with these keys held.
    fn click_with(&mut self, modifiers: Modifiers, button: PointerButton, x: f32, y: f32) {
        let pos = Pos2::new(x, y);
        let input = self.harness.input_mut();
        input.events.push(Event::PointerMoved(pos));
        self.step();
        self.harness.input_mut().modifiers = modifiers;
        for pressed in [true, false] {
            self.harness.input_mut().events.push(Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
            });
            self.step();
        }
        self.harness.input_mut().modifiers = Modifiers::NONE;
        self.settle(3);
    }

    fn right_click(&mut self, x: f32, y: f32) {
        let pos = Pos2::new(x, y);
        self.harness
            .input_mut()
            .events
            .push(Event::PointerMoved(pos));
        self.step();
        for pressed in [true, false] {
            self.harness.input_mut().events.push(Event::PointerButton {
                pos,
                button: PointerButton::Secondary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            self.step();
        }
        self.settle(3);
    }

    /// Each click in one frame: the harness moves time on 0.25 s a frame, and egui takes two
    /// clicks 0.3 s apart at most as a double click.
    fn double_click(&mut self, x: f32, y: f32) {
        let pos = Pos2::new(x, y);
        self.harness
            .input_mut()
            .events
            .push(Event::PointerMoved(pos));
        self.step();
        for _ in 0..2 {
            for pressed in [true, false] {
                self.harness.input_mut().events.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
            self.step();
        }
        self.settle(3);
    }

    fn drag(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.drag_with(Modifiers::NONE, PointerButton::Primary, from, to);
    }

    /// A drag with these keys held.
    fn drag_with(
        &mut self,
        modifiers: Modifiers,
        button: PointerButton,
        from: (f32, f32),
        to: (f32, f32),
    ) {
        let (from, to) = (Pos2::new(from.0, from.1), Pos2::new(to.0, to.1));
        self.harness
            .input_mut()
            .events
            .push(Event::PointerMoved(from));
        self.step();
        self.harness.input_mut().modifiers = modifiers;
        let event = |pos, pressed| Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        };
        self.harness.input_mut().events.push(event(from, true));
        self.step();
        for step in 1..=8 {
            let pos = from + (to - from) * (step as f32 / 8.0);
            self.harness
                .input_mut()
                .events
                .push(Event::PointerMoved(pos));
            self.step();
        }
        self.harness.input_mut().events.push(event(to, false));
        self.step();
        self.harness.input_mut().modifiers = Modifiers::NONE;
        self.settle(3);
    }

    fn hover(&mut self, x: f32, y: f32) {
        self.harness
            .input_mut()
            .events
            .push(Event::PointerMoved(Pos2::new(x, y)));
        self.settle(5);
    }
}

fn start(data: &Path, files: Vec<PathBuf>) -> Checks {
    // A test that failed holding it leaves it poisoned; the next one still runs.
    let turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("CODELIBRE_TEXT_DATA", data);
    let harness = Harness::builder()
        .with_size(egui::Vec2::new(1300.0, 860.0))
        .wgpu()
        .build_eframe(move |cc| {
            let (editor, view, engine) = compose(files, cc.egui_ctx.clone());
            App::new(cc, editor, view, engine)
        });
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui-checks");
    std::fs::create_dir_all(&out).unwrap();
    let mut checks = Checks {
        harness,
        out,
        _turn: turn,
    };
    // The plugins load in the background.
    for _ in 0..40 {
        checks.settle(5);
    }
    checks
}

fn write_profile(data: &Path) {
    let user = data.join("Packages/User");
    std::fs::create_dir_all(&user).unwrap();
    std::fs::write(user.join("gui_check.py"), PLUGIN).unwrap();
    std::fs::write(user.join("Default.sublime-keymap"), KEYMAP).unwrap();
}

fn write_png(path: &Path) {
    let image = image::RgbaImage::from_fn(160, 100, |x, y| {
        image::Rgba([(x * 255 / 160) as u8, (y * 255 / 100) as u8, 180, 255])
    });
    image.save(path).unwrap();
}

#[test]
#[ignore = "draws the window with a GPU; run by hand and look at target/tmp/gui-checks"]
fn todays_features_as_drawn() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    write_profile(data.path());
    let code = files.path().join("code.py");
    std::fs::write(&code, SOURCE).unwrap();
    let picture = files.path().join("picture.png");
    write_png(&picture);

    let long = files.path().join("long.rs");
    std::fs::write(&long, include_str!("../src/views/minimap.rs")).unwrap();
    let mut checks = start(data.path(), vec![long, code.clone(), picture.clone()]);
    checks.shot("01_image_sheet");
    // A longer file: the minimap's glyphs.
    checks.click(165.0, 63.0);
    checks.shot("01_minimap");

    // The code file's tab, a caret on line 2 (`if True:`) and the fold of its block.
    checks.click(165.0, 85.0);
    checks.shot("02_code");
    checks.click(400.0, 87.0);
    checks.typed_key(
        Modifiers::COMMAND | Modifiers::SHIFT,
        Key::OpenCurlyBracket,
        Key::OpenBracket,
    );
    checks.shot("03_folded");
    // Its "..." marker opens it.
    checks.click(407.0, 87.0);
    checks.shot("04_marker_clicked");
    // Every block at level 1 (Ctrl+K Ctrl+1), then everything open again (Ctrl+K Ctrl+J).
    checks.key(Modifiers::COMMAND, Key::K);
    checks.key(Modifiers::COMMAND, Key::Num1);
    checks.shot("05_fold_level_1");
    checks.key(Modifiers::COMMAND, Key::K);
    checks.key(Modifiers::COMMAND, Key::J);
    checks.shot("06_unfolded_all");
    // The gutter's fold arrows: shown while the mouse is over the gutter.
    checks.hover(245.0, 300.0);
    checks.shot("06b_gutter_arrows");
    // Line 7's arrow folds `class Thing:`.
    checks.click(281.0, 171.0);
    checks.shot("06c_gutter_arrow_clicked");

    // A plugin's popup menu at the mouse; its second item.
    checks.hover(500.0, 200.0);
    checks.key(Modifiers::NONE, Key::F9);
    checks.shot("07_popup_menu");
    checks.click(546.0, 236.0);
    checks.shot("08_popup_chosen");

    // A plugin's HTML sheet.
    checks.key(Modifiers::NONE, Key::F8);
    checks.shot("09_html_sheet");

    // File > Open Recent.
    checks.click(28.0, 11.0);
    checks.hover(60.0, 164.0);
    checks.hover(120.0, 164.0);
    checks.shot("10_open_recent");
}

#[test]
#[ignore = "draws the window with a GPU; run by hand and look at target/tmp/gui-checks"]
fn jump_history_and_context_menus_as_drawn() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    write_profile(data.path());
    let code = files.path().join("code.py");
    std::fs::write(&code, SOURCE).unwrap();
    let other = files.path().join("other.txt");
    std::fs::write(&other, "first line\nsecond line\nthird line\n").unwrap();
    let mut checks = start(data.path(), vec![other, code]);
    checks.shot("j01_start");
    // Two places in code.py, one in other.txt; then back twice and forward once.
    checks.click(400.0, 87.0);
    checks.settle(60);
    checks.click(400.0, 171.0);
    checks.settle(60);
    checks.key(Modifiers::COMMAND, Key::PageUp);
    checks.click(330.0, 104.0);
    checks.settle(60);
    checks.shot("j02_other_file");
    checks.typed_key(Modifiers::ALT, Key::Minus, Key::Minus);
    checks.shot("j03_jumped_back_to_code_line_7");
    checks.typed_key(Modifiers::ALT, Key::Minus, Key::Minus);
    checks.shot("j04_jumped_back_to_code_line_2");
    checks.typed_key(Modifiers::ALT | Modifiers::SHIFT, Key::Minus, Key::Minus);
    checks.shot("j05_jumped_forward_to_code_line_7");

    // Line 7 selected (Shift+End), then a right click on it: the selection stays for Copy.
    checks.click(298.0, 171.0);
    checks.key(Modifiers::SHIFT, Key::End);
    checks.right_click(340.0, 171.0);
    checks.shot("j05b_right_click_keeps_the_selection");
    checks.key(Modifiers::NONE, Key::Escape);

    // The text's menu on the gutter, at line 5.
    checks.right_click(250.0, 137.0);
    checks.shot("j06_gutter_menu");
    checks.key(Modifiers::NONE, Key::Escape);

    // The console (an output panel): its menu, Select All in it.
    checks.key(Modifiers::COMMAND, Key::Backtick);
    checks.shot("j07_console");
    // Its field (focused when it shows) runs Python in the plugin host, one namespace.
    checks.type_text("x = 6 * 7");
    checks.key(Modifiers::NONE, Key::Enter);
    checks.type_text("x");
    checks.key(Modifiers::NONE, Key::Enter);
    checks.shot("j07b_console_python");
    checks.right_click(60.0, 688.0);
    checks.shot("j08_console_menu");
    checks.click(110.0, 775.0);
    checks.shot("j08b_console_all_selected");
    checks.key(Modifiers::NONE, Key::Escape);

    // The find field's menu (Widget Context): Select All, then Cut empties the field.
    checks.key(Modifiers::NONE, Key::Escape);
    checks.key(Modifiers::COMMAND, Key::F);
    checks
        .harness
        .input_mut()
        .events
        .push(Event::Text("needle".into()));
    checks.settle(3);
    checks.right_click(600.0, 814.0);
    checks.shot("j09_find_field_menu");
    checks.click(655.0, 843.0);
    checks.shot("j10_find_field_all_selected");
    checks.right_click(600.0, 814.0);
    checks.click(627.0, 771.0);
    checks.shot("j11_find_field_cut");

    // The replace panel shows "Preserve case"; Alt+A, Alt+C and Alt+R light it, "Case sensitive"
    // and "Regular expression", the focus staying in the panel.
    checks.key(Modifiers::NONE, Key::Escape);
    checks.key(Modifiers::COMMAND, Key::H);
    checks.settle(2);
    checks.key(Modifiers::ALT, Key::A);
    checks.key(Modifiers::ALT, Key::C);
    checks.key(Modifiers::ALT, Key::R);
    checks.shot("j12_replace_panel_toggles");
    for key in [Key::A, Key::C, Key::R] {
        checks.key(Modifiers::ALT, key);
    }
    checks.key(Modifiers::NONE, Key::Escape);
}

#[test]
#[ignore = "draws the window with a GPU; run by hand and look at target/tmp/gui-checks"]
fn annotations_minimap_and_find_results_tab_as_drawn() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    write_profile(data.path());
    let code = files.path().join("code.py");
    std::fs::write(&code, SOURCE).unwrap();
    let long = files.path().join("long.rs");
    std::fs::write(&long, include_str!("../src/views/minimap.rs")).unwrap();
    let mut checks = start(data.path(), vec![code, long]);
    checks.shot("k01_long_file");
    // A plugin's annotations (line 5 and line 151), their regions on the minimap too.
    checks.key(Modifiers::NONE, Key::F7);
    checks.shot("k02_annotations");
    checks.hover(1180.0, 400.0);
    checks.shot("k03_minimap_hovered");
    // The annotation's link: the plugin's on_navigate says which in the status bar.
    checks.click(1118.0, 139.0);
    checks.shot("k04_annotation_link_clicked");
    // A click low on the minimap scrolls the text there; a drag back up scrolls it back.
    checks.click(1200.0, 780.0);
    checks.shot("k05_minimap_clicked");
    checks.drag((1200.0, 780.0), (1200.0, 300.0));
    checks.shot("k06_minimap_dragged");
    // toggle_minimap hides it.
    checks.key(Modifiers::NONE, Key::F6);
    checks.shot("k07_minimap_hidden");
    checks.key(Modifiers::NONE, Key::F6);

    // Find in Files with "Use Buffer": the results in a tab, the matches outlined.
    checks.key(Modifiers::NONE, Key::F5);
    checks
        .harness
        .input_mut()
        .events
        .push(Event::Text("Geometry".into()));
    checks.settle(3);
    checks.shot("k08_find_in_files_panel");
    checks.click(131.0, 773.0);
    checks.shot("k09_use_buffer_on");
    checks.click(600.0, 773.0);
    checks.key(Modifiers::NONE, Key::Enter);
    checks.settle(20);
    checks.shot("k10_find_results_tab");
    // A double click on `54: impl Geometry` opens long.rs there; F4 goes on to the next match.
    checks.double_click(380.0, 332.0);
    checks.shot("k11_result_double_clicked");
    checks.key(Modifiers::NONE, Key::F4);
    checks.shot("k12_next_result");
}

#[test]
#[ignore = "draws the window with a GPU; run by hand and look at target/tmp/gui-checks"]
fn undo_as_drawn() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    write_profile(data.path());
    let code = files.path().join("code.py");
    std::fs::write(&code, SOURCE).unwrap();
    let mut checks = start(data.path(), vec![code]);
    // Typing at the end of line 1, then a new line and more typing: two steps (a line break
    // starts one).
    checks.click(500.0, 70.0);
    checks.key(Modifiers::NONE, Key::End);
    for text in [" # one", "\n", "two"] {
        if text == "\n" {
            checks.key(Modifiers::NONE, Key::Enter);
        } else {
            // One char a frame, as a keyboard types.
            for c in text.chars() {
                checks
                    .harness
                    .input_mut()
                    .events
                    .push(Event::Text(c.to_string()));
                checks.settle(1);
            }
        }
    }
    checks.shot("u01_typed");
    checks.key(Modifiers::COMMAND, Key::Z);
    checks.shot("u02_undo_once");
    checks.key(Modifiers::COMMAND, Key::Z);
    checks.shot("u03_undo_twice");
    checks.key(Modifiers::COMMAND, Key::Y);
    checks.shot("u04_redo");
    // A command (Ctrl+Shift+K deletes the line) is a step of its own.
    checks.key(Modifiers::COMMAND | Modifiers::SHIFT, Key::K);
    checks.shot("u05_line_deleted");
    checks.key(Modifiers::COMMAND, Key::Z);
    checks.shot("u06_line_back");
}

#[test]
#[ignore = "draws the window with a GPU; run by hand and look at target/tmp/gui-checks"]
fn multiple_cursors_as_drawn() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    write_profile(data.path());
    let code = files.path().join("code.py");
    std::fs::write(&code, SOURCE).unwrap();
    let mut checks = start(data.path(), vec![code]);
    // Ctrl + click (Cmd on macOS) adds carets: at the ends of lines 3, 4 and 5.
    checks.click(500.0, 104.0);
    checks.key(Modifiers::NONE, Key::End);
    checks.click_with(Modifiers::COMMAND, PointerButton::Primary, 500.0, 121.0);
    checks.click_with(Modifiers::COMMAND, PointerButton::Primary, 500.0, 138.0);
    checks.shot("m01_carets_added");
    checks.type_text(" # x");
    checks.shot("m02_typed_at_every_caret");
    // Alt + click (Shift + Cmd on macOS) takes the one on line 4 away.
    let subtract = if cfg!(target_os = "macos") {
        Modifiers::COMMAND | Modifiers::SHIFT
    } else {
        Modifiers::ALT
    };
    checks.click_with(subtract, PointerButton::Primary, 500.0, 121.0);
    checks.shot("m03_caret_taken_away");
    checks.key(Modifiers::NONE, Key::Escape);
    // A column drag (Shift + Alt, Alt on macOS) over lines 7 to 9, then typing in it.
    let column = if cfg!(target_os = "macos") {
        Modifiers::ALT
    } else {
        Modifiers::ALT | Modifiers::SHIFT
    };
    checks.drag_with(
        column,
        PointerButton::Primary,
        (315.0, 172.0),
        (330.0, 206.0),
    );
    checks.shot("m04_column_selected");
    checks.type_text("Z");
    checks.shot("m05_typed_in_the_column");
    // Ctrl + double click adds a word.
    checks.key(Modifiers::NONE, Key::Escape);
    checks.click(320.0, 240.0);
    let pos = Pos2::new(340.0, 70.0);
    checks
        .harness
        .input_mut()
        .events
        .push(Event::PointerMoved(pos));
    checks.step();
    checks.harness.input_mut().modifiers = Modifiers::COMMAND;
    for _ in 0..2 {
        for pressed in [true, false] {
            checks
                .harness
                .input_mut()
                .events
                .push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::COMMAND,
                });
        }
        checks.step();
    }
    checks.harness.input_mut().modifiers = Modifiers::NONE;
    checks.shot("m06_word_added");
    // Down and Shift + Down move every caret (multiple cursors, phase 3).
    checks.key(Modifiers::NONE, Key::Escape);
    checks.click(330.0, 104.0);
    checks.click_with(Modifiers::COMMAND, PointerButton::Primary, 330.0, 138.0);
    checks.key(Modifiers::NONE, Key::ArrowDown);
    checks.shot("m07_carets_moved_down");
    checks.key(Modifiers::SHIFT, Key::ArrowDown);
    checks.shot("m08_selections_extended_down");
    checks.type_text("Y");
    checks.shot("m09_typed_after_moving");
    // Copy a column of two selections (lines 6 and 7) and paste it at two carets: a line each
    // (multiple cursors, phase 4). This goes through the system's clipboard.
    checks.key(Modifiers::NONE, Key::Escape);
    checks.drag_with(
        column,
        PointerButton::Primary,
        (314.0, 155.0),
        (350.0, 172.0),
    );
    checks.harness.input_mut().events.push(Event::Copy);
    checks.step();
    checks.shot("m10_column_copied");
    checks.click(500.0, 70.0);
    checks.click_with(Modifiers::COMMAND, PointerButton::Primary, 500.0, 87.0);
    checks
        .harness
        .input_mut()
        .events
        .push(Event::Paste(String::new()));
    checks.step();
    checks.shot("m11_pasted_a_line_at_each_caret");
}

#[test]
#[ignore = "draws the window with a GPU; run by hand and look at target/tmp/gui-checks"]
fn multiple_cursors_over_wrapped_lines_as_drawn() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    write_profile(data.path());
    std::fs::write(
        data.path()
            .join("Packages/User/Preferences.sublime-settings"),
        r#"{ "word_wrap": true }"#,
    )
    .unwrap();
    // A line that wraps into several rows, then a short one.
    let words: Vec<String> = (0..60).map(|i| format!("word{i:02}")).collect();
    let wrapped = files.path().join("wrapped.txt");
    std::fs::write(&wrapped, format!("{}\nshort\nlast line\n", words.join(" "))).unwrap();
    let mut checks = start(data.path(), vec![wrapped]);
    // Two carets on the first row, then Down: each goes to the row below at its column (not to
    // the next line), and on down the rows of the wrapped line, then to the short line.
    checks.click(330.0, 70.0);
    checks.click_with(Modifiers::COMMAND, PointerButton::Primary, 420.0, 70.0);
    checks.shot("w01_two_carets_on_the_first_row");
    checks.key(Modifiers::NONE, Key::ArrowDown);
    checks.shot("w02_down_to_the_second_row");
    checks.key(Modifiers::NONE, Key::ArrowDown);
    checks.key(Modifiers::NONE, Key::ArrowDown);
    checks.shot("w03_down_two_more_rows");
    checks.key(Modifiers::NONE, Key::ArrowDown);
    checks.key(Modifiers::NONE, Key::ArrowDown);
    checks.shot("w03b_down_to_the_short_line");
    // A column drag over the first rows: a selection on each row, the columns of the drag.
    checks.key(Modifiers::NONE, Key::Escape);
    let column = if cfg!(target_os = "macos") {
        Modifiers::ALT
    } else {
        Modifiers::ALT | Modifiers::SHIFT
    };
    checks.drag_with(
        column,
        PointerButton::Primary,
        (340.0, 70.0),
        (380.0, 104.0),
    );
    checks.shot("w04_column_over_rows");
    // Past the end of the rows: up to each row's end.
    checks.key(Modifiers::NONE, Key::Escape);
    checks.drag_with(
        column,
        PointerButton::Primary,
        (340.0, 70.0),
        (1250.0, 104.0),
    );
    checks.shot("w05_column_past_the_rows_ends");
}

#[test]
#[ignore = "draws the window with a GPU; run by hand and look at target/tmp/gui-checks"]
fn quick_add_and_skip_next_as_drawn() {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    write_profile(data.path());
    let code = files.path().join("skip.py");
    std::fs::write(&code, "x = x + x + x\n").unwrap();
    let mut checks = start(data.path(), vec![code]);
    // Ctrl+D (Cmd+D) twice: the first x, then the second one.
    checks.click(299.0, 70.0);
    checks.key(Modifiers::COMMAND, Key::D);
    checks.key(Modifiers::COMMAND, Key::D);
    checks.shot("k01_two_added");
    // Ctrl+K Ctrl+D: the last one (the second x) gives way to the third.
    checks.key(Modifiers::COMMAND, Key::K);
    checks.key(Modifiers::COMMAND, Key::D);
    checks.shot("k02_second_skipped_for_the_third");
    // Again: the third gives way to the fourth.
    checks.key(Modifiers::COMMAND, Key::K);
    checks.key(Modifiers::COMMAND, Key::D);
    checks.shot("k03_third_skipped_for_the_fourth");
    // Typing goes to the first and the fourth x.
    checks.type_text("Z");
    checks.shot("k04_typed_at_first_and_fourth");
}
