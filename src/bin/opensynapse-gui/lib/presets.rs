//! Lighting presets on the lighting tab. Loading one queues its zones like any
//! other lighting edit, so it still waits for WRITE TO DEVICE.

use crate::lib::lighting::ZoneUi;
use crate::shared::widgets::{modal, neon, note, outline, tall};
use crate::state::{App, Confirm};
use crate::theme::t;
use eframe::egui::{self, ComboBox, RichText};
use opensynapse::preset;

impl App {
    pub(crate) fn presets_row(&mut self, ui: &mut egui::Ui) {
        let mut pick = None;
        ui.horizontal(|ui| {
            note(ui, "PRESET");
            let shown = self.preset.clone().unwrap_or("NONE".into()).to_uppercase();
            ComboBox::from_id_salt("preset").selected_text(RichText::new(shown).color(t().warn).small()).width(180.0).show_ui(ui, |ui| {
                if self.presets.is_empty() {
                    ui.label(RichText::new("no presets yet").color(t().muted));
                }
                for name in &self.presets {
                    if ui.selectable_label(self.preset.as_ref() == Some(name), name.to_uppercase()).clicked() {
                        pick = Some(name.clone());
                    }
                }
            });
            if tall(ui, outline("SAVE AS", t().accent)).on_hover_text("save what the cards show as a preset").clicked() {
                self.preset_name = Some((self.preset.clone().unwrap_or_default(), None));
            }
            ui.add_enabled_ui(self.preset.is_some(), |ui| {
                if tall(ui, outline("DELETE", t().accent2)).on_hover_text("delete the picked preset").clicked() {
                    self.confirm = Some(Confirm::DeletePreset);
                }
            });
        });
        ui.add_space(8.0);
        if let Some(name) = pick {
            self.load_preset(name);
        }
        self.preset_dialog();
    }

    /// Re-reads the presets folder, after a save or a delete and when the GUI starts.
    pub(crate) fn scan_presets(&mut self) {
        self.presets = preset::list();
        if self.preset.as_ref().is_some_and(|p| !self.presets.contains(p)) {
            self.preset = None;
        }
    }

    fn load_preset(&mut self, name: String) {
        match preset::load(&name) {
            Ok(saved) => {
                let total = saved.len();
                let used = self.queue_zones(saved);
                self.status = Ok(format!("PRESET {} QUEUED > {used} OF {total} ZONES", name.to_uppercase()));
            }
            Err(e) => self.status = Err(e.to_string()),
        }
        self.preset = Some(name);
    }

    pub(crate) fn delete_preset(&mut self) {
        let Some(name) = self.preset.take() else { return };
        self.status = preset::delete(&name).map(|_| format!("PRESET {} DELETED", name.to_uppercase())).map_err(|e| e.to_string());
        self.scan_presets();
    }

    /// The SAVE AS dialog, open while `preset_name` holds the name typed so far and the last error.
    fn preset_dialog(&mut self) {
        let Some((mut name, error)) = self.preset_name.clone() else { return };
        let mut close = false;
        let mut failed = error;
        let mut saved = None;
        let exists = self.presets.contains(&name.trim().to_lowercase());
        modal(&self.ctx, "save-preset", 400.0, "SAVE PRESET", |ui| {
            note(ui, "Every zone's effect, colors and brightness as the cards show them. Letters, digits, - and _.");
            ui.text_edit_singleline(&mut name).request_focus();
            if exists {
                ui.label(RichText::new("a preset with this name is replaced").color(t().warn).small());
            }
            if let Some(e) = &failed {
                ui.label(RichText::new(e).color(t().error).small());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if tall(ui, outline("CANCEL", t().muted)).clicked() {
                    close = true;
                }
                if !tall(ui, neon("SAVE", t().accent)).clicked() {
                    return;
                }
                let zones: Vec<_> = self.zone_ui.iter().map(ZoneUi::state).collect();
                match preset::save(&name, &zones) {
                    Ok(path) => (close, saved) = (true, Some(path)),
                    Err(e) => failed = Some(e.to_string()),
                }
            });
        });
        self.preset_name = (!close).then_some((name, failed));
        let Some(path) = saved else { return };
        self.status = Ok(format!("PRESET SAVED > {}", path.display()));
        self.scan_presets();
        self.preset = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    }
}
