//! Tests for `entities::find_in_files`: the "Where" field, which files a search reads, and Find
//! Results as Sublime Text 4215 writes them (the expected texts are what it wrote).

use std::path::Path;

use entities::find::FindOptions;
use entities::find_in_files::{
    add_where_entry, format_file, is_binary, parse_where, search_bytes, search_text, shorten_line,
    where_snippet_text, PathFilter, ResultLine, Where,
};
use regex::Regex;

fn pattern(query: &str) -> Regex {
    FindOptions {
        query: query.to_string(),
        ..FindOptions::default()
    }
    .pattern()
    .unwrap()
    .unwrap()
}

fn skipped(n: usize) -> String {
    format!("<skipped {n} characters>")
}

/// The text Find Results writes for a file, with no limit on the line length.
fn results(path: &str, text: &str, query: &str) -> String {
    let (lines, _) = search_text(&pattern(query), text, 2);
    let lines: Vec<_> = lines
        .iter()
        .map(|l| (l.row, !l.matches.is_empty(), shorten_line(l, 300, &skipped)))
        .collect();
    format_file(path, &lines)
}

#[test]
fn where_lists_folders_files_and_patterns() {
    let parsed =
        parse_where("/p/src, *.rs,-*.min.js, -node_modules ,<open files>,<project filters>");
    assert_eq!(
        parsed,
        Where {
            paths: vec!["/p/src".into()],
            open_folders: false,
            open_files: true,
            current_file: false,
            include: vec!["*.rs".into()],
            exclude: vec!["*.min.js".into(), "node_modules".into()],
        }
    );
}

#[test]
fn where_without_places_searches_the_open_folders() {
    assert!(parse_where("").open_folders);
    assert!(parse_where("*.rs, -*.md").open_folders);
    assert!(!parse_where("<current file>").open_folders);
    assert!(parse_where("<current file>").current_file);
}

#[test]
fn where_snippets_leave_their_placeholders() {
    assert_eq!(where_snippet_text("*.${0:txt}"), "*.txt");
    assert_eq!(where_snippet_text("-*.${0:txt}"), "-*.txt");
    assert_eq!(where_snippet_text("<open folders>"), "<open folders>");
    assert_eq!(where_snippet_text("a$0b"), "ab");
    assert_eq!(add_where_entry("", "*.txt"), "*.txt");
    assert_eq!(add_where_entry("/p, ", "*.txt"), "/p,*.txt");
    assert_eq!(add_where_entry("/p", "<open files>"), "/p,<open files>");
}

#[test]
fn the_filter_reads_includes_excludes_and_the_settings() {
    let filter = PathFilter {
        include: vec!["*.rs".into(), "docs/*".into()],
        exclude: vec!["*_test.rs".into(), "generated".into()],
        folder_excludes: vec![".git".into()],
        file_excludes: vec!["*.pyc".into()],
        binary: vec!["*.png".into()],
    };
    assert!(filter.accepts_file(Path::new("/p/src/main.rs")));
    assert!(
        filter.accepts_file(Path::new("/p/docs/guide.md")),
        "a path pattern"
    );
    assert!(
        !filter.accepts_file(Path::new("/p/notes.md")),
        "not included"
    );
    assert!(
        !filter.accepts_file(Path::new("/p/src/main_test.rs")),
        "excluded"
    );
    assert!(!filter.accepts_file(Path::new("/p/a.png")));
    assert!(filter.skips_folder(Path::new("/p/.git")));
    assert!(filter.skips_folder(Path::new("/p/src/generated")));
    assert!(!filter.skips_folder(Path::new("/p/src")));
    // No include pattern: everything not excluded.
    let all = PathFilter::default();
    assert!(all.accepts_file(Path::new("/p/notes.md")));
}

