//! Views driven by synthetic input in a real `egui::Context`, without a window.

use codelibre_text::views::theme::Chrome;
use codelibre_text::views::{input_panel, output_panel, quick_panel};
use egui::{CentralPanel, Color32, Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use entities::minihtml::{Line, Span};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{CommandInputViewModel, InputPanelViewModel, OutputPanelViewModel, QuickPanelItemViewModel, QuickPanelViewModel};

fn chrome() -> Chrome {
    let color = Color32::from_gray(128);
    Chrome {
        dark: true,
        editor_bg: color,
        panel_bg: color,
        tab_bg: color,
        hover_bg: color,
        fg: color,
        fg_dim: color,
        accent: color,
        border: color,
    }
}

fn input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
        events,
        ..Default::default()
    }
}

/// Runs a frame of `show` with `events` and returns what it reported.
fn frame(ctx: &Context, events: Vec<Event>, show: &mut dyn FnMut(&mut egui::Ui, &mut Vec<UiEvent>)) -> Vec<UiEvent> {
    let mut reported = Vec::new();
    let _ = ctx.run(input(events), |ctx| {
        CentralPanel::default().show(ctx, |ui| show(ui, &mut reported));
    });
    reported
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

fn panel() -> OutputPanelViewModel {
    OutputPanelViewModel {
        name: "exec".into(),
        text: "compiling\na.py:2:5 boom\na.py:3:1 second\n".into(),
        font_size: 13.0,
        word_wrap: true,
        first_line: 10,
        current_line: None,
        result_revision: 0,
        highlight: None,
        selections: Vec::new(),
    }
}

/// Where the line `line` of the panel's text is, found by clicking down the panel.
fn opened_at(y: f32, clicks: usize) -> Vec<UiEvent> {
    let ctx = Context::default();
    let chrome = chrome();
    let panel = panel();
    let mut show = |ui: &mut egui::Ui, events: &mut Vec<UiEvent>| output_panel::show(ui, &chrome, &panel, None, events);
    let at = Pos2::new(40.0, y);
    frame(&ctx, Vec::new(), &mut show);
    frame(&ctx, vec![Event::PointerMoved(at)], &mut show);
    let mut reported = Vec::new();
    for _ in 0..clicks {
        reported.extend(frame(&ctx, vec![button(at, true)], &mut show));
        reported.extend(frame(&ctx, vec![button(at, false)], &mut show));
    }
    reported
}

fn opened_lines(y: f32, clicks: usize) -> Vec<usize> {
    let opened = opened_at(y, clicks).into_iter();
    opened.filter_map(|e| if let UiEvent::OutputPanelLineOpened(line) = e { Some(line) } else { None }).collect()
}

#[test]
fn double_clicking_a_line_of_the_output_panel_opens_it() {
    let lines: Vec<usize> = (20..120).step_by(4).flat_map(|y| opened_lines(y as f32, 2)).collect();
    assert!(!lines.is_empty(), "no double click was seen");
    let mut distinct = lines.clone();
    distinct.dedup();
    assert!(distinct.contains(&10) && distinct.contains(&11) && distinct.contains(&12), "{distinct:?}");
    assert!(lines.windows(2).all(|w| w[0] <= w[1]), "lines go down the panel: {lines:?}");
}

#[test]
fn one_click_opens_nothing() {
    assert_eq!(opened_lines(40.0, 1), Vec::<usize>::new());
}

fn quick_panel_frame(ctx: &Context, detail: &str, events: Vec<Event>) -> Vec<UiEvent> {
    let details = if detail.is_empty() { Vec::new() } else { vec![vec![Span { text: detail.into(), ..Default::default() }]] };
    quick_panel_details_frame(ctx, &details, events)
}

fn quick_panel_details_frame(ctx: &Context, details: &[Line], events: Vec<Event>) -> Vec<UiEvent> {
    let panel = QuickPanelViewModel {
        placeholder: String::new(),
        keep_open: false,
        no_matches: String::new(),
        query: String::new(),
        selected: 0,
        items: vec![QuickPanelItemViewModel {
            caption: "Preferences: Select Interface Language".into(),
            details: details.to_vec(),
            hint: String::new(),
            kind: String::new(),
        }],
        take_focus: false,
        monospace: false,
        input: None,
    };
    let mut reported = Vec::new();
    let _ = ctx.run(input(events), |ctx| quick_panel::show(ctx, &chrome(), &panel, &mut reported));
    reported
}

/// What one click at each of `points` on the first item reports, the click in frames of its own or,
/// as a fast click does, moving, pressing and releasing in one frame.
fn quick_panel_clicks(detail: &str, points: &[Pos2], one_frame: bool) {
    for &at in points {
        let ctx = Context::default();
        // An Area's first frame only measures it: nothing in it can be clicked yet.
        quick_panel_frame(&ctx, detail, Vec::new());
        quick_panel_frame(&ctx, detail, Vec::new());
        let reported = if one_frame {
            quick_panel_frame(&ctx, detail, vec![Event::PointerMoved(at), button(at, true), button(at, false)])
        } else {
            quick_panel_frame(&ctx, detail, vec![Event::PointerMoved(at)]);
            let mut reported = quick_panel_frame(&ctx, detail, vec![button(at, true)]);
            reported.extend(quick_panel_frame(&ctx, detail, vec![button(at, false)]));
            reported
        };
        assert!(matches!(reported.as_slice(), [UiEvent::QuickPanelAccepted(0)]), "click at {at:?}: {reported:?}");
    }
}

// The panel is 420 wide, centred in 800: its first row starts at x 190, y about 88.
fn along(y: f32) -> Vec<Pos2> {
    (220..400).step_by(20).map(|x| Pos2::new(x as f32, y)).collect()
}

#[test]
fn one_click_on_the_text_of_a_quick_panel_item_accepts_it() {
    quick_panel_clicks("", &along(100.0), false);
    quick_panel_clicks("", &along(100.0), true);
}

#[test]
fn one_click_on_the_caption_or_detail_of_a_two_line_item_accepts_it() {
    let detail = "assets/Default/Localization/messages.pot";
    for y in [96.0, 104.0, 112.0, 118.0] {
        quick_panel_clicks(detail, &along(y), false);
        quick_panel_clicks(detail, &along(y), true);
    }
}

#[test]
fn a_click_on_a_link_in_the_details_opens_it_and_anywhere_else_accepts_the_item() {
    let text = |text: &str| Span { text: text.into(), ..Default::default() };
    let link = Span { link: Some("https://github.com/x/y".into()), ..text("github.com/x/y") };
    let details = vec![vec![text("A package")], vec![text("install v2.0.0 "), link]];
    let mut links = 0;
    // Down the two detail lines, from their start to past the link.
    for (x, y) in (204..420).step_by(6).flat_map(|x| [(x, 112.0), (x, 126.0)]) {
        let at = Pos2::new(x as f32, y);
        let ctx = Context::default();
        quick_panel_details_frame(&ctx, &details, Vec::new());
        quick_panel_details_frame(&ctx, &details, Vec::new());
        quick_panel_details_frame(&ctx, &details, vec![Event::PointerMoved(at)]);
        let mut reported = quick_panel_details_frame(&ctx, &details, vec![button(at, true)]);
        reported.extend(quick_panel_details_frame(&ctx, &details, vec![button(at, false)]));
        match reported.as_slice() {
            [UiEvent::QuickPanelLinkClicked(href)] => {
                assert_eq!(href, "https://github.com/x/y");
                links += 1;
            }
            [UiEvent::QuickPanelAccepted(0)] => {}
            other => panic!("click at {at:?}: {other:?}"),
        }
    }
    assert!(links > 3, "the link takes clicks: {links}");
}

#[test]
fn enter_in_the_input_panel_leaves_nothing_for_the_text_area() {
    let ctx = Context::default();
    let chrome = chrome();
    let mut panel = InputPanelViewModel {
        id: 1,
        caption: "Name:".into(),
        text: "abc".into(),
        selection: (3, 3),
        selection_revision: 0,
        take_focus: true,
        input: None,
    };
    let mut left_over = Vec::new();
    let mut show = |ui: &mut egui::Ui, events: &mut Vec<UiEvent>| {
        input_panel::show(ui, &chrome, &panel, events);
        left_over = ui.input(|i| i.events.clone());
    };
    frame(&ctx, Vec::new(), &mut show);
    drop(show);
    panel.take_focus = false;
    let mut show = |ui: &mut egui::Ui, events: &mut Vec<UiEvent>| {
        input_panel::show(ui, &chrome, &panel, events);
        left_over = ui.input(|i| i.events.clone());
    };
    frame(&ctx, Vec::new(), &mut show);
    let key = |key: Key, pressed: bool| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
    let reported = frame(&ctx, vec![Event::Text("y".into()), key(Key::Enter, true), key(Key::Enter, false)], &mut show);
    assert!(reported.iter().any(|e| matches!(e, UiEvent::InputPanelChanged(text) if text == "abcy")), "{reported:?}");
    assert!(matches!(reported.last(), Some(UiEvent::InputPanelDone)), "{reported:?}");
    assert!(left_over.is_empty(), "the text area would take them: {left_over:?}");
}

#[test]
fn a_background_splits_the_sections_it_covers_and_keeps_their_colors() {
    let mut job = egui::text::LayoutJob::default();
    let red = egui::TextFormat::simple(egui::FontId::monospace(12.0), Color32::RED);
    let blue = egui::TextFormat::simple(egui::FontId::monospace(12.0), Color32::BLUE);
    job.append("abcd", 0.0, red);
    job.append("efgh", 0.0, blue);
    output_panel::paint_background(&mut job, 2..6, Color32::YELLOW);
    let parts: Vec<_> = job.sections.iter().map(|s| (s.byte_range.clone(), s.format.color, s.format.background)).collect();
    let none = Color32::TRANSPARENT;
    assert_eq!(
        parts,
        [
            (0..2, Color32::RED, none),
            (2..4, Color32::RED, Color32::YELLOW),
            (4..6, Color32::BLUE, Color32::YELLOW),
            (6..8, Color32::BLUE, none),
        ]
    );
}

fn backspace() -> Vec<Event> {
    let key = |pressed: bool| Event::Key { key: Key::Backspace, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE };
    vec![key(true), key(false)]
}

/// The input panel of an input handler with `text`, focused, after Backspace.
fn input_panel_back(text: &str, can_go_back: bool) -> Vec<UiEvent> {
    let ctx = Context::default();
    let chrome = chrome();
    let mut panel = InputPanelViewModel {
        id: 1,
        caption: "Name:".into(),
        text: text.into(),
        selection: (text.len(), text.len()),
        selection_revision: 0,
        take_focus: true,
        input: Some(CommandInputViewModel { breadcrumb: vec!["Rename".into()], can_go_back, preview: Vec::new() }),
    };
    frame(&ctx, Vec::new(), &mut |ui: &mut egui::Ui, events: &mut Vec<UiEvent>| input_panel::show(ui, &chrome, &panel, events));
    panel.take_focus = false;
    frame(&ctx, backspace(), &mut |ui: &mut egui::Ui, events: &mut Vec<UiEvent>| input_panel::show(ui, &chrome, &panel, events))
}

#[test]
fn backspace_in_an_input_handler_s_empty_field_goes_back() {
    assert!(matches!(input_panel_back("", true).as_slice(), [UiEvent::PluginPanelBack]));
    let typed = input_panel_back("ab", true);
    assert!(!typed.iter().any(|e| matches!(e, UiEvent::PluginPanelBack)), "it deletes: {typed:?}");
    assert!(input_panel_back("", false).is_empty(), "the first panel of a plain show_input_panel");
}

#[test]
fn backspace_in_an_input_handler_s_empty_list_query_goes_back() {
    let ctx = Context::default();
    let mut panel = QuickPanelViewModel {
        placeholder: String::new(),
        keep_open: false,
        no_matches: String::new(),
        query: String::new(),
        selected: 0,
        items: Vec::new(),
        take_focus: true,
        monospace: false,
        input: Some(CommandInputViewModel { breadcrumb: vec!["Open".into(), "a.txt".into()], can_go_back: true, preview: Vec::new() }),
    };
    let mut reported = Vec::new();
    let _ = ctx.run(input(Vec::new()), |ctx| quick_panel::show(ctx, &chrome(), &panel, &mut reported));
    panel.take_focus = false;
    let _ = ctx.run(input(backspace()), |ctx| quick_panel::show(ctx, &chrome(), &panel, &mut reported));
    assert!(matches!(reported.as_slice(), [UiEvent::PluginPanelBack]), "{reported:?}");
}
