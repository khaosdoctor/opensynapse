//! What the GUI holds between frames, and the queue of changes waiting for
//! WRITE TO DEVICE. Nothing in here talks to the mouse.

use crate::lib::lighting::ZoneUi;
use crate::lib::remapper::{Editor, Kind};
use eframe::egui;
use opensynapse::action::Action;
use opensynapse::device::{DeviceDef, Mouse, Profiles};
use opensynapse::input::Pressed;
use opensynapse::power::{Battery, Power};
use opensynapse::profile;
use opensynapse::rgb::{Effect, Lighting};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) struct App {
    pub(crate) mice: Vec<Mouse>,
    pub(crate) current: usize,
    /// connected Razer devices that have no definition, shown greyed out
    pub(crate) unsupported: Vec<String>,
    pub(crate) status: Result<String, String>,
    pub(crate) profile: u8,
    /// the stored profiles as the mouse reports them, None when it doesn't answer
    pub(crate) onboard: Option<Profiles>,
    pub(crate) names: BTreeMap<u8, String>,
    /// Some while RENAME is open, with the name typed so far
    pub(crate) renaming: Option<String>,
    pub(crate) layer: u8,
    pub(crate) actions: Vec<Option<Action>>,
    pub(crate) selected: usize,
    pub(crate) editor: Editor,
    /// the editor tab on screen, kept when another button is picked. It starts
    /// on the first button's type and only changes when you click a tab
    pub(crate) tab: Option<Kind>,
    pub(crate) dpi: u16,
    pub(crate) stages: Vec<u16>,
    pub(crate) active: u8,
    pub(crate) path: PathBuf,
    pub(crate) last_poll: Instant,
    pub(crate) ctx: egui::Context,
    pub(crate) pressed: Option<Arc<Mutex<Pressed>>>,
    /// button index lit briefly because the mouse changed DPI on its own
    pub(crate) flash: Option<(usize, Instant)>,
    /// changes waiting for APPLY
    pub(crate) ops: Vec<Op>,
    /// (dpi, active stage, stages) as last read from the device
    pub(crate) device_dpi: (u16, u8, Vec<u16>),
    /// a warning that waits for the user before IMPORT, RESET or SET DEFAULT
    pub(crate) confirm: Option<Confirm>,
    pub(crate) view: View,
    pub(crate) lighting: Option<Lighting>,
    pub(crate) zone_ui: Vec<ZoneUi>,
    /// what each zone has on the mouse, so edits can be queued as differences
    pub(crate) zone_base: Vec<ZoneUi>,
    /// one card drives every lighting zone
    pub(crate) sync_zones: bool,
    /// names in the presets folder, and the one last picked or saved
    pub(crate) presets: Vec<String>,
    pub(crate) preset: Option<String>,
    /// Some while SAVE AS is open: the name typed so far and the last error
    pub(crate) preset_name: Option<(String, Option<String>)>,
    /// polling rate and the rest that goes through the OpenRazer daemon
    pub(crate) power: Option<Power>,
    pub(crate) poll_rate: u16,
    /// the polling rate on the mouse, None when the daemon doesn't give one
    pub(crate) device_poll: Option<u16>,
    /// sleep timeout in seconds, the editor and the mouse's value like the polling rate
    pub(crate) idle: u16,
    pub(crate) device_idle: Option<u16>,
    /// None on wired mice
    pub(crate) battery: Option<Battery>,
    /// folder picked for profile files, None = the default config folder
    pub(crate) dir: Option<PathBuf>,
    /// (vid, pid) of a mouse that was unplugged; we keep polling until it comes back
    pub(crate) disconnected: Option<(u16, u16)>,
    pub(crate) last_daemon_check: Instant,
}

