//! Electron-style accelerators ("F8", "Ctrl+Shift+X") <-> Win32 virtual-key codes.
//! shortcut: punctuation/OEM keys are not supported (their vk depends on the keyboard layout); add `VkKeyScanW` if asked.

use std::fmt;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Accel {
    pub vk: u32,
    pub mods: Mods,
}

/// (accepted lowercase names, vk, canonical name). Canonical names are also valid
/// `tauri-plugin-global-shortcut` key names, which the fallback backend relies on.
const NAMED: &[(&[&str], u32, &str)] = &[
    (&["space"], 0x20, "Space"),
    (&["tab"], 0x09, "Tab"),
    (&["enter", "return"], 0x0D, "Enter"),
    (&["backspace"], 0x08, "Backspace"),
    (&["delete", "del"], 0x2E, "Delete"),
    (&["insert", "ins"], 0x2D, "Insert"),
    (&["home"], 0x24, "Home"),
    (&["end"], 0x23, "End"),
    (&["pageup"], 0x21, "PageUp"),
    (&["pagedown"], 0x22, "PageDown"),
    (&["up", "arrowup"], 0x26, "ArrowUp"),
    (&["down", "arrowdown"], 0x28, "ArrowDown"),
    (&["left", "arrowleft"], 0x25, "ArrowLeft"),
    (&["right", "arrowright"], 0x27, "ArrowRight"),
    (&["escape", "esc"], 0x1B, "Escape"),
    (
        &["nummult", "nummultiply", "numpadmultiply"],
        0x6A,
        "NumpadMultiply",
    ),
    (&["numadd", "numpadadd"], 0x6B, "NumpadAdd"),
    (
        &["numsub", "numsubtract", "numpadsubtract"],
        0x6D,
        "NumpadSubtract",
    ),
    (
        &["numdec", "numdecimal", "numpaddecimal"],
        0x6E,
        "NumpadDecimal",
    ),
    (
        &["numdiv", "numdivide", "numpaddivide"],
        0x6F,
        "NumpadDivide",
    ),
];

fn key_vk(name: &str) -> Option<u32> {
    let l = name.to_ascii_lowercase();
    if let Some(n) = l.strip_prefix('f').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&n)
    {
        return Some(0x70 + n - 1);
    }
    let b = l.as_bytes();
    if let [c] = b
        && c.is_ascii_alphanumeric()
    {
        return Some(c.to_ascii_uppercase() as u32);
    }
    if let Some(d) = l.strip_prefix("numpad").or_else(|| l.strip_prefix("num"))
        && let [c @ b'0'..=b'9'] = d.as_bytes()
    {
        return Some(0x60 + (c - b'0') as u32);
    }
    NAMED
        .iter()
        .find(|(names, ..)| names.contains(&l.as_str()))
        .map(|&(_, vk, _)| vk)
}

fn vk_name(vk: u32) -> Option<String> {
    Some(match vk {
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0x60..=0x69 => format!("Numpad{}", vk - 0x60),
        _ => NAMED.iter().find(|e| e.1 == vk)?.2.to_string(),
    })
}

pub fn is_modifier_vk(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

/// Syntax only: a key plus optional modifiers. See [`Accel::validate`] for policy.
pub fn parse(s: &str) -> Result<Accel, String> {
    let mut mods = Mods::default();
    let mut key = None;
    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "control" | "ctrl" | "commandorcontrol" | "command" | "cmd" => mods.ctrl = true,
            "alt" | "option" | "altgr" => mods.alt = true,
            "shift" => mods.shift = true,
            "super" | "meta" | "win" | "windows" => mods.win = true,
            _ if key.is_some() => return Err(format!("'{s}' has more than one key")),
            _ => key = Some(part),
        }
    }
    let key = key.ok_or_else(|| format!("'{s}' has no key"))?;
    let vk = key_vk(key).ok_or_else(|| format!("unsupported key '{key}'"))?;
    Ok(Accel { vk, mods })
}

