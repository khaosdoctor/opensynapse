//! Which buttons are held right now, read from hidraw input reports.
//! hidraw is not affected by evdev grabs, so this works while keyd is running.

use crate::action::Action;
use crate::hid;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A wheel notch is a single report, so it stays "pressed" this long.
const WHEEL_HOLD: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct Pressed {
    /// bit 0 = left ... bit 4 = forward
    mouse: u8,
    /// keyboard usages held, per interface
    keys: HashMap<u8, HashSet<u8>>,
    /// (9 = up | 10 = down, when)
    wheel: Option<(u8, Instant)>,
}

impl Pressed {
    fn mouse_down(&self, button: u8) -> bool {
        match button {
            1..=5 => self.mouse & (1 << (button - 1)) != 0,
            9 | 10 => self.wheel.is_some_and(|(b, t)| b == button && t.elapsed() < WHEEL_HOLD),
            _ => false,
        }
    }

    fn key_down(&self, usage: u8) -> bool {
        self.keys.values().any(|s| s.contains(&usage))
    }

    /// Is a button with this action being pressed? DPI/profile actions are handled
    /// inside the mouse and never show up as input.
    pub fn triggers(&self, action: &Action) -> bool {
        match action {
            Action::Key { usage: 0, mods } => (0..8).any(|i| mods & (1 << i) != 0 && self.key_down(0xe0 + i)),
            Action::Key { usage, .. } | Action::KeyTurbo { usage, .. } => self.key_down(*usage),
            Action::Mouse(b) | Action::MouseTurbo { button: b, .. } => self.mouse_down(*b),
            _ => false,
        }
    }

    /// Applies one report; returns whether anything visible changed.
    // ponytail: assumes the common Razer layout (interface 0 = mouse, others =
    // keyboard with an optional report id 1). Parse the report descriptor if a mouse differs.
    fn apply(&mut self, interface: u8, r: &[u8]) -> bool {
        if interface == 0 {
            let Some(&wheel) = r.get(3) else { return false };
            let buttons = r[0] & 0x1f;
            let changed = buttons != self.mouse || wheel != 0;
            self.mouse = buttons;
            if wheel != 0 {
                self.wheel = Some((if (wheel as i8) > 0 { 9 } else { 10 }, Instant::now()));
            }
            return changed;
        }
        let (mods, keys) = match r {
            [0x01, mods, keys @ ..] if r.len() == 16 => (*mods, keys),
            [mods, _, keys @ ..] if r.len() == 8 => (*mods, keys),
            _ => return false,
        };
        let mut held: HashSet<u8> = keys.iter().copied().filter(|&k| k > 3).collect();
        held.extend((0..8).filter(|i| mods & (1 << i) != 0).map(|i| 0xe0 + i));
        let old = self.keys.insert(interface, held.clone());
        old.as_ref() != Some(&held)
    }
}

/// Starts one reader thread per hidraw node of the device. `on_change` runs after
/// every visible change (use it to request a repaint). Threads end when the device goes away.
pub fn listen(vid: u16, pid: u16, on_change: impl Fn() + Send + Sync + 'static) -> Arc<Mutex<Pressed>> {
    let state = Arc::new(Mutex::new(Pressed::default()));
    let on_change = Arc::new(on_change);
    for (interface, path) in hid::find_all(vid, pid) {
        let Ok(mut file) = File::open(&path) else { continue };
        let (state, on_change) = (state.clone(), on_change.clone());
        thread::spawn(move || {
            let mut buf = [0u8; 64];
            while let Ok(n) = file.read(&mut buf) {
                let changed = state.lock().is_ok_and(|mut s| s.apply(interface, &buf[..n]));
                if changed {
                    on_change();
                }
            }
        });
    }
    state
}
