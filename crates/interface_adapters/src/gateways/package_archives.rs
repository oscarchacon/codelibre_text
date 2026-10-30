//! `.sublime-package` files are zips of a package. They are unpacked to a cache folder (see
//! `ArchiveFolder`), which from then on is a folder of packages like any other: every reader of
//! resources, and the plugin host, works on plain files.

use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use use_cases::gateways::{ArchiveFolder, PackagePaths};
use zip::ZipArchive;

const EXTENSION: &str = ".sublime-package";
/// Says which archive a package was unpacked from; written last, so a partial one is redone.
const STAMP_FILE: &str = ".codelibre-unpacked";
/// Not imported from Sublime Text: `Default` is written for its native core, and Package Control
/// runs at load, using the network. Copied to our own `Installed Packages`, it does load.
const NOT_IMPORTED: &[&str] = &["Default", "Package Control"];

pub fn unpack_all(paths: &PackagePaths, include_sublime: bool, errors: &mut Vec<String>) {
    for folder in paths.archive_folders.iter().filter(|f| include_sublime || !f.sublime) {
        unpack_folder(folder, errors);
    }
}

fn unpack_folder(folder: &ArchiveFolder, errors: &mut Vec<String>) {
    let mut names = Vec::new();
    for entry in fs::read_dir(&folder.archives).into_iter().flatten().flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(name) = file_name.strip_suffix(EXTENSION).map(String::from) else {
            continue;
        };
        if folder.sublime && NOT_IMPORTED.contains(&name.as_str()) {
            continue;
        }
        if let Err(e) = unpack_if_changed(&entry.path(), &folder.unpacked.join(&name), folder.shipped) {
            errors.push(format!("{}: {e}", entry.path().display()));
        }
        names.push(name);
    }
    // Packages that are gone (removed by the user or by Package Control).
    for entry in fs::read_dir(&folder.unpacked).into_iter().flatten().flatten() {
        if !names.iter().any(|n| entry.file_name().to_string_lossy() == *n) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn unpack_if_changed(archive: &Path, target: &Path, shipped: bool) -> Result<(), String> {
    let stamp = stamp_of(archive).map_err(|e| e.to_string())?;
    let stamp_file = target.join(STAMP_FILE);
    if fs::read_to_string(&stamp_file).is_ok_and(|old| old == stamp) {
        return Ok(());
    }
    if target.exists() {
        fs::remove_dir_all(target).map_err(|e| e.to_string())?;
    }
    unpack(archive, target, shipped).map_err(|e| e.to_string())?;
    fs::write(&stamp_file, stamp).map_err(|e| e.to_string())
}

/// Size and date: Package Control replaces a package's file when it upgrades it.
fn stamp_of(archive: &Path) -> io::Result<String> {
    let metadata = fs::metadata(archive)?;
    let modified = metadata.modified()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    Ok(format!("{} {modified}", metadata.len()))
}

/// What is of no use from a package Sublime Text ships (see `ArchiveFolder::shipped`).
fn left_out_of_shipped(path: &Path) -> bool {
    path.starts_with("tests") || path.extension().is_some_and(|e| e == "sublime-syntax")
}

fn unpack(archive: &Path, target: &Path, shipped: bool) -> zip::result::ZipResult<()> {
    let mut zip = ZipArchive::new(File::open(archive)?)?;
    fs::create_dir_all(target)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        // A name that climbs out of the package (`../x`) is not written anywhere.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if shipped && left_out_of_shipped(&relative) {
            continue;
        }
        let path = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut File::create(&path)?)?;
    }
    Ok(())
}