impl App {
    pub(crate) fn new(ctx: egui::Context) -> Self {
        let mut app = App {
            ctx,
            pressed: None,
            flash: None,
            ops: Vec::new(),
            device_dpi: (0, 0, Vec::new()),
            confirm: None,
            view: View::Buttons,
            lighting: None,
            zone_ui: Vec::new(),
            zone_base: Vec::new(),
            sync_zones: false,
            presets: Vec::new(),
            preset: None,
            preset_name: None,
            power: None,
            poll_rate: 0,
            device_poll: None,
            idle: 0,
            device_idle: None,
            battery: None,
            dir: None,
            disconnected: None,
            last_daemon_check: Instant::now(),
            mice: Vec::new(),
            current: 0,
            unsupported: Vec::new(),
            status: Ok(String::new()),
            profile: 0,
            onboard: None,
            names: BTreeMap::new(),
            renaming: None,
            layer: 0,
            actions: Vec::new(),
            selected: 0,
            editor: Editor::load(&Action::Disabled),
            tab: None,
            dpi: 800,
            stages: Vec::new(),
            active: 1,
            path: PathBuf::new(), // set per device/profile by update_path
            last_poll: Instant::now(),
        };
        app.connect();
        app.scan_presets();
        app
    }

    /// The path field follows the selected device, profile and folder.
    pub(crate) fn update_path(&mut self) {
        let Some(p) = self.mice.get(self.current).and_then(|m| profile::default_path(m, self.profile)) else { return };
        self.path = match (&self.dir, p.file_name()) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => p,
        };
    }

    pub(crate) fn select(&mut self, i: usize) {
        self.selected = i;
        if let Some(a) = self.effective(i) {
            self.editor = Editor::load(&a);
        }
    }

    /// Queued action for a button on the profile/layer being shown.
    pub(crate) fn pending(&self, index: usize) -> Option<&Action> {
        self.ops.iter().rev().find_map(|op| match op {
            Op::Button { profile, layer, index: i, action } if (*profile, *layer, *i) == (self.profile, self.layer, index) => {
                Some(action)
            }
            _ => None,
        })
    }

    /// What a button will do, the queued change or else the device value.
    pub(crate) fn effective(&self, index: usize) -> Option<Action> {
        self.pending(index).or(self.device_action(index)).cloned()
    }

    pub(crate) fn device_action(&self, i: usize) -> Option<&Action> {
        self.actions.get(i)?.as_ref()
    }

    /// Every edit goes straight to the queue. A valid action that differs
    /// from what the button will do is queued (or dropped when it's back to
    /// the device value). Buttons that couldn't be read are left alone.
    pub(crate) fn queue_edits(&mut self) {
        let (Ok(action), Some(now)) = (self.editor.build(), self.effective(self.selected)) else { return };
        // compared as bytes, a media key the MEDIA tab doesn't list comes back from the editor as Raw
        if action.encode() == now.encode() {
            return;
        }
        let index = self.selected;
        let name = self.mice.get(self.current).map_or(String::new(), |m| m.def.buttons[index].name.to_uppercase());
        let op = Op::Button { profile: self.profile, layer: self.layer, index, action: action.clone() };
        if self.device_action(index) == Some(&action) {
            unqueue(&mut self.ops, &op);
            self.status = Ok(format!("{name} REVERTED > nothing to write"));
            return;
        }
        queue(&mut self.ops, op);
        self.status = Ok(format!("QUEUED {name} > {action}"));
    }

    /// After ops are removed from the queue, put the editors back on what
    /// is still queued or on the device value, so they don't queue it again.
    pub(crate) fn reset_edits(&mut self) {
        self.select(self.selected);
        if !self.ops.iter().any(|o| matches!(o, Op::Dpi { .. })) {
            (self.dpi, self.active, self.stages) = self.device_dpi.clone();
        }
        for (z, base) in self.zone_ui.iter_mut().zip(&self.zone_base) {
            if !self.ops.iter().any(|o| matches!(o, Op::Light { zone, .. } if *zone == z.name)) {
                *z = base.clone();
            }
        }
        if !self.ops.iter().any(|o| matches!(o, Op::Light { .. })) {
            self.sync_zones = false;
        }
        if !self.ops.iter().any(|o| matches!(o, Op::Poll(_))) {
            self.poll_rate = self.device_poll.unwrap_or(0);
        }
        if !self.ops.iter().any(|o| matches!(o, Op::Idle(_))) {
            self.idle = self.device_idle.unwrap_or(0);
        }
    }

    /// The profiles on the mouse with the queued NEW and DELETE already applied.
    /// A mouse that doesn't answer the profile commands gets its device file's list, minus P0.
    pub(crate) fn profile_list(&self) -> Vec<u8> {
        let Some(on) = &self.onboard else {
            let all = self.mice.get(self.current).map_or(vec![0], |m| m.def.profiles.clone());
            return all.iter().copied().filter(|&p| p != 0 || all.len() == 1).collect();
        };
        let mut list = on.ids.clone();
        for op in &self.ops {
            match op {
                Op::CreateProfile(p) if !list.contains(p) => list.push(*p),
                Op::DeleteProfile(p) => list.retain(|x| x != p),
                _ => {}
            }
        }
        list.sort();
        list
    }

    pub(crate) fn profile_name(&self, profile: u8) -> Option<String> {
        // a queued DELETE PROFILE forgets the saved name, so a NEW PROFILE with the same number starts without one
        let mut name = self.names.get(&profile).cloned();
        for op in &self.ops {
            match op {
                Op::DeleteProfile(p) if *p == profile => name = None,
                Op::Rename { profile: p, name: n } if *p == profile => name = Some(n.clone()),
                _ => {}
            }
        }
        name.filter(|n| !n.trim().is_empty())
    }

    pub(crate) fn active_profile(&self) -> Option<u8> {
        self.onboard.as_ref().map(|o| o.active)
    }

    /// Same for DPI, valid values that differ from the device are queued and
    /// putting the device values back drops the queued change.
    pub(crate) fn queue_dpi_edits(&mut self) {
        let max = self.mice.get(self.current).map_or(0, |m| m.def.dpi_max);
        let valid = (1..=5).contains(&self.stages.len())
            && (1..=self.stages.len() as u8).contains(&self.active)
            && self.stages.iter().chain([&self.dpi]).all(|d| (100..=max).contains(d));
        let op = Op::Dpi { dpi: self.dpi, active: self.active, stages: self.stages.clone() };
        if (self.dpi, self.active, &self.stages) == (self.device_dpi.0, self.device_dpi.1, &self.device_dpi.2) {
            unqueue(&mut self.ops, &op);
            return;
        }
        if valid && queue(&mut self.ops, op) {
            self.status = Ok(format!("QUEUED DPI {} > STAGE {}", self.dpi, self.active));
        }
    }
}

