use entities::document::RegionSet;
use serde_json::Value;
use use_cases::Editor;

use crate::view_models::{EditorViewModel, RegionViewModel, Underline};
use crate::view_state::ViewState;

// Sublime's `add_regions` flags (`sublime.DRAW_EMPTY`, …).
const DRAW_EMPTY: u64 = 1;
const DRAW_NO_FILL: u64 = 32;
const HIDDEN: u64 = 128;
const DRAW_NO_OUTLINE: u64 = 256;
const DRAW_SOLID_UNDERLINE: u64 = 512;
const DRAW_STIPPLED_UNDERLINE: u64 = 1024;
const DRAW_SQUIGGLY_UNDERLINE: u64 = 2048;

/// The active document as the text area should show it.
pub fn present(editor: &mut Editor, view: &mut ViewState) -> Option<EditorViewModel> {
    let find_matches = if view.find_panel.visible { editor.find_matches() } else { Vec::new() };
    let settings = editor.view_settings();
    let color_scheme = editor.color_scheme_name();
    let doc = editor.active_document()?;

    // The text takes the keyboard focus unless a panel or dialog has it.
    let take_focus = std::mem::take(&mut view.focus_editor)
        && view.quick_panel.is_none()
        && !view.find_panel.focus_requested
        && editor.state().pending_close.is_none();

    let len = doc.char_count();
    Some(EditorViewModel {
        document: doc.id,
        read_only: doc.read_only,
        syntax: doc.syntax.clone(),
        color_scheme,
        selection: doc.selection(),
        extra_selections: doc.extra_selections().to_vec(),
        selection_revision: doc.selection_revision(),
        font_size: settings.f32("font_size", 11.0).clamp(4.0, 72.0),
        tab_size: settings.usize("tab_size", 4).max(1),
        word_wrap: settings.get("word_wrap") == Some(&Value::Bool(true)),
        line_numbers: settings.bool("line_numbers", true),
        highlight_line: settings.bool("highlight_line", true),
        scroll_past_end: settings.bool("scroll_past_end", true),
        rulers: settings.numbers("rulers"),
        auto_indent: settings.bool("auto_indent", true),
        find_matches,
        regions: doc.region_sets().iter().filter_map(|set| present_regions(set, len)).collect(),
        show: doc.show_request().map(|r| (r.point.min(len), r.center, doc.show_revision())),
        take_focus,
    })
}

/// A region set as drawn: Sublime fills and outlines regions unless the flags say otherwise
/// (plugins combine the underline flags with `DRAW_NO_FILL | DRAW_NO_OUTLINE`).
fn present_regions(set: &RegionSet, len: usize) -> Option<RegionViewModel> {
    if set.flags & HIDDEN != 0 {
        return None;
    }
    let has = |flag: u64| set.flags & flag != 0;
    let underline = if has(DRAW_SQUIGGLY_UNDERLINE) {
        Some(Underline::Squiggly)
    } else if has(DRAW_STIPPLED_UNDERLINE) {
        Some(Underline::Stippled)
    } else if has(DRAW_SOLID_UNDERLINE) {
        Some(Underline::Solid)
    } else {
        None
    };
    let mut ranges: Vec<_> = set
        .regions
        .iter()
        .map(|&(a, b)| a.min(b).min(len)..a.max(b).min(len))
        .collect();
    ranges.sort_by_key(|r| (r.start, r.end));
    Some(RegionViewModel {
        ranges,
        scope: set.scope.clone(),
        icon: set.icon.clone(),
        fill: !has(DRAW_NO_FILL),
        outline: !has(DRAW_NO_OUTLINE),
        underline,
        draw_empty: has(DRAW_EMPTY),
    })
}
