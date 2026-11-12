//! Named lighting presets in ~/.config/opensynapse/presets/<name>.json. They
//! belong to the user and not to a mouse, so loading one skips the zones the
//! mouse doesn't have.

use crate::device;
use crate::rgb::ZoneState;
use std::path::PathBuf;
use std::{fs, io};

fn dir() -> Option<PathBuf> {
    device::config_dir().map(|c| c.join("opensynapse/presets"))
}

pub fn path(name: &str) -> io::Result<PathBuf> {
    let name = device::check_name(name).map_err(io::Error::other)?;
    Ok(dir().ok_or_else(|| io::Error::other("no config folder"))?.join(format!("{name}.json")))
}

/// Every saved preset's name, sorted.
pub fn list() -> Vec<String> {
    let Some(entries) = dir().and_then(|d| fs::read_dir(d).ok()) else { return Vec::new() };
    let mut names = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            names.push(name.to_string());
        }
    }
    names.sort();
    names
}

/// Saves over a preset with the same name.
pub fn save(name: &str, zones: &[ZoneState]) -> io::Result<PathBuf> {
    let path = path(name)?;
    device::save_json(&path, &zones)?;
    Ok(path)
}

pub fn load(name: &str) -> io::Result<Vec<ZoneState>> {
    let path = path(name)?;
    device::load_json(&path).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", path.display())))
}

pub fn delete(name: &str) -> io::Result<()> {
    fs::remove_file(path(name)?)
}

/// The preset's zones that are in `zones`, the ones the mouse has.
pub fn for_zones(preset: Vec<ZoneState>, zones: &[String]) -> Vec<ZoneState> {
    preset.into_iter().filter(|s| zones.contains(&s.zone)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rgb::Effect;

    #[test]
    fn skips_zones_the_mouse_doesnt_have() {
        let zone = |name: &str| ZoneState { zone: name.into(), effect: Effect::Spectrum, brightness: 255 };
        let mouse = vec!["logo".to_string(), "scroll".to_string()];
        let kept = for_zones(vec![zone("logo"), zone("backlight"), zone("scroll")], &mouse);
        let names: Vec<&str> = kept.iter().map(|s| s.zone.as_str()).collect();
        assert_eq!(names, ["logo", "scroll"]);
    }
}