/// Combos the OS or the game's own input must keep.
const RESERVED: &[(u32, Mods)] = &[
    (
        0x73,
        Mods {
            alt: true,
            ctrl: false,
            shift: false,
            win: false,
        },
    ), // Alt+F4
    (
        0x09,
        Mods {
            alt: true,
            ctrl: false,
            shift: false,
            win: false,
        },
    ), // Alt+Tab
    (
        0x1B,
        Mods {
            alt: false,
            ctrl: true,
            shift: false,
            win: false,
        },
    ), // Ctrl+Esc
    (
        0x1B,
        Mods {
            alt: false,
            ctrl: true,
            shift: true,
            win: false,
        },
    ), // Ctrl+Shift+Esc
    (
        0x2E,
        Mods {
            alt: true,
            ctrl: true,
            shift: false,
            win: false,
        },
    ), // Ctrl+Alt+Del
    (
        0x4C,
        Mods {
            alt: false,
            ctrl: false,
            shift: false,
            win: true,
        },
    ), // Win+L
];

impl Accel {
    /// Rejects reserved combos and bare keys that would eat normal typing in the game.
    pub fn validate(&self) -> Result<(), String> {
        if RESERVED.contains(&(self.vk, self.mods)) {
            return Err(format!("{self} is reserved by the system"));
        }
        let bare_ok = matches!(self.vk, 0x70..=0x87 | 0x21..=0x24 | 0x2D | 0x2E | 0x60..=0x6F);
        if self.mods == Mods::default() && !bare_ok {
            return Err(format!(
                "{self} needs a modifier (only F-keys, navigation and numpad keys can be bare)"
            ));
        }
        Ok(())
    }
}

pub fn parse_valid(s: &str) -> Result<Accel, String> {
    let a = parse(s)?;
    a.validate()?;
    Ok(a)
}

impl fmt::Display for Accel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
            (self.mods.win, "Super"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&vk_name(self.vk).unwrap_or_else(|| format!("VK{:#x}", self.vk)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Accel {
        parse(s).unwrap()
    }

    #[test]
    fn parses_f_keys_letters_digits() {
        assert_eq!(a("F8").vk, 0x77);
        assert_eq!(a("f1").vk, 0x70);
        assert_eq!(a("F24").vk, 0x87);
        assert_eq!(a("x").vk, 0x58);
        assert_eq!(a("5").vk, 0x35);
        assert!(parse("F25").is_err());
        assert!(parse("F0").is_err());
    }

    #[test]
    fn parses_modifiers_and_aliases() {
        let m = a("CommandOrControl+Shift+X").mods;
        assert!(m.ctrl && m.shift && !m.alt && !m.win);
        assert_eq!(a("Control+Alt+R"), a("ctrl + option + r"));
        assert!(a("Super+Q").mods.win && a("Meta+Q").mods.win);
        assert_eq!(a("Alt+Up").vk, 0x26);
    }

    #[test]
    fn parses_numpad() {
        assert_eq!(a("num0").vk, 0x60);
        assert_eq!(a("Numpad9").vk, 0x69);
        assert_eq!(a("numadd").vk, 0x6B);
        assert_eq!(a("Ctrl+NumDec").vk, 0x6E);
        assert!(parse("num10").is_err());
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse("").is_err());
        assert!(parse("Ctrl+Shift").is_err(), "modifiers only");
        assert!(parse("A+B").is_err(), "two keys");
        assert!(parse("Ctrl+;").is_err(), "punctuation unsupported");
        assert!(parse("Ctrl+Foo").is_err());
    }

    #[test]
    fn display_roundtrips() {
        for s in [
            "F8",
            "Ctrl+Shift+X",
            "Alt+Numpad3",
            "Ctrl+Alt+Super+PageDown",
            "Shift+ArrowLeft",
            "Ctrl+NumpadAdd",
        ] {
            assert_eq!(a(s).to_string(), s);
            assert_eq!(a(&a(s).to_string()), a(s));
        }
        assert_eq!(a("control+esc").to_string(), "Ctrl+Escape");
    }

    #[test]
    fn validate_policy() {
        assert!(parse_valid("F8").is_ok());
        assert!(parse_valid("Num5").is_ok());
        assert!(parse_valid("Home").is_ok());
        assert!(parse_valid("Ctrl+A").is_ok());
        for bad in ["A", "Space", "Enter", "Escape", "9"] {
            assert!(parse_valid(bad).is_err(), "{bad} should need a modifier");
        }
        for bad in [
            "Alt+F4",
            "Alt+Tab",
            "Ctrl+Esc",
            "Ctrl+Shift+Esc",
            "Ctrl+Alt+Delete",
            "Win+L",
        ] {
            assert!(parse_valid(bad).is_err(), "{bad} is reserved");
        }
    }
}
