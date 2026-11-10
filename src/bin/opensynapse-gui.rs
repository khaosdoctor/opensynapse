use eframe::egui::{
    self, Align, Align2, Color32, ComboBox, StrokeKind, CornerRadius, DragValue, FontFamily, FontId, Frame, Layout, Margin, RichText, Stroke,
    TextEdit,
};
use opensynapse::action::{Action, key_name};
use opensynapse::device::{self, Button, DeviceDef, DpiStages, Mouse};
use opensynapse::hid;
use opensynapse::input::{self, Pressed};
use opensynapse::profile;
use opensynapse::rgb::{self, Effect, Lighting};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(500);
/// The mouse body ends at y 74%; reserve a bit below it for side plates.
const USED_HEIGHT: f32 = 0.78;
/// The remap editor at the bottom never grows; the mouse view gets the rest.
const EDITOR_HEIGHT: f32 = 344.0;
const SIDEBAR_WIDTH: f32 = 250.0;
/// The mouse drawing never grows past this, so big screens don't get a giant mouse.
const MAX_VIEW_WIDTH: f32 = 900.0;

const BG: Color32 = Color32::from_rgb(0x07, 0x07, 0x0d);
const PANEL: Color32 = Color32::from_rgb(0x0e, 0x0e, 0x1a);
const CARD: Color32 = Color32::from_rgb(0x14, 0x14, 0x24);
const CYAN: Color32 = Color32::from_rgb(0x00, 0xf0, 0xff);
const PINK: Color32 = Color32::from_rgb(0xff, 0x2a, 0x6d);
const YELLOW: Color32 = Color32::from_rgb(0xfc, 0xee, 0x0a);
const DIM: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x7a);
const RED: Color32 = Color32::from_rgb(0xff, 0x30, 0x30);
const GREEN: Color32 = Color32::from_rgb(0x39, 0xff, 0x14);
/// edge of a button whose action is OFF
const OFF: Color32 = Color32::from_rgb(0x2a, 0x2a, 0x3a);

/// (title, grid columns, keys): each group renders as its own column.
const SPECIAL_KEYS: &[(&str, usize, &[&str])] = &[
    (
        "FUNCTION",
        6,
        &[
            "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15", "f16", "f17",
            "f18", "f19", "f20", "f21", "f22", "f23", "f24",
        ],
    ),
    ("EDIT", 3, &["esc", "tab", "enter", "backspace", "space", "capslock", "insert", "delete", "menu"]),
    ("NAV", 2, &["home", "end", "pageup", "pagedown", "up", "down", "left", "right"]),
    ("SYSTEM", 1, &["sysrq", "scrolllock", "pause"]),
];

/// Media tab. Mute and volume are keyboard keys (verified on the Naga), the
/// rest are HID consumer usages.
const MEDIA_KEYS: &[(&str, Action)] = &[
    ("PLAY/PAUSE", Action::Media(0xcd)),
    ("PLAY", Action::Media(0xb0)),
    ("PAUSE", Action::Media(0xb1)),
    ("STOP", Action::Media(0xb7)),
    ("PREVIOUS", Action::Media(0xb6)),
    ("NEXT", Action::Media(0xb5)),
    ("MUTE", Action::Key { mods: 0, usage: 0x7f }),
    ("VOLUME -", Action::Key { mods: 0, usage: 0x81 }),
    ("VOLUME +", Action::Key { mods: 0, usage: 0x80 }),
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Key,
    Mouse,
    Media,
    Dpi,
    Hypershift,
    Disabled,
    Raw,
}

const KINDS: [(Kind, &str); 7] = [
    (Kind::Key, "KEY"),
    (Kind::Mouse, "MOUSE"),
    (Kind::Media, "MEDIA"),
    (Kind::Dpi, "DPI"),
    (Kind::Hypershift, "HYPERSHIFT"),
    (Kind::Raw, "RAW"),
    (Kind::Disabled, "OFF"),
];

struct Editor {
    kind: Kind,
    mods: u8,
    key: String,
    mouse: u8,
    /// repeat the key or mouse button while held
    turbo: bool,
    rate: u16,
    media: Action,
    dpi_up: bool,
    raw: String,
}

impl Editor {
    fn load(a: &Action) -> Self {
        let mut e = Editor {
            kind: Kind::Raw,
            mods: 0,
            key: String::new(),
            mouse: 1,
            turbo: false,
            rate: 20,
            media: MEDIA_KEYS[0].1.clone(),
            dpi_up: true,
            raw: String::new(),
        };
        if MEDIA_KEYS.iter().any(|(_, m)| m == a) {
            (e.kind, e.media) = (Kind::Media, a.clone());
            return e;
        }
        match a {
            Action::Key { mods, usage } | Action::KeyTurbo { mods, usage, .. } => {
                e.kind = Kind::Key;
                e.mods = *mods;
                e.key = key_name(*usage).unwrap_or(format!("0x{usage:02x}"));
            }
            Action::Mouse(b) => (e.kind, e.mouse) = (Kind::Mouse, *b),
            Action::MouseTurbo { button, .. } => (e.kind, e.mouse) = (Kind::Mouse, *button),
            Action::Dpi(n) => (e.kind, e.dpi_up) = (Kind::Dpi, *n == 1),
            Action::Hypershift => e.kind = Kind::Hypershift,
            Action::Disabled => e.kind = Kind::Disabled,
            _ => e.raw = a.encode().iter().map(|b| format!("{b:02x}")).collect(),
        }
        if let Action::KeyTurbo { interval, .. } | Action::MouseTurbo { interval, .. } = a {
            (e.turbo, e.rate) = (true, *interval);
        }
        e
    }

    fn build(&self) -> Result<Action, String> {
        Ok(match self.kind {
            Kind::Key => {
                // the box also takes `ctrl+shift+z`, on top of the modifier chips
                let text = self.key.trim();
                let Ok(Action::Key { mods, usage }) = format!("key:{text}").parse() else {
                    return Err(format!("unknown key '{text}'"));
                };
                let mods = mods | self.mods;
                if self.turbo {
                    return Ok(Action::KeyTurbo { mods, usage, interval: self.rate });
                }
                Action::Key { mods, usage }
            }
            Kind::Mouse if self.turbo => Action::MouseTurbo { button: self.mouse, interval: self.rate },
            Kind::Mouse => Action::Mouse(self.mouse),
            Kind::Media => self.media.clone(),
            Kind::Dpi => Action::Dpi(if self.dpi_up { 1 } else { 2 }),
            Kind::Hypershift => Action::Hypershift,
            Kind::Disabled => Action::Disabled,
            Kind::Raw => format!("raw:{}", self.raw.trim()).parse()?,
        })
    }
}

struct App {
    mice: Vec<Mouse>,
    current: usize,
    /// connected Razer devices that have no definition, shown greyed out
    unsupported: Vec<String>,
    status: Result<String, String>,
    profile: u8,
    layer: u8,
    actions: Vec<Option<Action>>,
    selected: usize,
    editor: Editor,
    dpi: u16,
    stages: Vec<u16>,
    active: u8,
    path: String,
    last_poll: Instant,
    ctx: egui::Context,
    pressed: Option<Arc<Mutex<Pressed>>>,
    /// button index lit briefly because the mouse changed DPI on its own
    flash: Option<(usize, Instant)>,
    /// changes waiting for APPLY
    ops: Vec<Op>,
    /// (dpi, active stage, stages) as last read from the device
    device_dpi: (u16, u8, Vec<u16>),
    /// IMPORT was clicked and the overwrite warning is showing
    confirm_import: bool,
    /// a RESET or SET DEFAULT warning is showing
    confirm: Option<Confirm>,
    view: View,
    lighting: Option<Lighting>,
    zone_ui: Vec<ZoneUi>,
    /// what each zone has on the mouse, so edits can be queued as differences
    zone_base: Vec<ZoneUi>,
    /// one card drives every lighting zone
    sync_zones: bool,
    /// folder picked for profile files, None = the default config folder
    dir: Option<PathBuf>,
    /// (vid, pid) of a mouse that was unplugged; we keep polling until it comes back
    disconnected: Option<(u16, u16)>,
    last_daemon_check: Instant,
}

