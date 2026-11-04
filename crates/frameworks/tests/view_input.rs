//! Views driven by synthetic input in a real `egui::Context`, without a window.

use codelibre_text::views::theme::Chrome;
use codelibre_text::views::{input_panel, output_panel, quick_panel};
use egui::{CentralPanel, Color32, Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use interface_adapters::controllers::ui_controller::UiEvent;
use interface_adapters::view_models::{InputPanelViewModel, OutputPanelViewModel, QuickPanelItemViewModel, QuickPanelViewModel};

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
        first_line: 10,
        current_line: None,
        result_revision: 0,
    }
}

/// Where the line `line` of the panel's text is, found by clicking down the panel.
fn opened_at(y: f32, clicks: usize) -> Vec<UiEvent> {
    let ctx = Context::default();
    let chrome = chrome();
    let panel = panel();
    let mut show = |ui: &mut egui::Ui, events: &mut Vec<UiEvent>| output_panel::show(ui, &chrome, &panel, events);
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
    let panel = QuickPanelViewModel {
        placeholder: String::new(),
        keep_open: false,
        no_matches: String::new(),
        query: String::new(),
        selected: 0,
        items: vec![QuickPanelItemViewModel {
            caption: "Preferences: Select Interface Language".into(),
            detail: detail.into(),
            hint: String::new(),
            kind: String::new(),
        }],
        take_focus: false,
        monospace: false,
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
