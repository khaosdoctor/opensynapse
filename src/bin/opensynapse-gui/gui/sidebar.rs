//! The two left columns, device and DPI on the far left and the profile column next to the mouse.

use crate::shared::widgets::{badge, delete_button, heading, neon, note, outline, sized, tall, toggle_button};
use crate::state::{App, Confirm, View};
use crate::theme::t;
use eframe::egui::{self, Align, ComboBox, DragValue, FontId, Layout, Margin, RichText};

impl App {
    pub(crate) fn device_box(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("OPEN").color(t().accent).size(22.0).strong());
            ui.label(RichText::new("SYNAPSE").color(t().accent2).size(22.0).strong());
        });
        let version = RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(t().warn).small();
        ui.horizontal(|ui| {
            badge(ui, version, t().warn, Margin::symmetric(5, 1));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let on = self.view == View::Settings;
                if toggle_button(ui, "SETTINGS", on).clicked() {
                    self.view = if on { View::Buttons } else { View::Settings };
                }
            });
        });
        ui.add_space(14.0);

        let names: Vec<String> = self.mice.iter().map(|m| m.def.name.to_uppercase()).collect();
        let current = names.get(self.current).cloned().unwrap_or("NO DEVICE".into());
        let mut pick = None;
        let width = ui.available_width();
        ComboBox::from_id_salt("device")
            .selected_text(RichText::new(current).color(t().warn).small())
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
        if sized(ui, [width, 28.0], neon("RESCAN", t().accent)).clicked() {
            self.connect();
        }
        ui.add_space(18.0);
    }

    /// The column left of the mouse drawing, with the profile and layer pickers and the profile file actions.
    pub(crate) fn profile_box(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.mice.get(self.current) else { return };
        let (profiles, layers) = (m.def.profiles.clone(), m.def.layers.clone());
        let mut changed = false;
        let width = ui.available_width();
        heading(ui, "PROFILE");
        // profile 0 is what the mouse runs right now and is lost on unplug (verified)
        let label = |p: u8| {
            let mut job = egui::text::LayoutJob::default();
            let font = FontId::monospace(14.0);
            job.append(&format!("P{p}"), 0.0, egui::TextFormat::simple(font.clone(), t().strong));
            if p == 0 {
                job.append("[LIVE]", 6.0, egui::TextFormat::simple(font, t().ok));
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
        let mut wide = |button, tip| sized(ui, [width, 26.0], button).on_hover_text(tip).clicked();
        if wide(neon("RESET", t().accent2), "put back this profile's original") {
            self.confirm = Some(Confirm::ResetButtons);
        }
        if wide(outline("SET DEFAULT", t().warn), "make what's on the mouse now the original RESET goes back to") {
            self.confirm = Some(Confirm::SetDefault);
        }
        if wide(outline("EXPORT", t().accent), "save this profile to a file") {
            self.export();
        }
        if wide(outline("IMPORT", t().accent2), "queue a profile file's buttons over this profile") {
            self.pick_import();
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

    pub(crate) fn dpi_panel(&mut self, ui: &mut egui::Ui) {
        heading(ui, "SENSITIVITY");
        let max = self.mice.get(self.current).map_or(20000, |m| m.def.dpi_max);
        ui.label(RichText::new(format!("{} DPI", self.dpi)).color(t().warn).size(26.0).strong());
        ui.spacing_mut().slider_width = ui.available_width();
        ui.add(egui::Slider::new(&mut self.dpi, 100..=max).step_by(50.0).show_value(false).trailing_fill(true));
        ui.add_space(14.0);

        heading(ui, "STAGES");
        let mut remove = None;
        // a grid with fixed-size cells, so the active marker doesn't push its row out of line
        egui::Grid::new("stages").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            for (i, dpi) in self.stages.iter_mut().enumerate() {
                let n = i as u8 + 1;
                let active = self.active == n;
                let mark = if active { RichText::new("▶").color(t().on_accent) } else { RichText::new("·").color(t().muted) };
                if ui.add_sized([26.0, 22.0], egui::Button::selectable(active, mark)).on_hover_text("make this the active stage").clicked() {
                    self.active = n;
                }
                ui.label(RichText::new(n.to_string()).color(t().muted));
                ui.add_sized([80.0, 22.0], DragValue::new(dpi).range(100..=max).speed(50).suffix(" DPI"));
                if delete_button(ui).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove.filter(|_| self.stages.len() > 1) {
            self.stages.remove(i);
            self.active = self.active.min(self.stages.len() as u8);
        }
        if self.stages.len() < 5 && ui.button("+ STAGE").clicked() {
            self.stages.push(*self.stages.last().unwrap_or(&800));
        }
        self.queue_dpi_edits();
        ui.add_space(24.0);

        // the buttons are pinned to the bottom, the list starts right under the stages so the
        // PENDING heading doesn't move as changes come in
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.add_enabled_ui(!self.ops.is_empty(), |ui| {
                ui.horizontal(|ui| {
                    if tall(ui, neon("CLEAR", t().accent2)).clicked() {
                        self.status = Ok(format!("CLEARED {} PENDING", self.ops.len()));
                        self.ops.clear();
                        self.reset_edits();
                    }
                    if tall(ui, neon("WRITE TO DEVICE", t().accent)).clicked() {
                        self.apply();
                    }
                });
            });
            ui.with_layout(Layout::top_down(Align::LEFT), |ui| self.pending_list(ui));
        });
    }

    /// Newest first, under a heading that stays put.
    fn pending_list(&mut self, ui: &mut egui::Ui) {
        heading(ui, &format!("PENDING ({})", self.ops.len()));
        if self.ops.is_empty() {
            note(ui, "no pending changes");
        }
        let Some(def) = self.mice.get(self.current).map(|m| &m.def) else { return };
        // newest 12 only, a scroll area here would eat the whole sidebar
        const SHOWN: usize = 12;
        let mut remove = None;
        let hidden = self.ops.len().saturating_sub(SHOWN);
        for (i, op) in self.ops.iter().enumerate().skip(hidden).rev() {
            // top-aligned so the X stays on the first line when a long change wraps
            ui.horizontal_top(|ui| {
                if delete_button(ui).clicked() {
                    remove = Some(i);
                }
                ui.add(egui::Label::new(RichText::new(op.describe(def)).color(t().strong).small()).wrap());
            });
        }
        if hidden > 0 {
            note(ui, format!("+{hidden} more"));
        }
        if let Some(i) = remove {
            let op = self.ops.remove(i);
            self.status = Ok(format!("REMOVED {}", op.describe(def)));
            self.reset_edits();
        }
    }
}
