//! The whole window, drawn off screen (egui_kittest with wgpu) and saved as pictures, to look at
//! what the views draw without a person at the keyboard. Run by hand (it needs a GPU, or a
//! software Vulkan such as lavapipe):
//! `cargo test -p codelibre_text --test app -- --ignored --nocapture`; the pictures land in
//! `target/tmp/gui-checks`.

use std::path::{Path, PathBuf};

use codelibre_text::app::App;
use codelibre_text::compose;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

const SOURCE: &str = "def outer():\n    if True:\n        a = 1\n        b = 2\n    return 3\n\nclass Thing:\n    def method(self):\n        pass\n\nprint('end')\n";

const PLUGIN: &str = r#"import sublime
import sublime_plugin


class GuiCheckMenuCommand(sublime_plugin.TextCommand):
    def run(self, edit):
        def chosen(index):
            self.view.window().status_message("popup chose %d" % index)
        self.view.show_popup_menu(["First choice", "Second choice", "Third choice"], chosen)


class GuiCheckHtmlCommand(sublime_plugin.WindowCommand):
    def run(self):
        self.window.new_html_sheet("Html sheet", "<h1>Heading</h1><p>Some <b>bold</b> text and <a href='x'>a link</a>.</p>")
"#;

const KEYMAP: &str = r#"[
    { "keys": ["f9"], "command": "gui_check_menu" },
    { "keys": ["f8"], "command": "gui_check_html" },
]"#;

struct Checks {
    harness: Harness<'static, App>,
    out: PathBuf,
}

impl Checks {
    fn shot(&mut self, name: &str) {
        self.settle(5);
        let image = self.harness.render().expect("the window could be drawn");
        let path = self.out.join(format!("{name}.png"));
        image.save(&path).expect("the picture was saved");
        println!("saved {}", path.display());
    }

    fn settle(&mut self, frames: usize) {
        for _ in 0..frames {
            self.harness.step();
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
            self.harness.step();
        }
        self.settle(3);
    }

    fn click(&mut self, x: f32, y: f32) {
        let pos = Pos2::new(x, y);
        let input = self.harness.input_mut();
        input.events.push(Event::PointerMoved(pos));
        self.harness.step();
        for pressed in [true, false] {
            self.harness.input_mut().events.push(Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            self.harness.step();
        }
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
    let mut checks = Checks { harness, out };
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
