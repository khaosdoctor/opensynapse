//! The theme part of the SETTINGS page, settings.json and the themes folder it lists.

use super::palette::BUILTIN;
use super::{FONTS, apply, load_previews, read, system_fonts, unload_previews, write};
use crate::shared::widgets::{heading, modal, neon, note, outline, tall, toggle_button};
use crate::theme::t;
use eframe::egui::{self, FontFamily, FontId, RichText};
use opensynapse::device;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Mode {
    System,
    Light,
    Dark,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Settings {
    pub(super) mode: Mode,
    pub(super) theme: String,
    /// a font family that replaces the theme's, None keeps the theme's
    #[serde(default)]
    pub(super) font: Option<String>,
    /// flattened, so its keys sit at the top of settings.json like the others
    #[serde(flatten)]
    pub(super) log: LogOptions,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct LogOptions {
    /// the bytes under each translated command
    #[serde(default)]
    pub raw_bytes: bool,
    #[serde(default = "yes")]
    pub log_wrap: bool,
    /// pixels, None keeps the theme's --log-font-size
    #[serde(default)]
    pub log_font_size: Option<f32>,
}

fn yes() -> bool {
    true
}

pub(super) static SETTINGS: RwLock<Settings> = RwLock::new(Settings {
    mode: Mode::System,
    theme: String::new(),
    font: None,
    log: LogOptions { raw_bytes: false, log_wrap: true, log_font_size: None },
});

pub(super) struct ThemeFile {
    id: String,
    css: String,
    /// None for the built-in ones
    path: Option<PathBuf>,
}

pub(super) static THEMES: RwLock<Vec<ThemeFile>> = RwLock::new(Vec::new());

/// Some while the NEW THEME dialog is open: the name typed so far and the last error.
static NEW_THEME: RwLock<Option<(String, Option<String>)>> = RwLock::new(None);

fn new_theme_dialog(ctx: &egui::Context) {
    let Some((mut name, error)) = read(&NEW_THEME).clone() else { return };
    let mut close = false;
    let mut failed = error;
    modal(ctx, "new-theme", 400.0, "NEW THEME", |ui| {
        note(ui, "We copy the OPENSYNAPSE theme, which lists every property, into your themes folder.");
        ui.text_edit_singleline(&mut name).request_focus();
        if let Some(e) = &failed {
            ui.label(RichText::new(e).color(t().error).small());
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if tall(ui, outline("CANCEL", t().muted)).clicked() {
                close = true;
            }
            if !tall(ui, neon("CREATE", t().accent)).clicked() {
                return;
            }
            match create_theme(&name) {
                Ok(file) => {
                    *write(&THEMES) = scan();
                    close = true;
                    // straight into whatever editor the desktop uses for .css
                    if let Err(e) = std::process::Command::new("xdg-open").arg(&file).spawn() {
                        eprintln!("could not open {}: {e}", file.display());
                    }
                }
                Err(e) => failed = Some(e),
            }
        });
    });
    *write(&NEW_THEME) = (!close).then_some((name, failed));
}

/// Copies the default theme to themes/<name>.css. The name becomes the file name and the button.
fn create_theme(name: &str) -> Result<PathBuf, String> {
    let name = device::check_name(name)?;
    let file = config("themes").ok_or("no config folder")?.join(format!("{name}.css"));
    if file.exists() {
        return Err(format!("{} already exists", file.display()));
    }
    device::write(&file, BUILTIN[0].1).map_err(|e| format!("could not write {}: {e}", file.display()))?;
    Ok(file)
}

pub(super) fn config(name: &str) -> Option<PathBuf> {
    device::config_dir().map(|c| c.join("opensynapse").join(name))
}

pub(super) fn scan() -> Vec<ThemeFile> {
    let mut themes: Vec<ThemeFile> =
        BUILTIN.iter().map(|(id, css)| ThemeFile { id: id.to_string(), css: css.to_string(), path: None }).collect();
    let Some(entries) = config("themes").and_then(|d| fs::read_dir(d).ok()) else { return themes };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "css")).collect();
    paths.sort();
    for path in paths {
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_lowercase) else { continue };
        let css = match fs::read_to_string(&path) {
            Ok(css) => css,
            Err(e) => {
                eprintln!("could not read {}: {e}", path.display());
                continue;
            }
        };
        let file = ThemeFile { id, css, path: Some(path) };
        match themes.iter_mut().find(|th| th.id == file.id) {
            Some(builtin) => *builtin = file,
            None => themes.push(file),
        }
    }
    themes
}

