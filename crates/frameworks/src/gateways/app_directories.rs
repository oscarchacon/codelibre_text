//! Where packages and the session live, per platform (`directories` crate).
//! On Windows: `%APPDATA%\codelibre_text\data\Packages` and `…\data\Local`.

use std::path::{Path, PathBuf};

use use_cases::gateways::{ArchiveFolder, PackagePaths};

pub fn package_paths() -> PackagePaths {
    // A scratch profile for trying things out: nothing of the user's is read or written.
    if let Some(data) = std::env::var_os("CODELIBRE_TEXT_DATA") {
        let packages = PathBuf::from(data).join("Packages");
        let archive_folders = archive_folders(&packages, None).into_iter().filter(|f| !f.sublime).collect();
        return PackagePaths { packages, sublime_packages: None, archive_folders };
    }
    let packages = directories::ProjectDirs::from("", "", "codelibre_text")
        .map(|d| d.data_dir().join("Packages"))
        .unwrap_or_else(|| PathBuf::from("Packages"));
    let sublime_packages = sublime_text_packages();
    let archive_folders = archive_folders(&packages, sublime_packages.as_deref());
    PackagePaths { packages, sublime_packages, archive_folders }
}

/// Where `.sublime-package` files are, least important first: those Sublime Text ships, the ones
/// installed in Sublime Text, and ours. Each kind is unpacked to its folder under `Cache`.
fn archive_folders(packages: &Path, sublime_packages: Option<&Path>) -> Vec<ArchiveFolder> {
    let data = packages.parent().map(Path::to_path_buf).unwrap_or_default();
    let cache = data.join("Cache").join("Packages");
    let shipped = sublime_install_packages().map(|archives| (archives, "shipped", true, true));
    let installed = sublime_packages.and_then(Path::parent).map(|dir| (dir.join("Installed Packages"), "sublime", true, false));
    let own = (data.join("Installed Packages"), "own", false, false);
    shipped
        .into_iter()
        .chain(installed)
        .chain([own])
        .map(|(archives, name, sublime, shipped)| ArchiveFolder { archives, unpacked: cache.join(name), sublime, shipped })
        .collect()
}

/// The `Packages` folder inside Sublime Text's installation, with the packages it ships.
fn sublime_install_packages() -> Option<PathBuf> {
    let candidates = if cfg!(target_os = "windows") {
        ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).map(|p| PathBuf::from(p).join("Sublime Text")).collect()
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Applications/Sublime Text.app/Contents/MacOS")]
    } else {
        vec![PathBuf::from("/opt/sublime_text")]
    };
    candidates.into_iter().map(|dir: PathBuf| dir.join("Packages")).find(|p| p.is_dir())
}

pub fn session_file(paths: &PackagePaths) -> PathBuf {
    paths
        .packages
        .parent()
        .map(|data| data.join("Local").join("Session.sublime_session"))
        .unwrap_or_else(|| PathBuf::from("Session.sublime_session"))
}

/// Where the plugin host's Python modules are written (`…\data\Lib\python`, as in Sublime).
pub fn python_lib_dir(paths: &PackagePaths) -> PathBuf {
    paths
        .packages
        .parent()
        .map(|data| data.join("Lib").join("python"))
        .unwrap_or_else(|| PathBuf::from("Lib").join("python"))
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