/// A change waiting to be written, like a GParted pending operation.
#[derive(PartialEq)]
pub(crate) enum Op {
    Button { profile: u8, layer: u8, index: usize, action: Action },
    Dpi { dpi: u16, active: u8, stages: Vec<u16> },
    Light { zone: String, effect: Effect, brightness: u8 },
    /// polling rate in Hz
    Poll(u16),
    /// sleep timeout in seconds
    Idle(u16),
    CreateProfile(u8),
    DeleteProfile(u8),
    SetActive(u8),
    /// only saved on this computer, the mouse has no room for names
    Rename { profile: u8, name: String },
}

impl Op {
    fn same_target(&self, other: &Op) -> bool {
        match (self, other) {
            (Op::Button { profile: p, layer: l, index: i, .. }, Op::Button { profile, layer, index, .. }) => {
                (p, l, i) == (profile, layer, index)
            }
            (Op::Dpi { .. }, Op::Dpi { .. }) => true,
            (Op::Light { zone: a, .. }, Op::Light { zone: b, .. }) => a == b,
            (Op::Poll(_), Op::Poll(_)) | (Op::Idle(_), Op::Idle(_)) | (Op::SetActive(_), Op::SetActive(_)) => true,
            (Op::CreateProfile(a), Op::CreateProfile(b)) | (Op::DeleteProfile(a), Op::DeleteProfile(b)) => a == b,
            (Op::Rename { profile: a, .. }, Op::Rename { profile: b, .. }) => a == b,
            _ => false,
        }
    }

    pub(crate) fn describe(&self, def: &DeviceDef) -> String {
        match self {
            Op::Button { profile, layer, index, action } => {
                let layer = def.layers.get(*layer as usize).map_or("?", String::as_str);
                format!("P{profile} {} {} > {action}", layer.to_uppercase(), def.buttons[*index].name.to_uppercase())
            }
            Op::Dpi { dpi, active, stages } => {
                let list: Vec<String> = stages.iter().map(u16::to_string).collect();
                format!("DPI {dpi} > STAGES {} (#{active})", list.join("/"))
            }
            Op::Light { zone, effect, brightness } => format!("LIGHT {} > {effect:?} @{brightness}", zone.to_uppercase()),
            Op::Poll(hz) => format!("POLLING RATE > {hz} HZ"),
            Op::Idle(s) => format!("SLEEP AFTER > {s} S"),
            Op::CreateProfile(p) => format!("NEW PROFILE P{p}"),
            Op::DeleteProfile(p) => format!("DELETE PROFILE P{p}"),
            Op::SetActive(p) => format!("SET ACTIVE > P{p}"),
            Op::Rename { profile, name } if name.trim().is_empty() => format!("P{profile} > NO NAME"),
            Op::Rename { profile, name } => format!("RENAME P{profile} > {name}"),
        }
    }
}

