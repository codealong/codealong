//! A key as a window's keyboard event names it (`KeyboardEvent.code`, the W3C name of the
//! physical key, whatever the layout), as libghostty's key encoder takes it (`key::Key`, which
//! uses the same names), with the character the key gives with no modifiers on a US layout,
//! which the Kitty keyboard protocol identifies keys by.

use libghostty_vt::key::Key;

/// The key a `KeyboardEvent.code` names, and the character it gives with no modifiers (`\0` for
/// one that gives none). Unidentified for a code libghostty has no key for.
pub fn of_code(code: &str) -> (Key, char) {
    if let Some(letter) = code.strip_prefix("Key").filter(|l| l.len() == 1) {
        let c = letter.as_bytes()[0];
        if c.is_ascii_uppercase() {
            return (LETTERS[usize::from(c - b'A')], char::from(c.to_ascii_lowercase()));
        }
    }
    if let Some(digit) = code.strip_prefix("Digit").filter(|d| d.len() == 1) {
        let c = digit.as_bytes()[0];
        if c.is_ascii_digit() {
            return (DIGITS[usize::from(c - b'0')], char::from(c));
        }
    }
    if let Some(n) = code.strip_prefix('F').and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=FKEYS.len()).contains(n)) {
        return (FKEYS[n - 1], '\0');
    }
    match code {
        "Backquote" => (Key::Backquote, '`'),
        "Backslash" => (Key::Backslash, '\\'),
        "BracketLeft" => (Key::BracketLeft, '['),
        "BracketRight" => (Key::BracketRight, ']'),
        "Comma" => (Key::Comma, ','),
        "Equal" => (Key::Equal, '='),
        "IntlBackslash" => (Key::IntlBackslash, '\0'),
        "IntlRo" => (Key::IntlRo, '\0'),
        "IntlYen" => (Key::IntlYen, '\0'),
        "Minus" => (Key::Minus, '-'),
        "Period" => (Key::Period, '.'),
        "Quote" => (Key::Quote, '\''),
        "Semicolon" => (Key::Semicolon, ';'),
        "Slash" => (Key::Slash, '/'),
        "AltLeft" => (Key::AltLeft, '\0'),
        "AltRight" => (Key::AltRight, '\0'),
        "Backspace" => (Key::Backspace, '\0'),
        "CapsLock" => (Key::CapsLock, '\0'),
        "ContextMenu" => (Key::ContextMenu, '\0'),
        "ControlLeft" => (Key::ControlLeft, '\0'),
        "ControlRight" => (Key::ControlRight, '\0'),
        "Enter" => (Key::Enter, '\0'),
        "MetaLeft" => (Key::MetaLeft, '\0'),
        "MetaRight" => (Key::MetaRight, '\0'),
        "ShiftLeft" => (Key::ShiftLeft, '\0'),
        "ShiftRight" => (Key::ShiftRight, '\0'),
        "Space" => (Key::Space, ' '),
        "Tab" => (Key::Tab, '\0'),
        "Delete" => (Key::Delete, '\0'),
        "End" => (Key::End, '\0'),
        "Help" => (Key::Help, '\0'),
        "Home" => (Key::Home, '\0'),
        "Insert" => (Key::Insert, '\0'),
        "PageDown" => (Key::PageDown, '\0'),
        "PageUp" => (Key::PageUp, '\0'),
        "ArrowDown" => (Key::ArrowDown, '\0'),
        "ArrowLeft" => (Key::ArrowLeft, '\0'),
        "ArrowRight" => (Key::ArrowRight, '\0'),
        "ArrowUp" => (Key::ArrowUp, '\0'),
        "NumLock" => (Key::NumLock, '\0'),
        "Numpad0" => (Key::Numpad0, '0'),
        "Numpad1" => (Key::Numpad1, '1'),
        "Numpad2" => (Key::Numpad2, '2'),
        "Numpad3" => (Key::Numpad3, '3'),
        "Numpad4" => (Key::Numpad4, '4'),
        "Numpad5" => (Key::Numpad5, '5'),
        "Numpad6" => (Key::Numpad6, '6'),
        "Numpad7" => (Key::Numpad7, '7'),
        "Numpad8" => (Key::Numpad8, '8'),
        "Numpad9" => (Key::Numpad9, '9'),
        "NumpadAdd" => (Key::NumpadAdd, '+'),
        "NumpadDecimal" => (Key::NumpadDecimal, '.'),
        "NumpadDivide" => (Key::NumpadDivide, '/'),
        "NumpadEnter" => (Key::NumpadEnter, '\0'),
        "NumpadEqual" => (Key::NumpadEqual, '='),
        "NumpadMultiply" => (Key::NumpadMultiply, '*'),
        "NumpadSubtract" => (Key::NumpadSubtract, '-'),
        "NumpadComma" => (Key::NumpadComma, ','),
        "Escape" => (Key::Escape, '\0'),
        "PrintScreen" => (Key::PrintScreen, '\0'),
        "ScrollLock" => (Key::ScrollLock, '\0'),
        "Pause" => (Key::Pause, '\0'),
        _ => (Key::Unidentified, '\0'),
    }
}

const LETTERS: [Key; 26] = [
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
];

const DIGITS: [Key; 10] = [Key::Digit0, Key::Digit1, Key::Digit2, Key::Digit3, Key::Digit4, Key::Digit5, Key::Digit6, Key::Digit7, Key::Digit8, Key::Digit9];

const FKEYS: [Key; 24] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
    Key::F13,
    Key::F14,
    Key::F15,
    Key::F16,
    Key::F17,
    Key::F18,
    Key::F19,
    Key::F20,
    Key::F21,
    Key::F22,
    Key::F23,
    Key::F24,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_keyboard_events_code_as_ghosttys_key_and_its_plain_character() {
        assert_eq!(of_code("KeyA"), (Key::A, 'a'));
        assert_eq!(of_code("KeyZ"), (Key::Z, 'z'));
        assert_eq!(of_code("Digit7"), (Key::Digit7, '7'));
        assert_eq!(of_code("F12"), (Key::F12, '\0'));
        assert_eq!(of_code("ArrowUp"), (Key::ArrowUp, '\0'));
        assert_eq!(of_code("Enter"), (Key::Enter, '\0'));
        assert_eq!(of_code("Space"), (Key::Space, ' '));
        assert_eq!(of_code("Slash"), (Key::Slash, '/'));
        // Not a key libghostty has, nor a name that only looks like one.
        assert_eq!(of_code("Keyboard"), (Key::Unidentified, '\0'));
        assert_eq!(of_code("F99"), (Key::Unidentified, '\0'));
        assert_eq!(of_code(""), (Key::Unidentified, '\0'));
    }
}
