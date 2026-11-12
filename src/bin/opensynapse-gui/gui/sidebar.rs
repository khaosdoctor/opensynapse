//! The two left columns, device and DPI on the far left and the profile column next to the mouse.

use crate::shared::widgets::{delete_button, heading, neon, note, outline, sized};
use crate::lib::debug;
use crate::state::{App, Confirm, Op, queue};
use crate::theme::t;
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Align, ComboBox, DragValue, Layout, RichText};

impl App {
    pub(crate) fn device_box(&mut self, ui: &mut egui::Ui) {
        // eframe already decodes the PNG for the window icon, so the logo needs no image loader
        let light = crate::theme::is_light();
        let logo = self.logo[light as usize].get_or_insert_with(|| {
            let png: &[u8] = match light {
                true => include_bytes!("../../../../assets/icon-light.png"),
                false => include_bytes!("../../../../assets/icon.png"),
            };
            let icon = eframe::icon_data::from_png_bytes(png).unwrap_or_default();
            let image = egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba);
            ui.ctx().load_texture("logo", image, egui::TextureOptions::LINEAR)
        });
        ui.vertical_centered(|ui| ui.add(egui::Image::new(&*logo).fit_to_exact_size(egui::vec2(72.0, 72.0))));
        // egui can't center a row, so it's pushed in by half of what's left
        let width = |text: &str| ui.painter().layout_no_wrap(text.into(), egui::FontId::proportional(22.0), t().text).size().x;
        let row = width("OPEN") + width("SYNAPSE") + ui.spacing().item_spacing.x;
        let indent = ((ui.available_width() - row) / 2.0).max(0.0);
        ui.horizontal(|ui| {
            ui.add_space(indent);
            ui.label(RichText::new("OPEN").color(t().accent).size(22.0).strong());
            ui.label(RichText::new("SYNAPSE").color(t().accent2).size(22.0).strong());
        });
        ui.add_space(14.0);

        heading(ui, "DEVICE");
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

    fn profile_label(&self, ui: &egui::Ui, p: u8) -> LayoutJob {
        let font = egui::TextStyle::Button.resolve(ui.style());
        let format = |color| TextFormat { font_id: font.clone(), color, ..Default::default() };
        let mut job = LayoutJob::default();
        let text = match self.profile_name(p) {
            Some(name) => format!("{name} (P{p})"),
            None => format!("P{p}"),
        };
        job.append(&text, 0.0, format(t().text));
        if self.active_profile() == Some(p) {
            job.append("[ACTIVE]", 6.0, format(t().ok));
        }
        job
    }

