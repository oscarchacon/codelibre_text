//! Finds resource files in package folders, in Sublime's load order: `Default` first, other
//! packages alphabetically, `User` last. A package can be in several places; for each of its files
//! the last place wins (see `PackagePaths::roots`), so an unpacked file overrides a zipped one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use use_cases::gateways::PackagePaths;

use super::sublime_json;

/// Package folders are searched this deep for resources.
const MAX_DEPTH: usize = 4;
/// `ignored_packages` when the user has not set it: Vintage changes every key.
const DEFAULT_IGNORED: &[&str] = &["Vintage"];

pub fn find_resources(paths: &PackagePaths, include_sublime: bool, matches: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let ignored = ignored_packages(paths, include_sublime);
    let mut packages: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for package in paths.roots(include_sublime).iter().filter_map(|root| std::fs::read_dir(root).ok()).flat_map(|e| e.flatten()) {
        let path = package.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if !path.is_dir() || ignored.contains(&name) {
            continue;
        }
        match packages.iter_mut().find(|(known, _)| *known == name) {
            Some((_, places)) => places.push(path),
            None => packages.push((name, vec![path])),
        }
    }
    packages.sort_by_key(|(name, _)| {
        let rank = match name.as_str() {
            "Default" => 0,
            "User" => 2,
            _ => 1,
        };
        (rank, name.to_lowercase())
    });

    let mut found = Vec::new();
    for (_, places) in packages {
        let mut files = BTreeMap::new();
        for place in &places {
            collect(place, place, 0, &matches, &mut files);
        }
        found.extend(files.into_values());
    }
    found
}

/// `ignored_packages` of the user's preferences (ours and, when they are read, Sublime's).
pub fn ignored_packages(paths: &PackagePaths, include_sublime: bool) -> Vec<String> {
    let sublime = paths.sublime_packages.iter().filter(|_| include_sublime).map(|p| p.join("User"));
    let mut ignored = Vec::new();
    let mut any_set = false;
    for folder in std::iter::once(paths.user_dir()).chain(sublime) {
        let text = std::fs::read_to_string(folder.join("Preferences.sublime-settings")).unwrap_or_default();
        let Some(Value::Array(names)) = sublime_json::parse(&text).ok().and_then(|p| p.get("ignored_packages").cloned()) else {
            continue;
        };
        any_set = true;
        ignored.extend(names.iter().filter_map(Value::as_str).map(String::from));
    }
    if !any_set {
        ignored.extend(DEFAULT_IGNORED.iter().map(|n| n.to_string()));
    }
    ignored
}

/// The matching files under `dir`, by their path in the package.
fn collect(package: &Path, dir: &Path, depth: usize, matches: &impl Fn(&str) -> bool, found: &mut BTreeMap<PathBuf, PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            collect(package, &path, depth + 1, matches, found);
        } else if path.file_name().and_then(|n| n.to_str()).is_some_and(matches) {
            let relative = path.strip_prefix(package).unwrap_or(&path).to_path_buf();
            found.insert(relative, path);
        }
    }
}

/// The resource name of a package file: `Packages/<package>/<path>` with forward slashes.
pub fn resource_name(paths: &PackagePaths, include_sublime: bool, path: &Path) -> Option<String> {
    let relative = paths.roots(include_sublime).into_iter().find_map(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))?;
    let parts: Vec<String> = relative.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    Some(format!("Packages/{}", parts.join("/")))
}

/// The file behind a resource name, searching our packages before Sublime's. `None` for names
/// outside `Packages/` or that climb out of it.
pub fn resource_path(paths: &PackagePaths, include_sublime: bool, name: &str) -> Option<PathBuf> {
    let relative = name.strip_prefix("Packages/")?;
    if relative.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return None;
    }
    paths.roots(include_sublime).into_iter().rev().map(|root| root.join(relative)).find(|p| p.is_file())
}

/// Files of Sublime's own `Default` package target its native core; our built-ins replace them.
pub fn is_in_default_package(path: &Path) -> bool {
    path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "Default")
}
