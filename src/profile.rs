//! Save a device profile to JSON and load it back. Actions use the CLI syntax,
//! so files are hand-editable: `"thumb1": "key:ctrl+1"`.

use crate::action::Action;
use crate::device::{self, DpiStages, Mouse};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileFile {
    pub device: String,
    pub vid: u16,
    pub pid: u16,
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

fn read(mouse: &Mouse, profile: u8) -> io::Result<ProfileFile> {
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
        firmware: mouse.firmware().unwrap_or_default(),
        profile,
        dpi: mouse.dpi()?.0,
        dpi_stages: stages.stages,
        active_stage: stages.active,
        layers,
    })
}

/// The button writes a file would make, as (layer, button index, action), only for
/// buttons that differ from the device, so an unknown flag byte on untouched
/// keys is preserved. Nothing is written here, so a typo writes nothing.
pub fn plan(mouse: &Mouse, profile: u8, file: &ProfileFile) -> io::Result<Vec<(u8, usize, Action)>> {
    let def = &mouse.def;
    if (file.vid, file.pid) != (def.vid, def.pid) {
        return Err(io::Error::other(format!("profile is for {} ({:04x}:{:04x})", file.device, file.vid, file.pid)));
    }
    let mut changed = Vec::new();
    for (layer, buttons) in &file.layers {
        let l = def.layer(layer).ok_or(io::Error::other(format!("unknown layer '{layer}'")))?;
        for (name, action) in buttons {
            let i = def.buttons.iter().position(|b| b.name == *name).ok_or(io::Error::other(format!("unknown button '{name}'")))?;
            let a: Action = action.parse().map_err(io::Error::other)?;
            if mouse.get_button(profile, def.buttons[i].id, l)? != a {
                changed.push((l, i, a));
            }
        }
    }
    Ok(changed)
}

/// Writes the file to the device. Returns buttons written.
pub fn apply(mouse: &Mouse, profile: u8, file: &ProfileFile) -> io::Result<usize> {
    let writes = plan(mouse, profile, file)?;
    mouse.set_dpi_stages(&DpiStages { active: file.active_stage, stages: file.dpi_stages.clone() })?;
    mouse.set_dpi(file.dpi, file.dpi)?;
    for (layer, i, action) in &writes {
        mouse.set_button(profile, mouse.def.buttons[*i].id, *layer, action)?;
    }
    Ok(writes.len())
}

/// <serial>-<profile>.json in the model folder.
pub fn default_path(mouse: &Mouse, profile: u8) -> Option<PathBuf> {
    device::model_file(mouse.def.vid, mouse.def.pid, &mouse.serial, &format!("{profile}.json"))
}

/// <serial>-<profile>.original.json next to the default path, which holds what
/// the profile looked like the first time opensynapse saw the mouse. RESET puts
/// it back, SET DEFAULT replaces it.
pub fn original_path(mouse: &Mouse, profile: u8) -> Option<PathBuf> {
    default_path(mouse, profile).map(|p| p.with_extension("original.json"))
}

/// Saves an original snapshot of every profile that doesn't have one yet,
/// so the first run doubles as a backup of what's on the mouse.
/// Returns how many files were created.
pub fn ensure_defaults(mouse: &Mouse) -> io::Result<usize> {
    let mut created = 0;
    // only the profiles that exist, so a deleted one doesn't come back as a snapshot
    let existing = mouse.profiles().map(|on| [0].into_iter().chain(on.ids).collect::<Vec<u8>>());
    for &p in existing.as_ref().unwrap_or(&mouse.def.profiles) {
        let (Some(original), Some(old)) = (original_path(mouse, p), default_path(mouse, p)) else { continue };
        if original.exists() {
            continue;
        }
        // before snapshots existed the first-run backup went to the default path, keep that one
        if old.exists() {
            fs::copy(&old, &original)?;
            continue;
        }
        export(mouse, p, &original)?;
        created += 1;
    }
    Ok(created)
}

pub fn export(mouse: &Mouse, profile: u8, path: &Path) -> io::Result<()> {
    device::save_json(path, &read(mouse, profile)?)
}