#[test]
fn results_show_two_lines_around_each_match_and_a_dot_between_runs() {
    let text = "one\ntwo foo\nthree\nfour\nfive\nsix\nseven\neight foo bar foo\nnine\n";
    // What Sublime 4215 wrote for this file (the empty line 10 is after the last newline).
    let expected = "~/data/a.txt:\n    1  one\n    2: two foo\n    3  three\n    4  four\n    .\n    6  six\n    7  seven\n    8: eight foo bar foo\n    9  nine\n   10  \n\n";
    assert_eq!(results("~/data/a.txt", text, "foo"), expected);
    let (_, count) = search_text(&pattern("foo"), text, 2);
    assert_eq!(count, 3, "two matches on one line count twice");
}

#[test]
fn results_of_a_match_near_the_end_and_at_the_start() {
    let text = "one\ntwo foo\nthree\nfour\nfive\nsix\nseven\neight foo bar foo\nnine\n";
    let expected =
        "~/data/a.txt:\n    7  seven\n    8  eight foo bar foo\n    9: nine\n   10  \n\n";
    assert_eq!(results("~/data/a.txt", text, "nine"), expected);
    assert_eq!(
        results("~/data/sub/b.txt", "foo\n", "foo"),
        "~/data/sub/b.txt:\n    1: foo\n    2  \n\n"
    );
}

#[test]
fn match_lines_are_numbered_with_matches_marked() {
    let (lines, count) = search_text(&pattern("b"), "ab\ncd\n", 0);
    assert_eq!(count, 1);
    let matches: Vec<std::ops::Range<usize>> = std::iter::once(1..2).collect();
    assert_eq!(
        lines,
        vec![ResultLine {
            row: 1,
            text: "ab".into(),
            matches,
        }]
    );
}

#[test]
fn a_match_over_several_lines_marks_each_of_them() {
    let regex = Regex::new(r"(?s)b.c").unwrap();
    let (lines, count) = search_text(&regex, "ab\ncd\nef\n", 0);
    assert_eq!(count, 1);
    let rows: Vec<_> = lines.iter().map(|l| (l.row, l.matches.len())).collect();
    assert_eq!(rows, vec![(1, 1), (2, 1)]);
}

#[test]
fn long_lines_are_shortened_as_sublime_does() {
    let skipped_text = |n: usize| format!("<skipped {n} characters>");
    // A line of context of 700 chars: its first and last 300.
    let context = ResultLine {
        row: 1,
        text: "c".repeat(700),
        matches: vec![],
    };
    assert_eq!(
        shorten_line(&context, 300, &skipped_text),
        format!(
            "{}... <skipped 100 characters> ...{}",
            "c".repeat(300),
            "c".repeat(300)
        )
    );
    // A match line: 300 chars before its first match and after its last.
    let text = format!(
        "{}foo{}foo{}",
        "a".repeat(500),
        "b".repeat(400),
        "d".repeat(900)
    );
    let line = &search_text(&pattern("foo"), &text, 0).0[0];
    assert_eq!(
        shorten_line(line, 300, &skipped_text),
        format!(
            "<skipped 200 characters> ...{}foo{}foo{}... <skipped 600 characters> ...",
            "a".repeat(300),
            "b".repeat(400),
            "d".repeat(300)
        )
    );
    // Chars, not bytes.
    let text = format!("{}foo", "é".repeat(400));
    let line = &search_text(&pattern("foo"), &text, 0).0[0];
    assert_eq!(
        shorten_line(line, 300, &skipped_text),
        format!("<skipped 100 characters> ...{}foo", "é".repeat(300))
    );
    assert_eq!(shorten_line(line, 0, &skipped_text), text, "0: no limit");
}

#[test]
fn files_with_a_nul_byte_are_binary() {
    assert!(is_binary(b"x\xe9\x00bin foo\n"));
    assert!(search_bytes(&pattern("foo"), b"x\x00foo", 2).is_none());
    let (lines, _) = search_bytes(&pattern("foo"), b"a\r\nfoo\r\n", 0).unwrap();
    assert_eq!(lines[0].text, "foo", "CRLF reads as LF");
}

#[test]
fn line_numbers_of_five_digits_keep_a_space_first() {
    let lines = vec![(12345, true, "x".to_string())];
    assert_eq!(format_file("/f", &lines), "/f:\n 12345: x\n\n");
}
