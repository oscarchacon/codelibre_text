//! Tests for `views::minimap`: where it is against the text area, and clicks on it.

use codelibre_text::views::minimap::{self, Colors, Geometry, Text, SCALE};
use egui::{CentralPanel, Color32, Context, Event, FontId, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use interface_adapters::view_models::MinimapViewModel;

fn geometry(text_height: f32, offset: f32) -> Geometry {
    Geometry { text_height, content_height: text_height, viewport_height: 500.0, offset, height: 500.0 }
}

#[test]
fn narrow_text_areas_have_no_minimap() {
    assert_eq!(minimap::width(200.0), 0.0);
    assert_eq!(minimap::width(800.0), 96.0);
    assert_eq!(minimap::width(4000.0), 140.0, "at most");
}

#[test]
fn a_text_that_fits_in_the_minimap_does_not_scroll_it() {
    // 2000 px of text are 300 px of minimap: it fits in 500.
    let at_end = geometry(2000.0, 1500.0);
    assert_eq!(at_end.scroll(), 0.0);
    assert_eq!(at_end.viewport_box(), 1500.0 * SCALE..2000.0 * SCALE);
    // Clicking puts the middle of the box there.
    assert!((at_end.offset_for_pointer(150.0) - (150.0 / SCALE - 250.0)).abs() < 0.01);
    assert_eq!(at_end.offset_for_pointer(0.0), 0.0, "not above the start");
    assert_eq!(at_end.offset_for_pointer(500.0), 1500.0, "nor past the end");
}

#[test]
fn a_long_text_scrolls_the_minimap_in_step() {
    // 10000 px of text are 1500 px of minimap: 1000 do not fit.
    let top = geometry(10_000.0, 0.0);
    assert_eq!((top.scroll(), top.viewport_box().start), (0.0, 0.0));
    let end = geometry(10_000.0, 9_500.0);
    assert_eq!(end.scroll(), 1000.0);
    let span = end.viewport_box();
    assert!((span.end - 500.0).abs() < 0.01, "the box ends at the minimap's bottom: {span:?}");
    let middle = geometry(10_000.0, 4_750.0);
    assert_eq!(middle.scroll(), 500.0);
    // A click moves the box under the pointer, so the same click again leaves it there.
    let offset = middle.offset_for_pointer(400.0);
    let moved = geometry(10_000.0, offset);
    let span = moved.viewport_box();
    assert!(((span.start + span.end) / 2.0 - 400.0).abs() < 0.01, "{span:?}");
    assert!((moved.offset_for_pointer(400.0) - offset).abs() < 0.01);
}

#[test]
fn past_the_end_scrolling_counts_for_the_box() {
    let geometry = Geometry { text_height: 2000.0, content_height: 2400.0, viewport_height: 500.0, offset: 1900.0, height: 500.0 };
    assert_eq!(geometry.viewport_box(), 1900.0 * SCALE..2400.0 * SCALE);
    assert_eq!(geometry.offset_for_pointer(500.0), 1900.0);
}

/// One frame of a minimap at (500, 0) 100×400 over 200 lines of text; returns the scroll asked for.
fn minimap_frame(ctx: &Context, events: Vec<Event>) -> Option<f32> {
    let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
    let mut asked = None;
    let _ = ctx.run(input, |ctx| {
        CentralPanel::default().show(ctx, |ui| {
            let text = "fn main() {}\n".repeat(200);
            let galley = ui.fonts(|f| f.layout_no_wrap(text, FontId::monospace(14.0), Color32::WHITE));
            let row_height = galley.rows[0].rect.height();
            let geometry = Geometry {
                text_height: galley.rect.height(),
                content_height: galley.rect.height(),
                viewport_height: 400.0,
                offset: 0.0,
                height: 400.0,
            };
            let text = Text { galley: &galley, row_height, selection: 0..4, regions: &[], region_colors: &[] };
            let colors = Colors { background: Color32::BLACK, foreground: Color32::WHITE, selection: Color32::BLUE, border: None };
            let area = Rect::from_min_size(Pos2::new(500.0, 0.0), Vec2::new(100.0, 400.0));
            let settings = MinimapViewModel { always_show_viewport: true, draw_border: true };
            asked = minimap::show(ui, egui::Id::new("minimap"), area, &settings, &text, geometry, &colors);
        });
    });
    asked
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

#[test]
fn clicking_the_minimap_scrolls_the_text_there_and_nothing_else_does() {
    let ctx = Context::default();
    assert_eq!(minimap_frame(&ctx, vec![]), None);
    let at = Pos2::new(550.0, 390.0);
    assert_eq!(minimap_frame(&ctx, vec![Event::PointerMoved(at)]), None, "hovering does not scroll");
    let offset = minimap_frame(&ctx, vec![button(at, true)]).expect("pressing scrolls");
    assert!(offset > 0.0);
    // Off the minimap nothing happens.
    let ctx = Context::default();
    let away = Pos2::new(100.0, 390.0);
    assert_eq!(minimap_frame(&ctx, vec![Event::PointerMoved(away), button(away, true)]), None);
}
