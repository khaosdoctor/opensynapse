//! Device definitions (devices/*.json) and the commands we know how to send.

use crate::action::Action;
use crate::hid::{self, Hid};
use serde::{Deserialize, Deserializer};
use std::{env, fs, io, path::PathBuf};

/// Built into the binary as (file name without .json, contents).
/// New devices: add a JSON file and a line here.
const BUILTIN: &[(&str, &str)] =
    &[("razer-naga-left-handed-2020", include_str!("../devices/razer-naga-left-handed-2020.json"))];

/// Ids in the device files are hex strings ("0x40") so they match the
/// protocol docs and probe output; plain numbers work too.
pub fn hex<'de, D: Deserializer<'de>, T: TryFrom<u64>>(d: D) -> Result<T, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Num(u64),
        Str(String),
    }
    let n = match Raw::deserialize(d)? {
        Raw::Num(n) => n,
        Raw::Str(s) => u64::from_str_radix(s.trim_start_matches("0x"), 16)
            .map_err(|_| serde::de::Error::custom(format!("bad hex '{s}'")))?,
    };
    T::try_from(n).map_err(|_| serde::de::Error::custom(format!("{n:#x} is too big")))
}

const VARSTORE: u8 = 0x01;
const CLASS_DPI: u8 = 0x04;
const CLASS_KEYMAP: u8 = 0x02;

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceDef {
    pub name: String,
    #[serde(deserialize_with = "hex")]
    pub vid: u16,
    #[serde(deserialize_with = "hex")]
    pub pid: u16,
    #[serde(default)]
    pub interface: u8,
    #[serde(default = "default_tid", deserialize_with = "hex")]
    pub transaction_id: u8,
    pub profiles: Vec<u8>,
    pub layers: Vec<String>,
    pub dpi_max: u16,
    pub buttons: Vec<Button>,
    /// File name without .json; the protocol file uses the same name.
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
    #[serde(default)]
    pub group: String,
    /// Optional [x, y] in percent of the mouse view. The mouse body is drawn at
    /// x 30..70, y 2..74; anything outside it gets a side plate. Devices where no button has `at` get a plain grid.
    #[serde(default)]
    pub at: Option<[f32; 2]>,
}

fn default_tid() -> u8 {
    0x1f
}

impl DeviceDef {
    pub fn button(&self, name: &str) -> Option<&Button> {
        self.buttons.iter().find(|b| b.name == name)
    }

    pub fn layer(&self, name: &str) -> Option<u8> {
        self.layers.iter().position(|l| l == name).map(|i| i as u8)
    }
}

/// Built-in definitions plus ~/.config/opensynapse/devices/<vid-pid>/buttons.json (user files win).
pub fn definitions() -> Vec<DeviceDef> {
    let mut defs: Vec<DeviceDef> = Vec::new();
    let mut add = |src: &str, file: &str, origin: &str| match serde_json::from_str::<DeviceDef>(src) {
        Ok(mut d) => {
            d.file = file.to_string();
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
    for path in entries.map(|e| e.path().join("buttons.json")) {
        let Ok(src) = fs::read_to_string(&path) else { continue };
        // the repo file name comes from the mouse name, "buttons" says nothing
        let file = serde_json::from_str::<DeviceDef>(&src).map_or(String::new(), |d| crate::scaffold::slug(&d.name));
        add(&src, &file, &path.display().to_string());
    }
    defs
}

/// Folder name for a mouse model: its USB id, "1532-008d".
pub fn storage_id(vid: u16, pid: u16) -> String {
    format!("{vid:04x}-{pid:04x}")
}

/// ~/.config/opensynapse/devices: one folder per model (see `storage_id`) with
/// its saved profiles and, for mice without a built-in definition, buttons.json.
pub fn devices_dir() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("opensynapse/devices"))
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
            let hint = if e.kind() == io::ErrorKind::PermissionDenied {
                " > install udev/70-opensynapse.rules (see README)"
            } else {
                ""
            };
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
        Ok(String::from_utf8_lossy(&r[8..8 + 0x16]).trim_end_matches('\0').trim().to_string())
    }

    /// Folder name for this model's saved profiles and device file.
    pub fn storage_id(&self) -> String {
        storage_id(self.def.vid, self.def.pid)
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

    fn check_dpi(&self, values: &[u16]) -> io::Result<()> {
        if values.iter().any(|&v| v < 100 || v > self.def.dpi_max) {
            return Err(io::Error::other(format!("DPI must be 100..={}", self.def.dpi_max)));
        }
        Ok(())
    }
}