impl App {
    fn new(ctx: egui::Context) -> Self {
        let mut app = App {
            ctx,
            pressed: None,
            flash: None,
            ops: Vec::new(),
            device_dpi: (0, 0, Vec::new()),
            confirm_import: false,
            confirm: None,
            view: View::Buttons,
            lighting: None,
            zone_ui: Vec::new(),
            zone_base: Vec::new(),
            sync_zones: false,
            dir: None,
            disconnected: None,
            last_daemon_check: Instant::now(),
            mice: Vec::new(),
            current: 0,
            unsupported: Vec::new(),
            status: Ok(String::new()),
            profile: 0,
            layer: 0,
            actions: Vec::new(),
            selected: 0,
            editor: Editor::load(&Action::Disabled),
            dpi: 800,
            stages: Vec::new(),
            active: 1,
            path: String::new(), // set per device/profile by update_path
            last_poll: Instant::now(),
        };
        app.connect();
        app
    }

    fn connect(&mut self) {
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

    fn switch_to(&mut self, i: usize) {
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

    /// The path field follows the selected device, profile and folder.
    fn update_path(&mut self) {
        let Some(p) = self.mice.get(self.current).and_then(|m| profile::default_path(m, self.profile)) else { return };
        let p = match (&self.dir, p.file_name()) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => p,
        };
        self.path = p.display().to_string();
    }

    fn reload(&mut self) {
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
    fn poll_dpi(&mut self) {
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
    fn poll_daemon(&mut self) {
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

    fn select(&mut self, i: usize) {
        self.selected = i;
        if let Some(a) = self.effective(i) {
            self.editor = Editor::load(&a);
        }
    }

    /// Queued action for a button on the profile/layer being shown.
    fn pending(&self, index: usize) -> Option<&Action> {
        self.ops.iter().rev().find_map(|op| match op {
            Op::Button { profile, layer, index: i, action } if (*profile, *layer, *i) == (self.profile, self.layer, index) => {
                Some(action)
            }
            _ => None,
        })
    }

    fn queue(&mut self, op: Op) {
        // a newer op for the same target replaces the older one
        self.ops.retain(|o| !o.same_target(&op));
        self.ops.push(op);
    }

    /// What a button will do: the queued change, or the device value.
    fn effective(&self, index: usize) -> Option<Action> {
        self.pending(index).or(self.device_action(index)).cloned()
    }

    fn device_action(&self, i: usize) -> Option<&Action> {
        self.actions.get(i)?.as_ref()
    }

    /// Every edit goes straight to the queue: a valid action that differs
    /// from what the button will do is queued (or dropped when it's back to
    /// the device value). Buttons that couldn't be read are left alone.
    fn queue_edits(&mut self) {
        let (Ok(a), Some(now)) = (self.editor.build(), self.effective(self.selected)) else { return };
        if a != now {
            self.queue_button();
        }
    }

    /// After ops are removed from the queue, put the editors back on what
    /// is still queued or on the device value, so they don't queue it again.
    fn reset_edits(&mut self) {
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
    }

    /// Same for DPI: valid values that differ from the device are queued,
    /// putting the device values back drops the queued change.
    fn queue_dpi_edits(&mut self) {
        let max = self.mice.get(self.current).map_or(0, |m| m.def.dpi_max);
        let valid = (1..=5).contains(&self.stages.len())
            && (1..=self.stages.len() as u8).contains(&self.active)
            && self.stages.iter().chain([&self.dpi]).all(|d| (100..=max).contains(d));
        let wanted = (self.dpi, self.active, &self.stages);
        if wanted == (self.device_dpi.0, self.device_dpi.1, &self.device_dpi.2) {
            self.ops.retain(|o| !matches!(o, Op::Dpi { .. }));
            return;
        }
        let queued = self.ops.iter().any(|o| matches!(o, Op::Dpi { dpi, active, stages } if (*dpi, *active, stages) == wanted));
        if valid && !queued {
            self.queue_dpi();
        }
    }

    /// Lighting edits are queued per zone, as differences from the mouse.
    fn queue_light_edits(&mut self) {
        if self.sync_zones && !self.zone_ui.is_empty() {
            let first = self.zone_ui[0].clone();
            for z in &mut self.zone_ui[1..] {
                *z = ZoneUi { name: z.name.clone(), ..first.clone() };
            }
        }
        for (z, base) in self.zone_ui.iter().zip(&self.zone_base) {
            let op = Op::Light { zone: z.name.clone(), effect: z.effect(), brightness: z.brightness };
            if z == base {
                self.ops.retain(|o| !o.same_target(&op));
                continue;
            }
            if !self.ops.contains(&op) {
                self.ops.retain(|o| !o.same_target(&op));
                self.ops.push(op);
            }
        }
    }

    fn queue_button(&mut self) {
        let action = match self.editor.build() {
            Ok(a) => a,
            Err(e) => {
                self.status = Err(e);
                return;
            }
        };
        let index = self.selected;
        let name = self.mice.get(self.current).map_or(String::new(), |m| m.def.buttons[index].name.to_uppercase());
        let op = Op::Button { profile: self.profile, layer: self.layer, index, action: action.clone() };
        if self.device_action(index) == Some(&action) {
            // back to what the device already has: nothing to do
            self.ops.retain(|o| !o.same_target(&op));
            self.status = Ok(format!("{name} REVERTED > nothing to write"));
            return;
        }
        self.queue(op);
        self.status = Ok(format!("QUEUED {name} > {action}"));
    }

    fn queue_dpi(&mut self) {
        self.queue(Op::Dpi { dpi: self.dpi, active: self.active, stages: self.stages.clone() });
        self.status = Ok(format!("QUEUED DPI {} > STAGE {}", self.dpi, self.active));
    }

    /// Runs the queue in order and stops at the first failure, keeping what's left.
    fn apply(&mut self) {
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
}

/// A change waiting to be written, like a GParted pending operation.
#[derive(PartialEq)]
enum Op {
    Button { profile: u8, layer: u8, index: usize, action: Action },
    Dpi { dpi: u16, active: u8, stages: Vec<u16> },
    Light { zone: String, effect: Effect, brightness: u8 },
}

impl Op {
    fn same_target(&self, other: &Op) -> bool {
        match (self, other) {
            (Op::Button { profile: p, layer: l, index: i, .. }, Op::Button { profile, layer, index, .. }) => {
                (p, l, i) == (profile, layer, index)
            }
            (Op::Dpi { .. }, Op::Dpi { .. }) => true,
            (Op::Light { zone: a, .. }, Op::Light { zone: b, .. }) => a == b,
            _ => false,
        }
    }

    fn describe(&self, def: &DeviceDef) -> String {
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
        }
    }
}

/// Top-down mouse silhouette: a squared-off ellipse, narrower at the front.
fn mouse_outline(body: egui::Rect) -> Vec<egui::Pos2> {
    let (c, a, b) = (body.center(), body.width() / 2.0, body.height() / 2.0);
    (0..96)
        .map(|i| {
            let (s, co) = (i as f32 / 96.0 * std::f32::consts::TAU).sin_cos();
            let taper = 0.8 + 0.2 * (s + 1.0) / 2.0; // s = -1 is the front (screen y grows down)
            let x = co.signum() * co.abs().powf(0.75) * a * taper;
            let y = s.signum() * s.abs().powf(0.9) * b;
            c + egui::vec2(x, y)
        })
        .collect()
}

/// Compact action text that fits on a drawn button.
fn short(a: &Action) -> String {
    match a {
        Action::Key { mods, usage } => {
            let mut s: String = opensynapse::action::MODS[..4].iter().filter(|(_, b)| mods & b != 0).map(|(n, _)| format!("{}+", n[..1].to_uppercase())).collect();
            s += &key_name(*usage).unwrap_or("?".into()).to_uppercase();
            s
        }
        Action::Mouse(b) => match b {
            1 => "LMB",
            2 => "RMB",
            3 => "MMB",
            4 => "BACK",
            5 => "FWD",
            9 => "W▲",
            10 => "W▼",
            _ => "M?",
        }
        .into(),
        Action::MouseTurbo { button, .. } => format!("T:{}", short(&Action::Mouse(*button))),
        Action::KeyTurbo { mods, usage, .. } => format!("T:{}", short(&Action::Key { mods: *mods, usage: *usage })),
        Action::Media(u) => match u {
            0xcd => "P/P",
            0xb0 => "PLAY",
            0xb1 => "PAUSE",
            0xb5 => "NEXT",
            0xb6 => "PREV",
            0xb7 => "STOP",
            0xe2 => "MUTE",
            0xe9 => "VOL+",
            0xea => "VOL-",
            _ => "MEDIA",
        }
        .into(),
        Action::Dpi(1 | 6) => "DPI+".into(),
        Action::Dpi(_) => "DPI-".into(),
        Action::DpiStage(n) => format!("DPI{n}"),
        Action::Profile(_) => "PROF".into(),
        Action::ProfileSet(n) => format!("P{n}"),
        Action::Hypershift => "HYPER".into(),
        Action::Disabled => "OFF".into(),
        Action::Raw(_) => "RAW".into(),
    }
}

/// Frameless tab: cyan and underlined when active.
fn tab(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let label = if selected { RichText::new(text).color(CYAN).strong().underline() } else { RichText::new(text).color(DIM) };
    ui.add(egui::Button::new(label).frame(false))
}

/// Top-level view switch, drawn as a tab with a border so it doesn't read as a setting.
fn view_tab(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let (fg, edge, fill) = match selected {
        true => (CYAN, CYAN, CARD),
        false => (DIM, DIM, BG),
    };
    ui.add(
        egui::Button::new(RichText::new(text).color(fg).strong())
            .fill(fill)
            .stroke(Stroke::new(1.0, edge))
            .corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

/// Height shared by every button that can sit in a row with a neon_button.
const BUTTON_HEIGHT: f32 = 28.0;

/// Bordered toggle-style button used across the editor.
fn chip(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    chip_sized(ui, text, selected, 0.0)
}

/// A chip as tall as the neon buttons next to it.
fn toggle_button(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    chip_sized(ui, text, selected, BUTTON_HEIGHT)
}

fn chip_sized(ui: &mut egui::Ui, text: &str, selected: bool, height: f32) -> egui::Response {
    let (fg, edge, fill) = if selected { (BG, PINK, PINK) } else { (Color32::WHITE, DIM, BG) };
    ui.add(
        egui::Button::new(RichText::new(text).color(fg))
            .fill(fill)
            .stroke(Stroke::new(1.0, edge))
            .corner_radius(CornerRadius::ZERO)
            .min_size(egui::vec2(0.0, height)),
    )
}

/// Secondary action: colored text and border on the background.
fn outline_button(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(color))
            .fill(BG)
            .stroke(Stroke::new(1.0, color))
            .corner_radius(CornerRadius::ZERO)
            .min_size(egui::vec2(0.0, BUTTON_HEIGHT)),
    )
}

fn neon_button(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(BG).strong())
            .fill(color)
            .corner_radius(CornerRadius::ZERO)
            .min_size(egui::vec2(0.0, BUTTON_HEIGHT)),
    )
}

/// Small red X that removes a row.
fn delete_button(ui: &mut egui::Ui) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new("X").color(RED).strong())
            .small()
            .fill(BG)
            .stroke(Stroke::new(1.0, RED))
            .corner_radius(CornerRadius::ZERO),
    )
    .on_hover_text("remove")
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(format!("> {text}")).color(PINK).strong());
    ui.add_space(4.0);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_dpi();
        self.poll_daemon();
        self.import_dialog();
        self.confirm_dialog();
        if self.disconnected.is_some() {
            // the modal's backdrop dims everything behind it and blocks clicks
            egui::Modal::new(egui::Id::unique("disconnected")).show(&self.ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(RichText::new("! MOUSE DISCONNECTED").color(PINK).strong());
                ui.add_space(6.0);
                ui.label("Plug it back in and opensynapse picks it up again. Pending changes are kept.");
            });
        }
        // the sidebar goes first so it runs the full height, the tabs only span the rest
        let side = Frame::new().fill(PANEL).inner_margin(Margin::same(14)).stroke(Stroke::new(1.0, DIM));
        egui::Panel::left("dpi").exact_size(SIDEBAR_WIDTH).resizable(false).frame(side).show(ui, |ui| {
            self.device_box(ui);
            self.dpi_panel(ui);
        });

        let bar = Frame::new().fill(PANEL).inner_margin(Margin { left: 12, right: 12, top: 12, bottom: 0 });
        egui::Panel::top("tabs").frame(bar).show(ui, |ui| self.tabs(ui));

        let status = Frame::new().fill(BG).inner_margin(Margin::symmetric(12, 6));
        let (text, color) = match &self.status {
            Ok(s) => (format!("> {s}"), CYAN),
            Err(e) => (format!("! {e}"), PINK),
        };
        egui::Panel::bottom("status").frame(status).show(ui, |ui| {
            ui.label(RichText::new(text).color(color));
        });

        let center = Frame::new().fill(BG).inner_margin(Margin::same(16));
        if self.view == View::Lighting {
            egui::CentralPanel::default().frame(center).show(ui, |ui| self.lighting_ui(ui));
            return;
        }

        let name = self.mice.get(self.current).and_then(|m| m.def.buttons.get(self.selected)).map(|b| b.name.clone());
        if let Some(name) = name {
            let editor = Frame::new().fill(PANEL).inner_margin(Margin::same(14)).stroke(Stroke::new(1.0, DIM));
            egui::Panel::bottom("editor").exact_size(EDITOR_HEIGHT).resizable(false).frame(editor).show(ui, |ui| {
                self.editor_ui(ui, &name);
            });
        }
        if !self.mice.is_empty() {
            let side = Frame::new().fill(BG).inner_margin(Margin::same(14)).stroke(Stroke::new(1.0, DIM));
            egui::Panel::left("profile").exact_size(190.0).resizable(false).frame(side).show(ui, |ui| self.profile_box(ui));
        }
        egui::CentralPanel::default().frame(center).show(ui, |ui| self.buttons(ui));
    }
}

