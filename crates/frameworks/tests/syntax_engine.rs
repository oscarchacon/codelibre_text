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
use use_cases::gateways::{ArchiveFolder, PackagePaths, SyntaxGateway};

fn engine_in(dir: &TempDir) -> SyntaxEngine {
    SyntaxEngine::new(PackagePaths { packages: dir.path().join("Packages"), sublime_packages: None, archive_folders: Vec::new() })
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

// ----- scopes --------------------------------------------------------------------------------------

#[test]
fn a_package_syntax_names_its_contexts_in_backtraces() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "Toy/Toy.sublime-syntax",
        "%YAML 1.2\n---\nname: Toy\nfile_extensions: [toy]\nscope: source.toy\ncontexts:\n  main:\n    - match: '\"'\n      push: string\n  string:\n    - meta_scope: string.quoted.toy\n    - match: '\"'\n      pop: true\n",
    );
    let mut engine = engine_in(&dir);
    engine.reload(false, &mut Vec::new());
    let text = "a \"b c\" d\n";
    let names = |column: usize, engine: &mut SyntaxEngine| -> Vec<String> {
        engine.context_backtrace(1, text, "Toy", 0, column).into_iter().map(|f| format!("{} {}", f.source_file, f.context_name)).collect()
    };
    assert_eq!(names(1, &mut engine), ["Packages/Toy/Toy.sublime-syntax main"]);
    assert_eq!(names(4, &mut engine), ["Packages/Toy/Toy.sublime-syntax main", "Packages/Toy/Toy.sublime-syntax string"]);
    let infos = engine.syntax_infos();
    let toy = infos.iter().find(|s| s.name == "Toy").unwrap();
    assert_eq!((toy.path.as_str(), toy.scope.as_str()), ("Packages/Toy/Toy.sublime-syntax", "source.toy"));
    // Every line's scopes at once, from the cache.
    let all = engine.document_scopes(1, text, "Toy");
    assert_eq!(all.len(), 1);
    assert!(all[0].iter().any(|s| s.scope == "source.toy string.quoted.toy"), "{all:?}");
}

/// A scope of more than 8 parts, as ActionScript's `<<=` (syntect took 8 at most).
const LONG_SCOPE: &str = "keyword.operator.assignment.augmented.bitwise.shift.left.signed.toy";

#[test]
fn a_syntax_with_scopes_of_more_than_eight_parts_loads_and_highlights() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir,
        "Toy/Toy.sublime-syntax",
        &format!("%YAML 1.2\n---\nname: Toy\nfile_extensions: [toy]\nscope: source.toy\ncontexts:\n  main:\n    - match: '<<='\n      scope: {LONG_SCOPE}\n"),
    );
    let mut engine = engine_in(&dir);
    let mut errors = Vec::new();
    engine.reload(false, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let spans = engine.line_scopes(1, "a <<= 1\n", "Toy", 0);
    assert!(spans.iter().any(|s| s.range == (2..5) && s.scope == format!("source.toy {LONG_SCOPE}")), "{spans:?}");
}

#[test]
fn line_scopes_cover_the_line_with_the_parser_scopes() {
    let (_dir, mut engine) = engine();
    let text = "x = 1\ns = \"hi\"\n";
    let spans = engine.line_scopes(1, text, "Python", 1);
    assert_eq!(spans.first().unwrap().range.start, 0);
    assert_eq!(spans.last().unwrap().range.end, "s = \"hi\"\n".len());
    assert!(spans.windows(2).all(|w| w[0].range.end == w[1].range.start), "{spans:?}");
    let at = |byte: usize| spans.iter().find(|s| s.range.contains(&byte)).unwrap().scope.clone();
    assert!(at(0).starts_with("source.python"), "{}", at(0));
    assert!(at(6).contains("string.quoted.double"), "{}", at(6));
    assert!(engine.line_scopes(1, text, "Python", 9).is_empty(), "past the end");
}

