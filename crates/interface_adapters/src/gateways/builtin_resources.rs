//! Built-in resources compiled into the binary: our replacement for Sublime's `Default` package.
//! The files live in `assets/Default/` at the repository root.

macro_rules! asset {
    ($file:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/Default/", $file))
    };
}

/// The key bindings of this platform: [`OSX_KEYMAP`] on macOS, [`PC_KEYMAP`] elsewhere.
pub const KEYMAP: &str = if cfg!(target_os = "macos") { OSX_KEYMAP } else { PC_KEYMAP };
/// Key bindings for Windows and Linux (Ctrl).
pub const PC_KEYMAP: &str = asset!("Default.sublime-keymap");
/// Key bindings for macOS (Cmd), as Sublime's `Default (OSX).sublime-keymap`.
pub const OSX_KEYMAP: &str = asset!("Default (OSX).sublime-keymap");
/// Every built-in keymap with its file name, to check them all whatever the platform.
pub const KEYMAPS: &[(&str, &str)] = &[("Default.sublime-keymap", PC_KEYMAP), ("Default (OSX).sublime-keymap", OSX_KEYMAP)];
pub const PALETTE_COMMANDS: &str = asset!("Default.sublime-commands");
pub const MAIN_MENU: &str = asset!("Main.sublime-menu");
/// Right-click menus: the text, a side bar entry, a top folder of the side bar (added after the
/// side bar's), a tab.
pub const CONTEXT_MENU: &str = asset!("Context.sublime-menu");
pub const SIDE_BAR_MENU: &str = asset!("Side Bar.sublime-menu");
pub const MOUNT_POINT_MENU: &str = asset!("Side Bar Mount Point.sublime-menu");
pub const TAB_CONTEXT_MENU: &str = asset!("Tab Context.sublime-menu");
/// Every built-in menu with its file name.
pub const MENUS: &[(&str, &str)] = &[
    ("Main.sublime-menu", MAIN_MENU),
    ("Context.sublime-menu", CONTEXT_MENU),
    ("Side Bar.sublime-menu", SIDE_BAR_MENU),
    ("Side Bar Mount Point.sublime-menu", MOUNT_POINT_MENU),
    ("Tab Context.sublime-menu", TAB_CONTEXT_MENU),
];
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