/// Falls back to the first theme when `id` is gone.
pub(super) fn theme_css(id: &str) -> (String, String) {
    let themes = read(&THEMES);
    let th = themes.iter().find(|th| th.id == id).or(themes.first());
    th.map_or((String::new(), String::new()), |th| (th.id.clone(), th.css.clone()))
}

fn save(ctx: &egui::Context, s: Settings) {
    *write(&SETTINGS) = s.clone();
    apply(ctx);
    store(&s);
}

fn store(s: &Settings) {
    let Some(file) = config("settings.json") else { return };
    if let Err(e) = device::save_json(&file, s) {
        eprintln!("could not save settings to {}: {e}", file.display());
    }
}

pub fn log_options() -> LogOptions {
    read(&SETTINGS).log
}

/// Skips `save`, which reloads the theme and every font. Nothing on screen changes but the log.
pub fn set_log_options(log: LogOptions) {
    write(&SETTINGS).log = log;
    store(&read(&SETTINGS));
}

pub fn settings_ui(ui: &mut egui::Ui) {
    let s = read(&SETTINGS).clone();
    heading(ui, "MODE");
    ui.horizontal(|ui| {
        for (m, label, tip) in [
            (Mode::System, "SYSTEM", "follow the desktop's light or dark setting"),
            (Mode::Light, "LIGHT", "always light"),
            (Mode::Dark, "DARK", "always dark"),
        ] {
            if toggle_button(ui, label, s.mode == m).on_hover_text(tip).clicked() && s.mode != m {
                save(ui.ctx(), Settings { mode: m, ..s.clone() });
            }
        }
    });

    ui.add_space(16.0);
    heading(ui, "THEME");
    let ids: Vec<(String, String)> = read(&THEMES)
        .iter()
        .map(|th| (th.id.clone(), th.path.as_ref().map_or("built in".into(), |p| p.display().to_string())))
        .collect();
    let current = theme_css(&s.theme).0;
    ui.horizontal_wrapped(|ui| {
        for (id, from) in ids {
            if toggle_button(ui, &id.to_uppercase(), id == current).on_hover_text(from).clicked() && id != current {
                save(ui.ctx(), Settings { theme: id, ..s.clone() });
            }
        }
    });
    let dir = config("themes").map(|d| d.display().to_string()).unwrap_or_default();
    ui.horizontal(|ui| {
        if tall(ui, outline("RELOAD", t().accent)).on_hover_text("read the themes folder and theme.css again").clicked() {
            *write(&THEMES) = scan();
            apply(ui.ctx());
        }
        if tall(ui, outline("NEW THEME", t().accent2)).on_hover_text("start your own theme from a copy of the default one").clicked() {
            *write(&NEW_THEME) = Some((String::new(), None));
        }
        note(ui, format!("add .css themes to {dir}"));
    });
    new_theme_dialog(ui.ctx());

    ui.add_space(16.0);
    heading(ui, "FONT");
    if read(&FONTS).is_empty() {
        *write(&FONTS) = system_fonts();
    }
    let fonts = read(&FONTS).clone();
    let mut pick = None;
    let open = egui::ComboBox::from_id_salt("font")
        .selected_text(s.font.clone().unwrap_or("THEME DEFAULT".into()))
        .width(320.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(s.font.is_none(), "THEME DEFAULT").clicked() {
                pick = Some(None);
            }
            let row = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
            egui::ScrollArea::vertical().max_height(400.0).show_rows(ui, row, fonts.len(), |ui, rows| {
                load_previews(ui.ctx(), &fonts, rows.clone());
                let loaded = ui.ctx().fonts(|f| f.families());
                for (name, _) in &fonts[rows] {
                    let family = FontFamily::Name(name.as_str().into());
                    // egui only has a family the frame after set_fonts, the normal font covers that frame
                    let family = if loaded.contains(&family) { family } else { FontFamily::Proportional };
                    let text = RichText::new(name).font(FontId::new(15.0, family));
                    if ui.selectable_label(s.font.as_deref() == Some(name.as_str()), text).clicked() {
                        pick = Some(Some(name.clone()));
                    }
                }
            });
        })
        .inner
        .is_some();
    if !open {
        unload_previews(ui.ctx());
    }
    if let Some(font) = pick.filter(|f| *f != s.font) {
        save(ui.ctx(), Settings { font, ..s });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn theme_names_stay_file_safe() {
        assert!(super::create_theme("  ").is_err());
        assert!(super::create_theme("../evil").is_err());
        assert!(super::create_theme("my theme").is_err());
    }
}
