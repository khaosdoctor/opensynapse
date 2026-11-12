//! The confirm modal, the native file dialogs and the SETTINGS page.

use crate::shared::widgets::{heading, modal, neon, note, outline, tall};
use crate::state::{App, Confirm, Op, queue, unqueue};
use crate::theme::{self, t};
use eframe::egui::{self, RichText};
use opensynapse::{device, profile};

impl App {
    pub(crate) fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
        heading(ui, "PROFILE FILES LOCATION");
        ui.horizontal(|ui| {
            if tall(ui, neon("CHANGE FOLDER", t().warn)).on_hover_text("where EXPORT PROFILE saves profile files").clicked() {
                self.pick_folder();
            }
            let dir = self.path.parent().map(|d| d.display().to_string()).unwrap_or_default();
            ui.add(egui::Label::new(RichText::new(&dir).color(t().muted)).truncate()).on_hover_text(&dir);
        });
        ui.add_space(16.0);
        theme::settings_ui(ui);
    }

    /// Native dialogs go through the XDG desktop portal and block the UI while open.
    fn dialog(&self) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new().add_filter("opensynapse profile", &["json"]);
        let Some(dir) = self.path.parent() else { return dialog };
        dialog.set_directory(dir)
    }

    /// The folder profile files go to. File names stay `<serial>-<profile>.json`.
    fn pick_folder(&mut self) {
        let Some(dir) = self.dialog().pick_folder() else { return };
        self.status = Ok(format!("PROFILE FOLDER > {}", dir.display()));
        self.dir = Some(dir);
        self.update_path();
    }

    pub(crate) fn pick_import(&mut self) {
        let Some(file) = self.dialog().pick_file() else { return };
        self.path = file;
        self.confirm = Some(Confirm::Import);
    }

    pub(crate) fn export(&mut self) {
        let name = self.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let Some(path) = self.dialog().set_file_name(name).save_file() else { return };
        self.export_profile(&path);
    }

    /// Queues the differences between a profile file and the device. Returns
    /// the status line, with `verb` saying why.
    fn queue_file(&mut self, path: &std::path::Path, verb: &str) -> Result<String, String> {
        let m = self.mice.get(self.current).ok_or("no mouse")?;
        let file: profile::ProfileFile = device::load_json(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let plan = profile::plan(m, self.profile, &file).map_err(|e| e.to_string())?;
        let n = plan.len();
        for (layer, index, action) in plan {
            queue(&mut self.ops, Op::Button { profile: self.profile, layer, index, action });
        }
        // the DPI editor queues its own changes, so give it the file's values
        (self.dpi, self.active, self.stages) = (file.dpi, file.active_stage, file.dpi_stages);
        self.select(self.selected);
        Ok(format!("P{} {verb} > {n} BUTTON CHANGES QUEUED", self.profile))
    }

    /// Queues the profile's original snapshot.
    fn reset_buttons(&mut self) {
        let Some(path) = self.mice.get(self.current).and_then(|m| profile::original_path(m, self.profile)) else { return };
        self.status = self.queue_file(&path, "RESET");
    }

    pub(crate) fn confirm_dialog(&mut self) {
        let Some(what) = self.confirm else { return };
        let p = self.profile;
        let (title, text, button) = match what {
            Confirm::Import => (
                "! OVERWRITE PROFILE",
                format!(
                    "Importing replaces profile P{p} with the contents of this file. The changes go to PENDING \
                     and reach the mouse when you press WRITE TO DEVICE."
                ),
                "IMPORT",
            ),
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
            Confirm::DeletePreset => (
                "! DELETE PRESET",
                format!("The preset {} is deleted from your presets folder.", self.preset.clone().unwrap_or_default().to_uppercase()),
                "DELETE",
            ),
        };
        let path = self.path.display().to_string();
        let modal = modal(&self.ctx, "confirm", 420.0, title, |ui| {
            ui.label(text);
            if matches!(what, Confirm::Import) {
                note(ui, path);
            }
            ui.add_space(10.0);
            let mut choice = None;
            ui.horizontal(|ui| {
                if tall(ui, neon(button, t().accent2)).clicked() {
                    choice = Some(true);
                }
                if tall(ui, neon("CANCEL", t().muted)).clicked() {
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
            Confirm::Import => self.status = self.queue_file(&self.path.clone(), "LOADED"),
            Confirm::ResetButtons => self.reset_buttons(),
            Confirm::SetDefault => self.set_default(),
            Confirm::ResetLighting => self.reset_lighting(),
            Confirm::DefaultLighting => self.set_lighting_default(),
            Confirm::FactoryLighting => self.factory_lighting(),
            Confirm::DeletePreset => self.delete_preset(),
        }
    }

    /// Open while `renaming` holds the name typed so far.
    pub(crate) fn rename_dialog(&mut self) {
        let Some(mut name) = self.renaming.clone() else { return };
        let p = self.profile;
        let (mut ok, mut cancel) = (false, false);
        let modal = modal(&self.ctx, "rename-profile", 400.0, &format!("RENAME P{p}"), |ui| {
            note(ui, "Profile names are local only and not saved on the mouse memory.");
            let field = ui.add(egui::TextEdit::singleline(&mut name).hint_text("no name").desired_width(f32::INFINITY));
            field.request_focus();
            ok = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                cancel = tall(ui, outline("CANCEL", t().muted)).clicked();
                ok |= tall(ui, neon("RENAME", t().accent)).clicked();
            });
        });
        if cancel || modal.should_close() {
            self.renaming = None;
            return;
        }
        if !ok {
            self.renaming = Some(name);
            return;
        }
        self.renaming = None;
        let name = name.trim().to_string();
        let op = Op::Rename { profile: p, name: name.clone() };
        if self.names.get(&p).map_or("", String::as_str) == name {
            unqueue(&mut self.ops, &op);
            return;
        }
        let Some(m) = self.mice.get(self.current) else { return };
        self.status = Ok(format!("QUEUED {}", op.describe(&m.def)));
        queue(&mut self.ops, op);
    }
}
