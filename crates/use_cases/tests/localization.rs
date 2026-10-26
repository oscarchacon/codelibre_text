//! Tests for the interface language use cases (`use_cases::localization`).

use entities::command::Command;
use entities::menu::PaletteCommand;
use serde_json::{json, Value};
use test_support::{status, World};
use use_cases::search::{CandidateTarget, SearchKind};

fn spanish_world() -> World {
    World::new()
        .with_language("es", "Español", &[("File: Save", "Archivo: Guardar"), ("Language: {name}", "Idioma: {name}")])
        .with_language("fr", "Français", &[("File: Save", "Fichier : Enregistrer")])
        .with_palette(PaletteCommand { caption: "File: Save".into(), command: Command::new("save") })
}

#[test]
fn the_interface_is_in_english_by_default() {
    let world = spanish_world();
    let editor = world.editor();
    assert_eq!(editor.translations().language(), "en");
    assert_eq!(editor.tr("File: Save"), "File: Save");
}

#[test]
fn auto_follows_the_operating_system_language() {
    let world = spanish_world().with_system_language("es-CL");
    let editor = world.editor();
    assert_eq!(editor.language_setting(), "auto");
    assert_eq!(editor.translations().language(), "es");
    assert_eq!(editor.tr("File: Save"), "Archivo: Guardar");
}

#[test]
fn the_language_setting_wins_over_the_operating_system() {
    let world = spanish_world().with_system_language("es").with_preferences(json!({ "language": "fr" }));
    let editor = world.editor();
    assert_eq!(editor.tr("File: Save"), "Fichier : Enregistrer");
}

#[test]
fn an_unknown_language_falls_back_to_english() {
    let world = spanish_world().with_preferences(json!({ "language": "ja" }));
    let editor = world.editor();
    assert_eq!(editor.translations().language(), "en");
}

#[test]
fn languages_lists_english_first_then_the_packages_ones() {
    let world = spanish_world();
    let editor = world.editor();
    let codes: Vec<String> = editor.languages().into_iter().map(|l| l.code).collect();
    assert_eq!(codes, ["en", "es", "fr"]);
}

#[test]
fn select_language_saves_the_setting_and_switches_at_once() {
    let world = spanish_world();
    let mut editor = world.editor();
    editor.select_language("es");
    assert_eq!(world.resources.user_settings.borrow().get("language"), Some(&Value::from("es")));
    assert_eq!(editor.tr("File: Save"), "Archivo: Guardar");
    assert_eq!(status(&editor), "Idioma: Español");

    editor.select_language("auto");
    assert_eq!(editor.translations().language(), "en");
}

#[test]
fn the_palette_shows_translated_captions_and_matches_english_too() {
    let world = spanish_world().with_preferences(json!({ "language": "es" }));
    let editor = world.editor();
    for query in ["guardar", "save"] {
        let results = editor.search(SearchKind::Commands, query);
        assert_eq!(results[0].caption, "Archivo: Guardar", "query {query}");
    }
}

#[test]
fn the_language_picker_offers_automatic_and_every_language() {
    let world = spanish_world();
    let mut editor = world.editor();
    let results = editor.search(SearchKind::Languages, "");
    let targets: Vec<CandidateTarget> = results.iter().map(|c| c.target.clone()).collect();
    assert_eq!(
        targets,
        ["auto", "en", "es", "fr"].map(|code| CandidateTarget::Language(code.into())).to_vec()
    );
    assert_eq!(results[2].caption, "Español");
    assert_eq!(results[2].hint, "es");

    editor.accept_candidate(CandidateTarget::Language("fr".into()));
    assert_eq!(editor.language_setting(), "fr");
}
