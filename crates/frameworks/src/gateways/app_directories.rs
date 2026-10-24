//! Where packages and the session live, per platform (`directories` crate).
//! On Windows: `%APPDATA%\codelibre_text\data\Packages` and `…\data\Local`.

use std::path::PathBuf;

use use_cases::gateways::PackagePaths;

pub fn package_paths() -> PackagePaths {
    let packages = directories::ProjectDirs::from("", "", "codelibre_text")
        .map(|d| d.data_dir().join("Packages"))
        .unwrap_or_else(|| PathBuf::from("Packages"));
    PackagePaths {
        packages,
        sublime_packages: sublime_text_packages(),
    }
}

pub fn session_file(paths: &PackagePaths) -> PathBuf {
    paths
        .packages
        .parent()
        .map(|data| data.join("Local").join("Session.sublime_session"))
        .unwrap_or_else(|| PathBuf::from("Session.sublime_session"))
}

/// Sublime Text's `Packages` folder, if it is installed.
fn sublime_text_packages() -> Option<PathBuf> {
    let base = directories::BaseDirs::new()?;
    let candidates = if cfg!(target_os = "windows") {
        vec![base.config_dir().join("Sublime Text"), base.config_dir().join("Sublime Text 3")]
    } else if cfg!(target_os = "macos") {
        vec![base.data_dir().join("Sublime Text"), base.data_dir().join("Sublime Text 3")]
    } else {
        vec![base.config_dir().join("sublime-text"), base.config_dir().join("sublime-text-3")]
    };
    candidates.into_iter().map(|dir| dir.join("Packages")).find(|p| p.is_dir())
}
