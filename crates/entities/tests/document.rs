//! Tests for `entities::document`.

use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use entities::document::*;
use entities::selection::Selection;

fn from_bytes(bytes: &[u8]) -> Document {
    Document::from_bytes(1, Path::new("/p/file.txt"), bytes.to_vec(), None).expect("text file")
}

// ----- reading and writing files -------------------------------------------------------------------

#[test]
fn new_untitled_documents_are_empty_and_clean() {
    let doc = Document::new_untitled(7, LineEnding::Windows);
    assert_eq!(doc.id, 7);
    assert!(doc.text.is_empty() && doc.path.is_none());
    assert_eq!(doc.line_ending, LineEnding::Windows);
    assert_eq!(doc.encoding, Encoding::Utf8);
    assert_eq!(doc.syntax, "Plain Text");
    assert!(!doc.is_dirty());
}

#[test]
fn crlf_files_are_read_as_lf_and_written_back_as_crlf() {
    let mut doc = from_bytes(b"a\r\nb");
    assert_eq!(doc.text, "a\nb");
    assert_eq!(doc.line_ending, LineEnding::Windows);
    assert_eq!(doc.to_bytes(), b"a\r\nb");
}

#[test]
fn lf_files_stay_lf() {
    let mut doc = from_bytes(b"a\nb");
    assert_eq!(doc.line_ending, LineEnding::Unix);
    assert_eq!(doc.to_bytes(), b"a\nb");
}

#[test]
fn the_utf8_bom_is_hidden_and_kept() {
    let mut doc = from_bytes(b"\xEF\xBB\xBFhi");
    assert_eq!(doc.text, "hi");
    assert_eq!(doc.encoding, Encoding::Utf8WithBom);
    assert_eq!(doc.to_bytes(), b"\xEF\xBB\xBFhi");
}

#[test]
fn invalid_utf8_is_read_as_latin1_and_written_back_as_latin1() {
    let mut doc = from_bytes(&[b'a', 0xE9]);
    assert_eq!(doc.text, "aé");
    assert_eq!(doc.encoding, Encoding::Latin1);
    assert_eq!(doc.to_bytes(), vec![b'a', 0xE9]);
}

#[test]
fn latin1_files_with_new_characters_become_utf8() {
    let mut doc = from_bytes(&[b'a', 0xE9]);
    doc.text.push('€');
    assert_eq!(doc.to_bytes(), "aé€".as_bytes());
    assert_eq!(doc.encoding, Encoding::Utf8);
}

#[test]
fn binary_files_are_rejected() {
    let result = Document::from_bytes(1, Path::new("x.bin"), vec![b'M', b'Z', 0, 1], None);
    assert_eq!(result.unwrap_err(), DocumentError::Binary);
}

#[test]
fn from_bytes_records_the_path_and_modification_time() {
    let time = UNIX_EPOCH + Duration::from_secs(5);
    let doc = Document::from_bytes(3, Path::new("/p/a.rs"), b"x".to_vec(), Some(time)).unwrap();
    assert_eq!(doc.path, Some(PathBuf::from("/p/a.rs")));
    assert_eq!(doc.modified_at, Some(time));
    assert!(!doc.is_dirty());
}

// ----- dirty state ---------------------------------------------------------------------------------

#[test]
fn editing_makes_a_document_dirty_until_saved() {
    let mut doc = Document::new_untitled(1, LineEnding::Unix);
    doc.text.push('x');
    assert!(doc.is_dirty());
    doc.mark_saved(Path::new("/p/x.txt"), None);
    assert!(!doc.is_dirty());
    assert_eq!(doc.path, Some(PathBuf::from("/p/x.txt")));
}

#[test]
fn undoing_back_to_the_saved_text_is_clean_again() {
    let mut doc = from_bytes(b"abc");
    doc.text.push('d');
    doc.text.pop();
    assert!(!doc.is_dirty());
}

#[test]
fn changing_line_endings_of_a_file_makes_it_dirty() {
    let mut doc = from_bytes(b"a\nb");
    doc.line_ending = LineEnding::Windows;
    assert!(doc.is_dirty());

    let mut untitled = Document::new_untitled(1, LineEnding::Unix);
    untitled.line_ending = LineEnding::Windows;
    assert!(!untitled.is_dirty(), "nothing on disk to differ from");
}

#[test]
fn read_only_documents_are_never_dirty() {
    let mut doc = Document::scratch(1, "Defaults", "{}", "JSON", true);
    doc.text.push('x');
    assert!(!doc.is_dirty());
}

#[test]
fn restoring_unsaved_text_keeps_the_document_dirty() {
    let mut doc = from_bytes(b"saved");
    doc.restore_unsaved_text("unsaved".into());
    assert_eq!(doc.text, "unsaved");
    assert!(doc.is_dirty());
}

