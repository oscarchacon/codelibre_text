use entities::command::Command;
use entities::document::{DocumentId, RegionSet};
use entities::minihtml;
use serde_json::{json, Value};
use use_cases::plugin_popups::{HIDE_ON_MOUSE_MOVE, HIDE_ON_MOUSE_MOVE_AWAY};
use use_cases::Editor;

use crate::view_models::{
    AnnotationViewModel, AutoCompleteViewModel, CompletionViewModel, EditorViewModel, HtmlViewModel, PhantomLayout, PhantomViewModel, PopupViewModel,
    RegionViewModel, Underline,
};
use crate::view_state::ViewState;

// Sublime's `add_regions` flags (`sublime.DRAW_EMPTY`, …).
const DRAW_EMPTY: u64 = 1;
const DRAW_NO_FILL: u64 = 32;
const HIDDEN: u64 = 128;
const DRAW_NO_OUTLINE: u64 = 256;
const DRAW_SOLID_UNDERLINE: u64 = 512;
const DRAW_STIPPLED_UNDERLINE: u64 = 1024;
const DRAW_SQUIGGLY_UNDERLINE: u64 = 2048;

/// What the empty window suggests, one line each: the key bound to a command (Ctrl+N, or ⌘ keys
/// on macOS, as the keymap says) and what it does. Commands without a key are left out.
pub fn present_hints(editor: &Editor) -> Vec<String> {
    let hints = [
        (Command::new("new_file"), editor.tr("New File")),
        (Command::new("prompt_open_file"), editor.tr("Open File")),
        (Command::new("prompt_open_folder"), editor.tr("Open Folder")),
        (Command::with_args("show_overlay", json!({ "overlay": "command_palette" })), editor.tr("Command Palette")),
    ];
    hints
        .iter()
        .filter_map(|(command, action)| {
            let keys = editor.state().keymap.binding_for(command)?;
            Some(format!("{keys}  {action}"))
        })
        .collect()
}

/// The active document as the text area should show it.
pub fn present(editor: &mut Editor, view: &mut ViewState) -> Option<EditorViewModel> {
    let index = editor.state().active;
    present_document(editor, view, index)
}

/// The document at `index` as the text area of its group should show it.
pub fn present_document(editor: &mut Editor, view: &mut ViewState, index: usize) -> Option<EditorViewModel> {
    let active = index == editor.state().active;
    let find_matches = if view.find_panel.visible && active { editor.find_matches() } else { Vec::new() };
    let color_scheme = editor.color_scheme_name();
    let doc = editor.state().documents.get(index)?;
    let settings = editor.settings_of(doc.id);

    // The text takes the keyboard focus unless a panel or dialog has it.
    let take_focus = active
        && std::mem::take(&mut view.focus_editor)
        && view.quick_panel.is_none()
        && !view.find_panel.focus_requested
        && editor.state().pending_close.is_none();

    let len = doc.char_count();
    let popup = editor.plugin_popup().filter(|p| p.view == doc.id).map(|p| PopupViewModel {
        location: p.location.min(len),
        lines: minihtml::parse(&p.content),
        html: present_html(editor, &p.content),
        max_width: p.max_width,
        max_height: p.max_height,
        hide_on_mouse_move_away: p.flags & (HIDE_ON_MOUSE_MOVE | HIDE_ON_MOUSE_MOVE_AWAY) != 0,
    });
    let mut phantoms: Vec<PhantomViewModel> = doc
        .phantoms()
        .iter()
        .map(|p| {
            // Sublime's `LAYOUT_INLINE` (0), `LAYOUT_BELOW` (1), `LAYOUT_BLOCK` (2).
            let layout = match p.layout {
                1 => PhantomLayout::Below,
                2 => PhantomLayout::Block,
                _ => PhantomLayout::Inline,
            };
            let (a, b) = p.region;
            let point = if layout == PhantomLayout::Inline { a.min(b) } else { a.max(b) };
            PhantomViewModel { id: p.id, point, lines: minihtml::parse(&p.content), html: present_html(editor, &p.content), layout }
        })
        .filter(|p| !p.lines.is_empty() || !p.html.images.is_empty())
        .collect();
    phantoms.sort_by_key(|p| (p.point, p.id));
    Some(EditorViewModel {
        document: doc.id,
        group: doc.group,
        active,
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
        regions: doc.region_sets().iter().filter_map(|set| present_regions(editor, set, len)).collect(),
        show: doc.show_request().map(|r| (r.point.min(len), r.center, doc.show_revision())),
        take_focus,
        popup,
        phantoms,
        report_hover: editor.plugins_hear_hover(),
        auto_complete: present_auto_complete(editor, doc.id, len),
    })
}