impl App {
    fn lighting_ui(&mut self, ui: &mut egui::Ui) {
        let area = ui.max_rect();
        let Some(light) = &self.lighting else {
            ui.label(RichText::new("NO LIGHTING").color(PINK).size(28.0));
            ui.label(RichText::new("lighting goes through the OpenRazer driver, install openrazer-driver-dkms").color(DIM));
            return;
        };
        let online = light.uses_daemon();
        let (badge, color, tip) = match online {
            true => ("OPENRAZER ONLINE", GREEN, "Lighting goes through the OpenRazer daemon, which keeps it after a replug."),
            false => (
                "OPENRAZER OFFLINE",
                RED,
                "The OpenRazer daemon isn't running or doesn't see this mouse, so lighting is off. Start it with \
                 `systemctl --user restart openrazer-daemon` and press RESCAN.",
            ),
        };
        // ui.horizontal centers items on the row, so the title lines up with the taller buttons
        ui.horizontal(|ui| {
            ui.label(RichText::new("> LIGHTING").color(PINK).strong());
            ui.add_space(12.0);
            // status badge on the far right of the row
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                Frame::new().stroke(Stroke::new(1.0, color)).inner_margin(Margin::symmetric(6, 2)).show(ui, |ui| {
                    ui.label(RichText::new(badge).color(color).small());
                })
                .response
                .on_hover_text(tip);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    if !online {
                        ui.disable();
                    }
                    self.lighting_actions(ui);
                });
            });
        });
        ui.add_space(10.0);
        // without the daemon the cards stay visible but greyed out
        if !online {
            ui.disable();
        }

        // with sync on, the first zone's card drives every zone
        let shown = if self.sync_zones { 1 } else { self.zone_ui.len() };
        let mut apply = None;
        let mut all = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for (i, z) in self.zone_ui.iter_mut().take(shown).enumerate() {
                let title = if self.sync_zones { "ALL ZONES".to_string() } else { z.name.to_uppercase() };
                Frame::new().fill(PANEL).stroke(Stroke::new(1.0, DIM)).inner_margin(Margin::same(14)).show(ui, |ui| {
                    ui.set_width(ui.available_width().min(760.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).color(CYAN).strong().underline());
                        ui.add_space(16.0);
                        ui.label(RichText::new("BRIGHTNESS").color(DIM).small());
                        ui.add(egui::Slider::new(&mut z.brightness, 0..=255).trailing_fill(true));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if neon_button(ui, "APPLY NOW", CYAN).clicked() {
                                apply = Some(i);
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 20.0;
                        for (fx, label) in FX {
                            if tab(ui, label, z.fx == fx).clicked() {
                                z.fx = fx;
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| z.options(ui));
                });
            }
            ui.add_space(4.0);
            if neon_button(ui, "APPLY ALL NOW", CYAN).on_hover_text("send every zone's change right away").clicked() {
                all = true;
            }
        });
        if !online {
            offline_overlay(ui, area);
        }

        self.queue_light_edits();
        let Some(card) = apply.or(all.then_some(0)) else { return };
        // APPLY NOW sends what the card shows, changed or not (every card when synced or APPLY ALL NOW)
        let names: Vec<String> = match self.sync_zones || all {
            true => self.zone_ui.iter().map(|z| z.name.clone()).collect(),
            false => vec![self.zone_ui[card].name.clone()],
        };
        self.ops.retain(|o| !matches!(o, Op::Light { zone, .. } if names.contains(zone)));
        let rest = std::mem::take(&mut self.ops);
        self.ops = (self.zone_ui.iter())
            .filter(|z| names.contains(&z.name))
            .map(|z| Op::Light { zone: z.name.clone(), effect: z.effect(), brightness: z.brightness })
            .collect();
        self.apply();
        self.ops.extend(rest);
    }

    /// Reads zones from the OpenRazer driver and pre-fills each card with what
    /// the mouse reports, falling back to OpenRazer's idea of it for zones the
    /// mouse doesn't answer for.
    fn load_lighting(&mut self) {
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
        if let Err(e) = rgb::save_zones(&path, &self.lighting_now()) {
            self.status = Err(format!("could not save the original lighting: {e}"));
        }
    }
}