#[test]
fn reload_takes_the_new_contents_and_keeps_the_caret_in_range() {
    let mut doc = from_bytes(b"abcdef");
    doc.set_selection(Selection::caret(5));
    doc.text.push('!');
    let revision = doc.selection_revision();

    doc.reload_from(from_bytes(b"ab\r\n"));
    assert_eq!(doc.text, "ab\n");
    assert_eq!(doc.line_ending, LineEnding::Windows);
    assert!(!doc.is_dirty());
    assert_eq!(doc.selection(), Selection::caret(3));
    assert!(doc.selection_revision() > revision);
}

// ----- names ---------------------------------------------------------------------------------------

#[test]
fn name_is_the_file_name() {
    assert_eq!(from_bytes(b"x").name(), "file.txt");
}

#[test]
fn untitled_documents_are_named_after_their_first_line() {
    let mut doc = Document::new_untitled(1, LineEnding::Unix);
    assert_eq!(doc.name(), "untitled");
    doc.text = "   \nsecond".into();
    assert_eq!(doc.name(), "untitled");
    doc.text = "  fn main() {\n}".into();
    assert_eq!(doc.name(), "fn main() {");
    doc.text = "a".repeat(60);
    assert_eq!(doc.name().len(), 40);
}

#[test]
fn scratch_documents_use_their_scratch_name() {
    let doc = Document::scratch(2, "Preferences — Default", "{}", "JSON", true);
    assert_eq!(doc.name(), "Preferences — Default");
    assert_eq!(doc.syntax, "JSON");
    assert!(doc.read_only);
}

#[test]
fn saving_a_scratch_document_turns_it_into_a_file() {
    let mut doc = Document::scratch(2, "Notes", "x", "Plain Text", false);
    doc.mark_saved(Path::new("/p/notes.txt"), None);
    assert_eq!(doc.name(), "notes.txt");
}

#[test]
fn first_line_and_char_count() {
    let mut doc = Document::new_untitled(1, LineEnding::Unix);
    doc.text = "ñandú\nx".into();
    assert_eq!(doc.first_line(), "ñandú");
    assert_eq!(doc.char_count(), 7);
}

// ----- selection and edits -------------------------------------------------------------------------

#[test]
fn commands_bump_the_selection_revision_but_the_view_does_not() {
    let mut doc = from_bytes(b"abc");
    let before = doc.selection_revision();
    doc.sync_selection_from_view(Selection::caret(2));
    assert_eq!(doc.selection_revision(), before);
    assert_eq!(doc.selection(), Selection::caret(2));
    doc.set_selection(Selection::caret(1));
    assert_eq!(doc.selection_revision(), before + 1);
}

#[test]
fn selections_are_clamped_to_the_text() {
    let mut doc = from_bytes(b"abc");
    doc.set_selection(Selection::new(1, 99));
    assert_eq!(doc.selection(), Selection::new(1, 3));
    doc.sync_selection_from_view(Selection::caret(50));
    assert_eq!(doc.selection(), Selection::caret(3));
}

#[test]
fn edit_applies_a_command_and_moves_the_selection() {
    let mut doc = from_bytes(b"abc");
    let changed = doc.edit(|text, _| {
        text.push('d');
        Some(Selection::caret(4))
    });
    assert!(changed);
    assert_eq!(doc.text, "abcd");
    assert_eq!(doc.selection(), Selection::caret(4));
}

#[test]
fn edit_that_does_nothing_keeps_the_selection_revision() {
    let mut doc = from_bytes(b"abc");
    let revision = doc.selection_revision();
    assert!(!doc.edit(|_, _| None));
    assert_eq!(doc.selection_revision(), revision);
}

#[test]
fn read_only_documents_cannot_be_edited() {
    let mut doc = Document::scratch(1, "Defaults", "{}", "JSON", true);
    assert!(!doc.edit(|text, sel| {
        text.clear();
        Some(sel)
    }));
    assert_eq!(doc.text, "{}");
}

// ----- line endings and encodings ------------------------------------------------------------------

#[test]
fn line_ending_settings() {
    assert_eq!(LineEnding::from_setting(Some("windows"), LineEnding::Unix), LineEnding::Windows);
    assert_eq!(LineEnding::from_setting(Some("unix"), LineEnding::Windows), LineEnding::Unix);
    assert_eq!(LineEnding::from_setting(Some("mac"), LineEnding::Windows), LineEnding::Unix);
    assert_eq!(LineEnding::from_setting(Some("system"), LineEnding::Windows), LineEnding::Windows);
    assert_eq!(LineEnding::from_setting(None, LineEnding::Unix), LineEnding::Unix);
}

#[test]
fn labels_for_the_status_bar() {
    assert_eq!(LineEnding::Unix.label(), "Unix");
    assert_eq!(LineEnding::Windows.label(), "Windows");
    assert_eq!(Encoding::Utf8.label(), "UTF-8");
    assert_eq!(Encoding::Utf8WithBom.label(), "UTF-8 with BOM");
    assert_eq!(Encoding::Latin1.label(), "Western (ISO 8859-1)");
}
