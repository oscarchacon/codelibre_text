//! Use cases: the Help menu's links (report an issue) and Sublime's `open_url`.

use crate::editor::Editor;

/// GitHub's "new issue" page of the project.
pub const NEW_ISSUE_URL: &str = "https://github.com/oscarchacon/codelibre_text/issues/new";

impl Editor {
    /// `open_url`: opens a link in the default browser.
    pub fn open_url(&mut self, url: &str) {
        if url.is_empty() {
            self.notify("open_url: no url given");
            return;
        }
        self.gateways.shell.open_url(url);
    }

    /// Help > Report an Issue: the bug report form, with the version and the OS filled in.
    pub fn report_issue(&mut self) {
        let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let url = issue_report_url(env!("CARGO_PKG_VERSION"), &os);
        self.gateways.shell.open_url(&url);
    }
}

/// The bug report form (`.github/ISSUE_TEMPLATE/bug_report.yml`) with its `version` and `os` fields filled in.
pub fn issue_report_url(version: &str, os: &str) -> String {
    format!(
        "{NEW_ISSUE_URL}?template=bug_report.yml&version={}&os={}",
        encode_query_value(version),
        encode_query_value(os)
    )
}

/// Percent-encodes everything but unreserved characters (RFC 3986).
fn encode_query_value(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}
