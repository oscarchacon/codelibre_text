//! Tests for `views::theme` (window colors derived from the color scheme).

use codelibre_text::syntax_engine::SyntaxEngine;
use codelibre_text::views::theme::{mix, Chrome};
use egui::Color32;
use use_cases::gateways::PackagePaths;

fn chrome(scheme: &str) -> Chrome {
    let dir = tempfile::tempdir().unwrap();
    let engine = SyntaxEngine::new(PackagePaths { packages: dir.path().join("Packages"), sublime_packages: None });
    Chrome::from_scheme(engine.scheme(scheme))
}

#[test]
fn dark_and_light_schemes_are_detected() {
    assert!(chrome("Monokai").dark);
    assert!(!chrome("Daylight").dark);
}

#[test]
fn the_editor_background_and_accent_come_from_the_scheme() {
    let monokai = chrome("Monokai");
    assert_eq!(monokai.editor_bg, Color32::from_rgb(0x27, 0x28, 0x22));
    assert_eq!(monokai.accent, Color32::from_rgb(0xfd, 0x97, 0x1f));
}

#[test]
fn panels_are_darker_than_the_editor_in_dark_schemes() {
    let monokai = chrome("Monokai");
    let brightness = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
    assert!(brightness(monokai.panel_bg) < brightness(monokai.editor_bg));
    assert!(brightness(monokai.tab_bg) < brightness(monokai.editor_bg));
}

#[test]
fn egui_visuals_follow_the_scheme() {
    let daylight = chrome("Daylight");
    let visuals = daylight.visuals();
    assert!(!visuals.dark_mode);
    assert_eq!(visuals.panel_fill, daylight.panel_bg);
    assert_eq!(visuals.extreme_bg_color, daylight.editor_bg);
}

#[test]
fn mix_interpolates_colors() {
    let black = Color32::BLACK;
    let white = Color32::WHITE;
    assert_eq!(mix(black, white, 0.0), black);
    assert_eq!(mix(black, white, 1.0), white);
    assert_eq!(mix(black, white, 0.5), Color32::from_rgb(128, 128, 128));
}
