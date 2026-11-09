//! RGB lighting through the OpenRazer kernel driver's sysfs files.
//! We don't reverse engineer lighting: OpenRazer already supports it for
//! most Razer mice, so any zone it exposes works here with no device file.

use std::path::PathBuf;
use std::{env, fs, io};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Off,
    Static([u8; 3]),
    Breath([u8; 3]),
    BreathRandom,
    Spectrum,
    /// false = one direction, true = the other (the driver's 1 and 2)
    Wave(bool),
    /// speed 1 (fast) to 3 (slow)
    Reactive { speed: u8, color: [u8; 3] },
}

impl Effect {
    fn file_and_data(&self) -> (&'static str, Vec<u8>) {
        match self {
            Effect::Off => ("none", b"1".to_vec()),
            Effect::Static(c) => ("static", c.to_vec()),
            Effect::Breath(c) => ("breath", c.to_vec()),
            // one byte means "random colors" to the driver
            Effect::BreathRandom => ("breath", vec![1]),
            Effect::Spectrum => ("spectrum", b"1".to_vec()),
            Effect::Wave(reverse) => ("wave", if *reverse { b"2".to_vec() } else { b"1".to_vec() }),
            Effect::Reactive { speed, color } => ("reactive", vec![(*speed).clamp(1, 3), color[0], color[1], color[2]]),
        }
    }
}

pub struct Lighting {
    dir: PathBuf,
    /// e.g. ["logo", "right", "scroll"]
    pub zones: Vec<String>,
}

/// The OpenRazer sysfs folder for a vid:pid that exposes lighting zones.
pub fn find(vid: u16, pid: u16) -> Option<Lighting> {
    let prefix = format!("0003:{vid:04X}:{pid:04X}.");
    let mut dirs: Vec<PathBuf> = fs::read_dir("/sys/bus/hid/drivers")
        .ok()?
        .flatten()
        .filter(|d| d.file_name().to_string_lossy().starts_with("razer"))
        .filter_map(|d| fs::read_dir(d.path()).ok())
        .flat_map(|it| it.flatten())
        .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let mut zones: Vec<String> = fs::read_dir(&dir)
            .ok()?
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().strip_suffix("_matrix_effect_static").map(str::to_string))
            .collect();
        if zones.is_empty() {
            continue;
        }
        zones.sort();
        return Some(Lighting { dir, zones });
    }
    None
}

fn hint(e: io::Error) -> io::Error {
    if e.kind() != io::ErrorKind::PermissionDenied {
        return e;
    }
    io::Error::new(e.kind(), format!("{e} > add yourself to the openrazer group and log in again"))
}

impl Lighting {
    pub fn brightness(&self, zone: &str) -> io::Result<u8> {
        let raw = fs::read_to_string(self.dir.join(format!("{zone}_led_brightness"))).map_err(hint)?;
        raw.trim().parse().map_err(io::Error::other)
    }

    pub fn set_brightness(&self, zone: &str, value: u8) -> io::Result<()> {
        fs::write(self.dir.join(format!("{zone}_led_brightness")), value.to_string()).map_err(hint)
    }

    pub fn set_effect(&self, zone: &str, effect: &Effect) -> io::Result<()> {
        let (name, data) = effect.file_and_data();
        let path = self.dir.join(format!("{zone}_matrix_effect_{name}"));
        if !path.exists() {
            return Err(io::Error::other(format!("{zone} doesn't support {name}")));
        }
        fs::write(path, data).map_err(hint)
    }
}

/// "ff00aa" or "#ff00aa" -> [255, 0, 170]
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let hex = s.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// The last effect the OpenRazer daemon applied to a zone, from its saved
/// state, since the driver's effect files can't be read back.
pub fn saved_effect(serial: &str, zone: &str) -> Option<Effect> {
    let home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let conf = fs::read_to_string(home.join("openrazer/persistence.conf")).ok()?;
    let section = conf.split(&format!("[{serial}]")).nth(1)?.split("\n[").next()?;
    let value = |key: &str| {
        section.lines().find_map(|l| l.strip_prefix(&format!("{zone}_{key} = ")).map(str::trim))
    };
    let nums: Vec<u8> = value("colors")?.split_whitespace().filter_map(|n| n.parse().ok()).collect();
    let color = [*nums.first()?, *nums.get(1)?, *nums.get(2)?];
    Some(match value("effect")? {
        "static" => Effect::Static(color),
        "breathSingle" => Effect::Breath(color),
        "breathRandom" => Effect::BreathRandom,
        "spectrum" => Effect::Spectrum,
        "wave" => Effect::Wave(value("wave_dir") == Some("2")),
        "reactive" => Effect::Reactive { speed: value("speed").and_then(|s| s.parse().ok()).unwrap_or(2), color },
        "none" => Effect::Off,
        _ => Effect::Static(color),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_what_the_driver_expects() {
        assert_eq!(parse_color("#FF00aa"), Some([255, 0, 170]));
        assert_eq!(parse_color("fff"), None);
        assert_eq!(Effect::Static([1, 2, 3]).file_and_data(), ("static", vec![1, 2, 3]));
        assert_eq!(Effect::BreathRandom.file_and_data(), ("breath", vec![1]));
        assert_eq!(Effect::Reactive { speed: 9, color: [1, 2, 3] }.file_and_data(), ("reactive", vec![3, 1, 2, 3]));
        assert_eq!(Effect::Wave(true).file_and_data(), ("wave", b"2".to_vec()));
    }
}
