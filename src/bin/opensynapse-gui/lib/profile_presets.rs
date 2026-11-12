//! Profiles for mice that don't answer the profile commands. Each one is a
//! profile file on this computer, and SET ACTIVE writes it to the mouse.

use crate::state::{App, Op};
use opensynapse::action::Action;
use opensynapse::device::{self, Mouse};
use opensynapse::profile::{self, ProfileFile};
use std::io;
use std::path::PathBuf;

/// More than any mouse stores, and the list is still short enough to scroll.
pub(crate) const MAX: u8 = 20;

pub(crate) struct Presets {
    pub(crate) ids: Vec<u8>,
    /// the one last written to the mouse
    pub(crate) active: u8,
}

pub(crate) fn original(m: &Mouse, n: u8) -> Option<PathBuf> {
    profile::preset_path(m, n).map(|p| p.with_extension("original.json"))
}

fn load(m: &Mouse, n: u8) -> io::Result<ProfileFile> {
    device::load_json(&profile::preset_path(m, n).ok_or_else(|| io::Error::other("no config folder"))?)
}

fn save(m: &Mouse, n: u8, file: &ProfileFile) -> io::Result<()> {
    device::save_json(&profile::preset_path(m, n).ok_or_else(|| io::Error::other("no config folder"))?, file)
}

impl App {
    /// The mouse slot presets are written to: its first stored profile, or P0 on a mouse with only that.
    pub(crate) fn preset_slot(&self) -> u8 {
        let profiles = self.mice.get(self.current).map_or(&[][..], |m| &m.def.profiles[..]);
        profiles.iter().copied().find(|&p| p != 0).unwrap_or(0)
    }

    /// The first time, what's on the mouse becomes preset 1. After that the
    /// active preset is read from the mouse again, in case something else changed it.
    pub(crate) fn load_presets(&self) -> Option<Presets> {
        let m = self.mice.get(self.current)?;
        let live = profile::read(m, self.preset_slot()).ok()?;
        let mut ids = profile::presets(m);
        if ids.is_empty() {
            ids.push(1);
            let _ = original(m, 1).map(|path| device::save_json(&path, &live));
        }
        let active = profile::active_preset(m).filter(|a| ids.contains(a)).unwrap_or(ids[0]);
        let _ = save(m, active, &live);
        let _ = profile::set_active_preset(m, active);
        Some(Presets { ids, active })
    }

    /// A preset's buttons from its file, None when it's the one on the mouse.
    pub(crate) fn preset_actions(&self, n: u8, layer: u8) -> Option<Vec<Option<Action>>> {
        self.profile_presets.as_ref().filter(|p| p.active != n)?;
        let m = self.mice.get(self.current)?;
        let file = load(m, n).ok()?;
        let buttons = file.layers.get(m.def.layers.get(layer as usize)?)?;
        Some(m.def.buttons.iter().map(|b| buttons.get(&b.name)?.parse().ok()).collect())
    }
}