/// A newer op for the same target replaces the older one. Returns false when
/// the exact op is queued already, so the editors that run every frame don't
/// move it to the end or repeat their status line.
pub(crate) fn queue(ops: &mut Vec<Op>, op: Op) -> bool {
    if ops.contains(&op) {
        return false;
    }
    unqueue(ops, &op);
    ops.push(op);
    true
}

pub(crate) fn unqueue(ops: &mut Vec<Op>, op: &Op) {
    ops.retain(|o| !o.same_target(op));
}

#[derive(Clone, Copy)]
pub(crate) enum Confirm {
    Import,
    ResetButtons,
    SetDefault,
    ResetLighting,
    DefaultLighting,
    FactoryLighting,
    DeletePreset,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum View {
    Buttons,
    Lighting,
    Settings,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(index: usize, action: Action) -> Op {
        Op::Button { profile: 0, layer: 0, index, action }
    }

    fn light(zone: &str, brightness: u8) -> Op {
        Op::Light { zone: zone.into(), effect: Effect::Spectrum, brightness }
    }

    #[test]
    fn queue_keeps_one_op_per_target() {
        let mut ops = Vec::new();
        assert!(queue(&mut ops, button(0, Action::Disabled)));
        assert!(queue(&mut ops, button(1, Action::Hypershift)));
        // editors queue every frame, the same op again must not move or repeat
        assert!(!queue(&mut ops, button(0, Action::Disabled)));
        assert!(ops == [button(0, Action::Disabled), button(1, Action::Hypershift)]);
        assert!(queue(&mut ops, button(0, Action::Mouse(1))));
        assert!(ops == [button(1, Action::Hypershift), button(0, Action::Mouse(1))]);
        // the same index on another layer or profile is another button
        assert!(queue(&mut ops, Op::Button { profile: 0, layer: 1, index: 0, action: Action::Disabled }));
        assert!(queue(&mut ops, Op::Button { profile: 1, layer: 0, index: 0, action: Action::Disabled }));
        assert_eq!(ops.len(), 4);

        let mut ops = Vec::new();
        queue(&mut ops, Op::Dpi { dpi: 800, active: 1, stages: vec![800] });
        queue(&mut ops, Op::Dpi { dpi: 1600, active: 2, stages: vec![800, 1600] });
        queue(&mut ops, Op::Poll(500));
        queue(&mut ops, Op::Poll(1000));
        queue(&mut ops, Op::Idle(60));
        queue(&mut ops, light("logo", 10));
        queue(&mut ops, light("scroll", 10));
        queue(&mut ops, light("logo", 20));
        let want = [
            Op::Dpi { dpi: 1600, active: 2, stages: vec![800, 1600] },
            Op::Poll(1000),
            Op::Idle(60),
            light("scroll", 10),
            light("logo", 20),
        ];
        assert!(ops == want);
    }

    #[test]
    fn unqueue_drops_by_target_whatever_the_value() {
        let mut ops = vec![button(0, Action::Mouse(1)), button(1, Action::Disabled), Op::Poll(500), light("logo", 10)];
        unqueue(&mut ops, &button(0, Action::Disabled));
        unqueue(&mut ops, &Op::Poll(0));
        unqueue(&mut ops, &light("logo", 99));
        assert!(ops == [button(1, Action::Disabled)]);
        // nothing queued for these, so nothing changes
        unqueue(&mut ops, &Op::Idle(60));
        unqueue(&mut ops, &light("scroll", 10));
        unqueue(&mut ops, &Op::Button { profile: 1, layer: 0, index: 1, action: Action::Disabled });
        assert!(ops == [button(1, Action::Disabled)]);
    }
}
