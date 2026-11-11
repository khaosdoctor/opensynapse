use crate::shared::widgets::{badge, chip, neon, note, outline, panel, pick, tab_row, tall, toggle_button};
use crate::state::{App, Confirm, Op, queue, unqueue};
use crate::theme::{self, t};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, RichText};
use opensynapse::device;
use opensynapse::rgb::{self, Effect};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
enum Fx {
    Static,
    Breath,
    Spectrum,
    Wave,
    Reactive,
    Off,
}

const FX: &[(&str, Fx)] = &[
    ("STATIC", Fx::Static),
    ("BREATH", Fx::Breath),
    ("SPECTRUM", Fx::Spectrum),
    ("WAVE", Fx::Wave),
    ("REACTIVE", Fx::Reactive),
    ("OFF", Fx::Off),
];

#[derive(Clone, PartialEq)]
pub(crate) struct ZoneUi {
    pub(crate) name: String,
    fx: Fx,
    color: [u8; 3],
    random: bool,
    reverse: bool,
    speed: u8,
    brightness: u8,
}

impl ZoneUi {
    pub(crate) fn from_effect(name: &str, e: Effect, brightness: u8) -> Self {
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

    pub(crate) fn op(&self) -> Op {
        Op::Light { zone: self.name.clone(), effect: self.effect(), brightness: self.brightness }
    }

    fn options(&mut self, ui: &mut egui::Ui) {
        let needs_color = matches!(self.fx, Fx::Static | Fx::Reactive) || (self.fx == Fx::Breath && !self.random);
        if self.fx == Fx::Breath && chip(ui, "RANDOM COLORS", self.random).clicked() {
            self.random = !self.random;
        }
        if needs_color {
            note(ui, "COLOR");
            ui.color_edit_button_srgb(&mut self.color);
            let [r, g, b] = self.color;
            ui.label(RichText::new(format!("#{r:02x}{g:02x}{b:02x}")).color(Color32::from_rgb(r, g, b)));
        }
        if self.fx == Fx::Wave {
            pick(ui, &[("◄ LEFT", false), ("RIGHT ►", true)], &mut self.reverse, true, chip);
        }
        if self.fx == Fx::Reactive {
            note(ui, "FADE");
            pick(ui, &[("FAST", 1), ("MEDIUM", 2), ("SLOW", 3)], &mut self.speed, true, chip);
        }
        if matches!(self.fx, Fx::Spectrum | Fx::Off) {
            note(ui, "no options");
        }
    }
}

impl App {
    pub(crate) fn lighting_ui(&mut self, ui: &mut egui::Ui) {
        let area = ui.max_rect();
        let Some(light) = &self.lighting else {
            ui.label(RichText::new("NO LIGHTING").color(t().accent2).size(28.0));
            ui.label(RichText::new("lighting goes through the OpenRazer driver, install openrazer-driver-dkms").color(t().muted));
            return;
        };
        let online = light.uses_daemon();
        let (state, color, tip) = match online {
            true => ("OPENRAZER ONLINE", t().ok, "Lighting goes through the OpenRazer daemon, which keeps it after a replug."),
            false => (
                "OPENRAZER OFFLINE",
                t().error,
                "The OpenRazer daemon isn't running or doesn't see this mouse, so lighting is off. Start it with \
                 `systemctl --user restart openrazer-daemon` and press RESCAN.",
            ),
        };
        // ui.horizontal centers items on the row, so the title lines up with the taller buttons
        ui.horizontal(|ui| {
            ui.label(RichText::new("> LIGHTING").color(t().accent2).strong());
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                badge(ui, RichText::new(state).color(color).small(), color, Margin::symmetric(6, 2)).on_hover_text(tip);
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
                panel(t().panel).show(ui, |ui| {
                    ui.set_width(ui.available_width().min(760.0));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).color(t().accent).strong().underline());
                        ui.add_space(16.0);
                        note(ui, "BRIGHTNESS");
                        ui.add(egui::Slider::new(&mut z.brightness, 0..=255).trailing_fill(true));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if tall(ui, neon("APPLY NOW", t().accent)).clicked() {
                                apply = Some(i);
                            }
                        });
                    });
                    tab_row(ui, FX, &mut z.fx);
                    ui.separator();
                    ui.horizontal(|ui| z.options(ui));
                });
            }
            ui.add_space(4.0);
            if tall(ui, neon("APPLY ALL NOW", t().accent)).on_hover_text("send every zone's change right away").clicked() {
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
        self.ops = self.zone_ui.iter().filter(|z| names.contains(&z.name)).map(ZoneUi::op).collect();
        self.apply();
        self.ops.extend(rest);
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
            if z == base {
                unqueue(&mut self.ops, &z.op());
                continue;
            }
            queue(&mut self.ops, z.op());
        }
    }

    pub(crate) fn lighting_original(&self) -> Option<PathBuf> {
        let m = self.mice.get(self.current)?;
        rgb::original_path(m.def.vid, m.def.pid, &m.serial)
    }

    fn lighting_actions(&mut self, ui: &mut egui::Ui) {
        if toggle_button(ui, "SYNC ALL ZONES", self.sync_zones).clicked() {
            self.sync_zones = !self.sync_zones;
        }
        for (button, tip, confirm) in [
            (neon("RESET", t().accent2), "put back the original lighting", Confirm::ResetLighting),
            (outline("SET DEFAULT", t().warn), "make the lighting on the mouse now the original RESET goes back to", Confirm::DefaultLighting),
            (outline("FACTORY RESET", t().accent2), "static Razer green on every zone", Confirm::FactoryLighting),
        ] {
            if tall(ui, button).on_hover_text(tip).clicked() {
                self.confirm = Some(confirm);
            }
        }
    }

    /// Lighting on the mouse as it was last read or written.
    pub(crate) fn lighting_now(&self) -> Vec<rgb::ZoneState> {
        self.zone_base.iter().map(|z| rgb::ZoneState { zone: z.name.clone(), effect: z.effect(), brightness: z.brightness }).collect()
    }

    /// Queues the original lighting snapshot.
    pub(crate) fn reset_lighting(&mut self) {
        let Some(path) = self.lighting_original() else { return };
        let saved: Vec<rgb::ZoneState> = match device::load_json(&path) {
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

    pub(crate) fn set_lighting_default(&mut self) {
        let Some(path) = self.lighting_original() else { return };
        self.status = device::save_json(&path,&self.lighting_now()).map(|_| format!("LIGHTING IS THE NEW ORIGINAL > {}", path.display())).map_err(|e| e.to_string());
    }

    pub(crate) fn factory_lighting(&mut self) {
        self.sync_zones = false;
        for z in &mut self.zone_ui {
            *z = ZoneUi::from_effect(&z.name.clone(), Effect::Static([0x00, 0xff, 0x00]), 255);
        }
        self.status = Ok("LIGHTING FACTORY RESET QUEUED".into());
    }
}

/// Darkens the lighting tab and says why it's off. Drawn on its own layer
/// because the tab's own painter is faded while it's disabled.
fn offline_overlay(ui: &egui::Ui, area: egui::Rect) {
    // Middle sits above the panels and below modals, so dialogs still show on top
    let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::unique("openrazer-offline")));
    p.rect_filled(area, theme::radius(), t().bg.gamma_multiply(0.75));
    let c = area.center();
    p.text(c - egui::vec2(0.0, 18.0), Align2::CENTER_CENTER, "OPENRAZER DAEMON OFFLINE", FontId::monospace(26.0), t().error);
    p.text(
        c + egui::vec2(0.0, 14.0),
        Align2::CENTER_CENTER,
        "lighting needs the daemon: systemctl --user restart openrazer-daemon, then RESCAN",
        FontId::monospace(13.0),
        t().strong,
    );
}
