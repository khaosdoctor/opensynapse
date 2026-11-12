//! Every styled button, frame, badge and modal comes from here, so the views
//! pick a widget and never deal with the theme's colors or radius themselves.

use crate::theme::{self, t};
use eframe::egui::{self, Color32, CornerRadius, Frame, Margin, RichText, Stroke};

pub(crate) fn tab(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let label = if selected { RichText::new(text).color(t().accent).strong().underline() } else { RichText::new(text).color(t().muted) };
    hover(ui.add(egui::Button::new(label).frame(false)))
}

/// Lays the theme's --hover color over a button under the mouse and shows a
/// hand cursor. egui skips its own hover look when a button has its own fill,
/// and all of ours do.
pub(crate) fn hover(r: egui::Response) -> egui::Response {
    if r.hovered() && r.enabled() {
        r.ctx.layer_painter(r.layer_id).rect_filled(r.rect, theme::radius(), t().hover);
        r.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// Top-level view switch, drawn as a tab with a border so it doesn't read as a setting.
pub(crate) fn view_tab(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let (fg, edge, fill) = match selected {
        true => (t().accent, t().accent, t().card),
        false => (t().muted, t().border, t().bg),
    };
    let button = framed(RichText::new(text).color(fg).strong(), fill, edge).corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 });
    tall(ui, button)
}

pub(crate) fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> egui::Response {
    let button = egui::Button::new(RichText::new(icon).font(theme::icon(17.0)).color(t().muted)).frame(false);
    hover(ui.add(button)).on_hover_text(tip)
}

fn framed(text: RichText, fill: Color32, edge: Color32) -> egui::Button<'static> {
    egui::Button::new(text).fill(fill).stroke(Stroke::new(1.0, edge)).corner_radius(theme::radius())
}

pub(crate) fn neon(text: &str, color: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text).color(t().on_accent).strong()).fill(color).corner_radius(theme::radius())
}

pub(crate) fn outline(text: &str, color: Color32) -> egui::Button<'static> {
    framed(RichText::new(text).color(color), t().button, color)
}

fn chip_button(text: &str, selected: bool) -> egui::Button<'static> {
    let (fg, edge, fill) = if selected { (t().on_accent, t().accent2, t().accent2) } else { (t().strong, t().border, t().button) };
    framed(RichText::new(text).color(fg), fill, edge)
}

pub(crate) fn chip(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    hover(ui.add(chip_button(text, selected)))
}

pub(crate) fn toggle_button(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    tall(ui, chip_button(text, selected))
}

/// Every button that can share a row with another is this tall, so the row lines up.
pub(crate) fn tall(ui: &mut egui::Ui, button: egui::Button) -> egui::Response {
    hover(ui.add(button.min_size(egui::vec2(0.0, 28.0))))
}

pub(crate) fn sized(ui: &mut egui::Ui, size: [f32; 2], button: egui::Button) -> egui::Response {
    hover(ui.add_sized(size, button))
}

pub(crate) fn delete_button(ui: &mut egui::Ui) -> egui::Response {
    hover(ui.add(framed(RichText::new("X").color(t().error).strong(), t().bg, t().error).small())).on_hover_text("remove")
}

/// One button per choice, the current one highlighted unless `on` is false.
/// True when a choice was clicked, even the one that was current already.
pub(crate) fn pick<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    list: &[(&str, T)],
    current: &mut T,
    on: bool,
    button: fn(&mut egui::Ui, &str, bool) -> egui::Response,
) -> bool {
    let mut clicked = false;
    for (label, v) in list {
        if button(ui, label, on && current == v).clicked() {
            (*current, clicked) = (v.clone(), true);
        }
    }
    clicked
}

pub(crate) fn tab_row<T: Clone + PartialEq>(ui: &mut egui::Ui, list: &[(&str, T)], current: &mut T) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        pick(ui, list, current, true, tab);
    });
}

pub(crate) fn panel(fill: Color32) -> Frame {
    Frame::new().fill(fill).inner_margin(Margin::same(14)).stroke(Stroke::new(1.0, t().border))
}

pub(crate) fn backdrop(fill: Color32, margin: Margin) -> Frame {
    Frame::new().fill(fill).inner_margin(margin)
}

pub(crate) fn card(edge: Color32) -> Frame {
    Frame::new().fill(t().card).stroke(Stroke::new(1.0, edge)).inner_margin(Margin::same(8))
}

pub(crate) fn badge(ui: &mut egui::Ui, text: RichText, color: Color32, margin: Margin) -> egui::Response {
    Frame::new().stroke(Stroke::new(1.0, color)).inner_margin(margin).show(ui, |ui| ui.label(text)).response
}

pub(crate) fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(format!("> {text}")).color(t().accent2).strong());
    ui.add_space(4.0);
}

pub(crate) fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(t().muted).small());
}

pub(crate) fn modal<R>(ctx: &egui::Context, id: &str, width: f32, title: &str, body: impl FnOnce(&mut egui::Ui) -> R) -> egui::ModalResponse<R> {
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_max_width(width);
        ui.label(RichText::new(title).color(t().accent2).strong());
        ui.add_space(6.0);
        body(ui)
    })
}