/// Copies every button on every layer. A deleted profile keeps its keymap on
/// the mouse, so a new profile gets this instead of whatever was there before.
pub fn copy(mouse: &Mouse, from: u8, to: u8) -> io::Result<()> {
    for layer in 0..mouse.def.layers.len() as u8 {
        for b in &mouse.def.buttons {
            let action = mouse.get_button(from, b.id, layer)?;
            if mouse.get_button(to, b.id, layer)?.encode() != action.encode() {
                mouse.set_button(to, b.id, layer, &action)?;
            }
        }
    }
    Ok(())
}

pub fn forget(mouse: &Mouse, profile: u8) -> io::Result<()> {
    for path in [default_path(mouse, profile), original_path(mouse, profile)].into_iter().flatten() {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    if names(mouse).contains_key(&profile) {
        rename(mouse, profile, "")?;
    }
    Ok(())
}

/// Kept on this computer, the mouse has nowhere to store names.
pub fn names(mouse: &Mouse) -> BTreeMap<u8, String> {
    names_path(mouse).and_then(|p| device::load_json(&p).ok()).unwrap_or_default()
}

/// An empty name removes it.
pub fn rename(mouse: &Mouse, profile: u8, name: &str) -> io::Result<()> {
    let path = names_path(mouse).ok_or_else(|| io::Error::other("no config folder"))?;
    let mut all = names(mouse);
    match name.trim() {
        "" => all.remove(&profile),
        name => all.insert(profile, name.to_string()),
    };
    device::save_json(&path, &all)
}

fn names_path(mouse: &Mouse) -> Option<PathBuf> {
    device::model_file(mouse.def.vid, mouse.def.pid, &mouse.serial, "names.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::tests::offline_mouse;
    use serde_json::json;

    fn file(layers: serde_json::Value) -> ProfileFile {
        ProfileFile {
            device: "Razer Test Mouse".into(),
            vid: 0x1532,
            pid: 0x0001,
            firmware: String::new(),
            profile: 0,
            dpi: 800,
            dpi_stages: vec![800],
            active_stage: 1,
            layers: serde_json::from_value(layers).unwrap(),
        }
    }

    fn plan_error(f: &ProfileFile) -> String {
        plan(&offline_mouse(), 0, f).unwrap_err().to_string()
    }

    /// A file plan can't apply fails before any button is read, so a typo writes nothing.
    #[test]
    fn plan_refuses_files_it_cant_apply() {
        let other = ProfileFile { pid: 0x0002, ..file(json!({})) };
        assert_eq!(plan_error(&other), "profile is for Razer Test Mouse (1532:0002)");
        assert_eq!(plan_error(&file(json!({ "Normal": {} }))), "unknown layer 'Normal'");
        assert_eq!(plan_error(&file(json!({ "normal": { "thumb13": "disabled" } }))), "unknown button 'thumb13'");
        assert_eq!(plan_error(&file(json!({ "normal": { "THUMB1": "disabled" } }))), "unknown button 'THUMB1'");
        assert_eq!(plan_error(&file(json!({ "normal": { "thumb1": "key:nosuchkey" } }))), "can't parse action 'key:nosuchkey'");
        assert_eq!(plan_error(&file(json!({ "normal": { "thumb1": "" } }))), "can't parse action ''");
        assert!(plan(&offline_mouse(), 0, &file(json!({ "normal": {} }))).unwrap().is_empty());
        // a valid file reads the button to compare, which /dev/null can't answer
        let valid = plan_error(&file(json!({ "hypershift": { "thumb1": "key:a" } })));
        assert!(!valid.starts_with("unknown") && !valid.starts_with("can't parse"), "{valid}");
    }

    #[test]
    fn files_from_before_firmware_and_profile_load() {
        let old = r#"{ "device": "M", "vid": 5426, "pid": 1, "dpi": 800, "dpi_stages": [800], "active_stage": 1, "layers": {} }"#;
        let f: ProfileFile = serde_json::from_str(old).unwrap();
        assert_eq!((f.firmware.as_str(), f.profile), ("", 0));
    }
}
