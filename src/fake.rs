//! OpenRazer's fake devices, so the lighting, polling rate, battery, sleep
//! timeout and presets can be tried without a mouse. Debug builds only.
//! scripts/fake-devices.sh creates them, starts a daemon for them on a private
//! session bus and sets OPENSYNAPSE_OPENRAZER_TEST to their folder.
//!
//! While it's set, only the fake devices are listed and nothing is sent over
//! hidraw, so a test session can't reach a real mouse.

use crate::device::{self, DeviceDef, Mouse};
use crate::hid::Hid;
use std::path::{Path, PathBuf};
use std::{env, fs};

/// The folder that replaces /sys/bus/hid/drivers/razer*, when testing.
pub fn dir() -> Option<PathBuf> {
    env::var_os("OPENSYNAPSE_OPENRAZER_TEST").filter(|d| !d.is_empty()).map(PathBuf::from)
}

/// One mouse per device folder, which is named like the driver's (0003:1532:007B.0001).
/// A model without a device file gets one with no buttons.
pub fn mice(dir: &Path) -> Vec<Mouse> {
    let defs = device::definitions();
    let mut paths: Vec<PathBuf> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    paths.sort();
    let mut found = Vec::new();
    for path in paths {
        let Some((vid, pid)) = path.file_name().and_then(|n| n.to_str()).and_then(ids) else { continue };
        let read = |name: &str| fs::read_to_string(path.join(name)).map(|s| s.trim().to_string());
        let Ok(name) = read("device_type") else { continue };
        let serial = read("device_serial").unwrap_or_default();
        let def = defs.iter().find(|d| (d.vid, d.pid) == (vid, pid)).cloned().unwrap_or_else(|| DeviceDef {
            file: crate::scaffold::slug(&name),
            name,
            vid,
            pid,
            interface: 0,
            transaction_id: 0,
            profiles: vec![0],
            layers: vec!["normal".into()],
            // only sets the range of the GUI's DPI slider, the fake can't take a DPI
            dpi_max: 20000,
            buttons: Vec::new(),
            source: String::new(),
        });
        // Hid::command refuses everything while testing, so this file is never written to
        let Ok(hid) = Hid::open(Path::new("/dev/null"), 0) else { continue };
        found.push(Mouse { def, hid, serial: serial.chars().filter(char::is_ascii_alphanumeric).collect() });
    }
    found
}

/// "0003:1532:007B.0001" -> (0x1532, 0x007b)
fn ids(name: &str) -> Option<(u16, u16)> {
    let mut parts = name.strip_prefix("0003:")?.split(['.', ':']);
    let vid = u16::from_str_radix(parts.next()?, 16).ok()?;
    let pid = u16::from_str_radix(parts.next()?, 16).ok()?;
    Some((vid, pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_driver_folder_names() {
        assert_eq!(ids("0003:1532:007B.0001"), Some((0x1532, 0x007b)));
        assert_eq!(ids("config"), None);
    }
}
