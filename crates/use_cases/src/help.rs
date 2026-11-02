//! Use cases: the Help menu's links (report an issue), the error log and Sublime's `open_url`.

use crate::editor::Editor;

/// GitHub's "new issue" page of the project.
pub const NEW_ISSUE_URL: &str = "https://github.com/oscarchacon/codelibre_text/issues/new";
/// How much of the report's URL the errors may take, encoded: GitHub refuses much longer links.
pub const REPORT_ERRORS_LIMIT: usize = 6000;

impl Editor {
    /// `open_url`: opens a link in the default browser.
    pub fn open_url(&mut self, url: &str) {
        if url.is_empty() {
            self.notify(self.tr("open_url: no url given").to_string());
            return;
        }
        self.gateways.shell.open_url(url);
    }

    /// Help > Report an Issue: the bug report form, with the version, the OS and this session's
    /// errors filled in. The user reads it all in the browser before sending it.
    pub fn report_issue(&mut self) {
        let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let url = issue_report_url(env!("CARGO_PKG_VERSION"), &os, &self.report_errors());
        self.gateways.shell.open_url(&url);
    }

    /// Help > Error Log: the log in a tab.
    pub fn open_error_log(&mut self) {
        let path = self.gateways.error_log.path();
        if !self.gateways.file_system.exists(&path) {
            self.notify(self.tr("No errors have been logged").to_string());
            return;
        }
        self.open_file(&path);
    }

    /// Writes errors the user is told about to the error log, and keeps them for the report.
    pub(crate) fn record_errors(&mut self, errors: &[String]) {
        if errors.is_empty() {
            return;
        }
        self.gateways.error_log.append(errors);
        for error in errors {
            if !self.state.session_errors.contains(error) {
                self.state.session_errors.push(error.clone());
            }
        }
    }

    /// The newest of this session's errors that fit in the report's link, without the user's
    /// home folder. It is for the maintainers, so it is not translated.
    fn report_errors(&self) -> String {
        let home = self.gateways.shell.home_folder().map(|h| h.display().to_string()).filter(|h| !h.is_empty());
        let hide_home = |text: &str| match &home {
            Some(home) => text.replace(home.as_str(), "~").replace(&home.replace('\\', "/"), "~"),
            None => text.to_string(),
        };
        let separator = encode_query_value("\n\n").len();
        let mut kept = Vec::new();
        let mut shortened = false;
        let mut size = 0;
        for error in self.state.session_errors.iter().rev() {
            let error = hide_home(error.trim_end());
            let cost = encode_query_value(&error).len() + separator;
            if size + cost > REPORT_ERRORS_LIMIT {
                if kept.is_empty() {
                    // Too long on its own (a long traceback): its end says the most.
                    kept.push(tail_within(&error, REPORT_ERRORS_LIMIT / 2));
                }
                shortened = true;
                break;
            }
            size += cost;
            kept.push(error);
        }
        kept.reverse();
        if shortened {
            let log = hide_home(&self.gateways.error_log.path().display().to_string());
            kept.insert(0, format!("(Shortened: every error is in {log})"));
        }
        kept.join("\n\n")
    }
}

/// The bug report form (`.github/ISSUE_TEMPLATE/bug_report.yml`) with its `version`, `os` and,
/// when there are any, `logs` fields filled in.
pub fn issue_report_url(version: &str, os: &str, errors: &str) -> String {
    let mut url = format!(
        "{NEW_ISSUE_URL}?template=bug_report.yml&version={}&os={}",
        encode_query_value(version),
        encode_query_value(os)
    );
    if !errors.is_empty() {
        url.push_str("&logs=");
        url.push_str(&encode_query_value(errors));
    }
    url
}

/// The end of `text` that takes at most `limit` characters once encoded, after a "…".
fn tail_within(text: &str, limit: usize) -> String {
    let mut size = 0;
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, c)| {
            size += encode_query_value(&c.to_string()).len();
            size <= limit
        })
        .last()
        .map_or(text.len(), |(i, _)| i);
    format!("…{}", &text[start..])
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
