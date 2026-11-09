//! Save a device profile to JSON and load it back. Actions use the CLI syntax,
//! so files are hand-editable: `"thumb1": "key:ctrl+1"`.

use crate::action::Action;
use crate::device::{DpiStages, Mouse};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileFile {
    pub device: String,
    pub vid: u16,
    pub pid: u16,
    /// Identifies the physical unit; informational, not required to match on import.
    #[serde(default)]
    pub serial: String,
    #[serde(default)]
    pub firmware: String,
    /// profile slot this was read from
    #[serde(default)]
    pub profile: u8,
    pub dpi: u16,
    pub dpi_stages: Vec<u16>,
    pub active_stage: u8,
    /// layer name -> button name -> action
    pub layers: BTreeMap<String, BTreeMap<String, String>>,
}

pub fn read(mouse: &Mouse, profile: u8) -> io::Result<ProfileFile> {
    let def = &mouse.def;
    let mut layers = BTreeMap::new();
    for (i, layer) in def.layers.iter().enumerate() {
        let mut buttons = BTreeMap::new();
        for b in &def.buttons {
            buttons.insert(b.name.clone(), mouse.get_button(profile, b.id, i as u8)?.to_string());
        }
        layers.insert(layer.clone(), buttons);
    }
    let stages = mouse.dpi_stages()?;
    Ok(ProfileFile {
        device: def.name.clone(),
        vid: def.vid,
        pid: def.pid,
        serial: mouse.serial.clone(),
        firmware: mouse.firmware().unwrap_or_default(),
        profile,
        dpi: mouse.dpi()?.0,
        dpi_stages: stages.stages,
        active_stage: stages.active,
        layers,
    })
}

/// The button writes a file would make: (layer, button id, action), only for
/// buttons that differ from the device, so an unknown flag byte on untouched
/// keys is preserved. Everything is parsed up front so a typo writes nothing.
pub fn plan(mouse: &Mouse, profile: u8, file: &ProfileFile) -> io::Result<Vec<(u8, u8, Action)>> {
    let def = &mouse.def;
    if (file.vid, file.pid) != (def.vid, def.pid) {
        return Err(io::Error::other(format!("profile is for {} ({:04x}:{:04x})", file.device, file.vid, file.pid)));
    }
    let mut writes = Vec::new();
    for (layer, buttons) in &file.layers {
        let l = def.layer(layer).ok_or(io::Error::other(format!("unknown layer '{layer}'")))?;
        for (name, action) in buttons {
            let b = def.button(name).ok_or(io::Error::other(format!("unknown button '{name}'")))?;
            let a: Action = action.parse().map_err(io::Error::other)?;
            writes.push((l, b.id, a));
        }
    }
    let mut changed = Vec::new();
    for (layer, button, action) in writes {
        if mouse.get_button(profile, button, layer)? != action {
            changed.push((layer, button, action));
        }
    }
    Ok(changed)
}

/// Writes the file to the device. Returns buttons written.
pub fn apply(mouse: &Mouse, profile: u8, file: &ProfileFile) -> io::Result<usize> {
    let writes = plan(mouse, profile, file)?;
    for (layer, button, action) in &writes {
        mouse.set_button(profile, *button, *layer, action)?;
    }
    let written = writes.len();
    mouse.set_dpi(file.dpi, file.dpi)?;
    mouse.set_dpi_stages(&DpiStages { active: file.active_stage, stages: file.dpi_stages.clone() })?;
    Ok(written)
}

/// ~/.config/opensynapse/devices/<vid-pid>/<profile>.json
pub fn default_path(mouse: &Mouse, profile: u8) -> Option<PathBuf> {
    crate::device::devices_dir().map(|d| d.join(mouse.storage_id()).join(format!("{profile}.json")))
}

/// Creates the device's profile folder and saves any profile that has no file
/// yet, so the first run doubles as a backup of what's on the mouse.
/// Returns how many files were created.
pub fn ensure_defaults(mouse: &Mouse) -> io::Result<usize> {
    let mut created = 0;
    for &p in &mouse.def.profiles {
        let Some(path) = default_path(mouse, p) else { continue };
        if path.exists() {
            continue;
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        save(&read(mouse, p)?, &path)?;
        created += 1;
    }
    Ok(created)
}

pub fn save(file: &ProfileFile, path: &Path) -> io::Result<()> {
    fs::write(path, serde_json::to_string_pretty(file).map_err(io::Error::other)?)
}

pub fn load(path: &Path) -> io::Result<ProfileFile> {
    serde_json::from_str(&fs::read_to_string(path)?).map_err(io::Error::other)
}
