//! The session's debug log. Every line goes to the journal when systemd runs
//! one, to a file that DEBUG LOG opens, and to the list under the queue.

use opensynapse::device::DeviceDef;
use opensynapse::hid::{self, Exchange};
use opensynapse::trace;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Kind {
    /// what was sent to or read from the mouse
    Raw,
    Info,
    Warn,
    Error,
}

#[derive(Clone)]
pub(crate) struct Line {
    pub(crate) time: String,
    pub(crate) kind: Kind,
    pub(crate) message: String,
    /// the bytes behind a translated command, None when the message is the bytes already
    pub(crate) bytes: Option<String>,
}

static LINES: Mutex<Vec<Line>> = Mutex::new(Vec::new());
static FILE: Mutex<Option<File>> = Mutex::new(None);
/// the mouse on screen, its device file names the buttons in translated commands
static DEVICE: Mutex<Option<DeviceDef>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// $XDG_STATE_HOME/opensynapse/debug.log, or ~/.local/state.
pub(crate) fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(state.join("opensynapse/debug.log"))
}

/// Each session starts an empty file, the journal keeps the history.
pub(crate) fn start() {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    *lock(&FILE) = File::create(&path).ok();
    hid::trace(write, exchange);
    write(&format!("opensynapse-gui v{} started", env!("CARGO_PKG_VERSION")));
}

/// A killed GUI never gets here, its file stays until the next start empties it.
pub(crate) fn finish() {
    *lock(&FILE) = None;
    if let Some(path) = path() {
        let _ = fs::remove_file(path);
    }
}

pub(crate) fn set_device(def: &DeviceDef) {
    *lock(&DEVICE) = Some(def.clone());
}

/// Status lines and OpenRazer writes, their first word says what kind they are.
pub(crate) fn write(message: &str) {
    let first = message.split_whitespace().next().unwrap_or_default();
    let kind = match first {
        "ERROR" | "FAILED" => Kind::Error,
        "WARN" => Kind::Warn,
        "OPENRAZER" | "SYSFS" => Kind::Raw,
        _ => Kind::Info,
    };
    push(kind, message.to_string(), None);
}

/// A command another mouse uses differently, or one nobody has decoded, stays as its bytes.
fn exchange(e: &Exchange) {
    let kind = if e.reply.is_err() { Kind::Error } else { Kind::Raw };
    let words = lock(&DEVICE).as_ref().and_then(|def| trace::describe(def, e));
    match words {
        Some(words) => push(kind, format!("HID {words}"), Some(trace::raw(e))),
        None => push(kind, format!("HID {}", trace::raw(e)), None),
    }
}

fn push(kind: Kind, message: String, bytes: Option<String>) {
    let time = clock();
    let full = bytes.as_ref().map_or(message.clone(), |b| format!("{message} | {b}"));
    if let Some(f) = lock(&FILE).as_mut() {
        let _ = writeln!(f, "[{time}] :: {full}");
    }
    // tests write lines too, the user's journal shouldn't get them
    #[cfg(not(test))]
    journal(kind, &full);
    let mut lines = lock(&LINES);
    // the file and the journal keep everything, the panel only needs the recent part
    if lines.len() >= 2000 {
        lines.drain(..500);
    }
    lines.push(Line { time, kind, message, bytes });
}

pub(crate) fn lines() -> Vec<Line> {
    lock(&LINES).clone()
}

/// journald's native protocol, one datagram per entry. A newline would start a new field, so it becomes a space.
#[cfg(not(test))]
fn journal(kind: Kind, message: &str) {
    let Ok(socket) = std::os::unix::net::UnixDatagram::unbound() else { return };
    // syslog levels, so journalctl -p can filter and colors them the same way the panel does
    let priority = match kind {
        Kind::Error => 3,
        Kind::Warn => 4,
        Kind::Info => 6,
        Kind::Raw => 7,
    };
    let entry = format!("SYSLOG_IDENTIFIER=opensynapse-gui\nPRIORITY={priority}\nMESSAGE={}\n", message.replace('\n', " "));
    let _ = socket.send_to(entry.as_bytes(), "/run/systemd/journal/socket");
}

/// Local HH:MM:SS, through libc since std has no time zones.
fn clock() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as libc::time_t;
    // SAFETY: localtime_r only writes into the tm we hand it
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Nothing here calls start(), so no file is written. push() still sends each
    // line to the journal when one is running, so the messages say they are tests.
    fn kind_of(message: &str) -> Kind {
        write(message);
        lines().into_iter().rev().find(|l| l.message == message).map(|l| l.kind).expect("the line was kept")
    }

    #[test]
    fn the_first_word_sets_the_kind() {
        assert_eq!(kind_of("ERROR test: the first word is ERROR"), Kind::Error);
        assert_eq!(kind_of("FAILED test: the first word is FAILED"), Kind::Error);
        assert_eq!(kind_of("WARN test: the first word is WARN"), Kind::Warn);
        assert_eq!(kind_of("OPENRAZER test: the first word is OPENRAZER"), Kind::Raw);
        assert_eq!(kind_of("SYSFS test: the first word is SYSFS"), Kind::Raw);
        assert_eq!(kind_of("test: any other first word"), Kind::Info);
        // only the first word counts
        assert_eq!(kind_of("test mentions ERROR later"), Kind::Info);
    }

    #[test]
    fn a_failed_exchange_is_an_error_and_an_untranslated_one_has_no_bytes_line() {
        let exchange_of = |reply: Result<&'static [u8], String>, sent: &str| {
            let got = if reply.is_ok() { "ok []".to_string() } else { "error: test".to_string() };
            Exchange { class: 0x7f, id: 0x01, args: &[], reply, sent: sent.into(), got }
        };
        exchange(&exchange_of(Ok(&[]), "7f/01 test ok"));
        exchange(&exchange_of(Err("test".into()), "7f/01 test failed"));
        let all = lines();
        let find = |text: &str| all.iter().rev().find(|l| l.message.contains(text)).cloned().expect("the exchange was kept");
        // an unknown class has no translation, so the message is the bytes and there is nothing under it
        let line = find("7f/01 test ok");
        assert_eq!((line.kind, line.bytes), (Kind::Raw, None));
        assert_eq!(find("7f/01 test failed").kind, Kind::Error);
    }
}
