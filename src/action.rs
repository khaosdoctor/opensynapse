//! What a button does, and its on-wire encoding (see devices/protocols/).

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Disabled,
    Mouse(u8),
    Key { mods: u8, usage: u8 },
    Dpi(u8),
    Profile(u8),
    /// Repeats a mouse button. Decoded from the wheel-tilt defaults; rate unit unknown.
    MouseTurbo { button: u8, rate: u16 },
    /// Anything not decoded yet, kept byte-exact so it can be written back.
    Raw(Vec<u8>),
}

impl Action {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Action::Disabled => vec![0x00, 0x00],
            Action::Mouse(b) => vec![0x01, 0x01, *b],
            // second byte: 0x02 verified on write, 0x01 also seen on reads
            Action::Key { mods, usage } => vec![0x02, 0x02, *mods, *usage],
            Action::Dpi(n) => vec![0x06, 0x01, *n],
            Action::Profile(n) => vec![0x07, 0x01, *n],
            Action::MouseTurbo { button, rate } => {
                let [hi, lo] = rate.to_be_bytes();
                vec![0x0e, 0x01, *button, hi, lo]
            }
            Action::Raw(b) => b.clone(),
        }
    }

    pub fn decode(b: &[u8]) -> Action {
        let at = |i: usize| b.get(i).copied().unwrap_or(0);
        match at(0) {
            0x00 => Action::Disabled,
            0x01 => Action::Mouse(at(2)),
            0x02 => Action::Key { mods: at(2), usage: at(3) },
            0x06 => Action::Dpi(at(2)),
            0x07 => Action::Profile(at(2)),
            0x0e => Action::MouseTurbo { button: at(2), rate: u16::from_be_bytes([at(3), at(4)]) },
            _ => {
                let end = b.iter().rposition(|&x| x != 0).map_or(0, |i| i + 1);
                Action::Raw(b[..end].to_vec())
            }
        }
    }
}

const MOUSE: &[(&str, u8)] = &[("left", 0x01), ("right", 0x02), ("middle", 0x03), ("back", 0x04), ("forward", 0x05), ("wheelup", 0x09), ("wheeldown", 0x0a)];
const MODS: &[(&str, u8)] = &[("ctrl", 0x01), ("shift", 0x02), ("alt", 0x04), ("meta", 0x08), ("rctrl", 0x10), ("rshift", 0x20), ("ralt", 0x40), ("rmeta", 0x80)];
const DPI: &[(&str, u8)] = &[("up", 0x01), ("down", 0x02)];

/// HID keyboard usage names (usage page 0x07).
pub fn key_name(usage: u8) -> Option<String> {
    let named = match usage {
        0x04..=0x1d => return Some(((b'a' + usage - 0x04) as char).to_string()),
        0x1e..=0x26 => return Some((usage - 0x1d).to_string()),
        0x27 => "0",
        0x3a..=0x45 => return Some(format!("f{}", usage - 0x39)),
        0x68..=0x73 => return Some(format!("f{}", usage - 0x68 + 13)),
        0x28 => "enter",
        0x29 => "esc",
        0x2a => "backspace",
        0x2b => "tab",
        0x2c => "space",
        0x2d => "minus",
        0x2e => "equal",
        0x2f => "leftbrace",
        0x30 => "rightbrace",
        0x31 => "backslash",
        0x33 => "semicolon",
        0x34 => "apostrophe",
        0x35 => "grave",
        0x36 => "comma",
        0x37 => "dot",
        0x38 => "slash",
        0x39 => "capslock",
        0x46 => "sysrq",
        0x47 => "scrolllock",
        0x48 => "pause",
        0x49 => "insert",
        0x4a => "home",
        0x4b => "pageup",
        0x4c => "delete",
        0x4d => "end",
        0x4e => "pagedown",
        0x4f => "right",
        0x50 => "left",
        0x51 => "down",
        0x52 => "up",
        0x65 => "menu",
        0x7f => "mute",
        0x80 => "volumeup",
        0x81 => "volumedown",
        _ => return None,
    };
    Some(named.to_string())
}

/// Symbols as typed on a US layout, so `key:[` works as well as `key:leftbrace`.
const SYMBOLS: &[(&str, u8)] = &[
    ("-", 0x2d),
    ("=", 0x2e),
    ("[", 0x2f),
    ("]", 0x30),
    ("\\", 0x31),
    (";", 0x33),
    ("'", 0x34),
    ("`", 0x35),
    (",", 0x36),
    (".", 0x37),
    ("/", 0x38),
];

