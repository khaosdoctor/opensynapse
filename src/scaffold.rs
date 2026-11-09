//! Starts a device file for a Razer mouse that doesn't have one, using only
//! read commands: the transport is found by trial and the buttons come from
//! asking the mouse, so there's no per-mouse table to keep up to date.

use crate::hid::{self, Hid};
use std::io;

const RAZER: u16 = 0x1532;
/// Transaction ids used across Razer mice (OpenRazer, OpenRGB), newest first.
const TIDS: [u8; 3] = [0x1f, 0x3f, 0xff];

pub struct Scaffold {
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

/// Finds the interface and transaction id the mouse answers on by trying
/// every interface with every known transaction id, with reads only.
fn connect(pid: u16) -> io::Result<(Hid, u8, u8)> {
    let mut ifaces = hid::find_all(RAZER, pid);
    if ifaces.is_empty() {
        return Err(io::Error::other(format!("no hidraw node for 1532:{pid:04x}, is it plugged in?")));
    }
    ifaces.sort_by_key(|(n, _)| *n);
    let mut last_err = io::Error::other("the mouse didn't answer on any interface");
    for (iface, path) in &ifaces {
        for tid in TIDS {
            // firmware version: a read every Razer mouse answers
            match Hid::open(path, tid) {
                Ok(hid) if hid.command(0x00, 0x81, 0x02, &[]).is_ok() => return Ok((hid, *iface, tid)),
                Ok(_) => {}
                Err(e) => last_err = e,
            }
        }
    }
    Err(last_err)
}

/// `name` is what the mouse reports over USB, like "Razer Naga Left Handed".
pub fn scaffold(pid: u16, name: &str) -> io::Result<Scaffold> {
    let (hid, interface, tid) = connect(pid)?;

    // 0x02/0x84: [count, ids...]
    let ids: Vec<u8> =
        hid.command(0x02, 0x84, 0x50, &[]).map(|r| r[9..9 + (r[8] as usize).min(78)].to_vec()).unwrap_or_default();
    // which profiles answer a keymap read for the first button
    let first = ids.first().copied().unwrap_or(0x01);
    let profiles: Vec<u8> =
        (0..=5u8).filter(|&p| hid.command(0x02, 0x8c, 0x0a, &[p, first, 0]).is_ok()).collect();
    let hypershift = hid.command(0x02, 0x8c, 0x0a, &[0, first, 1]).is_ok();

    let name_json = serde_json::to_string(&name).unwrap_or_default();
    let layers = if hypershift { "[\"normal\", \"hypershift\"]" } else { "[\"normal\"]" };
    let todo = if ids.is_empty() {
        "the mouse didn't answer 0x02/0x84 (button list), add buttons by hand"
    } else {
        "name the buttons, `opensynapse dump` shows what each one does and `opensynapse watch` shows the ones you press"
    };
    let buttons: Vec<String> = ids
        .iter()
        .map(|id| format!("    {{ \"id\": \"0x{id:02x}\", \"name\": \"{}\" }}", guess_name(*id)))
        .collect();
    let buttons = buttons.join(",\n");
    // written by hand to keep ids in hex and one button per line
    let json = format!(
        "// Started by `opensynapse init` with read-only queries. Check every TODO,
// then see https://github.com/{repo}/blob/main/CONTRIBUTING.md for testing it and sending it.
{{
  \"name\": {name_json}, // TODO: the product name, it also names the protocol and device files
  \"vid\": \"0x{RAZER:04x}\",
  \"pid\": \"0x{pid:04x}\",
  \"interface\": {interface},
  \"transaction_id\": \"0x{tid:02x}\",
  \"profiles\": {profiles:?},
  \"layers\": {layers},
  \"dpi_max\": 16000, // TODO: the top DPI on your mouse's Razer product page
  // TODO: {todo}
  \"buttons\": [
{buttons}
  ]
}}
",
        repo = crate::protocol::REPO
    );
    Ok(Scaffold { json, transaction_id: tid, interface, buttons: ids.len() })
}

/// "Razer Naga Pro (Wired)" -> "razer-naga-pro-wired"
pub fn slug(name: &str) -> String {
    name.to_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_comments() {
        let src = "// head\n{ \"a\": \"x // \\\"y\", // note\n \"b\": 1 } // end";
        let v: serde_json::Value = serde_json::from_str(&crate::device::strip_comments(src)).unwrap();
        assert_eq!((v["a"].as_str(), v["b"].as_u64()), (Some("x // \"y"), Some(1)));
    }

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
