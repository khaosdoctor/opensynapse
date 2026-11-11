//! Device definitions (built-in devices/*.jsonc plus ~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc) and the commands we know how to send.

use crate::action::Action;
use crate::hid::{self, Hid};
use crate::rgb::Effect;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};
use std::{env, fs, io};

/// (file name without .jsonc, contents), one line per device.
/// tools/device_from_issue.py adds new ones before the closing `];`.
const BUILTIN: &[(&str, &str)] = &[
    ("razer-naga-left-handed-2020", include_str!("../devices/razer-naga-left-handed-2020.jsonc")),
    ("razer-naga-x", include_str!("../devices/razer-naga-x.jsonc")),
];

/// Device files are JSONC: cuts `//` comments outside strings, line by line
/// so serde's line numbers still match. Block comments aren't supported.
pub fn strip_comments(src: &str) -> String {
    let strip = |line: &'_ str| -> String {
        let mut in_str = false;
        let mut prev = ' ';
        for (i, c) in line.char_indices() {
            if c == '"' && prev != '\\' {
                in_str = !in_str;
            }
            if !in_str && line[i..].starts_with("//") {
                return line[..i].to_string();
            }
            prev = c;
        }
        line.to_string()
    };
    src.lines().map(strip).collect::<Vec<_>>().join("\n")
}

/// Ids in the device files are hex strings ("0x40"), like the protocol docs.
fn hex<'de, D: Deserializer<'de>, T: TryFrom<u64>>(d: D) -> Result<T, D::Error> {
    let s = String::deserialize(d)?;
    u64::from_str_radix(s.trim_start_matches("0x"), 16)
        .ok()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| serde::de::Error::custom(format!("bad hex id '{s}'")))
}

const VARSTORE: u8 = 0x01;
const CLASS_DPI: u8 = 0x04;
const CLASS_KEYMAP: u8 = 0x02;
const CLASS_LIGHT: u8 = 0x0f;

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceDef {
    pub name: String,
    #[serde(deserialize_with = "hex")]
    pub vid: u16,
    #[serde(deserialize_with = "hex")]
    pub pid: u16,
    pub interface: u8,
    #[serde(deserialize_with = "hex")]
    pub transaction_id: u8,
    pub profiles: Vec<u8>,
    pub layers: Vec<String>,
    pub dpi_max: u16,
    pub buttons: Vec<Button>,
    /// File name without .jsonc; the protocol file uses the same name.
    #[serde(skip)]
    pub file: String,
    /// The JSON text this was parsed from, for `protocol send`.
    #[serde(skip)]
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Button {
    #[serde(deserialize_with = "hex")]
    pub id: u8,
    pub name: String,
    /// Optional [x, y] in percent of the mouse view. The mouse body is drawn at
    /// x 30..70, y 2..74; buttons left of x 30 or right of x 70 get a side plate. Devices where no button has `at` get a plain grid.
    pub at: Option<[f32; 2]>,
}

impl DeviceDef {
    pub fn button(&self, name: &str) -> Option<&Button> {
        self.buttons.iter().find(|b| b.name == name)
    }

    pub fn layer(&self, name: &str) -> Option<u8> {
        self.layers.iter().position(|l| l == name).map(|i| i as u8)
    }
}

/// Built-in definitions plus ~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc (user files win).
pub fn definitions() -> Vec<DeviceDef> {
    let mut defs: Vec<DeviceDef> = Vec::new();
    let mut add = |src: &str, file: &str, origin: &str| match serde_json::from_str::<DeviceDef>(&strip_comments(src)) {
        Ok(mut d) => {
            // user files are all "buttons.jsonc", so their repo name comes from the mouse name
            d.file = if file.is_empty() { crate::scaffold::slug(&d.name) } else { file.to_string() };
            d.source = src.to_string();
            defs.retain(|x| (x.vid, x.pid) != (d.vid, d.pid));
            defs.push(d);
        }
        Err(e) => eprintln!("skipping device definition {origin}: {e}"),
    };
    for (file, src) in BUILTIN {
        add(src, file, "(builtin)");
    }
    let entries = devices_dir().and_then(|d| fs::read_dir(d).ok()).into_iter().flatten().flatten();
    for path in entries.map(|e| e.path().join("buttons.jsonc")) {
        let Ok(src) = fs::read_to_string(&path) else { continue };
        add(&src, "", &path.display().to_string());
    }
    defs
}

/// ~/.config/opensynapse/devices/<vid-pid> (like 1532-008d) for one mouse model.
pub fn model_dir(vid: u16, pid: u16) -> Option<PathBuf> {
    devices_dir().map(|d| d.join(format!("{vid:04x}-{pid:04x}")))
}

/// <serial>-<name> in the model folder, or plain <name> when the mouse doesn't report a serial.
pub fn model_file(vid: u16, pid: u16, serial: &str, name: &str) -> Option<PathBuf> {
    let name = if serial.is_empty() { name.to_string() } else { format!("{serial}-{name}") };
    model_dir(vid, pid).map(|d| d.join(name))
}

/// One folder per model with its saved profiles and an optional buttons.jsonc,
/// which wins over a built-in definition.
fn devices_dir() -> Option<PathBuf> {
    config_dir().map(|c| c.join("opensynapse/devices"))
}

/// $XDG_CONFIG_HOME, or ~/.config.
pub fn config_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

/// Writes a file, creating its folder first.
pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents)
}

