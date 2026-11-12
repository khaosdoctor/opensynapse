//! Device definitions (installed devices/*.jsonc plus ~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc) and the commands we know how to send.

use crate::action::Action;
use crate::hid::{self, Hid};
use crate::rgb::Effect;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};
use std::{env, fs, io};

/// Device files are JSONC, so this cuts `//` comments outside strings, line by line
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
const CLASS_PROFILE: u8 = 0x05;

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
    /// File name without .jsonc, the protocol file uses the same name.
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
    /// x 30..70, y 2..74, buttons left of x 30 or right of x 70 get a side plate. Devices where no button has `at` get a plain grid.
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

/// Parses the text of one device file.
pub fn parse(src: &str) -> Result<DeviceDef, serde_json::Error> {
    let mut def: DeviceDef = serde_json::from_str(&strip_comments(src))?;
    def.source = src.to_string();
    Ok(def)
}

/// ~/.config/opensynapse/devices/<vid-pid>/buttons.jsonc first, then the installed
/// devices/*.jsonc. The first file for a vid:pid wins, so user files override installed ones.
pub fn definitions() -> Vec<DeviceDef> {
    let mut files: Vec<(PathBuf, Option<String>)> = Vec::new();
    // user files are all "buttons.jsonc", so their repo name comes from the mouse name
    for dir in sorted_entries(devices_dir()) {
        files.push((dir.join("buttons.jsonc"), None));
    }
    for dir in device_dirs() {
        for path in sorted_entries(Some(dir)) {
            if path.extension().is_none_or(|e| e != "jsonc") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).map(str::to_string);
            files.push((path, stem));
        }
    }

    let mut defs: Vec<DeviceDef> = Vec::new();
    for (path, file) in files {
        let Ok(src) = fs::read_to_string(&path) else { continue };
        let mut d = match parse(&src) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("skipping device definition {}: {e}", path.display());
                continue;
            }
        };
        if defs.iter().any(|x| (x.vid, x.pid) == (d.vid, d.pid)) {
            continue;
        }
        d.file = file.unwrap_or_else(|| crate::scaffold::slug(&d.name));
        defs.push(d);
    }
    defs
}

