//! Starts a device file for a Razer mouse that doesn't have one, using only
//! read commands: the transport is found by trial and the buttons come from
//! asking the mouse, so there's no per-mouse table to keep up to date.

use crate::hid::{self, Hid};
use std::io;
use std::thread::sleep;
use std::time::Duration;

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

/// Opens the hidraw node of one interface, looking it up again on every try.
/// A probe on the wrong interface can reset the mouse and bring it back under
/// new hidraw numbers, and the node can show up before udev gives us access,
/// so we keep trying for up to 3s.
fn open(pid: u16, iface: u8, tid: u8) -> io::Result<Hid> {
    let mut last_err = io::Error::other(format!("interface {iface} didn't come back"));
    for _ in 0..30 {
        if let Some(path) = hid::find(RAZER, pid, iface) {
            match Hid::open(&path, tid) {
                Ok(hid) => return Ok(hid),
                Err(e) => last_err = e,
            }
        }
        sleep(Duration::from_millis(100));
    }
    Err(last_err)
}

/// Finds the interface and transaction id the mouse answers on, only with
/// reads. Sending to the wrong interface can reset some mice, so we only try
/// the ones whose descriptor has the 90-byte Razer report. When none of them
/// has it we try every interface in order.
fn connect(pid: u16) -> io::Result<(Hid, u8, u8)> {
    let all = hid::find_all(RAZER, pid);
    if all.is_empty() {
        return Err(io::Error::other(format!("no hidraw node for 1532:{pid:04x}, is it plugged in?")));
    }
    let any_control = all.iter().any(|(_, p)| hid::has_control_report(p));
    let mut ifaces: Vec<u8> =
        all.iter().filter(|(_, p)| !any_control || hid::has_control_report(p)).map(|(n, _)| *n).collect();
    ifaces.sort();
    let mut last_err = io::Error::other("the mouse didn't answer on any interface");
    for iface in ifaces {
        for tid in TIDS {
            // firmware version: a read every Razer mouse answers
            match open(pid, iface, tid) {
                Ok(hid) if hid.command(0x00, 0x81, 0x02, &[]).is_ok() => return Ok((hid, iface, tid)),
                Ok(_) => {}
                // the open doesn't depend on the transaction id, so the other ids would fail too
                Err(e) => {
                    last_err = e;
                    break;
                }
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
    fn slugs() {
        assert_eq!(slug("Razer Naga Pro (Wired)"), "razer-naga-pro-wired");
    }
}
