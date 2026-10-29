//! Finds resource files in package folders, in Sublime's load order:
//! `Default` first, other packages alphabetically, `User` last.

use std::path::{Path, PathBuf};

use use_cases::gateways::PackagePaths;

/// Package folders are searched this deep for resources.
const MAX_DEPTH: usize = 4;

pub fn find_resources(paths: &PackagePaths, include_sublime: bool, matches: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut packages: Vec<PathBuf> = roots(paths, include_sublime)
        .iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flat_map(|entries| entries.flatten().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    packages.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let rank = match name.as_str() {
            "Default" => 0,
            "User" => 2,
            _ => 1,
        };
        (rank, name.to_lowercase())
    });

    let mut found = Vec::new();
    for package in packages {
        collect(&package, 0, &matches, &mut found);
    }
    found
}

fn collect(dir: &Path, depth: usize, matches: &impl Fn(&str) -> bool, found: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, depth + 1, matches, found);
        } else if path.file_name().and_then(|n| n.to_str()).is_some_and(matches) {
            found.push(path);
        }
    }
}

/// The package folders' parents: ours last, so their packages win over Sublime's of the same name.
fn roots(paths: &PackagePaths, include_sublime: bool) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = if include_sublime { paths.sublime_packages.iter().cloned().collect() } else { Vec::new() };
    roots.push(paths.packages.clone());
    roots
}

/// The resource name of a package file: `Packages/<package>/<path>` with forward slashes.
pub fn resource_name(paths: &PackagePaths, include_sublime: bool, path: &Path) -> Option<String> {
    let relative = roots(paths, include_sublime).into_iter().find_map(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))?;
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
    roots(paths, include_sublime).into_iter().rev().map(|root| root.join(relative)).find(|p| p.is_file())
}

/// Files of Sublime's own `Default` package target its native core; our built-ins replace them.
pub fn is_in_default_package(path: &Path) -> bool {
    path.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "Default")
}
