//! Starts a device file for a Razer mouse that doesn't have one, using only
//! read commands: the transport comes from devices/known-mice.toml (taken
//! from OpenRGB) and the buttons from asking the mouse.

use crate::hid::{self, Hid};
use serde::Deserialize;
use std::fmt::Write;
use std::io;

const KNOWN: &str = include_str!("../devices/known-mice.toml");
const RAZER: u16 = 0x1532;
/// Transaction ids seen across Razer mice, newest first.
const TIDS: [u8; 3] = [0x1f, 0x3f, 0xff];

#[derive(Deserialize)]
struct KnownFile {
    mice: Vec<Known>,
}

#[derive(Deserialize, Clone)]
pub struct Known {
    pub pid: u16,
    pub transaction_id: u8,
    pub interface: Option<u8>,
    pub name: String,
}

pub fn known(pid: u16) -> Option<Known> {
    let file: KnownFile = toml::from_str(KNOWN).ok()?;
    file.mice.into_iter().find(|m| m.pid == pid)
}

pub struct Scaffold {
    pub name: String,
    pub toml: String,
    pub transaction_id: u8,
    pub interface: u8,
    pub buttons: usize,
}

/// Common button ids (razerqdhid, Naga) so the file starts with some names.
fn guess_name(id: u8) -> String {
    let name = match id {
        0x01 => "left",
        0x02 => "right",
        0x03 => "middle",
        0x04 => "back",
        0x05 => "forward",
        0x09 => "wheel-up",
        0x0a => "wheel-down",
        0x34 => "tilt-left",
        0x35 => "tilt-right",
        _ => return format!("btn-0x{id:02x}"),
    };
    name.to_string()
}

/// Finds the interface and transaction id the mouse answers on, with reads only.
fn connect(pid: u16, known: Option<&Known>) -> io::Result<(Hid, u8, u8)> {
    let nodes = hid::find_all(RAZER, pid);
    if nodes.is_empty() {
        return Err(io::Error::other(format!("no hidraw node for 1532:{pid:04x}, is it plugged in?")));
    }
    let mut tids: Vec<u8> = known.map(|k| k.transaction_id).into_iter().chain(TIDS).collect();
    tids.dedup();
    let preferred = known.and_then(|k| k.interface).unwrap_or(0);
    let mut ifaces: Vec<(u8, std::path::PathBuf)> = nodes;
    ifaces.sort_by_key(|(n, _)| (*n != preferred, *n));
    let mut last_err = io::Error::other("the mouse didn't answer on any interface");
    for (iface, path) in &ifaces {
        for &tid in &tids {
            let hid = match Hid::open(path, tid) {
                Ok(h) => h,
                Err(e) => {
                    last_err = e;
                    continue;
                }
            };
            // firmware version: a read every Razer mouse answers
            if hid.command(0x00, 0x81, 0x02, &[]).is_ok() {
                return Ok((hid, *iface, tid));
            }
        }
    }
    Err(last_err)
}

pub fn scaffold(pid: u16, fallback_name: &str) -> io::Result<Scaffold> {
    let known = known(pid);
    let name = known.as_ref().map_or(fallback_name.to_string(), |k| k.name.clone());
    let (hid, interface, tid) = connect(pid, known.as_ref())?;

    // 0x02/0x84: [count, ids...]
    let ids: Vec<u8> = match hid.command(0x02, 0x84, 0x50, &[]) {
        Ok(r) => r[9..9 + (r[8] as usize).min(78)].to_vec(),
        Err(_) => Vec::new(),
    };
    // which profiles answer a keymap read for the first button
    let first = ids.first().copied().unwrap_or(0x01);
    let profiles: Vec<u8> =
        (0..=5u8).filter(|&p| hid.command(0x02, 0x8c, 0x0a, &[p, first, 0]).is_ok()).collect();
    let hypershift = hid.command(0x02, 0x8c, 0x0a, &[0, first, 1]).is_ok();

    let mut t = String::new();
    // writes to a String can't fail
    let _ = writeln!(t, "# Started by `opensynapse init` with read-only queries. Check every TODO,");
    let _ = writeln!(t, "# then see CONTRIBUTING.md for testing it and sending it.");
    let _ = writeln!(t, "name = {name:?}");
    let _ = writeln!(t, "vid = 0x{RAZER:04x}\npid = 0x{pid:04x}\ninterface = {interface}\ntransaction_id = 0x{tid:02x}");
    let _ = writeln!(t, "profiles = {profiles:?}");
    let layers = if hypershift { "[\"normal\", \"hypershift\"]" } else { "[\"normal\"]" };
    let _ = writeln!(t, "layers = {layers}");
    let _ = writeln!(t, "dpi_max = 16000 # TODO: your mouse's real maximum");
    if ids.is_empty() {
        let _ = writeln!(t, "\n# TODO: the mouse didn't answer 0x02/0x84 (button list), add buttons by hand");
        let _ = writeln!(t, "buttons = []");
    }
    for id in &ids {
        let _ = writeln!(t, "\n[[buttons]]\nid = 0x{id:02x}\nname = \"{}\" # TODO: check with `opensynapse watch`", guess_name(*id));
    }
    Ok(Scaffold { name, toml: t, transaction_id: tid, interface, buttons: ids.len() })
}

/// "Razer Naga Pro (Wired)" -> "razer-naga-pro-wired"
pub fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_table_parses_and_slugs() {
        let naga = known(0x008d).expect("naga in the table");
        assert_eq!((naga.transaction_id, naga.interface), (0x1f, Some(0)));
        assert_eq!(known(0x00e7).unwrap().interface, None);
        assert_eq!(slug("Razer Naga Pro (Wired)"), "razer-naga-pro-wired");
    }
}
