//! Tests for `gateways::folder_search` on the real disk (a temporary folder): the files it lists,
//! the order of its results while several threads search, open files' text, and cancelling.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use codelibre_text::gateways::folder_search::FolderSearch;
use entities::find::FindOptions;
use entities::find_in_files::PathFilter;
use use_cases::gateways::{FileSearchEvent, FileSearchGateway, FileSearchRequest};

fn request(roots: Vec<PathBuf>, query: &str) -> FileSearchRequest {
    FileSearchRequest {
        roots,
        filter: PathFilter {
            folder_excludes: vec![".git".into()],
            binary: vec!["*.png".into()],
            ..PathFilter::default()
        },
        options: FindOptions {
            query: query.into(),
            ..FindOptions::default()
        },
        context_lines: 0,
        max_file_size: 0,
        open_texts: Vec::new(),
    }
}

/// Every event until `Done` (or 10 s).
fn events(search: &mut FolderSearch) -> Vec<FileSearchEvent> {
    let start = Instant::now();
    let mut all = Vec::new();
    while start.elapsed() < Duration::from_secs(10) {
        all.extend(search.poll());
        if all.last() == Some(&FileSearchEvent::Done) {
            return all;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the search did not end: {all:?}");
}

fn found(events: &[FileSearchEvent]) -> Vec<PathBuf> {
    events
        .iter()
        .filter_map(|e| match e {
            FileSearchEvent::Found(hits) => Some(hits.path.clone()),
            _ => None,
        })
        .collect()
}

fn put(dir: &Path, name: &str, text: &[u8]) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn it_lists_the_files_and_sends_matches_in_path_order() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..40 {
        put(dir.path(), &format!("f{i:02}.txt"), b"x foo\n");
    }
    put(dir.path(), "sub/a.txt", b"foo\n");
    put(dir.path(), ".git/HEAD", b"foo\n");
    put(dir.path(), "image.png", b"foo\n");
    put(dir.path(), "binary.dat", b"foo\x00\n");
    put(dir.path(), "none.txt", b"bar\n");
    let mut search = FolderSearch::default();
    search.start(request(vec![dir.path().to_path_buf()], "foo"));
    let events = events(&mut search);
    // 40 + sub/a.txt + binary.dat + none.txt: not .git, not the png.
    assert_eq!(events[0], FileSearchEvent::Listed(43));
    let paths = found(&events);
    assert_eq!(paths.len(), 41, "the binary file has no results");
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "in path order");
    assert!(paths.contains(&dir.path().join("sub").join("a.txt")));
}

#[test]
fn open_files_are_searched_as_the_editor_has_them() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "a.txt", b"nothing\n");
    let file = dir.path().join("a.txt");
    let mut search = FolderSearch::default();
    let mut asked = request(vec![file.clone()], "foo");
    asked.open_texts = vec![(file.clone(), "unsaved foo\n".into())];
    search.start(asked);
    let events = events(&mut search);
    assert_eq!(found(&events), vec![file]);
}

#[test]
fn a_cancelled_search_sends_nothing_more() {
    let dir = tempfile::tempdir().unwrap();
    put(dir.path(), "a.txt", b"foo\n");
    let mut search = FolderSearch::default();
    search.start(request(vec![dir.path().to_path_buf()], "foo"));
    search.cancel();
    std::thread::sleep(Duration::from_millis(100));
    assert!(search.poll().is_empty());
}
