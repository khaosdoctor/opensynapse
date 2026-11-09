//! Starts a device file for a Razer mouse that doesn't have one, using only
//! read commands: the transport is found by trial and the buttons come from
//! asking the mouse, so there's no per-mouse table to keep up to date.

use crate::hid::{self, Hid};
use std::fmt::Write;
use std::io;

const RAZER: u16 = 0x1532;
/// Transaction ids used across Razer mice (OpenRazer, OpenRGB), newest first.
const TIDS: [u8; 3] = [0x1f, 0x3f, 0xff];

pub struct Scaffold {
    pub name: String,
    pub json: String,
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
/// Tries every interface with every known transaction id, so no per-mouse table is needed.
fn connect(pid: u16) -> io::Result<(Hid, u8, u8)> {
    let mut ifaces = hid::find_all(RAZER, pid);
    if ifaces.is_empty() {
        return Err(io::Error::other(format!("no hidraw node for 1532:{pid:04x}, is it plugged in?")));
    }
    ifaces.sort_by_key(|(n, _)| *n);
    let tids = TIDS;
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

/// `name` is what the mouse reports over USB, like "Razer Naga Left Handed".
pub fn scaffold(pid: u16, name: &str) -> io::Result<Scaffold> {
    let name = name.to_string();
    let (hid, interface, tid) = connect(pid)?;

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

    // written by hand to keep ids in hex and one button per line; writes to a String can't fail
    let mut t = String::from("{\n");
    let _ = writeln!(t, "  \"_comment\": \"Started by `opensynapse init` with read-only queries. Fix every _todo, then see CONTRIBUTING.md.\",");
    let _ = writeln!(t, "  \"name\": {},", serde_json::to_string(&name).unwrap_or_default());
    let _ = writeln!(t, "  \"vid\": \"0x{RAZER:04x}\",\n  \"pid\": \"0x{pid:04x}\",\n  \"interface\": {interface},\n  \"transaction_id\": \"0x{tid:02x}\",");
    let _ = writeln!(t, "  \"profiles\": {profiles:?},");
    let layers = if hypershift { "[\"normal\", \"hypershift\"]" } else { "[\"normal\"]" };
    let _ = writeln!(t, "  \"layers\": {layers},");
    let _ = writeln!(t, "  \"dpi_max\": 16000,\n  \"_todo\": \"set dpi_max to your mouse's real maximum\",");
    if ids.is_empty() {
        let _ = writeln!(t, "  \"_todo_buttons\": \"the mouse didn't answer 0x02/0x84 (button list), add buttons by hand\",");
    }
    let buttons: Vec<String> = ids
        .iter()
        .map(|id| format!("    {{ \"id\": \"0x{id:02x}\", \"name\": \"{}\", \"_todo\": \"check with opensynapse watch\" }}", guess_name(*id)))
        .collect();
    let _ = write!(t, "  \"buttons\": [\n{}\n  ]\n}}\n", buttons.join(",\n"));
    Ok(Scaffold { name, json: t, transaction_id: tid, interface, buttons: ids.len() })
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
    fn slugs() {
        assert_eq!(slug("Razer Naga Pro (Wired)"), "razer-naga-pro-wired");
    }

    #[test]
    fn builtin_device_parses_hex_ids() {
        let naga = crate::device::definitions().into_iter().find(|d| d.pid == 0x008d).expect("builtin naga");
        assert_eq!((naga.vid, naga.transaction_id, naga.buttons.len()), (0x1532, 0x1f, 22));
        assert_eq!(naga.button("thumb1").map(|b| b.id), Some(0x40));
    }
}
