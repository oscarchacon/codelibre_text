//! `.sublime-package` files are zips of a package. They are unpacked to a cache folder (see
//! `ArchiveFolder`), which from then on is a folder of packages like any other: every reader of
//! resources, and the plugin host, works on plain files.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use use_cases::gateways::{ArchiveFolder, PackagePaths};
use zip::ZipArchive;

const EXTENSION: &str = ".sublime-package";
/// Says which archive a package was unpacked from; written last, so a partial one is redone.
const STAMP_FILE: &str = ".codelibre-unpacked";
/// Not imported from Sublime Text: `Default` is written for its native core, and Package Control
/// runs at load, using the network. Copied to our own `Installed Packages`, it does load.
const NOT_IMPORTED: &[&str] = &["Default", "Package Control"];

/// Unpacks the archives that changed and removes the packages whose archive is gone. Answers
/// whether it changed anything: it may run in a thread while the packages are in use (the first
/// start unpacks everything Sublime Text ships), after which they are loaded again.
pub fn unpack_all(paths: &PackagePaths, include_sublime: bool, errors: &mut Vec<String>) -> bool {
    let mut changed = false;
    for folder in paths.archive_folders.iter().filter(|f| include_sublime || !f.sublime) {
        changed |= unpack_folder(folder, errors);
    }
    changed
}

/// Where the packages of `unpacked` are written before they take their place (next to it, so
/// the rename stays in one file system): nobody reads a package that is half unpacked.
fn staging_folder(unpacked: &Path) -> PathBuf {
    let mut name = unpacked.file_name().unwrap_or_default().to_os_string();
    name.push(".staging");
    unpacked.with_file_name(name)
}

fn unpack_folder(folder: &ArchiveFolder, errors: &mut Vec<String>) -> bool {
    let staging = staging_folder(&folder.unpacked);
    // What an unpacking that was cut short left.
    let _ = fs::remove_dir_all(&staging);
    let mut changed = false;
    let mut names = Vec::new();
    for entry in fs::read_dir(&folder.archives).into_iter().flatten().flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(name) = file_name.strip_suffix(EXTENSION).map(String::from) else {
            continue;
        };
        if folder.sublime && NOT_IMPORTED.contains(&name.as_str()) {
            continue;
        }
        match unpack_if_changed(&entry.path(), &folder.unpacked.join(&name), &staging.join(&name), folder.shipped) {
            Ok(unpacked) => changed |= unpacked,
            Err(e) => errors.push(format!("{}: {e}", entry.path().display())),
        }
        names.push(name);
    }
    let _ = fs::remove_dir_all(&staging);
    // Packages that are gone (removed by the user or by Package Control).
    for entry in fs::read_dir(&folder.unpacked).into_iter().flatten().flatten() {
        if !names.iter().any(|n| entry.file_name().to_string_lossy() == *n) {
            let _ = fs::remove_dir_all(entry.path());
            changed = true;
        }
    }
    changed
}

/// Unpacks `archive` to `staging` and moves it to `target`, unless `target` is that archive
/// already. Answers whether it unpacked it.
fn unpack_if_changed(archive: &Path, target: &Path, staging: &Path, shipped: bool) -> Result<bool, String> {
    let stamp = stamp_of(archive).map_err(|e| e.to_string())?;
    if fs::read_to_string(target.join(STAMP_FILE)).is_ok_and(|old| old == stamp) {
        return Ok(false);
    }
    unpack(archive, staging, shipped).map_err(|e| e.to_string())?;
    fs::write(staging.join(STAMP_FILE), stamp).map_err(|e| e.to_string())?;
    if target.exists() {
        fs::remove_dir_all(target).map_err(|e| e.to_string())?;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    move_folder(staging, target).map_err(|e| e.to_string())?;
    Ok(true)
}

/// A rename, tried again for a moment: on Windows a virus scanner may still hold the files
/// that were just written.
fn move_folder(from: &Path, to: &Path) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match fs::rename(from, to) {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            result => return result,
        }
    }
}

/// Size and date: Package Control replaces a package's file when it upgrades it.
fn stamp_of(archive: &Path) -> io::Result<String> {
    let metadata = fs::metadata(archive)?;
    let modified = metadata.modified()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    Ok(format!("{} {modified}", metadata.len()))
}

/// What is of no use from a package Sublime Text ships (see `ArchiveFolder::shipped`). Its
/// syntaxes stay: packages `extends` them, and the engine reads them without loading them.
fn left_out_of_shipped(path: &Path) -> bool {
    path.starts_with("tests")
}

fn unpack(archive: &Path, target: &Path, shipped: bool) -> zip::result::ZipResult<()> {
    let mut zip = ZipArchive::new(File::open(archive)?)?;
    if target.exists() {
        fs::remove_dir_all(target)?;
    }
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
