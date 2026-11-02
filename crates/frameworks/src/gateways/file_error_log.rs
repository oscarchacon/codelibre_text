//! [`ErrorLogGateway`]: `…\data\Local\Errors.log`, each error after the UTC time it happened.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use use_cases::gateways::ErrorLogGateway;

/// Past this size the oldest half of the log is dropped.
const MAX_SIZE: u64 = 512 * 1024;

pub struct FileErrorLog {
    path: PathBuf,
}

impl FileErrorLog {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn write(&self, text: &str) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() > MAX_SIZE) {
            let old = std::fs::read_to_string(&self.path).unwrap_or_default();
            let mut cut = old.len() / 2;
            while !old.is_char_boundary(cut) {
                cut += 1;
            }
            let start = old[cut..].find("\n[").map_or(old.len(), |i| cut + i + 1);
            std::fs::write(&self.path, &old[start..])?;
        }
        std::fs::OpenOptions::new().create(true).append(true).open(&self.path)?.write_all(text.as_bytes())
    }
}

impl ErrorLogGateway for FileErrorLog {
    fn append(&self, errors: &[String]) {
        let time = utc_time(SystemTime::now());
        let mut text = String::new();
        for error in errors {
            // Continuation lines (tracebacks) indented, so each entry starts with its time.
            let mut lines = error.trim_end().lines();
            let _ = writeln!(text, "[{time}] {}", lines.next().unwrap_or_default());
            for line in lines {
                let _ = writeln!(text, "    {line}");
            }
        }
        if let Err(e) = self.write(&text) {
            tracing::warn!("could not write the error log {}: {e}", self.path.display());
        }
    }

    fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

/// `2026-11-02 14:03:05 UTC`.
pub fn utc_time(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // Howard Hinnant's days-to-civil algorithm.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", rest / 3600, rest % 3600 / 60, rest % 60)
}
