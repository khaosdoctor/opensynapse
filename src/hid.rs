//! Razer 90-byte feature-report transport over Linux hidraw.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

const REPORT_LEN: usize = 90;
const STATUS_BUSY: u8 = 0x01;
const STATUS_OK: u8 = 0x02;

// _IOC(_IOC_READ | _IOC_WRITE, 'H', nr, size)
const fn hid_ioc(nr: u64) -> u64 {
    (3 << 30) | (((REPORT_LEN + 1) as u64) << 16) | ((b'H' as u64) << 8) | nr
}
const HIDIOCSFEATURE: u64 = hid_ioc(0x06);
const HIDIOCGFEATURE: u64 = hid_ioc(0x07);

pub struct Hid {
    file: File,
    transaction_id: u8,
}

impl Hid {
    pub fn open(path: &Path, transaction_id: u8) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Self { file, transaction_id })
    }

    /// Sends one command and returns the 90-byte response. Fails on any non-OK status.
    pub fn command(&self, class: u8, id: u8, size: u8, args: &[u8]) -> io::Result<[u8; REPORT_LEN]> {
        let mut buf = [0u8; REPORT_LEN + 1]; // byte 0 = report id 0
        let report = &mut buf[1..];
        report[1] = self.transaction_id;
        report[5] = size;
        report[6] = class;
        report[7] = id;
        report[8..8 + args.len()].copy_from_slice(args);
        report[88] = report[2..88].iter().fold(0, |crc, b| crc ^ b);
        self.ioctl(HIDIOCSFEATURE, &mut buf)?;

        // the device answers "busy" until the command is processed
        for _ in 0..50 {
            sleep(Duration::from_millis(2));
            let mut resp = [0u8; REPORT_LEN + 1];
            self.ioctl(HIDIOCGFEATURE, &mut resp)?;
            let status = resp[1];
            if status == STATUS_BUSY {
                continue;
            }
            if status != STATUS_OK {
                return Err(io::Error::other(format!(
                    "device rejected command {class:02x}/{id:02x}: status {status:02x}"
                )));
            }
            let [_, out @ ..] = resp;
            return Ok(out);
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "device stayed busy"))
    }

    fn ioctl(&self, request: u64, buf: &mut [u8]) -> io::Result<()> {
        // SAFETY: buf is REPORT_LEN + 1 bytes, matching the size encoded in the request.
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), request as _, buf.as_mut_ptr()) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Connected Razer USB devices as (pid, name), one entry per device.
pub fn razer_devices() -> Vec<(u16, String)> {
    let mut found: Vec<(u16, String)> = Vec::new();
    let Ok(dir) = fs::read_dir("/sys/class/hidraw") else { return found };
    for entry in dir.flatten() {
        let Ok(uevent) = fs::read_to_string(entry.path().join("device/uevent")) else { continue };
        let field = |k: &str| uevent.lines().find_map(|l| l.strip_prefix(k)).map(str::to_string);
        let Some(id) = field("HID_ID=0003:00001532:") else { continue };
        let Ok(pid) = u16::from_str_radix(&id, 16) else { continue };
        if !found.iter().any(|(p, _)| *p == pid) {
            // HID_NAME is "<manufacturer> <product>", and Razer puts "Razer" in both
            let name = field("HID_NAME=").unwrap_or_default().replacen("Razer Razer ", "Razer ", 1);
            found.push((pid, name));
        }
    }
    found
}

/// Every hidraw node of a USB vid:pid, with its interface number.
pub fn find_all(vid: u16, pid: u16) -> Vec<(u8, PathBuf)> {
    let want = format!("HID_ID=0003:{vid:08X}:{pid:08X}");
    let mut found = Vec::new();
    let Ok(dir) = fs::read_dir("/sys/class/hidraw") else { return found };
    for entry in dir.flatten() {
        let dev = entry.path().join("device");
        let Ok(uevent) = fs::read_to_string(dev.join("uevent")) else { continue };
        if !uevent.lines().any(|l| l == want) {
            continue;
        }
        // .../1-9.3.2:1.0/0003:1532:008D.0017 -> interface is the digit after the last '.'
        let Ok(real) = fs::canonicalize(&dev) else { continue };
        let usb_if = real.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
        let Some(n) = usb_if.and_then(|s| s.rsplit('.').next()).and_then(|n| n.parse::<u8>().ok()) else {
            continue;
        };
        found.push((n, Path::new("/dev").join(entry.file_name())));
    }
    found
}

/// Whether a hidraw node's report descriptor declares the 90-byte feature
/// report Razer mice take commands on. False when sysfs can't be read.
pub fn has_control_report(node: &Path) -> bool {
    let Some(name) = node.file_name() else { return false };
    let path = Path::new("/sys/class/hidraw").join(name).join("device/report_descriptor");
    fs::read(path).is_ok_and(|d| descriptor_has_control_report(&d))
}

/// Walks the HID items, tracking Report Size and Report Count, and looks for
/// a Feature item that is REPORT_LEN bytes long.
fn descriptor_has_control_report(desc: &[u8]) -> bool {
    let mut size = 0u32;
    let mut count = 0u32;
    let mut i = 0;
    while i < desc.len() {
        let prefix = desc[i];
        // long item: 0xfe, data size, tag, data
        if prefix == 0xfe {
            i += 3 + desc.get(i + 1).copied().unwrap_or(0) as usize;
            continue;
        }
        let len = [0, 1, 2, 4][(prefix & 0x03) as usize];
        let data = desc.get(i + 1..i + 1 + len).unwrap_or(&[]);
        let value = data.iter().rev().fold(0u32, |v, b| (v << 8) | *b as u32);
        match prefix & 0xfc {
            0x74 => size = value,
            0x94 => count = value,
            0xb0 if size * count == REPORT_LEN as u32 * 8 => return true,
            _ => {}
        }
        i += 1 + len;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_razer_control_report() {
        // tail of a Naga Left Handed interface 0 descriptor: size 8, count 0x5a, feature
        let naga = [0x06, 0x00, 0xff, 0x09, 0x02, 0x15, 0x00, 0x25, 0x01, 0x75, 0x08, 0x95, 0x5a, 0xb1, 0x01, 0xc0];
        // a keyboard interface: no feature report at all
        let keyboard = [0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0xc0];
        // a 16-bit feature report that isn't the Razer one
        let other = [0x75, 0x10, 0x95, 0x01, 0xb1, 0x03];
        assert!(descriptor_has_control_report(&naga));
        assert!(!descriptor_has_control_report(&keyboard));
        assert!(!descriptor_has_control_report(&other));
    }
}
