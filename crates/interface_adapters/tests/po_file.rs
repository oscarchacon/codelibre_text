use entities::localization::Translations;
use interface_adapters::gateways::po_file;

const SPANISH: &str = r#"# A comment
msgid ""
msgstr ""
"Language: es\n"
"X-Language-Name: Español\n"
"Plural-Forms: nplurals=2; plural=(n != 1);\n"

#: assets/Default/Main.sublime-menu
msgid "Save"
msgstr "Guardar"

msgid "Line one\n"
"line two"
msgstr "Línea uno\n"
"línea dos"

msgid "Say \"hi\""
msgstr "Di \"hola\""

msgid "{count} match"
msgid_plural "{count} matches"
msgstr[0] "{count} coincidencia"
msgstr[1] "{count} coincidencias"

#, fuzzy
msgid "Cancel"
msgstr "Cancelado"
msgid "Exit"
msgstr "Salir"

msgctxt "menu"
msgid "Find"
msgstr "Buscar"

#~ msgid "Old"
#~ msgstr "Viejo"
"#;

fn spanish() -> Translations {
    Translations::new("es", po_file::parse(SPANISH).unwrap().catalog)
}

#[test]
fn reads_the_header() {
    let po = po_file::parse(SPANISH).unwrap();
    assert_eq!(po.language.as_deref(), Some("es"));
    assert_eq!(po.language_name.as_deref(), Some("Español"));
    assert_eq!(po.catalog.plural_forms(), 2);
}

#[test]
fn reads_translations_split_over_several_lines_with_escapes() {
    let es = spanish();
    assert_eq!(es.tr("Save"), "Guardar");
    assert_eq!(es.tr("Line one\nline two"), "Línea uno\nlínea dos");
    assert_eq!(es.tr("Say \"hi\""), "Di \"hola\"");
}

#[test]
fn reads_plural_forms() {
    let es = spanish();
    assert_eq!(es.tr_n("{count} match", "{count} matches", 1, &[]), "1 coincidencia");
    assert_eq!(es.tr_n("{count} match", "{count} matches", 4, &[]), "4 coincidencias");
}

#[test]
fn skips_fuzzy_entries_but_not_the_next_one() {
    let es = spanish();
    assert_eq!(es.tr("Cancel"), "Cancel");
    assert_eq!(es.tr("Exit"), "Salir");
}

#[test]
fn skips_obsolete_entries_and_keeps_contexts_apart() {
    let es = spanish();
    assert_eq!(es.tr("Old"), "Old");
    assert_eq!(es.tr("Find"), "Find");
}

#[test]
fn reports_malformed_files_with_the_line() {
    let error = po_file::parse("msgid \"a\"\nmsgstr \"b").err().unwrap();
    assert!(error.starts_with("line 2:"), "{error}");
    assert!(po_file::parse("\"orphan\"\n").is_err());
    assert!(po_file::parse("msgfoo \"x\"\n").is_err());
}