/// Runs the ops that work differently on presets. None for the ones that don't.
pub(crate) fn apply(m: &Mouse, slot: u8, op: &Op, active: &mut u8) -> Option<io::Result<()>> {
    let r = match op {
        Op::Button { profile: n, layer, index, action } => (|| {
            let mut file = load(m, *n)?;
            let layer_name = &m.def.layers[*layer as usize];
            let button = &m.def.buttons[*index];
            file.layers.entry(layer_name.clone()).or_default().insert(button.name.clone(), action.to_string());
            save(m, *n, &file)?;
            if *n != *active {
                return Ok(());
            }
            m.set_button(slot, button.id, *layer, action)?;
            // P0 is what the mouse uses right now
            if slot != 0 {
                m.set_button(0, button.id, *layer, action)?;
            }
            Ok(())
        })(),
        Op::CreateProfile(n) => load(m, *active).and_then(|file| {
            save(m, *n, &file)?;
            original(m, *n).map_or(Ok(()), |path| device::save_json(&path, &file))
        }),
        Op::DeleteProfile(n) => (|| {
            for path in [profile::preset_path(m, *n), original(m, *n)].into_iter().flatten() {
                match std::fs::remove_file(&path) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
            profile::rename(m, *n, "")
        })(),
        Op::SetActive(n) => load(m, *n).and_then(|file| {
            profile::apply(m, slot, &file)?;
            if slot != 0 {
                profile::apply(m, 0, &file)?;
            }
            profile::set_active_preset(m, *n)?;
            *active = *n;
            Ok(())
        }),
        _ => return None,
    };
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(button: &str, action: &str) -> ProfileFile {
        ProfileFile {
            device: "Razer Test Mouse".into(),
            vid: 0x1532,
            pid: 0x0001,
            firmware: String::new(),
            profile: 0,
            dpi: 800,
            dpi_stages: vec![800],
            active_stage: 1,
            layers: [("normal".to_string(), [(button.to_string(), action.to_string())].into())].into(),
        }
    }

    fn run(m: &Mouse, slot: u8, op: &Op, active: &mut u8) -> io::Result<()> {
        apply(m, slot, op, active).expect("a preset op")
    }

    /// One test, since it sets XDG_CONFIG_HOME and parallel tests would share it.
    #[test]
    fn presets_that_are_not_active_only_touch_files() {
        let tmp = std::env::temp_dir().join(format!("opensynapse-presets-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        // SAFETY: this is the only test in this binary that reads or sets it
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &tmp) };
        let m = device::offline_mouse();
        let mut active = 1;
        assert_eq!(original(&m, 2), Some(tmp.join("opensynapse/devices/1532-0001/TEST-preset-2.original.json")));

        // a button edit on another preset is saved in its file and never sent
        save(&m, 1, &file("left", "disabled")).unwrap();
        save(&m, 2, &file("left", "disabled")).unwrap();
        let edit = Op::Button { profile: 2, layer: 1, index: 1, action: Action::Mouse(1) };
        run(&m, 0, &edit, &mut active).unwrap();
        let saved = load(&m, 2).unwrap();
        assert_eq!(saved.layers["hypershift"]["thumb1"], Action::Mouse(1).to_string());
        assert_eq!(saved.layers["normal"]["left"], "disabled");
        assert_eq!(load(&m, 1).unwrap().layers.len(), 1);

        // NEW copies the active preset, as the file and as the original
        run(&m, 0, &Op::CreateProfile(3), &mut active).unwrap();
        assert_eq!(load(&m, 3).unwrap().layers.len(), 1);
        let copied: ProfileFile = device::load_json(&original(&m, 3).unwrap()).unwrap();
        assert_eq!(copied.layers["normal"]["left"], "disabled");
        assert_eq!(profile::presets(&m), [1, 2, 3]);

        // DELETE removes the file, the original and the name, and a second one is no error
        profile::rename(&m, 3, "Temp").unwrap();
        run(&m, 0, &Op::DeleteProfile(3), &mut active).unwrap();
        assert!(profile::preset_path(&m, 3).is_some_and(|p| !p.exists()));
        assert!(original(&m, 3).is_some_and(|p| !p.exists()));
        assert!(profile::names(&m).is_empty());
        run(&m, 0, &Op::DeleteProfile(3), &mut active).unwrap();
        assert_eq!(profile::presets(&m), [1, 2]);

        // NEW from an active preset without a file fails and creates nothing
        active = 9;
        assert!(run(&m, 0, &Op::CreateProfile(4), &mut active).is_err());
        assert_eq!(profile::presets(&m), [1, 2]);

        // these need the mouse, and fail without changing which preset is active
        active = 1;
        assert!(run(&m, 0, &Op::SetActive(2), &mut active).is_err());
        assert_eq!(active, 1);
        assert_eq!(profile::active_preset(&m), None);
        let live = Op::Button { profile: 1, layer: 0, index: 0, action: Action::Mouse(1) };
        assert!(run(&m, 0, &live, &mut active).is_err());
        // an op that works the same on presets and on the mouse isn't handled here
        assert!(apply(&m, 0, &Op::Poll(500), &mut active).is_none());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