pub fn save_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    write(path, serde_json::to_string_pretty(value).map_err(io::Error::other)?)
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    serde_json::from_str(&fs::read_to_string(path)?).map_err(io::Error::other)
}

pub struct Mouse {
    pub def: DeviceDef,
    hid: Hid,
    /// factory serial read at detection, empty if the device doesn't report one
    pub serial: String,
}

pub struct DpiStages {
    pub active: u8,
    pub stages: Vec<u16>,
}

/// Every connected mouse that has a definition.
pub fn detect() -> io::Result<Vec<Mouse>> {
    let mut found = Vec::new();
    for def in definitions() {
        let Some(path) = hid::find(def.vid, def.pid, def.interface) else { continue };
        let hid = Hid::open(&path, def.transaction_id).map_err(|e| {
            let denied = e.kind() == io::ErrorKind::PermissionDenied;
            let hint = if denied { " > install udev/70-opensynapse.rules (see README)" } else { "" };
            io::Error::new(e.kind(), format!("{}: {e}{hint}", path.display()))
        })?;
        let mut mouse = Mouse { def, hid, serial: String::new() };
        mouse.serial = mouse.read_serial().unwrap_or_default();
        found.push(mouse);
    }
    Ok(found)
}

impl Mouse {
    pub fn get_button(&self, profile: u8, button: u8, layer: u8) -> io::Result<Action> {
        let r = self.hid.command(CLASS_KEYMAP, 0x8c, 0x0a, &[profile, button, layer])?;
        Ok(Action::decode(&r[11..18]))
    }

    pub fn set_button(&self, profile: u8, button: u8, layer: u8, action: &Action) -> io::Result<()> {
        let mut args = vec![profile, button, layer];
        args.extend(action.encode());
        if args.len() > 10 {
            return Err(io::Error::other("action too long"));
        }
        self.hid.command(CLASS_KEYMAP, 0x0c, 0x0a, &args)?;
        // read back: a write that "succeeds" but doesn't stick is worse than an error
        let got = self.get_button(profile, button, layer)?;
        if got.encode() != action.encode() {
            return Err(io::Error::other(format!("wrote {action}, device reports {got}")));
        }
        Ok(())
    }

    /// Factory serial, fixed per physical unit (survives replugs and renames).
    fn read_serial(&self) -> io::Result<String> {
        let r = self.hid.command(0x00, 0x82, 0x16, &[])?;
        // ends up in file names and D-Bus paths, so nothing like "../"
        Ok(String::from_utf8_lossy(&r[8..8 + 0x16]).chars().filter(char::is_ascii_alphanumeric).collect())
    }

    pub fn firmware(&self) -> io::Result<String> {
        let r = self.hid.command(0x00, 0x81, 0x02, &[])?;
        Ok(format!("v{}.{}", r[8], r[9]))
    }