    pub(crate) fn profile_box(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.mice.get(self.current) else { return };
        let layers = m.def.layers.clone();
        let mut changed = false;
        let width = ui.available_width();
        heading(ui, "PROFILE");
        let list = self.profile_list();
        let labels: Vec<(u8, LayoutJob)> = list.iter().map(|&p| (p, self.profile_label(ui, p))).collect();
        let selected = self.profile_label(ui, self.profile);
        // a queued SET ACTIVE counts, so it can't be queued twice
        let queued = self.ops.iter().rev().find_map(|o| if let Op::SetActive(p) = o { Some(*p) } else { None });
        let can_activate = queued.or(self.active_profile()) != Some(self.profile);
        ComboBox::from_id_salt("profile")
            .selected_text(selected)
            .width(width)
            .show_ui(ui, |ui| {
                for (p, label) in labels {
                    changed |= ui.selectable_value(&mut self.profile, p, label).changed();
                }
            })
            .response
            .on_hover_text("Changes to the active profile also go to the live copy, so they apply right away.");
        // only when the mouse could be read at all, presets need that too
        let supported = self.active_profile().is_some();
        let activate = supported
            && ui
                .add_enabled_ui(can_activate, |ui| sized(ui, [width, 22.0], neon("SET ACTIVE", t().accent)))
                .inner
                .on_hover_text("make the mouse use this profile")
                .clicked();
        if activate {
            let op = Op::SetActive(self.profile);
            self.say_queued(&op);
            queue(&mut self.ops, op);
        }
        changed |= self.profile_actions(ui, &list);

        let wide = |ui: &mut egui::Ui, button, tip| sized(ui, [width, 26.0], button).on_hover_text(tip).clicked();
        ui.add_space(8.0);
        if wide(ui, outline("EXPORT PROFILE", t().accent), "save this profile to a file") {
            self.export();
        }
        if wide(ui, outline("IMPORT PROFILE", t().accent2), "queue a profile file's buttons over this profile") {
            self.pick_import();
        }
        ui.add_space(8.0);
        if wide(ui, outline("SET DEFAULT", t().warn), "make what's on the mouse now the original RESET TO DEFAULT goes back to") {
            self.confirm = Some(Confirm::SetDefault);
        }
        if wide(ui, neon("RESET TO DEFAULT", t().accent2), "put back this profile's original") {
            self.confirm = Some(Confirm::ResetButtons);
        }
        ui.add_space(18.0);

        heading(ui, "LAYER");
        for (i, l) in layers.iter().enumerate() {
            let selected = self.layer == i as u8;
            // yellow like the mouse outline on any layer that isn't the default one
            let button = match (selected, i) {
                (true, 1..) => neon(&l.to_uppercase(), t().hypershift),
                _ => egui::Button::selectable(selected, l.to_uppercase()),
            };
            if ui.add_sized([width, 24.0], button).clicked() && !selected {
                self.layer = i as u8;
                changed = true;
            }
        }
        if changed {
            // an open RENAME would otherwise name the profile switched to
            self.renaming = None;
            self.reload();
            let layer = layers.get(self.layer as usize).map_or("?", String::as_str).to_uppercase();
            self.status = Ok(format!("VIEWING P{} > {layer}", self.profile));
            self.update_path();
        }
    }

    /// Returns true when the shown profile changed.
    fn profile_actions(&mut self, ui: &mut egui::Ui, list: &[u8]) -> bool {
        let p = self.profile;
        let max = match (&self.onboard, &self.profile_presets) {
            (Some(on), _) => on.max,
            (None, Some(_)) => crate::lib::profile_presets::MAX,
            (None, None) => 0,
        };
        let free = (1..=max).find(|i| !list.contains(i));
        // the mouse can't drop the profile it's using, and a queued SET ACTIVE would make it that one
        let activating = self.ops.contains(&Op::SetActive(p));
        let can_delete = list.len() > 1 && self.active_profile() != Some(p) && !activating;
        let size = [ui.available_width(), 22.0];
        let supported = self.active_profile().is_some();
        let new = supported && ui.add_enabled_ui(free.is_some(), |ui| sized(ui, size, outline("NEW PROFILE", t().accent))).inner.clicked();
        // names live on this computer, so RENAME works on any mouse
        let rename = sized(ui, size, outline("RENAME PROFILE", t().accent)).clicked();
        let delete = supported && ui.add_enabled_ui(can_delete, |ui| sized(ui, size, outline("DELETE PROFILE", t().error))).inner.clicked();
        ui.add_space(8.0);
        if rename {
            self.renaming = Some(self.profile_name(p).unwrap_or_default());
        }
        if let (true, Some(n)) = (new, free) {
            let op = Op::CreateProfile(n);
            self.say_queued(&op);
            queue(&mut self.ops, op);
            self.profile = n;
            return true;
        }
        if !delete {
            return false;
        }
        // a profile that only exists in the queue just leaves it, with everything queued for it
        let created = self.ops.contains(&Op::CreateProfile(p));
        self.ops.retain(|o| match o {
            Op::Button { profile, .. } | Op::Rename { profile, .. } => *profile != p,
            Op::CreateProfile(x) => *x != p,
            _ => true,
        });
        let op = Op::DeleteProfile(p);
        self.say_queued(&op);
        if !created {
            queue(&mut self.ops, op);
        }
        self.profile = self.active_profile().unwrap_or(list[0]);
        true
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

        let width = ui.available_width();
        ui.add_enabled_ui(!self.ops.is_empty(), |ui| {
            if sized(ui, [width, 28.0], neon("CLEAR QUEUE", t().accent2)).clicked() {
                self.status = Ok(format!("CLEARED {} PENDING", self.ops.len()));
                self.ops.clear();
                self.reset_edits();
            }
            if sized(ui, [width, 28.0], neon("WRITE TO DEVICE", t().accent)).clicked() {
                self.apply();
            }
        });
        ui.add_space(14.0);
        self.pending_list(ui);
    }

