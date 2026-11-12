//! Every place the GUI reads or writes the mouse or the OpenRazer daemon.

use crate::lib::lighting::ZoneUi;
use crate::lib::profile_presets;
use crate::state::{App, Op};
use opensynapse::action::Action;
use opensynapse::device::{self, DpiStages};
use opensynapse::power::{self, Power};
use opensynapse::rgb;
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
        self.onboard = None; // the previous device's answer says nothing about this one
        self.load_profiles();
        // P0 only mirrors the active profile, so the GUI opens on the active one
        let list = self.profile_list();
        self.profile = self.active_profile().filter(|p| list.contains(p)).or(list.first().copied()).unwrap_or(0);
        self.reload();
        self.update_path();
        self.load_lighting();
        self.load_power();
    }

    pub(crate) fn load_profiles(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        // a mouse that answered once has stored profiles, a later failed read doesn't make it lose them
        match m.profiles() {
            Ok(on) => self.onboard = Some(on),
            Err(_) if self.onboard.is_some() => {}
            Err(_) => self.onboard = None,
        }
        // only this mouse is at hand, so presets can be tried on it as if it had no stored profiles
        #[cfg(debug_assertions)]
        if std::env::var_os("OPENSYNAPSE_NO_PROFILES").is_some() {
            self.onboard = None;
        }
        self.names = profile::names(m);
        self.profile_presets = None;
        if self.onboard.is_none() {
            self.profile_presets = self.load_presets();
        }
    }

    /// Reads the polling rate, battery and sleep timeout from the OpenRazer
    /// daemon. A queued change stays in its editor.
    pub(crate) fn load_power(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        self.power = power::find(&m.serial);
        self.device_poll = self.power.as_ref().and_then(Power::poll_rate);
        self.device_idle = self.power.as_ref().and_then(Power::idle_time);
        self.battery = self.power.as_ref().and_then(Power::battery);
        if !self.ops.iter().any(|o| matches!(o, Op::Poll(_))) {
            self.poll_rate = self.device_poll.unwrap_or(0);
        }
        if !self.ops.iter().any(|o| matches!(o, Op::Idle(_))) {
            self.idle = self.device_idle.unwrap_or(0);
        }
    }

    pub(crate) fn reload(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        // a profile that NEW PROFILE hasn't written yet will start as a copy of the active one
        let fresh = self.ops.contains(&Op::CreateProfile(self.profile));
        let from = match self.active_profile() {
            Some(a) if fresh => a,
            _ => self.profile,
        };
        let slot = if self.profile_presets.is_some() { self.preset_slot() } else { from };
        self.actions = match self.preset_actions(from, self.layer) {
            Some(actions) => actions,
            None => m.def.buttons.iter().map(|b| m.get_button(slot, b.id, self.layer).ok()).collect(),
        };
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
        // fake devices have no DPI to follow and no hidraw node, which would read as unplugged
        #[cfg(debug_assertions)]
        if opensynapse::fake::dir().is_some() {
            return;
        }
        let Some(m) = self.mice.get(self.current) else { return };
        // the profile button changes the active profile without telling us
        if let (Some(on), Ok(p)) = (&mut self.onboard, m.profiles()) {
            *on = p;
        }
        let (Ok((dpi, _)), Ok(s)) = (m.dpi(), m.dpi_stages()) else {
            // a failed read is often just a busy mouse, gone from sysfs means unplugged
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
        // the charge moves on its own, so it's read again every few seconds
        self.battery = self.power.as_ref().and_then(Power::battery);
        let (Some(m), Some(light)) = (self.mice.get(self.current), &self.lighting) else { return };
        let online = rgb::daemon_sees(&m.serial);
        if online == light.uses_daemon() {
            return;
        }
        self.load_lighting();
        self.load_power();
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
        let mut active = self.active_profile();
        let preset_mode = self.profile_presets.is_some();
        let (slot, mut active_preset) = (self.preset_slot(), active.unwrap_or(1));
        while let Some(op) = self.ops.first() {
            let preset_op = preset_mode.then(|| profile_presets::apply(m, slot, op, &mut active_preset)).flatten();
            let r = match op {
                _ if preset_op.is_some() => preset_op.unwrap_or(Ok(())),
                // P0 is the live copy of the active profile, writing it too makes the change apply right away
                Op::Button { profile, layer, index, action } => {
                    let id = m.def.buttons[*index].id;
                    m.set_button(*profile, id, *layer, action).and_then(|_| match *profile {
                        // a mouse that can't say which profile is active gets P0 too, as before profiles were read
                        p if p != 0 && active.is_none_or(|a| a == p) => m.set_button(0, id, *layer, action),
                        _ => Ok(()),
                    })
                }
                // snapshot right away, before queued button changes reach the new profile
                Op::CreateProfile(p) => m
                    .create_profile(*p)
                    .and_then(|_| profile::copy(m, active.unwrap_or(1), *p))
                    .and_then(|_| match profile::original_path(m, *p) {
                        Some(path) => profile::export(m, *p, &path),
                        None => Ok(()),
                    }),
                Op::DeleteProfile(p) => m.delete_profile(*p).and_then(|_| profile::forget(m, *p)),
                Op::SetActive(p) => m.set_active_profile(*p).map(|_| active = Some(*p)),
                Op::Rename { profile: p, name } => profile::rename(m, *p, name),
                Op::Dpi { dpi, active, stages } => m
                    .set_dpi(*dpi, *dpi)
                    .and_then(|_| m.set_dpi_stages(&DpiStages { active: *active, stages: stages.clone() })),
                Op::Light { zone, effect, brightness } => match &self.lighting {
                    Some(light) => light.set_brightness(zone, *brightness).and_then(|_| light.set_effect(zone, effect)),
                    None => Err(std::io::Error::other("no lighting")),
                },
                Op::Poll(hz) => match &self.power {
                    Some(power) => power.set_poll_rate(*hz),
                    None => Err(std::io::Error::other("no OpenRazer daemon")),
                },
                Op::Idle(s) => match &self.power {
                    Some(power) => power.set_idle_time(*s),
                    None => Err(std::io::Error::other("no OpenRazer daemon")),
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
                self.after_apply();
                return;
            }
            self.ops.remove(0);
        }
        self.status = Ok(format!("{total} OPERATIONS APPLIED"));
        self.after_apply();
    }

    /// A deleted profile can't stay on screen, the active one takes its place.
    fn after_apply(&mut self) {
        self.load_profiles();
        let list = self.profile_list();
        if !list.contains(&self.profile) {
            self.profile = self.active_profile().or(list.first().copied()).unwrap_or(0);
            self.update_path();
        }
        self.reload();
        self.load_power();
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
        self.zone_ui = light.state(m).into_iter().map(|s| ZoneUi::from_effect(&s.zone, s.effect, s.brightness)).collect();
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
        self.status = self.save_profile(m, path).map(|_| format!("P{} SAVED > {}", self.profile, path.display())).map_err(|e| e.to_string());
    }

    /// Replaces the profile's original snapshot with what's on the mouse.
    pub(crate) fn set_default(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        let Some(path) = self.original_file(self.profile) else { return };
        self.status = self.save_profile(m, &path).map(|_| format!("P{} IS THE NEW ORIGINAL > {}", self.profile, path.display())).map_err(|e| e.to_string());
    }

    /// A preset that isn't on the mouse only exists as its file, so that's what gets copied.
    fn save_profile(&self, m: &device::Mouse, path: &std::path::Path) -> std::io::Result<()> {
        let preset = self.profile_presets.as_ref().filter(|p| p.active != self.profile);
        match (preset, profile::preset_path(m, self.profile)) {
            (Some(_), Some(file)) => std::fs::copy(file, path).map(|_| ()),
            _ if self.profile_presets.is_some() => profile::export(m, self.preset_slot(), path),
            _ => profile::export(m, self.profile, path),
        }
    }

    pub(crate) fn original_file(&self, p: u8) -> Option<std::path::PathBuf> {
        let m = self.mice.get(self.current)?;
        match self.profile_presets {
            Some(_) => profile_presets::original(m, p),
            None => profile::original_path(m, p),
        }
    }
}
