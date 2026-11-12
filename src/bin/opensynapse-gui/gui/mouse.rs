//! The BUTTONS view, a drawn mouse with its buttons on it and a grid for the ones without a position.

use crate::shared::widgets::card;
use crate::state::App;
use crate::theme::{self, t};
use eframe::egui::{self, Align2, CornerRadius, FontId, RichText, Stroke, StrokeKind};
use opensynapse::action::{Action, key_name};
use opensynapse::device::Button;
use std::time::Duration;

/// The mouse body ends at y 74%; reserve a bit below it for side plates.
const USED_HEIGHT: f32 = 0.78;
/// The mouse drawing never grows past this, so big screens don't get a giant mouse.
const MAX_VIEW_WIDTH: f32 = 900.0;

impl App {
    pub(crate) fn buttons(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.mice.get(self.current) else {
            ui.label(RichText::new("NO LINK").color(t().accent2).size(28.0));
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
                clicked = self.mouse_view(ui, buttons, &placed, &lit, view_height, loose.is_empty());
            }
            egui::Grid::new("buttons").spacing([8.0, 8.0]).show(ui, |ui| {
                for (col, &i) in loose.iter().enumerate() {
                    let pending = self.pending(i).is_some();
                    let action = self.effective(i);
                    let mark = if pending { " *" } else { "" };
                    let off = action.as_ref() == Some(&Action::Disabled);
                    let text = action.map_or("??".into(), |a| format!("{a}{mark}"));
                    let accent = match (lit[i], i == self.selected, pending) {
                        (true, ..) => t().warn,
                        (_, true, _) => t().accent,
                        (.., true) => t().accent2,
                        _ if off => t().off,
                        _ => t().muted,
                    };
                    let r = card(accent)
                        .show(ui, |ui| {
                            ui.set_min_width(150.0);
                            ui.label(RichText::new(buttons[i].name.to_uppercase()).color(accent).small());
                            ui.label(RichText::new(text).color(if off { t().muted } else { t().strong }));
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
    /// `alone` is true when no button grid goes under the drawing.
    fn mouse_view(&self, ui: &mut egui::Ui, buttons: &[Button], placed: &[usize], lit: &[bool], max_height: f32, alone: bool) -> Option<usize> {
        // 16:10 canvas so `at` percentages mean the same thing at any window size
        // center what's drawn (body plus side plates) instead of the canvas, since a thumb grid on one side
        // would otherwise push the whole drawing off to that side. lo..hi is that span in percent
        // of the canvas (a marker is 52 wide plus 12 of plate padding on a 736-wide canvas).
        let half_marker = (26.0 + 12.0) / 736.0 * 100.0;
        let xs = placed.iter().filter_map(|&i| buttons[i].at.map(|[x, _]| x));
        let (lo, hi) = xs.fold((30.0f32, 70.0f32), |(lo, hi), x| (lo.min(x - half_marker), hi.max(x + half_marker)));
        let fit_height = max_height.max(200.0) / USED_HEIGHT;
        // the canvas may be wider than the panel, only the drawn span has to fit
        let w = (ui.available_width() * 100.0 / (hi - lo)).min(fit_height * 1.6).min(MAX_VIEW_WIDTH);
        let k = w / (460.0 * 1.6); // everything below scales with the canvas
        let drawn = w / 1.6 * USED_HEIGHT;
        // with nothing below the mouse it takes the whole view and sits in the middle of it,
        // a few pixels short so the scroll area doesn't show a scrollbar for nothing
        let height = if alone { (max_height - 4.0).max(drawn) } else { drawn };
        let (full, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
        let left = full.center().x - (lo + hi) / 2.0 / 100.0 * w;
        let top = full.center().y - drawn / 2.0;
        let rect = egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(w, w / 1.6));
        let at = |x: f32, y: f32| rect.min + egui::vec2(rect.width() * x / 100.0, rect.height() * y / 100.0);
        let p = ui.painter_at(rect);
        // yellow outline while editing the Hypershift (or any non-default) layer
        let outline = match self.layer {
            0 => t().accent2,
            _ => t().warn,
        };

        let marker = |i: usize| {
            let [x, y] = buttons[i].at.unwrap_or_default();
            egui::Rect::from_center_size(at(x, y), egui::vec2(52.0, 26.0) * k)
        };

        // side plate behind any buttons placed outside the body (thumb grids, side buttons)
        let outside = placed.iter().filter(|&&i| buttons[i].at.is_some_and(|[x, _]| !(30.0..=70.0).contains(&x)));
        if let Some(plate) = outside.map(|&i| marker(i)).reduce(|a, b| a.union(b)) {
            p.rect(plate.expand(12.0 * k), CornerRadius::same(14), t().panel, Stroke::new(1.5, outline), StrokeKind::Outside);
        }

        let body = egui::Rect::from_min_max(at(30.0, 2.0), at(70.0, 74.0));
        let seam = Stroke::new(1.0, t().muted);
        p.add(egui::Shape::convex_polygon(mouse_outline(body), t().panel, Stroke::new(1.5, outline)));
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
        p.rect(wheel, CornerRadius::same(10), t().bg, seam, StrokeKind::Inside);

        let mut clicked = None;
        for &i in placed {
            let r = marker(i);
            let resp = ui.interact(r, ui.scope_id().with(("btn", i)), egui::Sense::click());
            let pending = self.pending(i).is_some();
            // queued changes show what the button will do after APPLY
            let shown = self.effective(i);
            let off = shown == Some(Action::Disabled);
            let (fill, edge, text) = match (lit[i], i == self.selected, resp.hovered(), pending) {
                (true, ..) => (t().warn, t().warn, t().bg),
                (_, true, ..) => (t().bg, t().accent, t().accent),
                (_, _, true, _) => (t().bg, t().accent, t().strong),
                (.., true) => (t().bg, t().accent2, t().accent2),
                _ if off => (t().bg, t().off, t().muted),
                _ => (t().bg, t().muted, t().strong),
            };
            p.rect(r, theme::radius(), fill, Stroke::new(1.0, edge), StrokeKind::Inside);
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
}

/// Top-down mouse silhouette, a squared-off ellipse that is narrower at the front.
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
            let held: String = opensynapse::action::MODS[..4].iter().filter(|(_, b)| mods & b != 0).map(|(n, _)| format!("{}+", n[..1].to_uppercase())).collect();
            held + &key_name(*usage).unwrap_or("?".into()).to_uppercase()
        }
        Action::Mouse(b) => match b {
            1 => "LMB",
            2 => "RMB",
            3 => "MMB",
            4 => "BACK",
            5 => "FWD",
            9 => "W▲",
            10 => "W▼",
            0x68 => "W◄",
            0x69 => "W►",
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
        // op bytes from action::PROFILE, WRAP marks the ones that go around
        Action::Profile(0x01) => "PROF+".into(),
        Action::Profile(0x02) => "PROF-".into(),
        Action::Profile(0x04) => "PWRAP+".into(),
        Action::Profile(0x05) => "PWRAP-".into(),
        Action::Profile(_) => "PROF".into(),
        Action::ProfileSet(n) => format!("P{n}"),
        Action::Hypershift => "HYPER".into(),
        Action::Disabled => "OFF".into(),
        Action::Raw(_) => "RAW".into(),
    }
}
