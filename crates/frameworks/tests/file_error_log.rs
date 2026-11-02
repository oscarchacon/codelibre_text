//! Tests for the error log on disk (`gateways::file_error_log`).

use std::time::{Duration, UNIX_EPOCH};

use codelibre_text::gateways::file_error_log::{utc_time, FileErrorLog};
use use_cases::gateways::ErrorLogGateway;

#[test]
fn utc_time_formats_dates_and_leap_days() {
    assert_eq!(utc_time(UNIX_EPOCH), "1970-01-01 00:00:00 UTC");
    assert_eq!(utc_time(UNIX_EPOCH + Duration::from_secs(951_825_600)), "2000-02-29 12:00:00 UTC");
    assert_eq!(utc_time(UNIX_EPOCH + Duration::from_secs(1_793_631_785)), "2026-11-02 15:03:05 UTC");
}

#[test]
fn errors_are_appended_with_their_time_and_tracebacks_indented() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Local").join("Errors.log");
    let log = FileErrorLog::new(path.clone());
    log.append(&["first".into()]);
    log.append(&["Traceback:\n  File \"a.py\"\nValueError: x\n".into()]);
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "{text}");
    assert!(lines[0].starts_with('[') && lines[0].ends_with("UTC] first"), "{text}");
    assert!(lines[1].ends_with("UTC] Traceback:"));
    assert_eq!(lines[2..], ["      File \"a.py\"", "    ValueError: x"]);
    assert_eq!(log.path(), path);
}

#[test]
fn a_big_log_drops_its_oldest_half() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Errors.log");
    let log = FileErrorLog::new(path.clone());
    for i in 0..1200 {
        log.append(&[format!("error {i:04} {}", "x".repeat(500))]);
    }
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.len() < 600 * 1024, "{}", text.len());
    assert!(text.starts_with('['));
    assert!(!text.contains("error 0000"));
    assert!(text.contains("error 1199"));
}
