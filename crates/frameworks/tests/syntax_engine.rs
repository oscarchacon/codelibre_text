//! Tests for `syntax_engine` (syntect: syntaxes, color schemes, highlighting) and the
//! `SyntectSyntaxes` gateway built on it.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use codelibre_text::gateways::syntect_syntaxes::SyntectSyntaxes;
use codelibre_text::syntax_engine::{to_color32, SyntaxEngine};
use egui::FontId;
use entities::symbol::Symbol;
use tempfile::TempDir;
use use_cases::gateways::{PackagePaths, SyntaxGateway};

fn engine_in(dir: &TempDir) -> SyntaxEngine {
    SyntaxEngine::new(PackagePaths { packages: dir.path().join("Packages"), sublime_packages: None })
}

fn engine() -> (TempDir, SyntaxEngine) {
    let dir = tempfile::tempdir().unwrap();
    let engine = engine_in(&dir);
    (dir, engine)
}

fn write(dir: &TempDir, relative: &str, contents: &str) {
    let path = dir.path().join("Packages").join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn font() -> FontId {
    FontId::monospace(14.0)
}

// ----- syntaxes ------------------------------------------------------------------------------------

#[test]
fn syntaxes_are_detected_by_file_name_and_first_line() {
    let (_dir, engine) = engine();
    assert_eq!(engine.detect(Path::new("main.rs"), ""), "Rust");
    assert_eq!(engine.detect(Path::new("app.py"), ""), "Python");
    assert_eq!(engine.detect(Path::new("index.html"), ""), "HTML");
    assert_eq!(engine.detect(Path::new("script"), "#!/usr/bin/env python"), "Python");
    assert_eq!(engine.detect(Path::new("notes.zzz"), "hello"), "Plain Text");
}

#[test]
fn syntax_names_and_paths_are_resolved() {
    let (_dir, engine) = engine();
    assert_eq!(engine.resolve("Rust"), "Rust");
    assert_eq!(engine.resolve("Packages/Python/Python.sublime-syntax"), "Python");
    assert_eq!(engine.resolve("Cobol"), "Plain Text");
}

#[test]
fn the_syntax_list_has_the_builtin_syntaxes() {
    let (_dir, engine) = engine();
    let names = engine.syntax_names();
    for expected in ["Plain Text", "Rust", "Python", "JSON", "Markdown"] {
        assert!(names.iter().any(|n| n == expected), "missing {expected}");
    }
}

const TOY_SYNTAX: &str = "%YAML 1.2\n---\nname: Toy\nfile_extensions: [toy]\nscope: source.toy\ncontexts:\n  main:\n    - match: '\\bdef\\b'\n      scope: keyword.toy\n";

#[test]
fn package_syntaxes_are_loaded_on_reload() {
    let (dir, mut engine) = engine();
    write(&dir, "Toy/Toy.sublime-syntax", TOY_SYNTAX);
    assert_eq!(engine.detect(Path::new("a.toy"), ""), "Plain Text");
    let mut errors = Vec::new();
    engine.reload(false, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(engine.detect(Path::new("a.toy"), ""), "Toy");
}

#[test]
fn broken_syntaxes_are_reported() {
    let (dir, mut engine) = engine();
    write(&dir, "Bad/Bad.sublime-syntax", "name: [unclosed");
    let mut errors = Vec::new();
    engine.reload(false, &mut errors);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Bad.sublime-syntax"));
    assert_eq!(engine.resolve("Rust"), "Rust", "the built-in syntaxes are still there");
}

// ----- color schemes -------------------------------------------------------------------------------

#[test]
fn builtin_color_schemes_are_available() {
    let (_dir, engine) = engine();
    let names = engine.scheme_names();
    for expected in ["Monokai", "Daylight", "InspiredGitHub", "Solarized (dark)"] {
        assert!(names.iter().any(|n| n == expected), "missing {expected}");
    }
    let background = engine.scheme("Monokai").settings.background.map(to_color32).unwrap();
    assert_eq!(background, egui::Color32::from_rgb(0x27, 0x28, 0x22));
}

#[test]
fn unknown_color_schemes_fall_back_to_monokai() {
    let (_dir, engine) = engine();
    assert_eq!(engine.scheme("Nope").name.as_deref(), Some("Monokai"));
}

#[test]
fn package_color_schemes_are_loaded_in_both_formats() {
    let (dir, mut engine) = engine();
    write(&dir, "User/Mine.sublime-color-scheme", r##"{ "name": "Mine", "globals": { "background": "#010203" } }"##);
    write(&dir, "User/Broken.sublime-color-scheme", "{ nope");
    let mut errors = Vec::new();
    engine.reload(false, &mut errors);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Broken"));
    let mine = engine.scheme("Mine");
    assert_eq!(mine.settings.background.map(to_color32), Some(egui::Color32::from_rgb(1, 2, 3)));
}

// ----- highlighting --------------------------------------------------------------------------------

#[test]
fn layout_jobs_cover_the_whole_text_with_several_colors() {
    let (_dir, mut engine) = engine();
    let text = "fn main() {\n    let x = \"hi\"; // note\n}\n";
    let job = engine.layout_job(1, text, "Rust", "Monokai", font());
    assert_eq!(job.text, text);
    let mut end = 0;
    for section in &job.sections {
        assert_eq!(section.byte_range.start, end, "sections are contiguous");
        end = section.byte_range.end;
    }
    assert_eq!(end, text.len());
    let colors: std::collections::HashSet<_> = job.sections.iter().map(|s| s.format.color).collect();
    assert!(colors.len() >= 3, "keywords, strings and comments are colored");
}

#[test]
fn plain_text_is_one_uncolored_section() {
    let (_dir, mut engine) = engine();
    let job = engine.layout_job(1, "just text", "Plain Text", "Monokai", font());
    assert_eq!(job.sections.len(), 1);
    assert_eq!(job.sections[0].byte_range, 0..9);
}

#[test]
fn incremental_highlighting_matches_a_fresh_highlight() {
    let (dir, mut engine) = engine();
    let before = "fn a() {}\nfn b() {}\nfn c() {}\n";
    let after = "fn a() {}\n/* opened\nfn b() {}\nfn c() {}\n";
    engine.layout_job(7, before, "Rust", "Monokai", font());
    let incremental = engine.layout_job(7, after, "Rust", "Monokai", font());

    let mut fresh_engine = engine_in(&dir);
    let fresh = fresh_engine.layout_job(7, after, "Rust", "Monokai", font());
    assert_eq!(incremental.sections, fresh.sections);
}

#[test]
fn changing_the_color_scheme_recolors() {
    let (_dir, mut engine) = engine();
    let text = "fn main() {}";
    let dark = engine.layout_job(1, text, "Rust", "Monokai", font());
    let light = engine.layout_job(1, text, "Rust", "Daylight", font());
    assert_ne!(dark.sections, light.sections);
}

#[test]
fn symbols_come_from_entity_names() {
    let (_dir, mut engine) = engine();
    let text = "fn main() {}\nstruct Point { x: i32 }\n";
    let symbols = engine.symbols(3, text, "Rust");
    assert_eq!(
        symbols,
        vec![Symbol { line: 0, name: "main".into() }, Symbol { line: 1, name: "Point".into() }]
    );
    engine.forget(3);
}

// ----- the SyntaxGateway implementation ------------------------------------------------------------

#[test]
fn the_gateway_delegates_to_the_shared_engine() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir, "Toy/Toy.sublime-syntax", TOY_SYNTAX);
    let shared = Rc::new(RefCell::new(engine_in(&dir)));
    let mut gateway = SyntectSyntaxes(shared.clone());

    let mut errors = Vec::new();
    gateway.reload(true, &mut errors);
    assert_eq!(gateway.detect(&PathBuf::from("x.toy"), ""), "Toy");
    assert_eq!(gateway.resolve("Toy"), "Toy");
    assert!(gateway.syntax_names().contains(&"Toy".to_string()));
    assert!(gateway.color_scheme_names().contains(&"Monokai".to_string()));
    assert_eq!(gateway.symbols(1, "fn go() {}", "Rust")[0].name, "go");
    gateway.forget(1);
    assert_eq!(shared.borrow().detect(Path::new("x.toy"), ""), "Toy", "same engine as the view");
}