// ponytail: linear scan over 256 usages, fine for a handful of lookups
pub fn key_usage(name: &str) -> Option<u8> {
    let name = name.to_lowercase();
    lookup(SYMBOLS, &name).or_else(|| (0..=255u8).find(|&u| key_name(u).as_deref() == Some(name.as_str())))
}

fn lookup(table: &[(&str, u8)], name: &str) -> Option<u8> {
    table.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn rlookup(table: &[(&str, u8)], value: u8) -> String {
    table.iter().find(|(_, v)| *v == value).map_or(format!("0x{value:02x}"), |(n, _)| n.to_string())
}

fn parse_u8(s: &str) -> Option<u8> {
    match s.strip_prefix("0x") {
        Some(hex) => u8::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Action::Disabled => write!(f, "disabled"),
            Action::Mouse(b) => write!(f, "mouse:{}", rlookup(MOUSE, *b)),
            Action::Key { mods, usage } => {
                write!(f, "key:")?;
                for (name, bit) in MODS {
                    if mods & bit != 0 {
                        write!(f, "{name}+")?;
                    }
                }
                match key_name(*usage) {
                    Some(n) => write!(f, "{n}"),
                    None => write!(f, "0x{usage:02x}"),
                }
            }
            Action::Dpi(n) => write!(f, "dpi:{}", rlookup(DPI, *n)),
            Action::Profile(n) => write!(f, "profile:0x{n:02x}"),
            Action::MouseTurbo { button, rate } => write!(f, "turbo:{}:{rate}", rlookup(MOUSE, *button)),
            Action::Raw(b) => {
                write!(f, "raw:")?;
                b.iter().try_for_each(|x| write!(f, "{x:02x}"))
            }
        }
    }
}

/// Parses the same syntax Display prints: `mouse:left`, `key:ctrl+shift+1`,
/// `dpi:up`, `profile:0x04`, `turbo:left:20`, `raw:0e010a0014`, `disabled`.
impl FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("can't parse action '{s}'");
        let (kind, rest) = s.split_once(':').unwrap_or((s, ""));
        match kind {
            "disabled" => Ok(Action::Disabled),
            "mouse" => lookup(MOUSE, rest).or_else(|| parse_u8(rest)).map(Action::Mouse).ok_or_else(bad),
            "key" => {
                let mut parts: Vec<&str> = rest.split('+').collect();
                let key = parts.pop().ok_or_else(bad)?;
                let mut mods = 0;
                for m in parts {
                    mods |= lookup(MODS, m).ok_or_else(bad)?;
                }
                let usage = key_usage(key).or_else(|| parse_u8(key)).ok_or_else(bad)?;
                Ok(Action::Key { mods, usage })
            }
            "dpi" => lookup(DPI, rest).map(Action::Dpi).ok_or_else(bad),
            "profile" => parse_u8(rest).map(Action::Profile).ok_or_else(bad),
            "turbo" => {
                let (btn, rate) = rest.split_once(':').ok_or_else(bad)?;
                let button = lookup(MOUSE, btn).or_else(|| parse_u8(btn)).ok_or_else(bad)?;
                Ok(Action::MouseTurbo { button, rate: rate.parse().map_err(|_| bad())? })
            }
            "raw" => (0..rest.len())
                .step_by(2)
                .map(|i| rest.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
                .collect::<Option<Vec<u8>>>()
                .map(Action::Raw)
                .ok_or_else(bad),
            _ => Err(bad()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_matches_device_bytes() {
        // bytes read from a real Naga Left-Handed 2020
        let cases: &[(&[u8], &str)] = &[
            (&[0x02, 0x02, 0x00, 0x1e], "key:1"),
            (&[0x01, 0x01, 0x02], "mouse:right"),
            (&[0x06, 0x01, 0x01], "dpi:up"),
            (&[0x0e, 0x01, 0x0a, 0x00, 0x14], "turbo:wheeldown:20"),
        ];
        for (bytes, text) in cases {
            let a = Action::decode(bytes);
            assert_eq!(a.to_string(), *text);
            assert_eq!(text.parse::<Action>().unwrap(), a);
        }
        let ctrl1: Action = "key:ctrl+1".parse().unwrap();
        assert_eq!(ctrl1.encode(), vec![0x02, 0x02, 0x01, 0x1e]);
        assert_eq!("raw:0b0c".parse::<Action>().unwrap().encode(), vec![0x0b, 0x0c]);
        assert_eq!(key_usage("["), Some(0x2f));
        assert_eq!(key_usage("F13"), Some(0x68));
        assert_eq!(key_usage("volumeup"), Some(0x80));
    }
}
