//! Every place the GUI reads or writes the mouse or the OpenRazer daemon.

use crate::lib::lighting::ZoneUi;
use crate::state::{App, Op};
use opensynapse::action::Action;
use opensynapse::device::{self, DpiStages};
use opensynapse::rgb::{self, Effect};
use opensynapse::{hid, input, profile};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(500);

impl App {
    pub(crate) fn connect(&mut self) {
        let detected = device::detect();
        let err = detected.as_ref().err().map(|e| e.to_string());
        self.mice = detected.unwrap_or_default();
        let known: Vec<u16> = self.mice.iter().map(|m| m.def.pid).collect();
        self.unsupported = hid::razer_devices()
            .into_iter()
            .filter(|(pid, _)| !known.contains(pid))
            .map(|(pid, name)| format!("{name} ({pid:04x})"))
            .collect();
        if self.mice.is_empty() {
            self.status = Err(err.unwrap_or_else(|| "NO SUPPORTED DEVICE > run `opensynapse init` for it".into()));
            return;
        }
        self.switch_to(self.current.min(self.mice.len() - 1));
    }

    pub(crate) fn switch_to(&mut self, i: usize) {
        self.current = i;
        let m = &self.mice[i];
        let ctx = self.ctx.clone();
        // the previous listener's threads keep running until their device goes away
        self.pressed = Some(input::listen(m.def.vid, m.def.pid, move || ctx.request_repaint()));
        self.status = match profile::ensure_defaults(m) {
            Ok(0) => Ok(format!("DEVICE > {}", m.def.name.to_uppercase())),
            Ok(n) => Ok(format!("DEVICE > {} > BACKED UP {n} PROFILES", m.def.name.to_uppercase())),
            Err(e) => Err(format!("could not create profile files: {e}")),
        };
        self.selected = 0;
        self.ops.clear(); // queued ops belong to the previous device
        self.reload();
        self.update_path();
        self.load_lighting();
    }

    pub(crate) fn reload(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        self.actions = m.def.buttons.iter().map(|b| m.get_button(self.profile, b.id, self.layer).ok()).collect();
        if let Ok((x, _)) = m.dpi() {
            self.dpi = x;
        }
        if let Ok(DpiStages { active, stages }) = m.dpi_stages() {
            (self.active, self.stages) = (active, stages);
        }
        self.device_dpi = (self.dpi, self.active, self.stages.clone());
        self.select(self.selected);
    }

    /// Follows DPI changes made with the mouse's own buttons, and notices
    /// when the mouse is unplugged or plugged back in.
    pub(crate) fn poll_dpi(&mut self) {
        self.ctx.request_repaint_after(POLL);
        if self.last_poll.elapsed() < POLL {
            return;
        }
        self.last_poll = Instant::now();
        if let Some((vid, pid)) = self.disconnected {
            self.reconnect(vid, pid);
            return;
        }
        let Some(m) = self.mice.get(self.current) else { return };
        let (Ok((dpi, _)), Ok(s)) = (m.dpi(), m.dpi_stages()) else {
            // a failed read is often just a busy mouse; gone from sysfs means unplugged
            if hid::find_all(m.def.vid, m.def.pid).is_empty() {
                self.disconnected = Some((m.def.vid, m.def.pid));
                self.status = Err(format!("{} DISCONNECTED", m.def.name.to_uppercase()));
            }
            return;
        };
        if (dpi, s.active) == (self.device_dpi.0, self.device_dpi.1) {
            return;
        }
        // light up whichever button is mapped to the DPI step that just happened
        let step = Action::Dpi(if dpi > self.device_dpi.0 { 1 } else { 2 });
        let button = self.actions.iter().position(|a| a.as_ref() == Some(&step));
        self.flash = button.map(|b| (b, Instant::now()));
        (self.dpi, self.active) = (dpi, s.active);
        self.device_dpi = (dpi, s.active, s.stages);
    }

    /// Notices the OpenRazer daemon starting, stopping or losing the mouse, and
    /// reloads the lighting tab when it does. Queued lighting changes are kept.
    pub(crate) fn poll_daemon(&mut self) {
        const EVERY: Duration = Duration::from_secs(3);
        self.ctx.request_repaint_after(EVERY);
        if self.last_daemon_check.elapsed() < EVERY {
            return;
        }
        self.last_daemon_check = Instant::now();
        let (Some(m), Some(light)) = (self.mice.get(self.current), &self.lighting) else { return };
        let online = rgb::daemon_sees(&m.serial);
        if online == light.uses_daemon() {
            return;
        }
        self.load_lighting();
        for op in &self.ops {
            let Op::Light { zone, effect, brightness } = op else { continue };
            if let Some(z) = self.zone_ui.iter_mut().find(|z| z.name == *zone) {
                *z = ZoneUi::from_effect(zone, *effect, *brightness);
            }
        }
        let state = match online {
            true => "ONLINE",
            false => "OFFLINE",
        };
        self.status = Ok(format!("OPENRAZER {state}"));
    }