#[derive(Clone, Copy)]
enum Confirm {
    ResetButtons,
    SetDefault,
    ResetLighting,
    DefaultLighting,
    FactoryLighting,
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Buttons,
    Lighting,
}

#[derive(Clone, Copy, PartialEq)]
enum Fx {
    Static,
    Breath,
    Spectrum,
    Wave,
    Reactive,
    Off,
}

const FX: [(Fx, &str); 6] = [
    (Fx::Static, "STATIC"),
    (Fx::Breath, "BREATH"),
    (Fx::Spectrum, "SPECTRUM"),
    (Fx::Wave, "WAVE"),
    (Fx::Reactive, "REACTIVE"),
    (Fx::Off, "OFF"),
];

/// Editable state of one lighting zone card.
#[derive(Clone, PartialEq)]
struct ZoneUi {
    name: String,
    fx: Fx,
    color: [u8; 3],
    random: bool,
    reverse: bool,
    speed: u8,
    brightness: u8,
}

impl ZoneUi {
    fn from_effect(name: &str, e: Effect, brightness: u8) -> Self {
        let mut z = ZoneUi { name: name.into(), fx: Fx::Static, color: [0, 240, 255], random: false, reverse: false, speed: 2, brightness };
        match e {
            Effect::Off => z.fx = Fx::Off,
            Effect::Static(c) => (z.fx, z.color) = (Fx::Static, c),
            Effect::Breath(c) => (z.fx, z.color) = (Fx::Breath, c),
            Effect::BreathRandom => (z.fx, z.random) = (Fx::Breath, true),
            Effect::Spectrum => z.fx = Fx::Spectrum,
            Effect::Wave(r) => (z.fx, z.reverse) = (Fx::Wave, r),
            Effect::Reactive { speed, color } => (z.fx, z.speed, z.color) = (Fx::Reactive, speed, color),
        }
        z
    }

    fn effect(&self) -> Effect {
        match self.fx {
            Fx::Off => Effect::Off,
            Fx::Static => Effect::Static(self.color),
            Fx::Breath if self.random => Effect::BreathRandom,
            Fx::Breath => Effect::Breath(self.color),
            Fx::Spectrum => Effect::Spectrum,
            Fx::Wave => Effect::Wave(self.reverse),
            Fx::Reactive => Effect::Reactive { speed: self.speed, color: self.color },
        }
    }

    /// The controls each effect needs (color, direction, speed).
    fn options(&mut self, ui: &mut egui::Ui) {
        let needs_color = matches!(self.fx, Fx::Static | Fx::Reactive) || (self.fx == Fx::Breath && !self.random);
        if self.fx == Fx::Breath && chip(ui, "RANDOM COLORS", self.random).clicked() {
            self.random = !self.random;
        }
        if needs_color {
            ui.label(RichText::new("COLOR").color(DIM).small());
            ui.color_edit_button_srgb(&mut self.color);
            let [r, g, b] = self.color;
            ui.label(RichText::new(format!("#{r:02x}{g:02x}{b:02x}")).color(Color32::from_rgb(r, g, b)));
        }
        if self.fx == Fx::Wave {
            if chip(ui, "◄ LEFT", !self.reverse).clicked() {
                self.reverse = false;
            }
            if chip(ui, "RIGHT ►", self.reverse).clicked() {
                self.reverse = true;
            }
        }
        if self.fx == Fx::Reactive {
            ui.label(RichText::new("FADE").color(DIM).small());
            for (s, label) in [(1, "FAST"), (2, "MEDIUM"), (3, "SLOW")] {
                if chip(ui, label, self.speed == s).clicked() {
                    self.speed = s;
                }
            }
        }
        if matches!(self.fx, Fx::Spectrum | Fx::Off) {
            ui.label(RichText::new("no options").color(DIM).small());
        }
    }
}

