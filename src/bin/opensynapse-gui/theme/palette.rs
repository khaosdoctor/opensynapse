use super::theme_parser::{Style, parse};
use eframe::egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    /// the button editor at the bottom, halfway between bg and panel unless a theme sets it
    pub pane: Color32,
    /// text fields and other sunken areas
    pub deep: Color32,
    /// buttons that aren't selected
    pub button: Color32,
    /// outline of panels and buttons
    pub border: Color32,
    pub text: Color32,
    /// text that has to stand out from `text`
    pub strong: Color32,
    /// text on filled buttons
    pub on_accent: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub warn: Color32,
    /// what the Hypershift layer is drawn in, apart from `warn` because in some themes warn looks like an accent
    pub hypershift: Color32,
    pub muted: Color32,
    pub error: Color32,
    pub ok: Color32,
    /// edge of a button whose action is OFF
    pub off: Color32,
    /// laid over a button under the mouse, so it wants some transparency
    pub hover: Color32,
}

/// One color everywhere, the starting point before any CSS is read.
pub(super) const fn uniform(c: Color32) -> Palette {
    Palette {
        bg: c, panel: c, card: c, pane: c, deep: c, button: c, border: c, text: c, strong: c,
        on_accent: c, accent: c, accent2: c, warn: c, hypershift: c, muted: c, error: c, ok: c, off: c, hover: c,
    }
}

/// Every theme goes on top of themes/opensynapse.css, so a theme file can skip properties.
pub(super) fn base(light: bool) -> Style {
    let mut style = Style { p: uniform(Color32::BLACK), radius: 0, font: String::new(), pane: None, hypershift: None };
    parse(BUILTIN[0].1, light, &mut style);
    // the default theme's yellow must not stand in for a theme that leaves --hypershift out, that one falls back to its own --warn
    style.hypershift = None;
    style
}

pub(super) const BUILTIN: &[(&str, &str)] = &[
    ("opensynapse", include_str!("../../../../themes/opensynapse.css")),
    ("synapse", include_str!("../../../../themes/synapse.css")),
    ("adwaita", include_str!("../../../../themes/adwaita.css")),
    ("breeze", include_str!("../../../../themes/breeze.css")),
    ("catppuccin", include_str!("../../../../themes/catppuccin.css")),
    ("kanagawa", include_str!("../../../../themes/kanagawa.css")),
    ("dracula", include_str!("../../../../themes/dracula.css")),
];
