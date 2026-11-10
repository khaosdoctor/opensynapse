//! RGB lighting through OpenRazer. We don't reverse engineer lighting:
//! OpenRazer already supports it for most Razer mice, so any zone its driver
//! exposes works here with no device file.
//!
//! When the OpenRazer daemon is around we go through its D-Bus API, so it
//! remembers the change and doesn't put its own saved colors back on the next
//! replug. Without it we write the driver's sysfs files directly.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{fs, io};
use zbus::Message;
use zbus::blocking::Connection;
use zbus::zvariant::DynamicType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Decodes the arguments of an extended matrix effect reply (class 0x0f, 0x82).
    pub fn from_reply(a: &[u8]) -> Effect {
        let at = |i: usize| a.get(i).copied().unwrap_or(0);
        let color = [at(6), at(7), at(8)];
        match at(2) {
            0x00 => Effect::Off,
            0x02 if at(5) == 0 => Effect::BreathRandom,
            0x02 => Effect::Breath(color),
            0x03 => Effect::Spectrum,
            0x04 => Effect::Wave(at(3) == 2),
            0x05 => Effect::Reactive { speed: at(4), color },
            // 01 static, and effects the GUI has no card for (starlight, wheel) show as static
            _ => Effect::Static(color),
        }
    }

    fn file_and_data(&self) -> (&'static str, Vec<u8>) {
        match self {
            Effect::Off => ("none", b"1".to_vec()),
            Effect::Static(c) => ("static", c.to_vec()),
            Effect::Breath(c) => ("breath", c.to_vec()),
            // one byte means "random colors" to the driver
            Effect::BreathRandom => ("breath", vec![1]),
            Effect::Spectrum => ("spectrum", b"1".to_vec()),
            Effect::Wave(reverse) => ("wave", vec![if *reverse { b'2' } else { b'1' }]),
            Effect::Reactive { speed, color } => ("reactive", vec![(*speed).clamp(1, 3), color[0], color[1], color[2]]),
        }
    }
}

/// One zone's look, as saved in the lighting original file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneState {
    pub zone: String,
    pub effect: Effect,
    pub brightness: u8,
}

fn lighting_file(vid: u16, pid: u16, serial: &str, name: &str) -> Option<PathBuf> {
    let prefix = if serial.is_empty() { String::new() } else { format!("{serial}-") };
    crate::device::model_dir(vid, pid).map(|d| d.join(format!("{prefix}{name}")))
}

/// <serial>-lighting.original.json in the device folder: the lighting the
/// first time opensynapse saw the mouse, or what SET DEFAULT saved.
pub fn original_path(vid: u16, pid: u16, serial: &str) -> Option<PathBuf> {
    lighting_file(vid, pid, serial, "lighting.original.json")
}

/// The LED id the mouse uses for an OpenRazer zone name (OpenRazer's razercommon.h).
pub fn led_id(zone: &str) -> Option<u8> {
    match zone {
        "scroll" => Some(0x01),
        "logo" => Some(0x04),
        "backlight" => Some(0x05),
        "right" => Some(0x10),
        "left" => Some(0x11),
        _ => None,
    }
}

pub fn save_zones(path: &std::path::Path, zones: &[ZoneState]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, serde_json::to_string_pretty(zones).map_err(io::Error::other)?)
}

pub fn load_zones(path: &std::path::Path) -> io::Result<Vec<ZoneState>> {
    serde_json::from_str(&fs::read_to_string(path)?).map_err(io::Error::other)
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

    /// False when there's no daemon or it doesn't know this zone.
    fn daemon_call<B: Serialize + DynamicType>(&self, zone: &str, name: &str, body: &B) -> bool {
        self.daemon.as_ref().is_some_and(|(conn, path)| call(conn, path, zone, "set", name, body).is_some())
    }
}

/// Calls razer.device.lighting.<zone>.<verb><Zone><name> on the OpenRazer daemon.
fn call<B: Serialize + DynamicType>(conn: &Connection, path: &str, zone: &str, verb: &str, name: &str, body: &B) -> Option<Message> {
    let mut cap = zone.to_string();
    cap[..1].make_ascii_uppercase();
    let iface = format!("razer.device.lighting.{zone}");
    conn.call_method(Some("org.razer"), path, Some(iface.as_str()), format!("{verb}{cap}{name}").as_str(), body).ok()
}

fn connect_daemon(serial: &str, zone: &str) -> Option<(Connection, String)> {
    if serial.is_empty() {
        return None;
    }
    let conn = Connection::session().ok()?;
    let path = format!("/org/razer/device/{serial}");
    // a read that also starts the daemon if it's only D-Bus activatable
    call(&conn, &path, zone, "get", "Brightness", &())?;
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
    dirs.into_iter().find_map(|dir| {
        let mut zones: Vec<String> = fs::read_dir(&dir)
            .ok()?
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().strip_suffix("_matrix_effect_static").map(str::to_string))
            .collect();
        if zones.is_empty() {
            return None;
        }
        zones.sort();
        let daemon = connect_daemon(serial, &zones[0]);
        Some(Lighting { dir, zones, daemon, serial: serial.to_string() })
    })
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
        if self.daemon_call(zone, "Brightness", &(f64::from(value) * 100.0 / 255.0,)) {
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
        if sent {
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
    crate::action::hex_bytes(s.trim_start_matches('#'))?.try_into().ok()
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
fn saved_effect(serial: &str, zone: &str) -> Option<Effect> {
    let conf = fs::read_to_string(crate::device::config_dir()?.join("openrazer/persistence.conf")).ok()?;
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
        let get = |what: &str| call(conn, path, zone, "get", what, &());
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
        // read from a Naga set to static green
        assert_eq!(Effect::from_reply(&[0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0xff, 0x00]), Effect::Static([0, 255, 0]));
        assert_eq!(Effect::from_reply(&[0x00, 0x04, 0x02, 0x00, 0x00, 0x00]), Effect::BreathRandom);
    }
}