impl App {
    /// Top of the sidebar: logo, version, the mouse picker and RESCAN.
    fn device_box(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("OPEN").color(CYAN).size(22.0).strong());
            ui.label(RichText::new("SYNAPSE").color(PINK).size(22.0).strong());
        });
        let version = RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(YELLOW).small();
        Frame::new().stroke(Stroke::new(1.0, YELLOW)).inner_margin(Margin::symmetric(5, 1)).show(ui, |ui| {
            ui.label(version);
        });
        ui.add_space(14.0);

        let names: Vec<String> = self.mice.iter().map(|m| m.def.name.to_uppercase()).collect();
        let current = names.get(self.current).cloned().unwrap_or("NO DEVICE".into());
        let mut pick = None;
        let width = ui.available_width();
        ComboBox::from_id_salt("device")
            .selected_text(RichText::new(current).color(YELLOW).small())
            .width(width)
            .truncate()
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(i == self.current, n).clicked() {
                        pick = Some(i);
                    }
                }
                for n in &self.unsupported {
                    ui.add_enabled(false, egui::Button::new(format!("{n} > no definition")));
                }
            });
        if let Some(i) = pick.filter(|&i| i != self.current) {
            self.switch_to(i);
        }
        let rescan = egui::Button::new(RichText::new("RESCAN").color(BG).strong()).fill(CYAN).corner_radius(CornerRadius::ZERO);
        if ui.add_sized([width, 28.0], rescan).clicked() {
            self.connect();
        }
        ui.add_space(18.0);
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if view_tab(ui, "BUTTONS", self.view == View::Buttons).clicked() {
                self.view = View::Buttons;
            }
            if view_tab(ui, "LIGHTING", self.view == View::Lighting).clicked() {
                self.view = View::Lighting;
            }
        });
    }

    /// Left of the mouse drawing: which profile and layer are shown, RESET and SET DEFAULT.
    fn profile_box(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.mice.get(self.current) else { return };
        let (profiles, layers) = (m.def.profiles.clone(), m.def.layers.clone());
        let mut changed = false;
        let width = ui.available_width();
        heading(ui, "PROFILE");
        // profile 0 is what the mouse runs right now and is lost on unplug (verified)
        let label = |p: u8| {
            let mut job = egui::text::LayoutJob::default();
            let font = FontId::monospace(14.0);
            job.append(&format!("P{p}"), 0.0, egui::TextFormat::simple(font.clone(), Color32::WHITE));
            if p == 0 {
                job.append("[LIVE]", 6.0, egui::TextFormat::simple(font, GREEN));
            }
            job
        };
        ComboBox::from_id_salt("profile")
            .selected_text(label(self.profile))
            .width(width)
            .show_ui(ui, |ui| {
                for p in profiles {
                    changed |= ui.selectable_value(&mut self.profile, p, label(p)).changed();
                }
            })
            .response
            .on_hover_text("P0 is live and lost when you unplug the mouse. P1 is stored on it and loads after a replug.");
        let reset = egui::Button::new(RichText::new("RESET").color(BG).strong()).fill(PINK).corner_radius(CornerRadius::ZERO);
        if ui.add_sized([width, 26.0], reset).on_hover_text("put back this profile's original").clicked() {
            self.confirm = Some(Confirm::ResetButtons);
        }
        let set = egui::Button::new(RichText::new("SET DEFAULT").color(YELLOW))
            .fill(BG)
            .stroke(Stroke::new(1.0, YELLOW))
            .corner_radius(CornerRadius::ZERO);
        if ui.add_sized([width, 26.0], set).on_hover_text("make what's on the mouse now the original RESET goes back to").clicked() {
            self.confirm = Some(Confirm::SetDefault);
        }
        ui.add_space(18.0);

        heading(ui, "LAYER");
        for (i, l) in layers.iter().enumerate() {
            let selected = self.layer == i as u8;
            if ui.add_sized([width, 24.0], egui::Button::selectable(selected, l.to_uppercase())).clicked() && !selected {
                self.layer = i as u8;
                changed = true;
            }
        }
        if changed {
            self.reload();
            let layer = layers.get(self.layer as usize).map_or("?", String::as_str).to_uppercase();
            self.status = Ok(format!("VIEWING P{} > {layer}", self.profile));
            self.update_path();
        }
    }

    fn dpi_panel(&mut self, ui: &mut egui::Ui) {
        heading(ui, "SENSITIVITY");
        let max = self.mice.get(self.current).map_or(20000, |m| m.def.dpi_max);
        ui.label(RichText::new(format!("{} DPI", self.dpi)).color(YELLOW).size(26.0).strong());
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(&mut self.dpi, 100..=max).step_by(50.0).show_value(false).trailing_fill(true));
        ui.add_space(14.0);

        heading(ui, "STAGES");
        let mut remove = None;
        for (i, dpi) in self.stages.iter_mut().enumerate() {
            let n = i as u8 + 1;
            ui.horizontal(|ui| {
                let mark = if self.active == n { RichText::new("▶").color(CYAN) } else { RichText::new("·").color(DIM) };
                if ui.selectable_label(self.active == n, mark).clicked() {
                    self.active = n;
                }
                ui.add(DragValue::new(dpi).range(100..=max).speed(50));
                if delete_button(ui).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove.filter(|_| self.stages.len() > 1) {
            self.stages.remove(i);
            self.active = self.active.min(self.stages.len() as u8);
        }
        if self.stages.len() < 5 && ui.button("+ STAGE").clicked() {
            self.stages.push(*self.stages.last().unwrap_or(&800));
        }
        self.queue_dpi_edits();
        ui.add_space(24.0);

        // pending operations, pinned to the bottom of the sidebar (bottom_up: first added = lowest)
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.add_enabled_ui(!self.ops.is_empty(), |ui| {
                ui.horizontal(|ui| {
                    if neon_button(ui, "CLEAR", PINK).clicked() {
                        self.status = Ok(format!("CLEARED {} PENDING", self.ops.len()));
                        self.ops.clear();
                        self.reset_edits();
                    }
                    if neon_button(ui, "WRITE TO DEVICE", CYAN).clicked() {
                        self.apply();
                    }
                });
            });
            if self.ops.is_empty() {
                ui.label(RichText::new("no pending changes").color(DIM).small());
            }
            let Some(def) = self.mice.get(self.current).map(|m| &m.def) else { return };
            // newest 12 only, a scroll area here would eat the whole sidebar
            const SHOWN: usize = 12;
            let mut remove = None;
            let hidden = self.ops.len().saturating_sub(SHOWN);
            for (i, op) in self.ops.iter().enumerate().skip(hidden).rev() {
                ui.horizontal(|ui| {
                    if delete_button(ui).clicked() {
                        remove = Some(i);
                    }
                    ui.label(RichText::new(op.describe(def)).color(Color32::WHITE).small());
                });
            }
            if hidden > 0 {
                ui.label(RichText::new(format!("+{hidden} more")).color(DIM).small());
            }
            if let Some(i) = remove {
                let op = self.ops.remove(i);
                self.status = Ok(format!("REMOVED {}", op.describe(def)));
                self.reset_edits();
            }
            heading(ui, &format!("PENDING ({})", self.ops.len()));
        });
    }

    fn export(&mut self) {
        let name = PathBuf::from(&self.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let Some(path) = self.dialog().set_file_name(name).save_file() else { return };
        let Some(m) = self.mice.get(self.current) else { return };
        let r = profile::export(m, self.profile, &path);
        self.status = r.map(|_| format!("P{} SAVED > {}", self.profile, path.display())).map_err(|e| e.to_string());
    }

    fn import(&mut self) {
        let path = PathBuf::from(&self.path);
        self.status = self.queue_file(&path).map(|n| format!("P{} LOADED > {n} BUTTON CHANGES QUEUED", self.profile));
    }

    /// Queues the differences between a profile file and the device. Returns
    /// how many buttons differ.
    fn queue_file(&mut self, path: &std::path::Path) -> Result<usize, String> {
        let m = self.mice.get(self.current).ok_or("no mouse")?;
        let file = profile::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let plan = profile::plan(m, self.profile, &file).map_err(|e| e.to_string())?;
        let n = plan.len();
        for (layer, index, action) in plan {
            self.queue(Op::Button { profile: self.profile, layer, index, action });
        }
        // the DPI editor queues its own changes, so give it the file's values
        (self.dpi, self.active, self.stages) = (file.dpi, file.active_stage, file.dpi_stages);
        self.select(self.selected);
        Ok(n)
    }

    fn buttons(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.mice.get(self.current) else {
            ui.label(RichText::new("NO LINK").color(PINK).size(28.0));
            return;
        };
        let buttons = &m.def.buttons;
        let lit: Vec<bool> = (0..buttons.len()).map(|i| self.lit(i)).collect();
        if lit.contains(&true) {
            ui.ctx().request_repaint_after(Duration::from_millis(100)); // let highlights fade
        }

        // buttons with a position go on the drawn mouse, the rest into the grid
        let (placed, loose): (Vec<usize>, Vec<usize>) = (0..buttons.len()).partition(|&i| buttons[i].at.is_some());
        let mut clicked = None;
        let view_height = ui.available_height();
        egui::ScrollArea::vertical().max_height(view_height).show(ui, |ui| {
            if !placed.is_empty() {
                clicked = self.mouse_view(ui, buttons, &placed, &lit, view_height);
            }
            egui::Grid::new("buttons").spacing([8.0, 8.0]).show(ui, |ui| {
                for (col, &i) in loose.iter().enumerate() {
                    let pending = self.pending(i).is_some();
                    let action = self.effective(i);
                    let mark = if pending { " *" } else { "" };
                    let off = action.as_ref() == Some(&Action::Disabled);
                    let text = action.map_or("??".into(), |a| format!("{a}{mark}"));
                    let accent = match (lit[i], i == self.selected, pending) {
                        (true, ..) => YELLOW,
                        (_, true, _) => CYAN,
                        (.., true) => PINK,
                        _ if off => OFF,
                        _ => DIM,
                    };
                    let card = Frame::new().fill(CARD).stroke(Stroke::new(1.0, accent)).inner_margin(Margin::same(8));
                    let r = card
                        .show(ui, |ui| {
                            ui.set_min_width(150.0);
                            ui.label(RichText::new(buttons[i].name.to_uppercase()).color(accent).small());
                            ui.label(RichText::new(text).color(if off { DIM } else { Color32::WHITE }));
                        })
                        .response
                        .interact(egui::Sense::click());
                    if r.clicked() {
                        clicked = Some(i);
                    }
                    if (col + 1) % 4 == 0 {
                        ui.end_row();
                    }
                }
            });
        });
        if let Some(i) = clicked {
            self.select(i);
        }
    }

    /// Draws a mouse body and the positioned buttons on it. Returns a clicked button.
    fn mouse_view(&self, ui: &mut egui::Ui, buttons: &[Button], placed: &[usize], lit: &[bool], max_height: f32) -> Option<usize> {
        // 16:10 canvas so `at` percentages mean the same thing at any window size
        let fit_height = max_height.max(200.0) / USED_HEIGHT;
        let w = ui.available_width().min(fit_height * 1.6).min(MAX_VIEW_WIDTH);
        let k = w / (460.0 * 1.6); // everything below scales with the canvas
        let (full, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), w / 1.6 * USED_HEIGHT), egui::Sense::hover());
        let rect = egui::Rect::from_min_size(egui::pos2(full.center().x - w / 2.0, full.top()), egui::vec2(w, w / 1.6));
        let at = |x: f32, y: f32| rect.min + egui::vec2(rect.width() * x / 100.0, rect.height() * y / 100.0);
        let p = ui.painter_at(rect);
        // yellow outline while editing the Hypershift (or any non-default) layer
        let outline = match self.layer {
            0 => PINK,
            _ => YELLOW,
        };

        let marker = |i: usize| {
            let [x, y] = buttons[i].at.unwrap_or_default();
            egui::Rect::from_center_size(at(x, y), egui::vec2(52.0, 26.0) * k)
        };

        // side plate behind any buttons placed outside the body (thumb grids, side buttons)
        let outside = placed.iter().filter(|&&i| buttons[i].at.is_some_and(|[x, _]| !(30.0..=70.0).contains(&x)));
        if let Some(plate) = outside.map(|&i| marker(i)).reduce(|a, b| a.union(b)) {
            let plate = plate.expand(12.0 * k);
            p.rect_filled(plate, CornerRadius::same(14), PANEL);
            p.rect_stroke(plate, CornerRadius::same(14), Stroke::new(1.5, outline), StrokeKind::Outside);
        }

        let body = egui::Rect::from_min_max(at(30.0, 2.0), at(70.0, 74.0));
        let seam = Stroke::new(1.0, DIM);
        p.add(egui::Shape::convex_polygon(mouse_outline(body), PANEL, Stroke::new(1.5, outline)));
        // where the buttons end and the palm begins
        let split = body.top() + body.height() * 0.6;
        let curve: Vec<egui::Pos2> = (0..=20)
            .map(|i| {
                let x = i as f32 / 10.0 - 1.0;
                egui::pos2(body.center().x + x * body.width() * 0.43, split + (1.0 - x * x) * body.height() * 0.04)
            })
            .collect();
        p.add(egui::Shape::line(curve, seam));
        p.line_segment([egui::pos2(body.center().x, body.top()), egui::pos2(body.center().x, split)], seam);
        let wheel = egui::Rect::from_center_size(at(50.0, 18.0), egui::vec2(body.width() * 0.12, body.height() * 0.28));
        p.rect_filled(wheel, CornerRadius::same(10), BG);
        p.rect_stroke(wheel, CornerRadius::same(10), seam, StrokeKind::Inside);

        let mut clicked = None;
        for &i in placed {
            let r = marker(i);
            let resp = ui.interact(r, ui.scope_id().with(("btn", i)), egui::Sense::click());
            let pending = self.pending(i).is_some();
            // queued changes show what the button will do after APPLY
            let shown = self.effective(i);
            let off = shown == Some(Action::Disabled);
            let (fill, edge, text) = match (lit[i], i == self.selected, resp.hovered(), pending) {
                (true, ..) => (YELLOW, YELLOW, BG),
                (_, true, ..) => (BG, CYAN, CYAN),
                (_, _, true, _) => (BG, CYAN, Color32::WHITE),
                (.., true) => (BG, PINK, PINK),
                _ if off => (BG, OFF, DIM),
                _ => (BG, DIM, Color32::WHITE),
            };
            p.rect_filled(r, CornerRadius::ZERO, fill);
            p.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, edge), StrokeKind::Inside);
            let mark = if pending { "*" } else { "" };
            let label = shown.as_ref().map_or("??".into(), |a| format!("{}{mark}", short(a)));
            p.text(r.center(), Align2::CENTER_CENTER, label, FontId::monospace(11.0 * k), text);
            let tip = shown.map_or(String::new(), |a| a.to_string());
            if resp.on_hover_text(format!("{} > {tip}", buttons[i].name)).clicked() {
                clicked = Some(i);
            }
        }
        clicked
    }

    /// Held right now, or a DPI button that just changed the DPI.
    fn lit(&self, i: usize) -> bool {
        let flashing = self.flash.is_some_and(|(f, t)| f == i && t.elapsed() < Duration::from_millis(400));
        flashing || self.device_action(i).is_some_and(|a| self.pressed.as_ref().and_then(|p| p.lock().ok()).is_some_and(|p| p.triggers(a)))
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui, name: &str) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
        self.file_row(ui);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("> REMAP:").color(PINK).strong());
            ui.label(RichText::new(name.to_uppercase()).color(CYAN).strong().underline());
            if matches!(self.editor.kind, Kind::Key | Kind::Mouse) {
                ui.add_space(32.0);
                turbo_toggle(ui, &mut self.editor);
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| self.editor_fields(ui));
        self.queue_edits();
    }

    /// Profile file: the folder files are saved in, export and import.
    fn file_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("> PROFILE FILE:").color(PINK).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if neon_button(ui, "IMPORT", PINK).clicked() {
                    self.pick_import();
                }
                if neon_button(ui, "EXPORT", CYAN).clicked() {
                    self.export();
                }
                if neon_button(ui, "CHANGE FOLDER", YELLOW).on_hover_text("where profile files are saved").clicked() {
                    self.pick_folder();
                }
                // the path gets whatever width the buttons leave
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(&self.path).color(DIM)).truncate()).on_hover_text(&self.path);
                });
            });
        });
    }

    /// Native dialogs go through the XDG desktop portal and block the UI while open.
    fn dialog(&self) -> rfd::FileDialog {
        let path = PathBuf::from(&self.path);
        let dialog = rfd::FileDialog::new().add_filter("opensynapse profile", &["json"]);
        let Some(dir) = path.parent() else { return dialog };
        dialog.set_directory(dir)
    }

    /// The folder profile files go to. File names stay `<serial>-<profile>.json`.
    fn pick_folder(&mut self) {
        let Some(dir) = self.dialog().pick_folder() else { return };
        self.status = Ok(format!("PROFILE FOLDER > {}", dir.display()));
        self.dir = Some(dir);
        self.update_path();
    }

    fn pick_import(&mut self) {
        let Some(file) = self.dialog().pick_file() else { return };
        self.path = file.display().to_string();
        self.confirm_import = true;
    }

    /// Asks before IMPORT queues changes over the selected profile.
    fn import_dialog(&mut self) {
        if !self.confirm_import {
            return;
        }
        let modal = egui::Modal::new(egui::Id::unique("confirm-import")).show(&self.ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(RichText::new("! OVERWRITE PROFILE").color(PINK).strong());
            ui.add_space(6.0);
            ui.label(format!(
                "Importing replaces profile P{} with the contents of this file. The changes go to PENDING \
                 and reach the mouse when you press WRITE TO DEVICE.",
                self.profile
            ));
            ui.label(RichText::new(&self.path).color(DIM).small());
            ui.add_space(10.0);
            let mut choice = None;
            ui.horizontal(|ui| {
                if neon_button(ui, "IMPORT", PINK).clicked() {
                    choice = Some(true);
                }
                if neon_button(ui, "CANCEL", DIM).clicked() {
                    choice = Some(false);
                }
            });
            choice
        });
        let choice = modal.inner.or(modal.should_close().then_some(false));
        let Some(go) = choice else { return };
        self.confirm_import = false;
        if go {
            self.import();
        }
    }

    /// Asks before RESET or SET DEFAULT.
    fn confirm_dialog(&mut self) {
        let Some(what) = self.confirm else { return };
        let p = self.profile;
        let (title, text, button) = match what {
            Confirm::ResetButtons => (
                "! RESET PROFILE",
                format!(
                    "Every button on P{p} and the DPI go back to the original, what was on the mouse the first time \
                     opensynapse saw it (or what you saved with SET DEFAULT). The changes go to PENDING and reach the \
                     mouse when you press WRITE TO DEVICE."
                ),
                "RESET",
            ),
            Confirm::ResetLighting => (
                "! RESET LIGHTING",
                "Every zone goes back to the original lighting, what the mouse had the first time opensynapse saw \
                 it (or what you saved with SET DEFAULT). The changes go to PENDING, APPLY NOW or WRITE TO DEVICE \
                 sends them."
                    .to_string(),
                "RESET",
            ),
            Confirm::DefaultLighting => (
                "! SET DEFAULT",
                "The lighting on the mouse right now becomes the original that RESET goes back to. The old \
                 original is replaced."
                    .to_string(),
                "SET DEFAULT",
            ),
            Confirm::FactoryLighting => (
                "! FACTORY RESET",
                "Every zone goes back to static Razer green at full brightness. The changes go to PENDING, \
                 APPLY NOW or WRITE TO DEVICE sends them."
                    .to_string(),
                "FACTORY RESET",
            ),
            Confirm::SetDefault => (
                "! SET DEFAULT",
                format!("What's on P{p} right now becomes the original that RESET goes back to. The old original is replaced."),
                "SET DEFAULT",
            ),
        };
        let modal = egui::Modal::new(egui::Id::unique("confirm")).show(&self.ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(RichText::new(title).color(PINK).strong());
            ui.add_space(6.0);
            ui.label(text);
            ui.add_space(10.0);
            let mut choice = None;
            ui.horizontal(|ui| {
                if neon_button(ui, button, PINK).clicked() {
                    choice = Some(true);
                }
                if neon_button(ui, "CANCEL", DIM).clicked() {
                    choice = Some(false);
                }
            });
            choice
        });
        let Some(go) = modal.inner.or(modal.should_close().then_some(false)) else { return };
        self.confirm = None;
        if !go {
            return;
        }
        match what {
            Confirm::ResetButtons => self.reset_buttons(),
            Confirm::SetDefault => self.set_default(),
            Confirm::ResetLighting => self.reset_lighting(),
            Confirm::DefaultLighting => self.set_lighting_default(),
            Confirm::FactoryLighting => self.factory_lighting(),
        }
    }

    /// Queues the profile's original snapshot.
    fn reset_buttons(&mut self) {
        let Some(path) = self.mice.get(self.current).and_then(|m| profile::original_path(m, self.profile)) else { return };
        self.status = self.queue_file(&path).map(|n| format!("P{} RESET > {n} BUTTON CHANGES QUEUED", self.profile));
    }

    /// Replaces the profile's original snapshot with what's on the mouse.
    fn set_default(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        let Some(path) = profile::original_path(m, self.profile) else { return };
        let r = profile::export(m, self.profile, &path);
        self.status = r.map(|_| format!("P{} IS THE NEW ORIGINAL > {}", self.profile, path.display())).map_err(|e| e.to_string());
    }

    fn lighting_original(&self) -> Option<PathBuf> {
        let m = self.mice.get(self.current)?;
        rgb::original_path(m.def.vid, m.def.pid, &m.serial)
    }

    fn lighting_actions(&mut self, ui: &mut egui::Ui) {
        if toggle_button(ui, "SYNC ALL ZONES", self.sync_zones).clicked() {
            self.sync_zones = !self.sync_zones;
        }
        if neon_button(ui, "RESET", PINK).on_hover_text("put back the original lighting").clicked() {
            self.confirm = Some(Confirm::ResetLighting);
        }
        if outline_button(ui, "SET DEFAULT", YELLOW)
            .on_hover_text("make the lighting on the mouse now the original RESET goes back to")
            .clicked()
        {
            self.confirm = Some(Confirm::DefaultLighting);
        }
        if outline_button(ui, "FACTORY RESET", PINK).on_hover_text("static Razer green on every zone").clicked() {
            self.confirm = Some(Confirm::FactoryLighting);
        }
    }

    /// Lighting on the mouse as it was last read or written.
    fn lighting_now(&self) -> Vec<rgb::ZoneState> {
        let state = |z: &ZoneUi| rgb::ZoneState { zone: z.name.clone(), effect: z.effect(), brightness: z.brightness };
        self.zone_base.iter().map(state).collect()
    }

    /// Queues the original lighting snapshot.
    fn reset_lighting(&mut self) {
        let Some(path) = self.lighting_original() else { return };
        let saved = match rgb::load_zones(&path) {
            Ok(s) => s,
            Err(e) => {
                self.status = Err(format!("{}: {e}", path.display()));
                return;
            }
        };
        self.sync_zones = false;
        for s in saved {
            if let Some(z) = self.zone_ui.iter_mut().find(|z| z.name == s.zone) {
                *z = ZoneUi::from_effect(&s.zone, s.effect, s.brightness);
            }
        }
        self.status = Ok("LIGHTING RESET QUEUED".into());
    }

    fn set_lighting_default(&mut self) {
        let Some(path) = self.lighting_original() else { return };
        let r = rgb::save_zones(&path, &self.lighting_now());
        self.status = r.map(|_| format!("LIGHTING IS THE NEW ORIGINAL > {}", path.display())).map_err(|e| e.to_string());
    }

    fn factory_lighting(&mut self) {
        self.sync_zones = false;
        for z in &mut self.zone_ui {
            *z = ZoneUi::from_effect(&z.name.clone(), Effect::Static([0x00, 0xff, 0x00]), 255);
        }
        self.status = Ok("LIGHTING FACTORY RESET QUEUED".into());
    }

    fn editor_fields(&mut self, ui: &mut egui::Ui) {
        let e = &mut self.editor;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            for (k, label) in KINDS {
                if tab(ui, label, e.kind == k).clicked() {
                    e.kind = k;
                }
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| match e.kind {
            Kind::Key => {} // laid out as columns below
            Kind::Mouse => {
                for &(n, v) in &opensynapse::action::MOUSE[..7] {
                    if chip(ui, &n.to_uppercase(), e.mouse == v).clicked() {
                        e.mouse = v;
                    }
                }
            }
            Kind::Media => {
                for (label, a) in MEDIA_KEYS {
                    if chip(ui, label, e.media == *a).clicked() {
                        e.media = a.clone();
                    }
                }
            }
            Kind::Hypershift => {
                ui.label(
                    RichText::new(
                        "Hold this button and the other buttons do what their HYPERSHIFT layer says \
                         (switch LAYER at the top to edit it). Let go and they go back to normal.",
                    )
                    .color(DIM),
                );
            }
            Kind::Dpi => {
                if chip(ui, "DPI UP", e.dpi_up).clicked() {
                    e.dpi_up = true;
                }
                if chip(ui, "DPI DOWN", !e.dpi_up).clicked() {
                    e.dpi_up = false;
                }
            }
            Kind::Disabled => {
                ui.label(RichText::new("button does nothing").color(DIM));
            }
            Kind::Raw => {
                ui.add(TextEdit::singleline(&mut e.raw).hint_text("hex bytes").desired_width(200.0));
            }
        });
        if e.kind != Kind::Key {
            return;
        }
        // one column per group; a group that doesn't fit goes to the next row
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 28.0;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                ui.label(RichText::new("COMMON KEYS").color(DIM).small());
                egui::Grid::new("mods").spacing([6.0, 6.0]).show(ui, |ui| {
                    for (i, (label, bit)) in opensynapse::action::MODS[..4].iter().enumerate() {
                        if chip(ui, &label.to_uppercase(), e.mods & bit != 0).clicked() {
                            e.mods ^= bit;
                        }
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                // an empty box just means no key picked yet
                let err = e.build().err().filter(|_| !e.key.trim().is_empty());
                let color = match err {
                    Some(_) => PINK,
                    None => YELLOW,
                };
                let edit = TextEdit::singleline(&mut e.key).hint_text("a, 1, ctrl+z, f13").desired_width(140.0);
                ui.add(edit.text_color(color));
                if let Some(err) = err {
                    ui.label(RichText::new(err).color(PINK).small());
                }
            });
            for (group, cols, keys) in SPECIAL_KEYS {
                // egui only wraps single widgets, so measure the group: a chip is its text
                // plus padding, the widest key sets the column width
                let widest = keys.iter().map(|k| k.len()).max().unwrap_or(0) as f32;
                let font = egui::TextStyle::Button.resolve(ui.style());
                let char_w = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
                let spacing = ui.spacing().item_spacing.x;
                let width = *cols as f32 * (widest * char_w + 32.0) + spacing;
                // available_width() in a wrapping layout is the whole row, so measure from the cursor
                if ui.max_rect().right() - ui.cursor().min.x < width {
                    ui.end_row();
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new(*group).color(DIM).small());
                    egui::Grid::new(group).spacing([6.0, 6.0]).show(ui, |ui| {
                        for (i, k) in keys.iter().enumerate() {
                            if chip(ui, &k.to_uppercase(), e.key == *k).clicked() {
                                e.key = k.to_string();
                            }
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
                });
            }
        });
    }
}

