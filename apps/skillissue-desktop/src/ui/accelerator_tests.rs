use super::{Keystroke, keyboard::accelerator};
use skillissue_domain::shortcuts::{SETTINGS_ACCELERATOR, default_shortcuts, parse};

const SHIFTED_KEYS: [(&str, &str); 21] = [
    ("~", "Backquote"),
    ("!", "Digit1"),
    ("@", "Digit2"),
    ("#", "Digit3"),
    ("$", "Digit4"),
    ("%", "Digit5"),
    ("^", "Digit6"),
    ("&", "Digit7"),
    ("*", "Digit8"),
    ("(", "Digit9"),
    (")", "Digit0"),
    ("_", "Minus"),
    ("+", "Equal"),
    ("{", "BracketLeft"),
    ("}", "BracketRight"),
    ("|", "Backslash"),
    (":", "Semicolon"),
    ("\"", "Quote"),
    ("<", "Comma"),
    (">", "Period"),
    ("?", "Slash"),
];

#[test]
fn shifted_ascii_records_physical_key_with_exactly_one_shift_modifier() {
    for (symbol, physical_key) in SHIFTED_KEYS {
        for already_shifted in [false, true] {
            let mut key = Keystroke::parse("ctrl-k").unwrap();
            key.key = symbol.into();
            key.modifiers.shift = already_shifted;
            let recorded = accelerator(&key).unwrap();
            let expected = format!("Ctrl+Shift+{physical_key}");
            assert_eq!(recorded, expected, "{symbol:?}, shift={already_shifted}");
            assert_eq!(
                parse(&recorded).unwrap().id(),
                parse(&expected).unwrap().id()
            );
        }
    }
}

#[test]
fn shifted_ascii_preserves_control_alt_and_platform_modifiers() {
    for (symbol, physical_key) in SHIFTED_KEYS {
        let mut key = Keystroke::parse("ctrl-alt-k").unwrap();
        key.key = symbol.into();
        key.modifiers.platform = true;
        let recorded = accelerator(&key).unwrap();
        let expected = format!("Ctrl+Alt+Shift+Super+{physical_key}");
        assert_eq!(recorded, expected, "{symbol:?}");
        assert_eq!(
            parse(&recorded).unwrap().id(),
            parse(&expected).unwrap().id()
        );
    }
}

#[test]
fn windows_shifted_events_record_default_settings_and_session_chords() {
    for (symbol, expected) in [
        ("<", SETTINGS_ACCELERATOR.to_owned()),
        ("|", default_shortcuts()["toggleSession"].clone()),
    ] {
        let mut key = Keystroke::parse("ctrl-k").unwrap();
        key.key = symbol.into();
        assert!(!key.modifiers.shift);
        let recorded = accelerator(&key).unwrap();
        assert_eq!(
            parse(&recorded).unwrap().id(),
            parse(&expected).unwrap().id()
        );
    }
}

#[test]
fn unshifted_ascii_keeps_existing_accelerator_spelling_and_modifiers() {
    for value in [
        "`", "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "[", "]", "\\", ";", "'",
        ",", ".", "/", "k", "f11", "left",
    ] {
        let mut key = Keystroke::parse("ctrl-k").unwrap();
        key.key = value.into();
        let recorded = accelerator(&key).unwrap();
        assert_eq!(recorded, format!("Ctrl+{value}"));
        assert_eq!(
            parse(&recorded).unwrap().id(),
            parse(&format!("Ctrl+{value}")).unwrap().id()
        );
    }
    for (key_name, expected) in [("return", "Ctrl+Enter"), (" ", "Ctrl+Space")] {
        let mut key = Keystroke::parse("ctrl-k").unwrap();
        key.key = key_name.into();
        assert_eq!(accelerator(&key).as_deref(), Some(expected));
    }
}

#[test]
fn pressing_only_a_modifier_does_not_finish_recording() {
    for name in ["control", "alt", "shift", "super", "fn"] {
        let mut key = Keystroke::parse("ctrl-k").unwrap();
        key.key = name.into();
        assert!(accelerator(&key).is_none(), "{name}");
    }
}
