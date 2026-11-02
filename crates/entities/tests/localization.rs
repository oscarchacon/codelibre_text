use entities::localization::{fill, normalize_language_code, resolve_language, Catalog, Language, Translations};

fn spanish() -> Translations {
    let mut catalog = Catalog::new(Some("nplurals=2; plural=(n != 1);")).unwrap();
    catalog.insert(None, "Save", vec!["Guardar".into()]);
    catalog.insert(None, "Saved {path}", vec!["Guardado: {path}".into()]);
    catalog.insert(None, "{count} match", vec!["{count} coincidencia".into(), "{count} coincidencias".into()]);
    catalog.insert(None, "Untranslated", vec![String::new()]);
    Translations::new("es", catalog)
}

fn languages(codes: &[&str]) -> Vec<Language> {
    codes.iter().map(|c| Language { code: c.to_string(), name: c.to_string() }).collect()
}

#[test]
fn translated_text_replaces_the_english_one() {
    let es = spanish();
    assert_eq!(es.language(), "es");
    assert_eq!(es.tr("Save"), "Guardar");
    assert_eq!(es.tr_args("Saved {path}", &[("path", &"a.txt")]), "Guardado: a.txt");
}

#[test]
fn text_without_a_translation_stays_in_english() {
    let es = spanish();
    assert_eq!(es.tr("Find"), "Find");
    assert_eq!(es.tr("Untranslated"), "Untranslated");
    assert_eq!(Translations::default().tr("Save"), "Save");
}

#[test]
fn plurals_follow_the_language_rule() {
    let es = spanish();
    assert_eq!(es.tr_n("{count} match", "{count} matches", 1, &[]), "1 coincidencia");
    assert_eq!(es.tr_n("{count} match", "{count} matches", 0, &[]), "0 coincidencias");
    assert_eq!(es.tr_n("{count} match", "{count} matches", 7, &[]), "7 coincidencias");
    let english = Translations::default();
    assert_eq!(english.tr_n("{count} file", "{count} files", 1, &[]), "1 file");
    assert_eq!(english.tr_n("{count} file", "{count} files", 2, &[]), "2 files");
}

#[test]
fn plural_formulas_with_several_forms_choose_the_right_one() {
    // Polish: 1 plik, 2-4 pliki, 5+ plików (but 22 pliki).
    let rule = "nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);";
    let mut catalog = Catalog::new(Some(rule)).unwrap();
    catalog.insert(None, "{count} file", vec!["{count} plik".into(), "{count} pliki".into(), "{count} plików".into()]);
    let pl = Translations::new("pl", catalog);
    let form = |n| pl.tr_n("{count} file", "{count} files", n, &[]);
    assert_eq!(form(1), "1 plik");
    assert_eq!(form(3), "3 pliki");
    assert_eq!(form(5), "5 plików");
    assert_eq!(form(12), "12 plików");
    assert_eq!(form(22), "22 pliki");
}

#[test]
fn invalid_plural_formulas_are_rejected() {
    assert!(Catalog::new(Some("nplurals=2; plural=(n != ;")).is_err());
    assert!(Catalog::new(Some("plural=(n != 1);")).is_err());
}

#[test]
fn later_catalogs_replace_earlier_translations() {
    let mut builtin = Catalog::default();
    builtin.insert(None, "Save", vec!["Guardar".into()]);
    builtin.insert(None, "Cancel", vec!["Cancelar".into()]);
    let mut package = Catalog::default();
    package.insert(None, "Save", vec!["Grabar".into()]);
    builtin.merge(package);
    let es = Translations::new("es", builtin);
    assert_eq!(es.tr("Save"), "Grabar");
    assert_eq!(es.tr("Cancel"), "Cancelar");
}

#[test]
fn messages_with_a_context_do_not_match_plain_lookups() {
    let mut catalog = Catalog::default();
    catalog.insert(Some("menu"), "Find", vec!["Buscar".into()]);
    assert_eq!(Translations::new("es", catalog).tr("Find"), "Find");
}

#[test]
fn fill_replaces_known_placeholders_only() {
    assert_eq!(fill("{a} and {b}", &[("a", &1), ("b", &"two")]), "1 and two");
    assert_eq!(fill("{unknown} {a}", &[("a", &1)]), "{unknown} 1");
    assert_eq!(fill("no { closing", &[]), "no { closing");
}

#[test]
fn language_codes_are_normalized() {
    assert_eq!(normalize_language_code("pt-br"), "pt_BR");
    assert_eq!(normalize_language_code("es_CL.UTF-8"), "es_CL");
    assert_eq!(normalize_language_code("DE"), "de");
}

#[test]
fn the_language_setting_resolves_to_an_available_language() {
    let available = languages(&["en", "es", "pt_BR"]);
    assert_eq!(resolve_language("es", None, &available), "es");
    assert_eq!(resolve_language("auto", Some("es-CL"), &available), "es");
    assert_eq!(resolve_language("pt", None, &available), "pt_BR");
    assert_eq!(resolve_language("pt-BR", None, &available), "pt_BR");
    assert_eq!(resolve_language("auto", Some("ja-JP"), &available), "en");
    assert_eq!(resolve_language("auto", None, &available), "en");
    assert_eq!(resolve_language("", Some("es"), &available), "es");
}
