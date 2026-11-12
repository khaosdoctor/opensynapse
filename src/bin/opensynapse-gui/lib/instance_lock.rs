use crate::shared::widgets::{modal, neon, outline, tall};
use crate::state::App;
use crate::theme::t;
use eframe::egui;
use std::path::PathBuf;
use std::time::Duration;

fn lock_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    dir.join("opensynapse-gui.lock")
}

pub(crate) enum Lock {
    /// ours now, None when we couldn't even create the file and just start anyway
    Free(Option<std::fs::File>),
    /// another GUI has it, with its PID if the file says
    Busy(Option<i32>),
}

/// Only one GUI at a time, otherwise two of them fight over the same mouse.
/// We take an flock on the lock file and write our PID in it. The lock goes
/// away with the open file, so the kernel drops it when we exit, even after a crash.
pub(crate) fn take_lock() -> Lock {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    let path = lock_path();
    // no truncate here, the PID in it belongs to whoever holds the lock
    let opened = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path);
    let Ok(mut file) = opened else { return Lock::Free(None) };
    // SAFETY: flock only reads the fd, which `file` keeps open
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let pid = std::fs::read_to_string(&path).ok().and_then(|s| s.trim().parse().ok());
        return Lock::Busy(pid);
    }
    let _ = file.set_len(0).and_then(|_| write!(file, "{}", std::process::id()));
    Lock::Free(Some(file))
}

/// True while `pid` is a running opensynapse-gui. Checking the name means a
/// stale PID that now belongs to something else never gets killed.
fn is_gui(pid: i32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == "opensynapse-gui")
}

/// Stops the other GUI, asking nicely first and killing it after 2s, then
/// deletes the lock and takes a new one.
fn take_over(pid: Option<i32>) -> Option<std::fs::File> {
    if let Some(pid) = pid.filter(|&p| is_gui(p)) {
        // SAFETY: kill only sends a signal, and is_gui checked the PID is the other GUI
        unsafe { libc::kill(pid, libc::SIGTERM) };
        for _ in 0..20 {
            if !is_gui(pid) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if is_gui(pid) {
            // SAFETY: same as above
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
    }
    // a new file gets a new lock even if the old process somehow still holds the old one
    let _ = std::fs::remove_file(lock_path());
    match take_lock() {
        Lock::Free(file) => file,
        Lock::Busy(_) => None,
    }
}

pub(crate) struct Launcher {
    pub(crate) app: Option<App>,
    /// Some while the alert shows, with the other GUI's PID if we know it
    pub(crate) busy: Option<Option<i32>>,
    pub(crate) _lock: Option<std::fs::File>,
}

impl eframe::App for Launcher {
    fn on_exit(&mut self) {
        // only the window that started the log owns it, the "already running" one never did
        if self.app.is_some() {
            crate::lib::debug::finish();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(app) = &mut self.app {
            app.ui(ui, frame);
            return;
        }
        let pid = self.busy.flatten();
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |_| {});
        modal(&ctx, "already-running", 460.0, "! OPENSYNAPSE IS ALREADY RUNNING", |ui| {
            let which = pid.map_or("Another window".to_string(), |p| format!("Another window, PID {p},"));
            ui.label(format!(
                "{which} is open already, maybe minimized or on another workspace. You can go back to it, or close it and \
                 open here. Anything pending in the other window is lost."
            ));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if tall(ui, outline("QUIT", t().muted)).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if tall(ui, neon("CLOSE IT AND OPEN HERE", t().accent2)).clicked() {
                    self._lock = take_over(pid);
                    self.busy = None;
                    self.app = Some(App::new(ctx.clone()));
                }
            });
        });
    }
}
