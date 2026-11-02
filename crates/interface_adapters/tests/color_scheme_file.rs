//! Tests for `gateways::color_scheme_file` (`.sublime-color-scheme` files).

use interface_adapters::gateways::color_scheme_file::{parse, Rgba};

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    Rgba { r, g, b, a }
}

/// The color a scheme gives to `globals.background`.
fn background(spec: &str) -> Option<Rgba> {
    let source = format!(r#"{{ "globals": {{ "background": "{spec}" }} }}"#);
    parse(&source, "Test").unwrap().globals.get("background").copied()
}

#[test]
fn hex_colors_in_every_length() {
    assert_eq!(background("#abc"), Some(rgba(0xaa, 0xbb, 0xcc, 255)));
    assert_eq!(background("#abcd"), Some(rgba(0xaa, 0xbb, 0xcc, 0xdd)));
    assert_eq!(background("#102030"), Some(rgba(0x10, 0x20, 0x30, 255)));
    assert_eq!(background("#10203040"), Some(rgba(0x10, 0x20, 0x30, 0x40)));
    assert_eq!(background("#12345"), None);
}

#[test]
fn rgb_and_rgba_functions() {
    assert_eq!(background("rgb(1, 2, 3)"), Some(rgba(1, 2, 3, 255)));
    assert_eq!(background("rgba(1, 2, 3, 0.5)"), Some(rgba(1, 2, 3, 128)));
}

#[test]
fn hsl_and_hsla_functions() {
    assert_eq!(background("hsl(0, 100%, 50%)"), Some(rgba(255, 0, 0, 255)));
    assert_eq!(background("hsl(120, 100%, 50%)"), Some(rgba(0, 255, 0, 255)));
    assert_eq!(background("hsla(240, 100%, 50%, 0.5)"), Some(rgba(0, 0, 255, 128)));
}

#[test]
fn named_colors() {
    assert_eq!(background("white"), Some(rgba(255, 255, 255, 255)));
    assert_eq!(background("black"), Some(rgba(0, 0, 0, 255)));
    assert_eq!(background("transparent"), Some(rgba(0, 0, 0, 0)));
    assert_eq!(background("chartreuse-ish"), None);
}

#[test]
fn variables_can_refer_to_other_variables() {
    let scheme = parse(
        r##"{
            "variables": { "red": "#ff0000", "alias": "var(red)", "loop": "var(loop)" },
            "globals": { "background": "var(alias)", "foreground": "var(loop)", "caret": "var(missing)" }
        }"##,
        "Test",
    )
    .unwrap();
    assert_eq!(scheme.globals.get("background"), Some(&rgba(255, 0, 0, 255)));
    assert!(!scheme.globals.contains_key("foreground"), "circular variables are ignored");
    assert!(!scheme.globals.contains_key("caret"), "unknown variables are ignored");
}

#[test]
fn color_function_with_alpha() {
    let scheme = parse(
        r##"{
            "variables": { "blue": "#0000ff" },
            "globals": {
                "selection": "color(var(blue) alpha(0.25))",
                "line_highlight": "color(#ffffff a(50%))",
                "gutter": "color(rgb(1, 2, 3) blend(#000 50%))"
            }
        }"##,
        "Test",
    )
    .unwrap();
    assert_eq!(scheme.globals["selection"], rgba(0, 0, 255, 64));
    assert_eq!(scheme.globals["line_highlight"], rgba(255, 255, 255, 128));
    assert_eq!(scheme.globals["gutter"], rgba(1, 2, 3, 255), "other adjusters are ignored");
}

#[test]
fn rules_keep_scope_colors_and_font_styles() {
    let scheme = parse(
        r##"{
            "rules": [
                { "scope": "comment", "foreground": "#808080", "font_style": "italic bold" },
                { "scope": "string", "background": "#000001", "font_style": "" },
                { "scope": "keyword", "foreground": ["#ff0000", "#00ff00"] },
                { "foreground": "#123456" }
            ]
        }"##,
        "Test",
    )
    .unwrap();
    assert_eq!(scheme.rules.len(), 3, "rules without a scope are dropped");

    let comment = &scheme.rules[0];
    assert_eq!(comment.scope, "comment");
    assert_eq!(comment.foreground, Some(rgba(128, 128, 128, 255)));
    assert!(comment.italic && comment.bold && !comment.underline && comment.has_font_style);

    let string = &scheme.rules[1];
    assert_eq!(string.background, Some(rgba(0, 0, 1, 255)));
    assert!(string.has_font_style && !string.italic, "an empty font_style resets the style");

    let keyword = &scheme.rules[2];
    assert_eq!(keyword.foreground, Some(rgba(255, 0, 0, 255)), "gradients use their first stop");
    assert!(!keyword.has_font_style);
}

#[test]
fn name_and_author() {
    let named = parse(r#"{ "name": "Mariana", "author": "Sublime HQ" }"#, "file stem").unwrap();
    assert_eq!(named.name, "Mariana");
    assert_eq!(named.author.as_deref(), Some("Sublime HQ"));
    assert_eq!(parse("{}", "file stem").unwrap().name, "file stem");
}

#[test]
fn comments_and_trailing_commas_are_accepted() {
    let scheme = parse("{ // scheme\n \"globals\": { \"background\": \"#000\", }, }", "Test").unwrap();
    assert_eq!(scheme.globals["background"], rgba(0, 0, 0, 255));
}

#[test]
fn invalid_files_are_errors() {
    assert!(parse("{ nope", "Test").is_err());
}