    /// Tries to open the mouse again, keeping what was queued before the unplug.
    fn reconnect(&mut self, vid: u16, pid: u16) {
        if hid::find_all(vid, pid).is_empty() {
            return;
        }
        let ops = std::mem::take(&mut self.ops);
        self.connect();
        self.ops = ops;
        // right after a replug udev may not have given us access yet, then the next poll tries again
        if self.mice.iter().any(|m| (m.def.vid, m.def.pid) == (vid, pid)) {
            self.disconnected = None;
        }
    }

    /// Runs the queue in order and stops at the first failure, keeping what's left.
    pub(crate) fn apply(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        let total = self.ops.len();
        while let Some(op) = self.ops.first() {
            let r = match op {
                Op::Button { profile, layer, index, action } => {
                    m.set_button(*profile, m.def.buttons[*index].id, *layer, action)
                }
                Op::Dpi { dpi, active, stages } => m
                    .set_dpi(*dpi, *dpi)
                    .and_then(|_| m.set_dpi_stages(&DpiStages { active: *active, stages: stages.clone() })),
                Op::Light { zone, effect, brightness } => match &self.lighting {
                    Some(light) => light.set_brightness(zone, *brightness).and_then(|_| light.set_effect(zone, effect)),
                    None => Err(std::io::Error::other("no lighting")),
                },
            };
            if let (Ok(_), Op::Light { zone, .. }) = (&r, op) {
                let done = self.zone_ui.iter().find(|z| z.name == *zone).cloned();
                if let (Some(done), Some(base)) = (done, self.zone_base.iter_mut().find(|z| z.name == *zone)) {
                    *base = done;
                }
            }
            if let Err(e) = r {
                let done = total - self.ops.len();
                self.status = Err(format!("{done}/{total} APPLIED > {} FAILED: {e}", op.describe(&m.def)));
                self.reload();
                return;
            }
            self.ops.remove(0);
        }
        self.status = Ok(format!("{total} OPERATIONS APPLIED"));
        self.reload();
    }

    /// Reads zones from the OpenRazer driver and pre-fills each card with what
    /// the mouse reports, falling back to OpenRazer's idea of it for zones the
    /// mouse doesn't answer for.
    pub(crate) fn load_lighting(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        self.lighting = rgb::find(m.def.vid, m.def.pid, &m.serial);
        let Some(light) = &self.lighting else {
            self.zone_ui.clear();
            return;
        };
        self.zone_ui = light
            .zones
            .iter()
            .map(|z| match rgb::led_id(z).and_then(|led| m.led(led).ok()) {
                Some((effect, brightness)) => ZoneUi::from_effect(z, effect, brightness),
                None => ZoneUi::from_effect(z, light.current_effect(z).unwrap_or(Effect::Spectrum), light.brightness(z).unwrap_or(255)),
            })
            .collect();
        self.zone_base = self.zone_ui.clone();
        // the first time we see this mouse its lighting becomes the original RESET goes back to
        let Some(path) = self.lighting_original().filter(|p| !p.exists()) else { return };
        if let Err(e) = device::save_json(&path, &self.lighting_now()) {
            self.status = Err(format!("could not save the original lighting: {e}"));
        }
    }

    /// Saves what's on the mouse for the shown profile to `path`.
    pub(crate) fn export_profile(&mut self, path: &std::path::Path) {
        let Some(m) = self.mice.get(self.current) else { return };
        self.status = profile::export(m, self.profile, path).map(|_| format!("P{} SAVED > {}", self.profile, path.display())).map_err(|e| e.to_string());
    }

    /// Replaces the profile's original snapshot with what's on the mouse.
    pub(crate) fn set_default(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        let Some(path) = profile::original_path(m, self.profile) else { return };
        self.status = profile::export(m, self.profile, &path).map(|_| format!("P{} IS THE NEW ORIGINAL > {}", self.profile, path.display())).map_err(|e| e.to_string());
    }
}
