//! Tests for what code folding folds (`entities::folding`).

use entities::folding::{fold_headers, fold_region_at, folds_at_level, merge_folds};

const CODE: &str =
    "fn a() {\n    one();\n\n    if x {\n        two();\n    }\n}\nfn b() {\n\tthree();\n}\n";

/// The text a fold hides.
fn hidden(fold: (usize, usize)) -> String {
    CODE.chars().skip(fold.0).take(fold.1 - fold.0).collect()
}

#[test]
fn the_caret_s_block_folds_after_its_header() {
    // On the header: the block it heads.
    let fold = fold_region_at(CODE, 2, 4).unwrap();
    assert_eq!(
        hidden(fold),
        "\n    one();\n\n    if x {\n        two();\n    }"
    );
    // Inside: the block it is in, blank lines too.
    assert_eq!(fold_region_at(CODE, 12, 4), Some(fold));
    assert_eq!(fold_region_at(CODE, 20, 4), Some(fold), "a blank line");
    // A nested header folds its own block.
    let inner = fold_region_at(CODE, 28, 4).unwrap();
    assert_eq!(hidden(inner), "\n        two();");
    // A tab counts as indentation.
    assert_eq!(hidden(fold_region_at(CODE, 66, 4).unwrap()), "\n\tthree();");
    // Nothing indented under the last line.
    assert_eq!(fold_region_at("a\nb", 2, 4), None);
}

#[test]
fn fold_by_level_folds_the_blocks_at_that_depth() {
    let level1: Vec<String> = folds_at_level(CODE, 1, 4).into_iter().map(hidden).collect();
    assert_eq!(
        level1,
        vec![
            "\n    one();\n\n    if x {\n        two();\n    }".to_string(),
            "\n\tthree();".to_string()
        ]
    );
    let level2: Vec<String> = folds_at_level(CODE, 2, 4).into_iter().map(hidden).collect();
    assert_eq!(level2, vec!["\n        two();".to_string()]);
}

#[test]
fn folds_merge_when_they_overlap_or_touch() {
    let mut folds = vec![(10, 20), (0, 5), (15, 30), (30, 31), (40, 40)];
    merge_folds(&mut folds);
    assert_eq!(folds, vec![(0, 5), (10, 31)]);
}

#[test]
fn the_gutter_s_fold_buttons_go_on_the_lines_that_head_a_block() {
    assert_eq!(fold_headers(CODE, 0, 99, 4), vec![0, 3, 7]);
    assert_eq!(
        fold_headers(CODE, 2, 5, 4),
        vec![3],
        "only the lines asked for"
    );
    assert!(fold_headers("", 0, 0, 4).is_empty());
}

/// A text and its tokens as `(text, scope)` pieces, for `tag_attribute_folds`.
fn tokens(pieces: &[(&str, &'static str)]) -> (String, Vec<(usize, usize, &'static str)>) {
    let mut text = String::new();
    let mut out = Vec::new();
    for (piece, scope) in pieces {
        let start = text.chars().count();
        text.push_str(piece);
        out.push((start, text.chars().count(), *scope));
    }
    (text, out)
}

const TAG: &str = "text.xml meta.tag.xml";
const NAME: &str = "text.xml meta.tag.xml entity.name.tag.namespace.xml";
const LOCAL: &str = "text.xml meta.tag.xml entity.name.tag.localname.xml";
const SEP: &str = "text.xml meta.tag.xml entity.name.tag.xml punctuation.separator.namespace.xml";
const BEGIN: &str = "text.xml meta.tag.xml punctuation.definition.tag.begin.xml";
const END: &str = "text.xml meta.tag.xml punctuation.definition.tag.end.xml";
const ATTR: &str = "text.xml meta.tag.xml entity.other.attribute-name.xml";

#[test]
fn tag_attributes_fold_from_the_name_to_the_closing_bracket() {
    // `<x:y z:w='1'  >q</x:y>`: Sublime 4215 folds ` z:w='1'  ` (the name is in pieces).
    let (text, pieces) = tokens(&[
        ("<", BEGIN),
        ("x", NAME),
        (":", SEP),
        ("y", LOCAL),
        (" ", TAG),
        ("z:w", ATTR),
        ("='1'  ", TAG),
        (">", END),
        ("q", "text.xml"),
        ("</", BEGIN),
        ("x:y", LOCAL),
        (">", END),
    ]);
    let folds = entities::folding::tag_attribute_folds(&text, &pieces);
    assert_eq!(folds, [(4, 14)]);
    assert_eq!(&text[4..14], " z:w='1'  ");
}

#[test]
fn tags_without_attributes_processing_instructions_and_other_scopes_have_no_fold() {
    let preprocessor = "text.xml meta.tag.preprocessor.xml entity.name.tag.xml";
    let (text, pieces) = tokens(&[
        ("<", BEGIN),
        ("p", LOCAL),
        (" ", TAG),
        (">", END),
        ("<?", "text.xml meta.tag.preprocessor.xml punctuation.definition.tag.begin.xml"),
        ("xml", preprocessor),
        (" version=\"1.0\"", "text.xml meta.tag.preprocessor.xml"),
        ("?>", "text.xml meta.tag.preprocessor.xml punctuation.definition.tag.end.xml"),
        // CSS names tags too, outside any `meta.tag`.
        ("div", "source.css meta.selector.css entity.name.tag.css"),
        (" { color: red }", "source.css meta.property-list.css"),
    ]);
    assert!(entities::folding::tag_attribute_folds(&text, &pieces).is_empty());
}
