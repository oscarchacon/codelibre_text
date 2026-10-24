//! Translates egui key events into the keymap's [`KeyCombo`]s.

use egui::{Key, Modifiers};
use entities::keymap::KeyCombo;

/// The ways a key press can be written in a keymap: by the character it produced (logical key)
/// and by its position on the keyboard (physical key). They differ on non-US layouts.
pub fn key_combos(key: Key, physical_key: Option<Key>, modifiers: Modifiers) -> Vec<KeyCombo> {
    [Some(key), physical_key]
        .into_iter()
        .flatten()
        .filter_map(|k| combo(k, modifiers))
        .collect()
}

fn combo(key: Key, modifiers: Modifiers) -> Option<KeyCombo> {
    Some(KeyCombo {
        ctrl: modifiers.ctrl || (modifiers.command && !cfg!(target_os = "macos")),
        alt: modifiers.alt,
        shift: modifiers.shift,
        super_key: modifiers.mac_cmd,
        key: key_name(key)?.to_string(),
    })
}

/// Sublime's name for an egui key.
fn key_name(key: Key) -> Option<&'static str> {
    use Key::*;
    Some(match key {
        ArrowDown => "down",
        ArrowLeft => "left",
        ArrowRight => "right",
        ArrowUp => "up",
        Escape => "escape",
        Tab => "tab",
        Backspace => "backspace",
        Enter => "enter",
        Space => "space",
        Insert => "insert",
        Delete => "delete",
        Home => "home",
        End => "end",
        PageUp => "pageup",
        PageDown => "pagedown",
        Colon => ":",
        Comma => ",",
        Backslash => "\\",
        Slash => "/",
        Pipe => "|",
        Questionmark => "?",
        Exclamationmark => "!",
        OpenBracket => "[",
        CloseBracket => "]",
        OpenCurlyBracket => "{",
        CloseCurlyBracket => "}",
        Backtick => "`",
        Minus => "-",
        Period => ".",
        Plus => "+",
        Equals => "=",
        Semicolon => ";",
        Quote => "'",
        Num0 => "0",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Num4 => "4",
        Num5 => "5",
        Num6 => "6",
        Num7 => "7",
        Num8 => "8",
        Num9 => "9",
        A => "a",
        B => "b",
        C => "c",
        D => "d",
        E => "e",
        F => "f",
        G => "g",
        H => "h",
        I => "i",
        J => "j",
        K => "k",
        L => "l",
        M => "m",
        N => "n",
        O => "o",
        P => "p",
        Q => "q",
        R => "r",
        S => "s",
        T => "t",
        U => "u",
        V => "v",
        W => "w",
        X => "x",
        Y => "y",
        Z => "z",
        F1 => "f1",
        F2 => "f2",
        F3 => "f3",
        F4 => "f4",
        F5 => "f5",
        F6 => "f6",
        F7 => "f7",
        F8 => "f8",
        F9 => "f9",
        F10 => "f10",
        F11 => "f11",
        F12 => "f12",
        _ => return None,
    })
}
