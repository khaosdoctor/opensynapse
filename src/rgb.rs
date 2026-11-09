//! RGB lighting through OpenRazer. We don't reverse engineer lighting:
//! OpenRazer already supports it for most Razer mice, so any zone its driver
//! exposes works here with no device file.
//!
//! When the OpenRazer daemon is around we go through its D-Bus API, so it
//! remembers the change and doesn't put its own saved colors back on the next
//! replug. Without it we write the driver's sysfs files directly.

use serde::Serialize;
use std::path::PathBuf;
use std::{env, fs, io};
use zbus::blocking::Connection;
use zbus::zvariant::DynamicType;

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
    /// session bus + /org/razer/device/<serial>, when the daemon answers
    daemon: Option<(Connection, String)>,
    serial: String,
}

impl Lighting {
    pub fn uses_daemon(&self) -> bool {
        self.daemon.is_some()
    }

    /// Calls razer.device.lighting.<zone>.set<Zone><name> on the daemon.
    /// Returns None when there's no daemon or it doesn't know this zone.
    fn daemon_call<B: Serialize + DynamicType>(&self, zone: &str, name: &str, body: &B) -> Option<()> {
        let (conn, path) = self.daemon.as_ref()?;
        let mut cap = zone.to_string();
        cap[..1].make_ascii_uppercase();
        let iface = format!("razer.device.lighting.{zone}");
        conn.call_method(Some("org.razer"), path.as_str(), Some(iface.as_str()), format!("set{cap}{name}").as_str(), body)
            .ok()
            .map(|_| ())
    }
}

fn connect_daemon(serial: &str, zone: &str) -> Option<(Connection, String)> {
    if serial.is_empty() {
        return None;
    }
    let conn = Connection::session().ok()?;
    let path = format!("/org/razer/device/{serial}");
    let mut cap = zone.to_string();
    cap[..1].make_ascii_uppercase();
    let iface = format!("razer.device.lighting.{zone}");
    // a read that also starts the daemon if it's only D-Bus activatable
    conn.call_method(Some("org.razer"), path.as_str(), Some(iface.as_str()), format!("get{cap}Brightness").as_str(), &())
        .ok()?;
    Some((conn, path))
}

/// Lighting for a vid:pid (and serial, to find it on the OpenRazer daemon).
pub fn find(vid: u16, pid: u16, serial: &str) -> Option<Lighting> {
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
        let daemon = connect_daemon(serial, &zones[0]);
        return Some(Lighting { dir, zones, daemon, serial: serial.to_string() });
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
        // the daemon takes a percentage
        if self.daemon_call(zone, "Brightness", &(f64::from(value) * 100.0 / 255.0,)).is_some() {
            return Ok(());
        }
        fs::write(self.dir.join(format!("{zone}_led_brightness")), value.to_string()).map_err(hint)
    }

    pub fn set_effect(&self, zone: &str, effect: &Effect) -> io::Result<()> {
        let sent = match *effect {
            Effect::Off => self.daemon_call(zone, "None", &()),
            Effect::Static([r, g, b]) => self.daemon_call(zone, "Static", &(r, g, b)),
            Effect::Breath([r, g, b]) => self.daemon_call(zone, "BreathSingle", &(r, g, b)),
            Effect::BreathRandom => self.daemon_call(zone, "BreathRandom", &()),
            Effect::Spectrum => self.daemon_call(zone, "Spectrum", &()),
            Effect::Wave(reverse) => self.daemon_call(zone, "Wave", &(if reverse { 2i32 } else { 1 },)),
            Effect::Reactive { speed, color: [r, g, b] } => self.daemon_call(zone, "Reactive", &(r, g, b, speed.clamp(1, 3))),
        };
        if sent.is_some() {
            return Ok(());
        }
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

/// OpenRazer's effect names (daemon and persistence.conf use the same ones).
fn effect_from(name: &str, colors: &[u8], speed: Option<u8>, wave_dir: Option<i32>) -> Effect {
    let color = [colors.first(), colors.get(1), colors.get(2)].map(|c| c.copied().unwrap_or(0));
    match name {
        "static" => Effect::Static(color),
        "breathSingle" => Effect::Breath(color),
        "breathRandom" => Effect::BreathRandom,
        "spectrum" => Effect::Spectrum,
        "wave" => Effect::Wave(wave_dir == Some(2)),
        "reactive" => Effect::Reactive { speed: speed.unwrap_or(2), color },
        "none" => Effect::Off,
        _ => Effect::Static(color),
    }
}

/// The last effect the OpenRazer daemon saved for a zone in persistence.conf.
/// The daemon only writes that file on exit, so prefer Lighting::current_effect.
pub fn saved_effect(serial: &str, zone: &str) -> Option<Effect> {
    let home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let conf = fs::read_to_string(home.join("openrazer/persistence.conf")).ok()?;
    let section = conf.split(&format!("[{serial}]")).nth(1)?.split("\n[").next()?;
    let value = |key: &str| {
        section.lines().find_map(|l| l.strip_prefix(&format!("{zone}_{key} = ")).map(str::trim))
    };
    let colors: Vec<u8> = value("colors")?.split_whitespace().filter_map(|n| n.parse().ok()).collect();
    let speed = value("speed").and_then(|s| s.parse().ok());
    let wave_dir = value("wave_dir").and_then(|s| s.parse().ok());
    Some(effect_from(value("effect")?, &colors, speed, wave_dir))
}

impl Lighting {
    /// What a zone shows right now: asked from the daemon when it's running,
    /// otherwise its saved state (the driver's effect files are write-only).
    pub fn current_effect(&self, zone: &str) -> Option<Effect> {
        let Some((conn, path)) = &self.daemon else { return saved_effect(&self.serial, zone) };
        let mut cap = zone.to_string();
        cap[..1].make_ascii_uppercase();
        let iface = format!("razer.device.lighting.{zone}");
        let get = |what: &str| {
            conn.call_method(Some("org.razer"), path.as_str(), Some(iface.as_str()), format!("get{cap}{what}").as_str(), &())
                .ok()
        };
        let name: String = get("Effect")?.body().deserialize().ok()?;
        let colors: Vec<u8> = get("EffectColors").and_then(|m| m.body().deserialize().ok()).unwrap_or_default();
        let speed: Option<i32> = get("EffectSpeed").and_then(|m| m.body().deserialize().ok());
        let wave_dir: Option<i32> = get("WaveDir").and_then(|m| m.body().deserialize().ok());
        Some(effect_from(&name, &colors, speed.map(|s| s as u8), wave_dir))
    }
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