    pub fn dpi(&self) -> io::Result<(u16, u16)> {
        let r = self.hid.command(CLASS_DPI, 0x85, 0x07, &[0x00])?;
        Ok((u16::from_be_bytes([r[9], r[10]]), u16::from_be_bytes([r[11], r[12]])))
    }

    pub fn set_dpi(&self, x: u16, y: u16) -> io::Result<()> {
        self.check_dpi(&[x, y])?;
        let ([xh, xl], [yh, yl]) = (x.to_be_bytes(), y.to_be_bytes());
        self.hid.command(CLASS_DPI, 0x05, 0x07, &[VARSTORE, xh, xl, yh, yl, 0, 0])?;
        Ok(())
    }

    pub fn dpi_stages(&self) -> io::Result<DpiStages> {
        let r = self.hid.command(CLASS_DPI, 0x86, 0x26, &[0x00])?;
        let count = r[10] as usize;
        // each stage: id, x(2), y(2), 0, 0 starting at byte 11
        let stages = (0..count.min(5))
            .map(|i| u16::from_be_bytes([r[12 + i * 7], r[13 + i * 7]]))
            .collect();
        Ok(DpiStages { active: r[9], stages })
    }

    pub fn set_dpi_stages(&self, s: &DpiStages) -> io::Result<()> {
        self.check_dpi(&s.stages)?;
        if s.stages.is_empty() || s.stages.len() > 5 || s.active == 0 || s.active as usize > s.stages.len() {
            return Err(io::Error::other("need 1-5 stages and an active stage in range"));
        }
        let mut args = vec![VARSTORE, s.active, s.stages.len() as u8];
        for (i, dpi) in s.stages.iter().enumerate() {
            let [h, l] = dpi.to_be_bytes();
            args.extend([i as u8 + 1, h, l, h, l, 0, 0]);
        }
        self.hid.command(CLASS_DPI, 0x06, 0x26, &args)?;
        Ok(())
    }

    /// Effect and brightness of one LED zone, read from the mouse (verified on
    /// the Naga: static green read back as `01 .. 01 00 ff 00`). The reply uses
    /// the layout OpenRazer writes in razerchromacommon.c:
    /// [store, led, effect, direction/type, speed, colour count, r, g, b].
    pub fn led(&self, led: u8) -> io::Result<(Effect, u8)> {
        let r = self.hid.command(CLASS_LIGHT, 0x82, 0x0c, &[VARSTORE, led])?;
        let b = self.hid.command(CLASS_LIGHT, 0x84, 0x03, &[VARSTORE, led])?;
        Ok((Effect::from_reply(&r[8..]), b[10]))
    }

    fn check_dpi(&self, values: &[u16]) -> io::Result<()> {
        if values.iter().any(|&v| v < 100 || v > self.def.dpi_max) {
            return Err(io::Error::other(format!("DPI must be 100..={}", self.def.dpi_max)));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Catches a broken device file before it's compiled into a release,
    /// including the ones tools/device_from_issue.py adds from issues.
    #[test]
    fn builtin_device_files_parse() {
        for (file, src) in BUILTIN {
            let def: DeviceDef = serde_json::from_str(&strip_comments(src)).unwrap_or_else(|e| panic!("{file}.jsonc: {e}"));
            assert!(!def.buttons.is_empty(), "{file}.jsonc has no buttons");
        }
        let naga = definitions().into_iter().find(|d| d.pid == 0x008d).expect("builtin naga");
        assert_eq!((naga.vid, naga.transaction_id, naga.buttons.len()), (0x1532, 0x1f, 22));
        assert_eq!(naga.button("thumb1").map(|b| b.id), Some(0x40));
    }

    #[test]
    fn jsonc_comments() {
        let src = "// head\n{ \"a\": \"x // \\\"y\", // note\n \"b\": 1 } // end";
        let v: serde_json::Value = serde_json::from_str(&strip_comments(src)).unwrap();
        assert_eq!((v["a"].as_str(), v["b"].as_u64()), (Some("x // \"y"), Some(1)));
    }
}
