//! Where each panel goes in the window.

use crate::lib::debug;
use crate::shared::widgets::{backdrop, icon_button, modal, panel, pick, view_tab};
use crate::state::{App, View};
use crate::theme::{self, t};
use eframe::egui::{self, Align, Layout, Margin, RichText};

/// The remap editor at the bottom never grows, the mouse view gets the rest.
const EDITOR_HEIGHT: f32 = 344.0;
const SIDEBAR_WIDTH: f32 = 250.0;
const LOG_HEIGHT: f32 = 160.0;

/// Status bar links, Phosphor codepoints for the GitHub logo, a bug and a globe.
const LINKS: &[(&str, &str, &str)] = &[
    ("\u{E576}", "source code on GitHub", "https://github.com/khaosdoctor/opensynapse"),
    (
        "\u{E5F4}",
        "report a bug",
        concat!(
            "https://github.com/khaosdoctor/opensynapse/issues/new?labels=bug&body=opensynapse%20v",
            env!("CARGO_PKG_VERSION"),
            "%0A%0AWhat%20happened%3A%0A"
        ),
    ),
    ("\u{E288}", "website", "https://oss.lsantos.dev/opensynapse/"),
];

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_dpi();
        self.poll_daemon();
        // every action reports through the status line, so logging its changes logs what happened
        let line = match &self.status {
            Ok(s) => s.clone(),
            Err(e) => format!("ERROR {e}"),
        };
        if line != self.logged && !line.is_empty() {
            debug::write(&line);
            self.logged = line;
        }
        self.confirm_dialog();
        self.rename_dialog();
        if self.disconnected.is_some() {
            modal(&self.ctx, "disconnected", 420.0, "! MOUSE DISCONNECTED", |ui| {
                ui.label("Plug it back in and opensynapse picks it up again. Pending changes are kept.");
            });
        }
        // the sidebar goes first so it runs the full height, the tabs only span the rest
        egui::Panel::left("dpi").exact_size(SIDEBAR_WIDTH).resizable(false).frame(panel(t().panel)).show(ui, |ui| {
            // laid out first so it keeps the bottom, the rest of the sidebar fills what's above
            egui::Panel::bottom("log").exact_size(LOG_HEIGHT).resizable(false).show_separator_line(false).frame(egui::Frame::new()).show(ui, |ui| self.log_panel(ui));
            self.device_box(ui);
            self.dpi_panel(ui);
        });

        let bar = backdrop(t().panel, Margin { left: 12, right: 12, top: 12, bottom: 0 });
        egui::Panel::top("tabs").frame(bar).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                pick(ui, &[("BUTTONS", View::Buttons), ("LIGHTING", View::Lighting)], &mut self.view, true, view_tab);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    pick(ui, &[("SETTINGS", View::Settings)], &mut self.view, true, view_tab);
                });
            });
        });

        let status = backdrop(t().panel, Margin::symmetric(12, 6));
        let (text, color) = match &self.status {
            Ok(s) => (format!("> {s}"), t().accent),
            Err(e) => (format!("! {e}"), t().accent2),
        };
        egui::Panel::bottom("status").frame(status).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(text).color(color));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let version = RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).color(t().muted).small();
                    ui.label(version);
                    // right to left, so the website ends up last before the version
                    for (icon, tip, url) in LINKS.iter().rev() {
                        if !icon_button(ui, icon, tip).clicked() {
                            continue;
                        }
                        // eframe can't open links here, xdg-open hands it to the default browser
                        if let Err(e) = std::process::Command::new("xdg-open").arg(url).spawn() {
                            self.status = Err(format!("could not open {url}: {e}"));
                        }
                    }
                });
            });
        });

        let center = egui::CentralPanel::default().frame(backdrop(t().bg, Margin::same(16)));
        if self.view != View::Buttons {
            center.show(ui, |ui| match self.view {
                View::Lighting => self.lighting_ui(ui),
                _ => self.settings_ui(ui),
            });
            return;
        }

        let name = self.mice.get(self.current).and_then(|m| m.def.buttons.get(self.selected)).map(|b| b.name.clone());
        let hyper = self.layer != 0;
        let mut editor_top = None;
        if let Some(name) = name {
            // a Frame strokes all four sides, the window edge on the right needs no line, so only the top is drawn
            let frame = panel(t().pane).stroke(egui::Stroke::NONE);
            let editor = egui::Panel::bottom("editor").exact_size(EDITOR_HEIGHT).resizable(false).frame(frame).show(ui, |ui| {
                theme::tinted(hyper, || {
                    // selectable buttons and combo boxes take egui's selection color, not ours
                    ui.visuals_mut().selection.bg_fill = t().accent2;
                    self.editor_ui(ui, &name);
                });
            });
            editor_top = Some(editor.response.rect);
        }
        if !self.mice.is_empty() {
            egui::Panel::left("profile").exact_size(190.0).resizable(false).frame(panel(t().bg)).show(ui, |ui| self.profile_box(ui));
        }
        center.show(ui, |ui| theme::tinted(hyper, || self.buttons(ui)));
        // drawn last so the center panel can't paint over it, two pixels like the profile column's border meeting it
        if let Some(r) = editor_top {
            ui.painter().rect_filled(egui::Rect::from_x_y_ranges(r.x_range(), r.top() - 1.0..=r.top() + 1.0), 0.0, t().border);
        }
    }
}