/// At most this many rows of the auto-complete list are shown.
const MAX_COMPLETIONS: usize = 200;

fn present_auto_complete(editor: &Editor, document: DocumentId, len: usize) -> Option<AutoCompleteViewModel> {
    let list = editor.auto_complete().filter(|l| l.view == document)?;
    let items = list
        .shown
        .iter()
        .take(MAX_COMPLETIONS)
        .map(|shown| {
            let c = &shown.completion;
            CompletionViewModel {
                trigger: c.trigger.clone(),
                matched: shown.matched.clone(),
                annotation: c.annotation.clone(),
                kind_letter: c.kind.letter.chars().take(1).collect(),
                kind_name: c.kind.name.clone(),
                kind_id: c.kind.id,
            }
        })
        .collect();
    let selected = list.selected.min(MAX_COMPLETIONS - 1);
    let details_source = list.shown.get(selected).map(|s| s.completion.details.clone()).unwrap_or_default();
    let details = minihtml::parse(&details_source);
    let details_html = present_html(editor, &details_source);
    Some(AutoCompleteViewModel { location: list.start.min(len), items, selected, details, details_html })
}

/// minihtml read into boxes, with the bytes of its images (`<img src>`).
pub fn present_html(editor: &Editor, html: &str) -> HtmlViewModel {
    let block = minihtml::parse_html(html);
    let mut images: Vec<(String, std::rc::Rc<Vec<u8>>)> = Vec::new();
    let mut sources = Vec::new();
    image_sources(&block, &mut sources);
    for src in sources {
        if images.iter().all(|(s, _)| *s != src) {
            if let Some(bytes) = editor.html_image(&src) {
                images.push((src, bytes));
            }
        }
    }
    HtmlViewModel { block, images }
}

fn image_sources(block: &minihtml::Block, sources: &mut Vec<String>) {
    for content in &block.content {
        match content {
            minihtml::Content::Line(line) => sources.extend(line.iter().filter_map(|s| s.image.as_ref().map(|i| i.src.clone()))),
            minihtml::Content::Block(inner) => image_sources(inner, sources),
            minihtml::Content::Table(rows) => rows.iter().flatten().for_each(|cell| image_sources(cell, sources)),
            minihtml::Content::Rule => {}
        }
    }
}

/// A region set as drawn: Sublime fills and outlines regions unless the flags say otherwise
/// (plugins combine the underline flags with `DRAW_NO_FILL | DRAW_NO_OUTLINE`).
fn present_regions(editor: &Editor, set: &RegionSet, len: usize) -> Option<RegionViewModel> {
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
    let mut annotations: Vec<AnnotationViewModel> = set
        .regions
        .iter()
        .zip(&set.annotations)
        .map(|(&(a, b), html)| AnnotationViewModel { point: a.min(b).min(len), lines: minihtml::parse(html), html: present_html(editor, html) })
        .filter(|a| !a.lines.is_empty())
        .collect();
    annotations.sort_by_key(|a| a.point);
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
        key: set.key.clone(),
        annotations,
        annotation_color: set.annotation_color.clone(),
    })
}
