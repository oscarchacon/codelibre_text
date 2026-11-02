//! Tests for `gateways::session_file` (hot-exit session storage).

use std::path::PathBuf;

use entities::selection::Selection;
use interface_adapters::gateways::session_file::SessionFileGateway;
use use_cases::gateways::SessionGateway;
use use_cases::session::{Session, SessionDocument};

fn sample() -> Session {
    Session {
        folders: vec![PathBuf::from("/p")],
        documents: vec![
            SessionDocument {
                path: Some(PathBuf::from("/p/a.rs")),
                unsaved_text: None,
                syntax: "Rust".into(),
                selection: Selection::new(3, 1),
            },
            SessionDocument {
                path: None,
                unsaved_text: Some("notes\nwith ñ".into()),
                syntax: "Plain Text".into(),
                selection: Selection::caret(0),
            },
        ],
        active: 1,
        layout: vec![("side_bar".into(), false), ("status_bar".into(), true)],
    }
}

#[test]
fn a_saved_session_loads_back_identically() {
    let dir = tempfile::tempdir().unwrap();
    let gateway = SessionFileGateway::new(dir.path().join("Local").join("Session.sublime_session"));
    gateway.save(&sample()).unwrap();
    assert_eq!(gateway.load(), Some(sample()));
}

#[test]
fn saving_creates_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a").join("b").join("session.json");
    SessionFileGateway::new(path.clone()).save(&Session::default()).unwrap();
    assert!(path.is_file());
}

#[test]
fn a_missing_session_is_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(SessionFileGateway::new(dir.path().join("none.json")).load(), None);
}

#[test]
fn a_corrupted_session_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(SessionFileGateway::new(path).load(), None);
}

#[test]
fn sessions_from_older_versions_without_optional_fields_still_load() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    std::fs::write(&path, r#"{ "folders": ["/p"] }"#).unwrap();
    let session = SessionFileGateway::new(path).load().unwrap();
    assert_eq!(session.folders, vec![PathBuf::from("/p")]);
    assert!(session.documents.is_empty() && session.layout.is_empty());
    assert_eq!(session.active, 0);
}