    /// Follows the newest line, the panel around it keeps it a fixed height.
    pub(crate) fn log_panel(&mut self, ui: &mut egui::Ui) {
        heading(ui, "LOG");
        let options = crate::theme::log_options();
        let p = t();
        egui::Frame::new().fill(p.log_bg).inner_margin(6).show(ui, |ui| {
            // unwrapped lines need room to the side
            let scroll = match options.log_wrap {
                true => egui::ScrollArea::vertical(),
                false => egui::ScrollArea::both(),
            };
            scroll
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .show(ui, |ui| {
                    // the theme's font goes on every text style, a FontId keeps the log monospace
                    let font = egui::FontId::monospace(crate::theme::log_font_size());
                    let format = |color| TextFormat { font_id: font.clone(), color, ..Default::default() };
                    for line in debug::lines() {
                        let mut job = LayoutJob::default();
                        job.append(&format!("[{}] :: ", line.time), 0.0, format(p.log_time));
                        let color = match line.kind {
                            debug::Kind::Raw => p.log_raw,
                            debug::Kind::Info => p.log_text,
                            debug::Kind::Warn => p.warn,
                            debug::Kind::Error => p.error,
                        };
                        job.append(&line.message, 0.0, format(color));
                        if let Some(bytes) = line.bytes.filter(|_| options.raw_bytes) {
                            job.append(&format!("\n    {bytes}"), 0.0, format(p.log_raw.gamma_multiply(0.7)));
                        }
                        let label = egui::Label::new(job);
                        ui.add(if options.log_wrap { label.wrap() } else { label.extend() });
                    }
                });
        });
    }

    fn pending_list(&mut self, ui: &mut egui::Ui) {
        let mut remove = None;
        ui.horizontal(|ui| {
            heading(ui, &format!("PENDING QUEUE ({})", self.ops.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // as tall as the heading, so the row doesn't grow
                let height = ui.text_style_height(&egui::TextStyle::Body);
                let undo = ui.add_enabled_ui(!self.ops.is_empty(), |ui| sized(ui, [52.0, height], outline("UNDO", t().accent).small())).inner;
                if undo.on_hover_text("drop the last change").clicked() {
                    remove = self.ops.len().checked_sub(1);
                }
            });
        });
        if self.ops.is_empty() {
            note(ui, "no pending changes");
        }
        let Some(def) = self.mice.get(self.current).map(|m| &m.def) else { return };
        // in the order WRITE TO DEVICE runs them, so the list reads as what will happen. The last 12 only,
        // so the newest stays in sight, a scroll area here would eat the whole sidebar
        const SHOWN: usize = 12;
        let hidden = self.ops.len().saturating_sub(SHOWN);
        if hidden > 0 {
            note(ui, format!("+{hidden} earlier"));
        }
        let last = self.ops.len().saturating_sub(1);
        for (i, op) in self.ops.iter().enumerate().skip(hidden) {
            // right to left so the X is laid out first and the text wraps in whatever width is left
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let x = delete_button(ui);
                if x.clicked() {
                    remove = Some(i);
                }
                // the newest is what UNDO drops
                let text = RichText::new(format!("#{i} {}", op.describe(def))).small();
                let text = if i == last { text.color(t().accent).underline() } else { text.color(t().strong) };
                // egui puts a wrapping label at the top of its row, so the first line is pushed down to the X's middle by hand
                let line = ui.text_style_height(&egui::TextStyle::Small);
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.add_space(((x.rect.height() - line) / 2.0).max(0.0));
                    ui.add(egui::Label::new(text).wrap());
                });
            });
        }
        if let Some(i) = remove {
            let op = self.ops.remove(i);
            self.status = Ok(format!("REMOVED {}", op.describe(def)));
            self.reset_edits();
        }
    }
}