/// Where installed device files can be, in search order: $XDG_DATA_DIRS for distro
/// packages, next to the binary for the AppImage, Homebrew and the tar.gz, and the
/// checkout this was built from so `cargo run` and `cargo install --path .` work.
fn device_dirs() -> Vec<PathBuf> {
    let data = env::var("XDG_DATA_DIRS").ok().filter(|d| !d.trim().is_empty());
    let data = data.unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    let mut candidates: Vec<PathBuf> =
        data.split(':').filter(|d| !d.is_empty()).map(|d| Path::new(d).join("opensynapse/devices")).collect();
    if let Some(exe_dir) = env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        candidates.push(exe_dir.join("../share/opensynapse/devices"));
        candidates.push(exe_dir.join("devices"));
    }
    candidates.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/devices")));

    // /usr/bin/../share is also /usr/share, and reading it twice would print every bad file twice
    let mut dirs = Vec::new();
    for dir in candidates.iter().filter_map(|d| fs::canonicalize(d).ok()) {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The entries of a folder sorted by name, so the same file wins on every run.
fn sorted_entries(dir: Option<PathBuf>) -> Vec<PathBuf> {
    let entries = dir.and_then(|d| fs::read_dir(d).ok()).into_iter().flatten().flatten();
    let mut paths: Vec<PathBuf> = entries.map(|e| e.path()).collect();
    paths.sort();
    paths
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
/// which wins over an installed device file.
fn devices_dir() -> Option<PathBuf> {
    config_dir().map(|c| c.join("opensynapse/devices"))
}

/// $XDG_CONFIG_HOME, or ~/.config.
pub fn config_dir() -> Option<PathBuf> {
    // fake devices get their own presets and lighting files, gone with the test folder
    #[cfg(debug_assertions)]
    if let Some(dir) = crate::fake::dir() {
        return Some(dir.join("config"));
    }
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

/// A theme or preset name, trimmed and lowercased. It becomes a file name, so
/// we keep it to letters, digits, - and _.
pub fn check_name(name: &str) -> Result<String, String> {
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return Err("give it a name".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("only letters, digits, - and _".into());
    }
    Ok(name)
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
    pub(crate) hid: Hid,
    /// factory serial read at detection, empty if the device doesn't report one
    pub serial: String,
}

pub struct DpiStages {
    pub active: u8,
    pub stages: Vec<u16>,
}

/// The stored profiles that exist on the mouse. The profile button and
/// `profile:N` only reach these, a profile has to be created before it can be switched to.
#[derive(Debug, PartialEq)]
pub struct Profiles {
    pub max: u8,
    pub ids: Vec<u8>,
    pub active: u8,
}

/// (max, ids) from a profile list reply, which holds [max, id, id, ...] from byte 8.
/// The size byte counts max too.
fn parse_profile_list(r: &[u8]) -> (u8, Vec<u8>) {
    (r[8], r[9..(8 + r[5] as usize).clamp(9, r.len())].to_vec())
}

/// A mouse whose hidraw node is /dev/null, for tests in this crate and the GUI's.
/// Every command fails before reaching a device.
#[doc(hidden)]
pub fn offline_mouse() -> Mouse {
    let src = r#"{ "name": "Razer Test Mouse", "vid": "0x1532", "pid": "0x0001", "interface": 0, "transaction_id": "0x1f",
        "profiles": [0, 1], "layers": ["normal", "hypershift"], "dpi_max": 20000,
        "buttons": [{ "id": "0x01", "name": "left" }, { "id": "0x40", "name": "thumb1" }] }"#;
    let def = DeviceDef { file: "razer-test-mouse".into(), ..parse(src).unwrap() };
    Mouse { def, hid: Hid::open(Path::new("/dev/null"), 0x1f).unwrap(), serial: "TEST".into() }
}

/// Every connected mouse that has a definition.
pub fn detect() -> io::Result<Vec<Mouse>> {
    #[cfg(debug_assertions)]
    if let Some(dir) = crate::fake::dir() {
        return Ok(crate::fake::mice(&dir));
    }
    let mut found = Vec::new();
    for def in definitions() {
        let Some(path) = hid::find(def.vid, def.pid, def.interface) else { continue };
        let hid = Hid::open(&path, def.transaction_id).map_err(|e| {
            let denied = e.kind() == io::ErrorKind::PermissionDenied;
            let hint = if denied { " > install udev/70-opensynapse.rules (see README)" } else { "" };
            io::Error::new(e.kind(), format!("{}: {e}{hint}", path.display()))
        })?;
        let serial = Mouse::read_serial(&hid).unwrap_or_default();
        found.push(Mouse { def, hid, serial });
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
        // we read it back, since a write that "succeeds" but doesn't stick is worse than an error
        let got = self.get_button(profile, button, layer)?;
        if got.encode() != action.encode() {
            return Err(io::Error::other(format!("wrote {action}, device reports {got}")));
        }
        Ok(())
    }

    /// Factory serial, fixed per physical unit (survives replugs and renames).
    fn read_serial(hid: &Hid) -> io::Result<String> {
        let r = hid.command(0x00, 0x82, 0x16, &[])?;
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
        // each stage is id, x(2), y(2), 0, 0, starting at byte 11
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

    /// Tried a few times, one failed read would make a mouse with stored profiles look like it has none.
    pub fn profiles(&self) -> io::Result<Profiles> {
        let mut last = self.read_profiles();
        for _ in 0..3 {
            if last.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
            last = self.read_profiles();
        }
        last
    }

    fn read_profiles(&self) -> io::Result<Profiles> {
        // [max, id, id, ...], the size byte counts max too
        let r = self.hid.command(CLASS_PROFILE, 0x81, 0x50, &[])?;
        let (max, ids) = parse_profile_list(&r);
        let active = self.hid.command(CLASS_PROFILE, 0x84, 0x01, &[])?[8];
        Ok(Profiles { max, ids, active })
    }

    pub fn create_profile(&self, profile: u8) -> io::Result<()> {
        self.hid.command(CLASS_PROFILE, 0x02, 0x01, &[profile]).map(|_| ())
    }

    pub fn delete_profile(&self, profile: u8) -> io::Result<()> {
        self.hid.command(CLASS_PROFILE, 0x03, 0x01, &[profile]).map(|_| ())
    }

    /// Like pressing the profile button, the mouse copies this profile into P0.
    pub fn set_active_profile(&self, profile: u8) -> io::Result<()> {
        self.hid.command(CLASS_PROFILE, 0x04, 0x01, &[profile]).map(|_| ())
    }

    /// Effect and brightness of one LED zone, read from the mouse (verified on
    /// the Naga, where static green reads back as `01 .. 01 00 ff 00`). The reply
    /// uses the layout OpenRazer writes in razerchromacommon.c, which is
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
pub(crate) mod tests {
    use super::*;

    /// A mouse whose hidraw node is /dev/null, so every command fails before
    /// reaching a device. Only the checks that run before sending can pass.
    pub(crate) fn offline_mouse() -> Mouse {
        super::offline_mouse()
    }

    /// The size byte counts the max byte, so `05 01 02` is max 5 with profiles 1 and 2.
    #[test]
    fn profile_list_reply_is_max_then_ids() {
        let reply = |data: &[u8]| {
            let mut r = [0u8; 90];
            r[5] = data.len() as u8;
            r[8..8 + data.len()].copy_from_slice(data);
            r
        };
        assert_eq!(parse_profile_list(&reply(&[5, 1])), (5, vec![1]));
        assert_eq!(parse_profile_list(&reply(&[5, 1, 2])), (5, vec![1, 2]));
        assert_eq!(parse_profile_list(&reply(&[5])), (5, vec![]));
        assert_eq!(parse_profile_list(&reply(&[])), (0, vec![]));
        // a size byte past the end of the report can't read out of bounds
        let mut long = reply(&[5, 1]);
        long[5] = 255;
        assert_eq!(parse_profile_list(&long).1.len(), 81);
    }

    #[test]
    fn profiles_error_on_a_mouse_that_does_not_answer() {
        assert!(offline_mouse().profiles().is_err());
    }

    fn error(r: io::Result<()>) -> String {
        r.expect_err("/dev/null can't take a command").to_string()
    }

    const DPI_RULE: &str = "DPI must be 100..=20000";
    const STAGE_RULE: &str = "need 1-5 stages and an active stage in range";
    const TOO_LONG: &str = "action too long";

    #[test]
    fn checks_values_before_sending() {
        let m = offline_mouse();
        for (x, y) in [(99, 800), (800, 0), (20001, 800), (u16::MAX, u16::MAX)] {
            assert_eq!(error(m.set_dpi(x, y)), DPI_RULE, "{x}x{y}");
        }
        // the limits themselves pass the check and only fail on /dev/null
        for (x, y) in [(100, 100), (20000, 20000)] {
            assert_ne!(error(m.set_dpi(x, y)), DPI_RULE, "{x}x{y}");
        }
        let stages = |active: u8, stages: &[u16]| error(m.set_dpi_stages(&DpiStages { active, stages: stages.to_vec() }));
        assert_eq!(stages(1, &[]), STAGE_RULE);
        assert_eq!(stages(0, &[800]), STAGE_RULE);
        assert_eq!(stages(2, &[800]), STAGE_RULE);
        assert_eq!(stages(1, &[800; 6]), STAGE_RULE);
        assert_eq!(stages(1, &[800, 99]), DPI_RULE);
        let five = stages(5, &[100, 400, 800, 1600, 20000]);
        assert!(five != STAGE_RULE && five != DPI_RULE, "{five}");
        // profile, button and layer take 3 of the 10 argument bytes
        assert_eq!(error(m.set_button(0, 0x01, 0, &Action::Raw(vec![0x0b; 8]))), TOO_LONG);
        assert_ne!(error(m.set_button(0, 0x01, 0, &Action::Raw(vec![0x0b; 7]))), TOO_LONG);
    }

    #[test]
    fn names_are_exact() {
        let def = offline_mouse().def;
        assert_eq!(def.button("thumb1").map(|b| b.id), Some(0x40));
        assert!(def.button("THUMB1").is_none());
        assert!(def.button("").is_none());
        assert_eq!(def.layer("hypershift"), Some(1));
        assert_eq!(def.layer("Normal"), None);
    }

    #[test]
    fn rejects_broken_device_files() {
        let good = r#"{ "name": "M", "vid": "0x1532", "pid": "0x0001", "interface": 0, "transaction_id": "0x1f",
            "profiles": [0], "layers": ["normal"], "dpi_max": 100, "buttons": [] }"#;
        assert!(parse(good).is_ok());
        let broken = [
            String::new(),
            "   ".into(),
            "{".into(),
            "// only a comment".into(),
            format!("/* block comments aren't supported */ {good}"),
            good.replace("\"0x1532\"", "\"0xzz\""),
            good.replace("\"0x1532\"", "\"0x10000\""),
            good.replace("\"0x1f\"", "\"0x100\""),
            good.replace("\"pid\": \"0x0001\",", ""),
            good.replace("[] }", "[], }"),
            good.replace("[] }", r#"[{ "id": "0x01" }] }"#),
        ];
        for src in &broken {
            assert!(parse(src).is_err(), "{src}");
        }
    }

    /// Catches a broken device file before it ships in a release,
    /// including the ones device-from-issue adds from issues.
    #[test]
    fn repo_device_files_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("devices");
        for path in sorted_entries(Some(dir)) {
            if path.extension().is_none_or(|e| e != "jsonc") {
                continue;
            }
            let src = fs::read_to_string(&path).unwrap();
            let def = parse(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(!def.buttons.is_empty(), "{} has no buttons", path.display());
        }
        let naga = definitions().into_iter().find(|d| d.pid == 0x008d).expect("naga from devices/");
        assert_eq!((naga.vid, naga.transaction_id, naga.buttons.len()), (0x1532, 0x1f, 22));
        assert_eq!(naga.button("thumb1").map(|b| b.id), Some(0x40));
    }

    #[test]
    fn names_stay_file_names() {
        assert_eq!(check_name(" Night_Blue-2 "), Ok("night_blue-2".into()));
        assert!(check_name("  ").is_err());
        assert!(check_name("../evil").is_err());
        assert!(check_name("my preset").is_err());
    }

    #[test]
    fn jsonc_comments() {
        let src = "// head\n{ \"a\": \"x // \\\"y\", // note\n \"b\": 1 } // end";
        let v: serde_json::Value = serde_json::from_str(&strip_comments(src)).unwrap();
        assert_eq!((v["a"].as_str(), v["b"].as_u64()), (Some("x // \"y"), Some(1)));
    }
}
