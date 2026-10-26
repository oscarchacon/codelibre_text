//! Built-in resources compiled into the binary: our replacement for Sublime's `Default` package.
//! The files live in `assets/Default/` at the repository root.

macro_rules! asset {
    ($file:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Default/", $file))
    };
}

pub const KEYMAP: &str = asset!("Default.sublime-keymap");
pub const PALETTE_COMMANDS: &str = asset!("Default.sublime-commands");
pub const MAIN_MENU: &str = asset!("Main.sublime-menu");
pub const PREFERENCES: &str = asset!("Preferences.sublime-settings");

/// Built-in `.sublime-color-scheme` files, by name.
pub const COLOR_SCHEMES: &[(&str, &str)] = &[
    ("Monokai", asset!("Monokai.sublime-color-scheme")),
    ("Daylight", asset!("Daylight.sublime-color-scheme")),
];

/// Built-in translations of the interface (`assets/Default/Localization/<language>.po`), by
/// language code. Add new ones here; `messages.pot` next to them is the template.
pub const TRANSLATIONS: &[(&str, &str)] = &[
    ("de", asset!("Localization/de.po")),
    ("es", asset!("Localization/es.po")),
    ("fr", asset!("Localization/fr.po")),
    ("pt_BR", asset!("Localization/pt_BR.po")),
];

/// The template every translation starts from: all the interface's English text.
pub const TRANSLATION_TEMPLATE: &str = asset!("Localization/messages.pot");
