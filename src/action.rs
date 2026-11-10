//! What a button does, and its on-wire encoding (see devices/protocols/).

use std::fmt;
use std::str::FromStr;

/// Every action is `[type, length of the data, data...]`. Types and payloads
/// marked "razerqdhid" come from geezmolycos/razerqdhid (Basilisk V3) and
/// haven't been seen working on the Naga yet; the rest were pressed and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Disabled,
    Mouse(u8),
    Key { mods: u8, usage: u8 },
    /// Repeats a key every `interval` ms.
    KeyTurbo { mods: u8, usage: u8, interval: u16 },
    /// HID consumer page usage: play/pause, next, previous...
    Media(u16),
    /// DPI step: 1 up, 2 down, 6 up with wrap, 7 down with wrap
    Dpi(u8),
    /// Jump straight to a DPI stage, 1-based (razerqdhid)
    DpiStage(u8),
    /// Profile step: 1 next, 2 previous, 4 next with wrap, 5 previous with wrap
    Profile(u8),
    /// Jump straight to a profile (razerqdhid)
    ProfileSet(u8),
    /// Repeats a mouse button every `interval` ms.
    MouseTurbo { button: u8, interval: u16 },
    /// Holding this button switches to the Hypershift layer
    Hypershift,
    /// Anything not decoded yet, kept byte-exact so it can be written back.
    Raw(Vec<u8>),
}

impl Action {
    pub fn encode(&self) -> Vec<u8> {
        match *self {
            Action::Disabled => vec![0x00, 0x00],
            Action::Mouse(b) => vec![0x01, 0x01, b],
            Action::Key { mods, usage } => vec![0x02, 0x02, mods, usage],
            Action::KeyTurbo { mods, usage, interval } => {
                let [hi, lo] = interval.to_be_bytes();
                vec![0x0d, 0x04, mods, usage, hi, lo]
            }
            Action::Media(usage) => {
                let [hi, lo] = usage.to_be_bytes();
                vec![0x0a, 0x02, hi, lo]
            }
            Action::Dpi(op) => vec![0x06, 0x01, op],
            Action::DpiStage(n) => vec![0x06, 0x02, 0x03, n],
            Action::Profile(op) => vec![0x07, 0x01, op],
            Action::ProfileSet(n) => vec![0x07, 0x02, 0x03, n],
            Action::MouseTurbo { button, interval } => {
                // the Naga's own tilt defaults read back with length 01, but
                // the data is 3 bytes and razerqdhid writes 03
                let [hi, lo] = interval.to_be_bytes();
                vec![0x0e, 0x03, button, hi, lo]
            }
            Action::Hypershift => vec![0x0c, 0x01, 0x01],
            Action::Raw(ref b) => b.clone(),
        }
    }

    /// Decodes by type. The length byte is ignored on purpose: the Naga reads
    /// back lengths that don't match the data (`02 01 00 1e`, `0e 01 ...`).
    pub fn decode(b: &[u8]) -> Action {
        let at = |i: usize| b.get(i).copied().unwrap_or(0);
        let u16_at = |i: usize| u16::from_be_bytes([at(i), at(i + 1)]);
        match (at(0), at(1), at(2)) {
            (0x00, ..) => Action::Disabled,
            (0x01, ..) => Action::Mouse(at(2)),
            (0x02, ..) => Action::Key { mods: at(2), usage: at(3) },
            (0x0d, ..) => Action::KeyTurbo { mods: at(2), usage: at(3), interval: u16_at(4) },
            (0x0a, ..) => Action::Media(u16_at(2)),
            (0x06, 0x02, 0x03) => Action::DpiStage(at(3)),
            (0x06, ..) => Action::Dpi(at(2)),
            (0x07, 0x02, 0x03) => Action::ProfileSet(at(3)),
            (0x07, ..) => Action::Profile(at(2)),
            (0x0e, ..) => Action::MouseTurbo { button: at(2), interval: u16_at(3) },
            (0x0c, _, 0x01) => Action::Hypershift,
            _ => {
                let end = b.iter().rposition(|&x| x != 0).map_or(0, |i| i + 1);
                Action::Raw(b[..end].to_vec())
            }
        }
    }
}