#[test]
fn line_scopes_of_plain_text_are_its_base_scope() {
    let (_dir, mut engine) = engine();
    let spans = engine.line_scopes(1, "a \"b\"", "Plain Text", 0);
    assert_eq!(spans.len(), 1);
    assert_eq!((spans[0].range.clone(), spans[0].scope.as_str()), (0..5, "text.plain"));
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

const BASE_SYNTAX: &str = "%YAML 1.2\n---\nname: Base\nfile_extensions: [base]\nscope: source.base\ncontexts:\n  main:\n    - match: '\"'\n      push: string\n  string:\n    - meta_scope: string.quoted.base\n    - match: '\"'\n      pop: true\n";

const CHILD_SYNTAX: &str = "%YAML 1.2\n---\nname: Child\nfile_extensions: [child]\nscope: source.child\nextends: Packages/Base/Base.sublime-syntax\ncontexts:\n  main:\n    - meta_prepend: true\n    - match: '(?=foo)'\n      branch_point: word\n      branch: [first, second]\n  first:\n    - match: 'foo'\n      scope: keyword.first\n      pop: true\n  second:\n    - match: 'foo'\n      scope: keyword.second\n      pop: true\n";

#[test]
fn extends_takes_the_parents_contexts_and_branches_take_their_first_alternative() {
    let dir = tempfile::tempdir().unwrap();
    let shipped = dir.path().join("Cache/shipped");
    fs::create_dir_all(shipped.join("Base")).unwrap();
    fs::write(shipped.join("Base/Base.sublime-syntax"), BASE_SYNTAX).unwrap();
    write(&dir, "Child/Child.sublime-syntax", CHILD_SYNTAX);
    let paths = PackagePaths {
        packages: dir.path().join("Packages"),
        sublime_packages: None,
        archive_folders: vec![ArchiveFolder { archives: dir.path().join("Installed"), unpacked: shipped, sublime: true, shipped: true }],
    };
    let mut engine = SyntaxEngine::new(paths);
    let mut errors = Vec::new();
    engine.reload(true, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let all = engine.document_scopes(1, "\"x\" foo", "Child");
    assert!(all[0].iter().any(|s| s.scope == "source.child string.quoted.base"), "{all:?}");
    assert!(all[0].iter().any(|s| s.scope == "source.child keyword.first"), "{all:?}");
    assert!(!engine.syntax_infos().iter().any(|s| s.name == "Base"), "the shipped parent is not a syntax of its own");
}

const FENCE_SYNTAX: &str = "%YAML 1.2\n---\nname: Fence\nfile_extensions: [fence]\nscope: source.fence\ncontexts:\n  main:\n    - match: '^(-)()(x)?'\n      scope: keyword.fence\n      push: body\n      escape: '^\\1(?:\\2)*(?:\\3)*$'\n  body:\n    - meta_scope: string.fence\n";

#[test]
fn a_backreference_to_an_empty_capture_can_be_repeated() {
    let (dir, mut engine) = engine();
    write(&dir, "Fence/Fence.sublime-syntax", FENCE_SYNTAX);
    let mut errors = Vec::new();
    engine.reload(false, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    // `\2` is empty and `\3` (`(x)?`) did not take part, so the escape `^-(?:\2)*(?:\3)*$` must compile. It closes the block, and the same `-`
    // then opens a new one (keyword), which can only happen if the escape matched.
    let all = engine.document_scopes(1, "-\nabc\n-\nafter\n", "Fence");
    assert!(all[1].iter().any(|s| s.scope == "source.fence string.fence"), "{all:?}");
    assert!(all[2].iter().any(|s| s.scope == "source.fence string.fence keyword.fence"), "the escape closed the block: {all:?}");
}

const FENCE_LANGUAGE_SYNTAX: &str = r#"%YAML 1.2
---
name: Fence Language
file_extensions: [fl]
scope: text.fence-language
contexts:
  main:
    - match: '^(`{3})()$'
      scope: markup.raw.fence-language
      embed: scope:source.python
      embed_scope: meta.code.fence-language
      escape: '^\1(?:\2)*$'
"#;

#[test]
fn a_code_fence_embeds_its_language_even_with_an_empty_capture_in_its_closing_pattern() {
    let (dir, mut engine) = engine();
    write(&dir, "Fence/Fence Language.sublime-syntax", FENCE_LANGUAGE_SYNTAX);
    let mut errors = Vec::new();
    engine.reload(false, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let all = engine.document_scopes(1, "```\nx = 1\n```\nafter\n", "Fence Language");
    assert!(all[1].iter().any(|s| s.scope.contains("source.python") && s.scope.contains("keyword.operator.assignment.python")), "{all:?}");
    // The closing fence is the escape: it leaves the embedded Python, and the text after it is plain.
    assert!(all[2].iter().all(|s| !s.scope.contains("source.python")), "the closing fence leaves Python: {all:?}");
    assert!(all[3].iter().all(|s| !s.scope.contains("source.python")), "back to the text after the fence: {all:?}");
}

#[test]
fn a_panic_while_highlighting_is_an_error_not_a_crash() {
    use codelibre_text::syntax_engine::guard;
    assert_eq!(guard(|| 1), Ok(1));
    assert_eq!(guard(|| -> u8 { panic!("regex string should be pre-tested") }), Err("regex string should be pre-tested".to_string()));
    assert_eq!(guard(|| -> u8 { panic!("{} {}", "formatted", 2) }), Err("formatted 2".to_string()));
}

#[test]
fn extends_without_a_folder_is_read_next_to_the_child() {
    let (dir, mut engine) = engine();
    write(&dir, "Toy/Toy Base.sublime-syntax", BASE_SYNTAX);
    let child = CHILD_SYNTAX.replace("extends: Packages/Base/Base.sublime-syntax", "extends: Toy Base.sublime-syntax");
    write(&dir, "Toy/Toy.sublime-syntax", &child.replace("name: Child", "name: Toy").replace("source.child", "source.toy"));
    let mut errors = Vec::new();
    engine.reload(false, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let all = engine.document_scopes(1, "\"x\" foo", "Toy");
    assert!(all[0].iter().any(|s| s.scope == "source.toy string.quoted.base"), "{all:?}");
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

#[test]
fn a_color_scheme_with_a_builtin_name_patches_it() {
    let (dir, mut engine) = engine();
    // MarkdownEditing, LSP and a user's own customizations ship a partial scheme under the name
    // of the one it patches: its globals override and its rules are appended, the base stays.
    write(
        &dir,
        "User/Monokai.sublime-color-scheme",
        r##"{ "globals": { "background": "#010203" }, "rules": [ { "scope": "comment", "foreground": "#00AA00" } ] }"##,
    );
    engine.reload(false, &mut Vec::new());
    let scheme = engine.scheme("Monokai");
    assert_eq!(scheme.settings.background.map(to_color32), Some(egui::Color32::from_rgb(1, 2, 3)), "the patch's global");
    assert_eq!(
        scheme.settings.foreground.map(to_color32),
        Some(egui::Color32::from_rgb(0xf8, 0xf8, 0xf2)),
        "the built-in's other globals stay"
    );
    assert_eq!(engine.scope_style("Monokai", "keyword.control.python")["foreground"], "#f92672", "its rules stay");
    assert_eq!(engine.scope_style("Monokai", "comment.line.python")["foreground"], "#00aa00", "the patch's rule wins");
}

#[test]
fn the_palette_has_the_colors_terminal_plugins_ask_for() {
    let (_dir, engine) = engine();
    let palette = engine.scheme_palette("Monokai");
    assert_eq!(palette["background"], "#272822");
    assert_eq!(palette["foreground"], "#f8f8f2");
    for (name, color) in [
        ("redish", "#f92672"),
        ("orangish", "#fd971f"),
        ("yellowish", "#e6db74"),
        ("greenish", "#a6e22e"),
        ("cyanish", "#66d9ef"),
        ("bluish", "#66d9ef"),
        ("purplish", "#ae81ff"),
        ("pinkish", "#f92672"),
    ] {
        assert_eq!(palette[name], color, "{name}");
    }
}

#[test]
fn styles_for_plugins_come_from_the_scheme_rules() {
    let (dir, mut engine) = engine();
    let scheme = r##"{
        "name": "Mine",
        "globals": { "foreground": "#F0F0F0", "background": "#010203", "selection": "#11223380" },
        "rules": [
            { "scope": "string", "foreground": "#AA0000", "font_style": "bold italic" },
            { "scope": "string.quoted.double", "background": "#002200" }
        ]
    }"##;
    write(&dir, "User/Mine.sublime-color-scheme", scheme);
    engine.reload(false, &mut Vec::new());
    let style = engine.scheme_style("Mine");
    assert_eq!(style["foreground"], "#f0f0f0");
    assert_eq!(style["background"], "#010203");
    assert_eq!(style["selection"], "#11223380", "not opaque: with its alpha");
    assert_eq!(style["caret"], "#f0f0f0", "no caret: the foreground");

    let string = engine.scope_style("Mine", "source.python string.quoted.double.python");
    assert_eq!(serde_json::Value::Object(string), serde_json::json!({"foreground": "#aa0000", "background": "#002200", "bold": true, "italic": true}));
    let plain = engine.scope_style("Mine", "source.python");
    assert_eq!(serde_json::Value::Object(plain), serde_json::json!({"foreground": "#f0f0f0"}));
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
