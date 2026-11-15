//! Tests for the recent files and folders (`entities::recent`).

use std::path::{Path, PathBuf};

use entities::recent::{RecentPaths, RECENT_LIMIT};

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

#[test]
fn a_path_opened_again_moves_first_and_the_list_has_a_limit() {
    let mut recent = RecentPaths::default();
    recent.push("/a".into());
    recent.push("/b".into());
    recent.push("/a".into());
    assert_eq!(recent.paths(), paths(&["/a", "/b"]));
    for i in 0..RECENT_LIMIT + 5 {
        recent.push(format!("/f{i}").into());
    }
    assert_eq!(recent.paths().len(), RECENT_LIMIT);
    assert_eq!(
        recent.get(0),
        Some(Path::new(&format!("/f{}", RECENT_LIMIT + 4)))
    );
    // Saved lists come back in their order, without duplicates.
    let saved = RecentPaths::from_paths(paths(&["/x", "/y", "/x"]));
    assert_eq!(saved.paths(), paths(&["/x", "/y"]));
}
