//! Tests for the help use cases (`use_cases::help`).

use test_support::{status, World, ERROR_LOG, HOME_FOLDER};
use use_cases::help::{issue_report_url, NEW_ISSUE_URL, REPORT_ERRORS_LIMIT};

#[test]
fn issue_report_url_fills_in_the_bug_report_form() {
    assert_eq!(
        issue_report_url("0.1.0", "windows x86_64", ""),
        format!("{NEW_ISSUE_URL}?template=bug_report.yml&version=0.1.0&os=windows%20x86_64")
    );
}

#[test]
fn issue_report_url_encodes_reserved_characters() {
    assert!(issue_report_url("1.0&x=y", "a/b", "").ends_with("version=1.0%26x%3Dy&os=a%2Fb"));
}

#[test]
fn issue_report_url_puts_the_errors_in_the_logs_field() {
    assert!(issue_report_url("0.1.0", "linux", "a: b\nc").ends_with("&os=linux&logs=a%3A%20b%0Ac"));
}

#[test]
fn report_issue_opens_the_form_with_this_version_and_os() {
    let world = World::new();
    let mut editor = world.editor();
    editor.report_issue();
    let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    assert_eq!(*world.opened_urls.borrow(), vec![issue_report_url(env!("CARGO_PKG_VERSION"), &os, "")]);
}

fn reported_errors(world: &World) -> String {
    let url = world.opened_urls.borrow().last().cloned().unwrap();
    let logs = url.split("&logs=").nth(1).unwrap_or_default();
    // Enough decoding for these tests: ASCII only.
    let bytes: Vec<u8> = logs.as_bytes().to_vec();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            decoded.push(u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap(), 16).unwrap());
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).unwrap()
}

#[test]
fn load_errors_go_to_the_error_log_and_into_the_report_without_the_home_folder() {
    let world = World::new();
    world.resources.errors.borrow_mut().push(format!("{HOME_FOLDER}/Packages/bad.sublime-keymap: expected `]`"));
    let mut editor = world.editor();
    assert_eq!(*world.logged_errors.borrow(), vec![format!("{HOME_FOLDER}/Packages/bad.sublime-keymap: expected `]`")]);
    editor.report_issue();
    assert_eq!(reported_errors(&world), "~/Packages/bad.sublime-keymap: expected `]`");
}

#[test]
fn an_error_seen_again_is_logged_again_but_reported_once() {
    let world = World::new();
    world.resources.errors.borrow_mut().push("bad.sublime-keymap: expected `]`".into());
    let mut editor = world.editor();
    editor.reload_packages();
    assert_eq!(world.logged_errors.borrow().len(), 2);
    assert_eq!(editor.state().session_errors.len(), 1);
}

#[test]
fn the_report_keeps_the_newest_errors_that_fit_and_says_where_the_rest_are() {
    let world = World::new();
    for i in 0..100 {
        world.resources.errors.borrow_mut().push(format!("error {i:03}: {}", "x".repeat(100)));
    }
    let mut editor = world.editor();
    editor.report_issue();
    let url = world.opened_urls.borrow()[0].clone();
    let logs = url.split("&logs=").nth(1).unwrap();
    assert!(logs.len() <= REPORT_ERRORS_LIMIT + 200, "{}", logs.len());
    let report = reported_errors(&world);
    assert!(report.starts_with(&format!("(Shortened: every error is in {ERROR_LOG})\n\nerror ")), "{report}");
    assert!(report.ends_with(&format!("error 099: {}", "x".repeat(100))));
}

#[test]
fn an_error_too_long_for_the_report_keeps_its_end() {
    let world = World::new();
    world.resources.errors.borrow_mut().push(format!("Traceback:\n{}\nValueError: the cause", "  line\n".repeat(2000)));
    let mut editor = world.editor();
    editor.report_issue();
    let report = reported_errors(&world);
    assert!(report.ends_with("ValueError: the cause"));
    assert!(report.starts_with("(Shortened"), "{report}");
    assert!(!report.contains("Traceback"));
}

#[test]
fn open_error_log_opens_it_in_a_tab() {
    let world = World::new();
    world.resources.errors.borrow_mut().push("bad.sublime-keymap: expected `]`".into());
    let mut editor = world.editor();
    editor.open_error_log();
    assert_eq!(editor.active_document().unwrap().text, "bad.sublime-keymap: expected `]`");
}

#[test]
fn open_error_log_without_errors_says_so() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_error_log();
    assert_eq!(status(&editor), "No errors have been logged");
}

#[test]
fn open_url_opens_the_link_in_the_browser() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_url("https://example.com");
    assert_eq!(*world.opened_urls.borrow(), vec!["https://example.com".to_string()]);
}

#[test]
fn open_url_without_a_url_only_tells_the_user() {
    let world = World::new();
    let mut editor = world.editor();
    editor.open_url("");
    assert!(world.opened_urls.borrow().is_empty());
    assert_eq!(status(&editor), "open_url: no url given");
}
