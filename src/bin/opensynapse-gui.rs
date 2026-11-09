use eframe::egui::{
    self, Align, Align2, Color32, ComboBox, StrokeKind, CornerRadius, DragValue, FontFamily, FontId, Frame, Layout, Margin, RichText, Stroke,
    TextEdit,
};
use opensynapse::action::{Action, key_name, key_usage};
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
/// The mouse drawing never grows past this, so big screens don't get a giant mouse.
const MAX_VIEW_WIDTH: f32 = 900.0;

const BG: Color32 = Color32::from_rgb(0x07, 0x07, 0x0d);
const PANEL: Color32 = Color32::from_rgb(0x0e, 0x0e, 0x1a);
const CARD: Color32 = Color32::from_rgb(0x14, 0x14, 0x24);
const CYAN: Color32 = Color32::from_rgb(0x00, 0xf0, 0xff);
const PINK: Color32 = Color32::from_rgb(0xff, 0x2a, 0x6d);
const YELLOW: Color32 = Color32::from_rgb(0xfc, 0xee, 0x0a);
const DIM: Color32 = Color32::from_rgb(0x5a, 0x5a, 0x7a);
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
    ("MEDIA", 1, &["mute", "volumedown", "volumeup"]),
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Key,
    Mouse,
    Turbo,
    Dpi,
    Disabled,
    Raw,
}

const KINDS: [(Kind, &str); 6] = [
    (Kind::Key, "KEY"),
    (Kind::Mouse, "MOUSE"),
    (Kind::Turbo, "TURBO"),
    (Kind::Dpi, "DPI"),
    (Kind::Raw, "RAW"),
    (Kind::Disabled, "OFF"),
];

struct Editor {
    kind: Kind,
    mods: u8,
    key: String,
    mouse: u8,
    rate: u16,
    dpi_up: bool,
    raw: String,
}

impl Editor {
    fn load(a: &Action) -> Self {
        let mut e = Editor { kind: Kind::Raw, mods: 0, key: String::new(), mouse: 1, rate: 20, dpi_up: true, raw: String::new() };
        match a {
            Action::Key { mods, usage } => {
                e.kind = Kind::Key;
                e.mods = *mods;
                e.key = key_name(*usage).unwrap_or(format!("0x{usage:02x}"));
            }
            Action::Mouse(b) => (e.kind, e.mouse) = (Kind::Mouse, *b),
            Action::MouseTurbo { button, interval } => (e.kind, e.mouse, e.rate) = (Kind::Turbo, *button, *interval),
            Action::Dpi(n) => (e.kind, e.dpi_up) = (Kind::Dpi, *n == 1),
            Action::Disabled => e.kind = Kind::Disabled,
            _ => e.raw = a.encode().iter().map(|b| format!("{b:02x}")).collect(),
        }
        e
    }