pub const MOUSE: &[(&str, u8)] = &[
    ("left", 0x01),
    ("right", 0x02),
    ("middle", 0x03),
    ("back", 0x04),
    ("forward", 0x05),
    ("wheelup", 0x09),
    ("wheeldown", 0x0a),
    ("wheelleft", 0x68),
    ("wheelright", 0x69),
];
pub const MODS: &[(&str, u8)] = &[("ctrl", 0x01), ("shift", 0x02), ("alt", 0x04), ("meta", 0x08), ("rctrl", 0x10), ("rshift", 0x20), ("ralt", 0x40), ("rmeta", 0x80)];
const DPI: &[(&str, u8)] = &[("up", 0x01), ("down", 0x02), ("up-wrap", 0x06), ("down-wrap", 0x07)];
const PROFILE: &[(&str, u8)] = &[("next", 0x01), ("prev", 0x02), ("next-wrap", 0x04), ("prev-wrap", 0x05)];
/// HID consumer page usages (0x0C).
const MEDIA: &[(&str, u16)] = &[
    ("playpause", 0xcd),
    ("play", 0xb0),
    ("pause", 0xb1),
    ("next", 0xb5),
    ("prev", 0xb6),
    ("stop", 0xb7),
    ("mute", 0xe2),
    ("volumeup", 0xe9),
    ("volumedown", 0xea),
];