/// Darkens the lighting tab and says why it's off. Drawn on its own layer
/// because the tab's own painter is faded while it's disabled.
fn offline_overlay(ui: &egui::Ui, area: egui::Rect) {
    // Middle sits above the panels and below modals, so dialogs still show on top
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("openrazer-offline")));
    p.rect_filled(area, CornerRadius::ZERO, BG.gamma_multiply(0.75));
    let c = area.center();
    p.text(c - egui::vec2(0.0, 18.0), Align2::CENTER_CENTER, "OPENRAZER DAEMON OFFLINE", FontId::monospace(26.0), RED);
    p.text(
        c + egui::vec2(0.0, 14.0),
        Align2::CENTER_CENTER,
        "lighting needs the daemon: systemctl --user restart openrazer-daemon, then RESCAN",
        FontId::monospace(13.0),
        Color32::WHITE,
    );
}

fn turbo_toggle(ui: &mut egui::Ui, e: &mut Editor) {
    const TIP: &str = "While you hold the button, TURBO repeats the key or click every this many milliseconds, \
                       instead of sending it once.";
    ui.checkbox(&mut e.turbo, RichText::new("TURBO").color(YELLOW)).on_hover_text(TIP);
    if e.turbo {
        ui.label(RichText::new("EVERY").color(DIM)).on_hover_text(TIP);
        // no vertical padding, so the box is as tall as the text and the row doesn't jump
        ui.spacing_mut().button_padding.y = 0.0;
        ui.add(DragValue::new(&mut e.rate).range(1..=2000).suffix(" ms")).on_hover_text(TIP);
    }
}

fn cyberpunk(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x03, 0x03, 0x08);
    v.override_text_color = Some(Color32::from_rgb(0xd0, 0xd0, 0xe8));
    v.selection.bg_fill = PINK;
    v.selection.stroke = Stroke::new(1.0, BG);
    for (w, stroke) in [
        (&mut v.widgets.noninteractive, DIM),
        (&mut v.widgets.inactive, DIM),
        (&mut v.widgets.hovered, CYAN),
        (&mut v.widgets.active, PINK),
        (&mut v.widgets.open, CYAN),
    ] {
        w.corner_radius = CornerRadius::ZERO;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.weak_bg_fill = CARD;
        w.bg_fill = CARD;
    }
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        for font in s.text_styles.values_mut() {
            *font = FontId::new(font.size, FontFamily::Monospace);
        }
    });
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 760.0])
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png")).unwrap_or_default()),
        ..Default::default()
    };
    eframe::run_native(
        "opensynapse",
        options,
        Box::new(|cc| {
            cyberpunk(&cc.egui_ctx);
            Ok(Box::new(App::new(cc.egui_ctx.clone())))
        }),
    )
}