    fn build(&self) -> Result<Action, String> {
        Ok(match self.kind {
            Kind::Key => {
                let usage = key_usage(self.key.trim()).ok_or(format!("unknown key '{}'", self.key))?;
                Action::Key { mods: self.mods, usage }
            }
            Kind::Mouse => Action::Mouse(self.mouse),
            Kind::Turbo => Action::MouseTurbo { button: self.mouse, interval: self.rate },
            Kind::Dpi => Action::Dpi(if self.dpi_up { 1 } else { 2 }),
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
    view: View,
    lighting: Option<Lighting>,
    zone_ui: Vec<ZoneUi>,
    /// one card drives every lighting zone
    sync_zones: bool,
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
            view: View::Buttons,
            lighting: None,
            zone_ui: Vec::new(),
            sync_zones: false,
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

    /// The path field follows the selected device and profile.
    fn update_path(&mut self) {
        if let Some(p) = self.mice.get(self.current).and_then(|m| profile::default_path(m, self.profile)) {
            self.path = p.display().to_string();
        }
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

    /// Follows DPI changes made with the mouse's own buttons.
    fn poll_dpi(&mut self) {
        self.ctx.request_repaint_after(POLL);
        if self.last_poll.elapsed() < POLL {
            return;
        }
        self.last_poll = Instant::now();
        let Some(m) = self.mice.get(self.current) else { return };
        let (Ok((dpi, _)), Ok(s)) = (m.dpi(), m.dpi_stages()) else { return };
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

    /// SAVE needs a valid action that differs from what the button will already do.
    fn can_save_button(&self) -> bool {
        self.editor.build().is_ok_and(|a| Some(a) != self.effective(self.selected))
    }

    /// SAVE DPI needs valid stages that differ from the device and from what's queued.
    fn can_save_dpi(&self) -> bool {
        let max = self.mice.get(self.current).map_or(0, |m| m.def.dpi_max);
        let valid = (1..=5).contains(&self.stages.len())
            && (1..=self.stages.len() as u8).contains(&self.active)
            && self.stages.iter().chain([&self.dpi]).all(|d| (100..=max).contains(d));
        let wanted = (self.dpi, self.active, &self.stages);
        let queued = self.ops.iter().any(|o| matches!(o, Op::Dpi { dpi, active, stages } if (*dpi, *active, stages) == wanted));
        valid && wanted != (self.device_dpi.0, self.device_dpi.1, &self.device_dpi.2) && !queued
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
            };
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
enum Op {
    Button { profile: u8, layer: u8, index: usize, action: Action },
    Dpi { dpi: u16, active: u8, stages: Vec<u16> },
}

impl Op {
    fn same_target(&self, other: &Op) -> bool {
        match (self, other) {
            (Op::Button { profile: p, layer: l, index: i, .. }, Op::Button { profile, layer, index, .. }) => {
                (p, l, i) == (profile, layer, index)
            }
            (Op::Dpi { .. }, Op::Dpi { .. }) => true,
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
        Action::Media(_) => "MEDIA".into(),
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

/// Bordered toggle-style button used across the editor.
fn chip(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let (fg, edge, fill) = if selected { (BG, PINK, PINK) } else { (Color32::WHITE, DIM, BG) };
    ui.add(
        egui::Button::new(RichText::new(text).color(fg))
            .fill(fill)
            .stroke(Stroke::new(1.0, edge))
            .corner_radius(CornerRadius::ZERO),
    )
}

fn neon_button(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(BG).strong())
            .fill(color)
            .corner_radius(CornerRadius::ZERO)
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(format!("> {text}")).color(PINK).strong());
    ui.add_space(4.0);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_dpi();
        self.import_dialog();
        let bar = Frame::new().fill(PANEL).inner_margin(Margin::same(12)).stroke(Stroke::new(1.0, PINK));
        egui::Panel::top("top").frame(bar).show(ui, |ui| self.top_bar(ui));

        let side = Frame::new().fill(PANEL).inner_margin(Margin::same(14)).stroke(Stroke::new(1.0, DIM));
        egui::Panel::left("dpi").exact_size(230.0).resizable(false).frame(side).show(ui, |ui| self.dpi_panel(ui));

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
        egui::CentralPanel::default().frame(center).show(ui, |ui| self.buttons(ui));
    }
}

impl App {
    fn lighting_ui(&mut self, ui: &mut egui::Ui) {
        let Some(light) = &self.lighting else {
            ui.label(RichText::new("NO LIGHTING").color(PINK).size(28.0));
            ui.label(RichText::new("lighting goes through the OpenRazer driver, install openrazer-driver-dkms").color(DIM));
            return;
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new("> LIGHTING").color(PINK).strong());
            ui.add_space(12.0);
            if chip(ui, "SYNC ALL ZONES", self.sync_zones).clicked() {
                self.sync_zones = !self.sync_zones;
            }
            let via = if light.uses_daemon() { "via openrazer daemon" } else { "via openrazer driver, the daemon isn't running" };
            ui.label(RichText::new(format!("changes apply right away, they don't go to PENDING > {via}")).color(DIM).small());
        });
        ui.add_space(10.0);

        // with sync on, the first zone's card drives every zone
        let shown = if self.sync_zones { 1 } else { self.zone_ui.len() };
        let mut apply: Option<(usize, bool)> = None; // (card, brightness only)
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
                        let slider = ui.add(egui::Slider::new(&mut z.brightness, 0..=255).trailing_fill(true));
                        if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                            apply = Some((i, true));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if neon_button(ui, "APPLY", CYAN).clicked() {
                                apply = Some((i, false));
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
        });

        let Some((card, brightness_only)) = apply else { return };
        let targets: Vec<usize> = if self.sync_zones { (0..self.zone_ui.len()).collect() } else { vec![card] };
        let source = self.zone_ui[card].clone();
        let result = targets.iter().try_for_each(|&t| {
            let z = &mut self.zone_ui[t];
            if self.sync_zones {
                *z = ZoneUi { name: z.name.clone(), ..source.clone() };
            }
            light.set_brightness(&z.name, z.brightness)?;
            if brightness_only {
                return Ok(());
            }
            light.set_effect(&z.name, &z.effect())
        });
        let what = if brightness_only { format!("BRIGHTNESS {}", source.brightness) } else { format!("{:?}", source.effect()) };
        let names: Vec<String> = targets.iter().map(|&t| self.zone_ui[t].name.to_uppercase()).collect();
        self.status = result.map(|_| format!("LIGHTING {} > {what}", names.join(","))).map_err(|e| e.to_string());
    }

    /// Reads zones from the OpenRazer driver and pre-fills each card from
    /// OpenRazer's saved state (the effect files are write-only).
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
            .map(|z| {
                let saved = light.current_effect(z).unwrap_or(Effect::Spectrum);
                ZoneUi::from_effect(z, saved, light.brightness(z).unwrap_or(255))
            })
            .collect();
    }
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
#[derive(Clone)]
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
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("OPEN").color(CYAN).size(22.0).strong());
            ui.label(RichText::new("SYNAPSE").color(PINK).size(22.0).strong());
            let version = RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(YELLOW).small();
            Frame::new().stroke(Stroke::new(1.0, YELLOW)).inner_margin(Margin::symmetric(5, 1)).show(ui, |ui| {
                ui.label(version);
            });
            ui.add_space(16.0);

            let names: Vec<String> = self.mice.iter().map(|m| m.def.name.to_uppercase()).collect();
            let current = names.get(self.current).cloned().unwrap_or("NO DEVICE".into());
            let mut pick = None;
            ComboBox::from_id_salt("device").selected_text(RichText::new(current).color(YELLOW)).width(320.0).show_ui(ui, |ui| {
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
            ui.add_space(16.0);
            ui.spacing_mut().item_spacing.x = 16.0;
            if tab(ui, "BUTTONS", self.view == View::Buttons).clicked() {
                self.view = View::Buttons;
            }
            if tab(ui, "LIGHTING", self.view == View::Lighting).clicked() {
                self.view = View::Lighting;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if neon_button(ui, "RESCAN", CYAN).clicked() {
                    self.connect();
                }
                let Some(m) = self.mice.get(self.current) else { return };
                let (profiles, layers) = (m.def.profiles.clone(), m.def.layers.clone());
                let mut changed = false;
                for (i, l) in layers.iter().enumerate().rev() {
                    changed |= ui.selectable_value(&mut self.layer, i as u8, l.to_uppercase()).changed();
                }
                ui.label(RichText::new("LAYER").color(DIM));
                ComboBox::from_id_salt("profile").selected_text(format!("P{}", self.profile)).show_ui(ui, |ui| {
                    for p in profiles {
                        changed |= ui.selectable_value(&mut self.profile, p, format!("P{p}")).changed();
                    }
                });
                ui.label(RichText::new("PROFILE").color(DIM));
                if changed {
                    self.reload();
                    let layer = layers.get(self.layer as usize).map_or("?", String::as_str).to_uppercase();
                    self.status = Ok(format!("VIEWING P{} > {layer}", self.profile));
                    self.update_path();
                }
            });
        });
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
                if ui.small_button("✗").clicked() {
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
        ui.add_space(14.0);
        let can_save = self.can_save_dpi();
        if ui.add_enabled_ui(can_save, |ui| neon_button(ui, "SAVE DPI", YELLOW)).inner.clicked() {
            self.queue_dpi();
        }
        ui.add_space(24.0);

        // pending operations, pinned to the bottom of the sidebar (bottom_up: first added = lowest)
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.add_enabled_ui(!self.ops.is_empty(), |ui| {
                ui.horizontal(|ui| {
                    if neon_button(ui, "CLEAR", PINK).clicked() {
                        self.status = Ok(format!("CLEARED {} PENDING", self.ops.len()));
                        self.ops.clear();
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
                    if ui.small_button("✗").clicked() {
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
            }
            heading(ui, &format!("PENDING ({})", self.ops.len()));
        });
    }

    fn export(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        let path = PathBuf::from(&self.path);
        let r = profile::export(m, self.profile, &path);
        self.status = r.map(|_| format!("P{} SAVED > {}", self.profile, path.display())).map_err(|e| e.to_string());
    }

    /// Queues the differences between a profile file and the device.
    fn import(&mut self) {
        let Some(m) = self.mice.get(self.current) else { return };
        let file = profile::load(&PathBuf::from(&self.path));
        let plan = file.and_then(|f| profile::plan(m, self.profile, &f).map(|p| (f, p)));
        let (file, plan) = match plan {
            Ok(x) => x,
            Err(e) => {
                self.status = Err(e.to_string());
                return;
            }
        };
        let n = plan.len();
        self.queue(Op::Dpi { dpi: file.dpi, active: file.active_stage, stages: file.dpi_stages });
        for (layer, index, action) in plan {
            self.queue(Op::Button { profile: self.profile, layer, index, action });
        }
        self.status = Ok(format!("P{} LOADED > {n} BUTTON CHANGES + DPI QUEUED", self.profile));
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

        let marker = |i: usize| {
            let [x, y] = buttons[i].at.unwrap_or_default();
            egui::Rect::from_center_size(at(x, y), egui::vec2(52.0, 26.0) * k)
        };

        // side plate behind any buttons placed outside the body (thumb grids, side buttons)
        let outside = placed.iter().filter(|&&i| buttons[i].at.is_some_and(|[x, _]| !(30.0..=70.0).contains(&x)));
        if let Some(plate) = outside.map(|&i| marker(i)).reduce(|a, b| a.union(b)) {
            let plate = plate.expand(12.0 * k);
            p.rect_filled(plate, CornerRadius::same(14), PANEL);
            p.rect_stroke(plate, CornerRadius::same(14), Stroke::new(1.5, PINK), StrokeKind::Outside);
        }

        let body = egui::Rect::from_min_max(at(30.0, 2.0), at(70.0, 74.0));
        let seam = Stroke::new(1.0, DIM);
        p.add(egui::Shape::convex_polygon(mouse_outline(body), PANEL, Stroke::new(1.5, PINK)));
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
        let mut save = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new("> REMAP:").color(PINK).strong());
            ui.label(RichText::new(name.to_uppercase()).color(CYAN).strong().underline());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let can_save = self.can_save_button();
                save = ui.add_enabled_ui(can_save, |ui| neon_button(ui, "SAVE", CYAN)).inner.clicked();
            });
        });
        egui::ScrollArea::vertical().show(ui, |ui| self.editor_fields(ui));
        if save {
            self.queue_button();
        }
    }

    /// Profile file: read-only path, a portal file picker, export and import.
    fn file_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("> PROFILE FILE:").color(PINK).strong());
            ui.add(egui::Label::new(RichText::new(&self.path).color(DIM)).truncate());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if neon_button(ui, "IMPORT", PINK).clicked() {
                    self.confirm_import = true;
                }
                if neon_button(ui, "EXPORT", CYAN).clicked() {
                    self.export();
                }
                if neon_button(ui, "OPEN", YELLOW).clicked() {
                    self.pick_file();
                }
            });
        });
    }

    /// Native file chooser via the XDG desktop portal (blocks the UI while open).
    fn pick_file(&mut self) {
        let current = PathBuf::from(&self.path);
        let mut dialog = rfd::FileDialog::new().add_filter("opensynapse profile", &["json"]);
        if let Some(dir) = current.parent() {
            dialog = dialog.set_directory(dir);
        }
        let Some(picked) = dialog.pick_file() else { return };
        self.path = picked.display().to_string();
        self.status = Ok(format!("PROFILE FILE > {}", self.path));
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
        ui.horizontal(|ui| match e.kind {
            Kind::Key => {} // laid out as columns below
            Kind::Mouse | Kind::Turbo => {
                for &(n, v) in &opensynapse::action::MOUSE[..7] {
                    if chip(ui, &n.to_uppercase(), e.mouse == v).clicked() {
                        e.mouse = v;
                    }
                }
                if e.kind == Kind::Turbo {
                    ui.label(RichText::new("EVERY").color(DIM));
                    ui.add(DragValue::new(&mut e.rate).range(1..=2000).suffix(" ms"));
                }
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
        // one column per group; columns wrap onto a new row when the window is narrow
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
                let ok = key_usage(e.key.trim()).is_some();
                let edit = TextEdit::singleline(&mut e.key).hint_text("a-z 0-9 - = [ ] ; ' , . /").desired_width(140.0);
                ui.add(edit.text_color(if ok { YELLOW } else { PINK }));
            });
            for (group, cols, keys) in SPECIAL_KEYS {
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
