//! Tests for the help use cases (`use_cases::help`).

use test_support::{status, World};
use use_cases::help::{issue_report_url, NEW_ISSUE_URL};

#[test]
fn issue_report_url_fills_in_the_bug_report_form() {
    assert_eq!(
        issue_report_url("0.1.0", "windows x86_64"),
        format!("{NEW_ISSUE_URL}?template=bug_report.yml&version=0.1.0&os=windows%20x86_64")
    );
}

#[test]
fn issue_report_url_encodes_reserved_characters() {
    assert!(issue_report_url("1.0&x=y", "a/b").ends_with("version=1.0%26x%3Dy&os=a%2Fb"));
}

#[test]
fn report_issue_opens_the_form_with_this_version_and_os() {
    let world = World::new();
    let mut editor = world.editor();
    editor.report_issue();
    let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    assert_eq!(*world.opened_urls.borrow(), vec![issue_report_url(env!("CARGO_PKG_VERSION"), &os)]);
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