/// HID keyboard usage names (usage page 0x07).
pub fn key_name(usage: u8) -> Option<String> {
    let named = match usage {
        0x04..=0x1d => return Some(((b'a' + usage - 0x04) as char).to_string()),
        0x1e..=0x26 => return Some((usage - 0x1d).to_string()),
        0x27 => "0",
        0x3a..=0x45 => return Some(format!("f{}", usage - 0x39)),
        0x68..=0x73 => return Some(format!("f{}", usage - 0x68 + 13)),
        0x59..=0x61 => return Some(format!("kp{}", usage - 0x58)),
        0x62 => "kp0",
        0x53 => "numlock",
        0x54 => "kpslash",
        0x55 => "kpasterisk",
        0x56 => "kpminus",
        0x57 => "kpplus",
        0x58 => "kpenter",
        0x63 => "kpdot",
        // modifiers as plain keys, so a button can just hold Shift (untested on hardware)
        0xe0 => "leftctrl",
        0xe1 => "leftshift",
        0xe2 => "leftalt",
        0xe3 => "leftmeta",
        0xe4 => "rightctrl",
        0xe5 => "rightshift",
        0xe6 => "rightalt",
        0xe7 => "rightmeta",
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

// linear scan over 256 usages, fine for a handful of lookups
pub fn key_usage(name: &str) -> Option<u8> {
    let name = name.to_lowercase();
    lookup(SYMBOLS, &name).or_else(|| (0..=255u8).find(|&u| key_name(u).as_deref() == Some(name.as_str())))
}

fn lookup<T: Copy>(table: &[(&str, T)], name: &str) -> Option<T> {
    table.iter().find(|(n, _)| *n == name).map(|(_, v)| *v)
}

fn rlookup<T: Copy + PartialEq + fmt::LowerHex>(table: &[(&str, T)], value: T) -> String {
    table.iter().find(|(_, v)| *v == value).map_or(format!("0x{value:02x}"), |(n, _)| n.to_string())
}

fn parse_u8(s: &str) -> Option<u8> {
    match s.strip_prefix("0x") {
        Some(hex) => u8::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

/// "ff00aa" -> [255, 0, 170], None on odd length or non-hex.
pub fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect()
}

/// `ctrl+shift+a` style text for a modifier mask and a key.
fn key_text(mods: u8, usage: u8) -> String {
    let mut s: String = MODS.iter().filter(|(_, bit)| mods & bit != 0).map(|(name, _)| format!("{name}+")).collect();
    s += &key_name(usage).unwrap_or(format!("0x{usage:02x}"));
    s
}

fn parse_key(text: &str) -> Option<(u8, u8)> {
    let mut parts: Vec<&str> = text.split('+').collect();
    let key = parts.pop()?;
    let mut mods = 0;
    for m in parts {
        mods |= lookup(MODS, m)?;
    }
    Some((mods, key_usage(key).or_else(|| parse_u8(key))?))
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Action::Disabled => write!(f, "disabled"),
            Action::Mouse(b) => write!(f, "mouse:{}", rlookup(MOUSE, *b)),
            Action::Key { mods, usage } => write!(f, "key:{}", key_text(*mods, *usage)),
            Action::KeyTurbo { mods, usage, interval } => write!(f, "keyturbo:{}:{interval}", key_text(*mods, *usage)),
            Action::Media(u) => write!(f, "media:{}", rlookup(MEDIA, *u)),
            Action::Dpi(op) => write!(f, "dpi:{}", rlookup(DPI, *op)),
            Action::DpiStage(n) => write!(f, "dpi:stage{n}"),
            Action::Profile(op) => write!(f, "profile:{}", rlookup(PROFILE, *op)),
            Action::ProfileSet(n) => write!(f, "profile:{n}"),
            Action::MouseTurbo { button, interval } => write!(f, "turbo:{}:{interval}", rlookup(MOUSE, *button)),
            Action::Hypershift => write!(f, "hypershift"),
            Action::Raw(b) => {
                write!(f, "raw:")?;
                b.iter().try_for_each(|x| write!(f, "{x:02x}"))
            }
        }
    }
}

/// Parses the same syntax Display prints: `mouse:left`, `key:ctrl+shift+1`,
/// `keyturbo:a:50`, `media:playpause`, `dpi:up`, `dpi:stage2`, `profile:next`,
/// `profile:3`, `turbo:left:20`, `hypershift`, `raw:0e010a0014`, `disabled`.
/// Intervals are milliseconds between repeats.
impl FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("can't parse action '{s}'");
        let (kind, rest) = s.split_once(':').unwrap_or((s, ""));
        match kind {
            "disabled" => Ok(Action::Disabled),
            "hypershift" => Ok(Action::Hypershift),
            "mouse" => lookup(MOUSE, rest).or_else(|| parse_u8(rest)).map(Action::Mouse).ok_or_else(bad),
            "key" => parse_key(rest).map(|(mods, usage)| Action::Key { mods, usage }).ok_or_else(bad),
            "keyturbo" => {
                let (key, ms) = rest.rsplit_once(':').ok_or_else(bad)?;
                let (mods, usage) = parse_key(key).ok_or_else(bad)?;
                Ok(Action::KeyTurbo { mods, usage, interval: ms.parse().map_err(|_| bad())? })
            }
            "media" => lookup(MEDIA, rest)
                .or_else(|| u16::from_str_radix(rest.strip_prefix("0x")?, 16).ok())
                .map(Action::Media)
                .ok_or_else(bad),
            "dpi" => match rest.strip_prefix("stage") {
                Some(n) => n.parse().map(Action::DpiStage).map_err(|_| bad()),
                None => lookup(DPI, rest).map(Action::Dpi).ok_or_else(bad),
            },
            // `profile:0x04` is the old syntax (a raw step op), `profile:3` jumps to profile 3
            "profile" if rest.starts_with("0x") => parse_u8(rest).map(Action::Profile).ok_or_else(bad),
            "profile" => lookup(PROFILE, rest)
                .map(Action::Profile)
                .or_else(|| rest.parse().ok().map(Action::ProfileSet))
                .ok_or_else(bad),
            "turbo" => {
                let (btn, ms) = rest.split_once(':').ok_or_else(bad)?;
                let button = lookup(MOUSE, btn).or_else(|| parse_u8(btn)).ok_or_else(bad)?;
                Ok(Action::MouseTurbo { button, interval: ms.parse().map_err(|_| bad())? })
            }
            "raw" if !rest.is_empty() => hex_bytes(rest).map(Action::Raw).ok_or_else(bad),
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
            (&[0x02, 0x01, 0x00, 0x1e], "key:1"), // Hypershift layer reads back length 01
            (&[0x01, 0x01, 0x02], "mouse:right"),
            (&[0x06, 0x01, 0x01], "dpi:up"),
            (&[0x07, 0x01, 0x04], "profile:next-wrap"),
            (&[0x0e, 0x01, 0x0a, 0x00, 0x14], "turbo:wheeldown:20"),
        ];
        for (bytes, text) in cases {
            let a = Action::decode(bytes);
            assert_eq!(a.to_string(), *text);
            assert_eq!(text.parse::<Action>().unwrap(), a);
        }
        // razerqdhid encodings: text -> bytes -> same text
        let qd: &[(&str, &[u8])] = &[
            ("media:playpause", &[0x0a, 0x02, 0x00, 0xcd]),
            ("keyturbo:ctrl+a:50", &[0x0d, 0x04, 0x01, 0x04, 0x00, 0x32]),
            ("turbo:wheelup:100", &[0x0e, 0x03, 0x09, 0x00, 0x64]),
            ("dpi:stage2", &[0x06, 0x02, 0x03, 0x02]),
            ("profile:3", &[0x07, 0x02, 0x03, 0x03]),
            ("hypershift", &[0x0c, 0x01, 0x01]),
        ];
        for (text, bytes) in qd {
            let a: Action = text.parse().unwrap();
            assert_eq!(a.encode(), bytes.to_vec(), "{text}");
            assert_eq!(Action::decode(bytes).to_string(), *text);
        }
        // profile files saved before the rename still load
        assert_eq!("profile:0x04".parse::<Action>().unwrap(), Action::Profile(4));
        let ctrl1: Action = "key:ctrl+1".parse().unwrap();
        assert_eq!(ctrl1.encode(), vec![0x02, 0x02, 0x01, 0x1e]);
        assert_eq!("raw:0b0c".parse::<Action>().unwrap().encode(), vec![0x0b, 0x0c]);
        assert_eq!(key_usage("["), Some(0x2f));
        assert_eq!(key_usage("F13"), Some(0x68));
        assert_eq!(key_usage("volumeup"), Some(0x80));
        assert_eq!(key_usage("kp7"), Some(0x5f));
        assert_eq!(key_usage("kp0"), Some(0x62));
        assert_eq!(key_usage("leftshift"), Some(0xe1));
    }
}
