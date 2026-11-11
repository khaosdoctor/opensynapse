//! How the GUI looks. Every theme is a CSS file, the ones in themes/ go inside
//! the binary and you can drop more in ~/.config/opensynapse/themes, each file
//! becomes a button on the settings page. If you name one like a built-in
//! theme, yours replaces it.
//!
//! The settings page also picks the mode and the font, and we save both in
//! ~/.config/opensynapse/settings.json. On top of everything,
//! ~/.config/opensynapse/theme.css can override single properties of whatever
//! theme is on.
//!
//! We only read custom properties like `--name: value;`, themes/opensynapse.css
//! lists all of them, and `color-scheme: light|dark` in theme.css. Selectors
//! are ignored, so anything outside a `prefers-color-scheme` block counts for
//! both modes.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontFamily, FontId, RichText, Stroke, Theme};
use opensynapse::device;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

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
    pub muted: Color32,
    pub error: Color32,
    pub ok: Color32,
    /// edge of a button whose action is OFF
    pub off: Color32,
    /// laid over a button under the mouse, so it wants some transparency
    pub hover: Color32,
}

/// Every theme gets applied on top of this, so a theme file can skip
/// properties. It has to match themes/opensynapse.css, there's a test for that.
const FALLBACK_DARK: Palette = Palette {
    bg: Color32::from_rgb(0x07, 0x07, 0x0d),
    panel: Color32::from_rgb(0x0e, 0x0e, 0x1a),
    card: Color32::from_rgb(0x14, 0x14, 0x24),
    pane: Color32::from_rgb(0x0e, 0x0e, 0x1a),
    deep: Color32::from_rgb(0x03, 0x03, 0x08),
    button: Color32::from_rgb(0x07, 0x07, 0x0d),
    border: Color32::from_rgb(0x5a, 0x5a, 0x7a),
    text: Color32::from_rgb(0xd0, 0xd0, 0xe8),
    strong: Color32::WHITE,
    on_accent: Color32::from_rgb(0x07, 0x07, 0x0d),
    accent: Color32::from_rgb(0x00, 0xf0, 0xff),
    accent2: Color32::from_rgb(0xff, 0x2a, 0x6d),
    warn: Color32::from_rgb(0xfc, 0xee, 0x0a),
    muted: Color32::from_rgb(0x5a, 0x5a, 0x7a),
    error: Color32::from_rgb(0xff, 0x30, 0x30),
    ok: Color32::from_rgb(0x39, 0xff, 0x14),
    off: Color32::from_rgb(0x2a, 0x2a, 0x3a),
    hover: Color32::from_rgba_unmultiplied_const(0xff, 0xff, 0xff, 0x1a),
};

const FALLBACK_LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf4, 0xf4, 0xfa),
    panel: Color32::from_rgb(0xe8, 0xe8, 0xf2),
    card: Color32::from_rgb(0xff, 0xff, 0xff),
    pane: Color32::from_rgb(0xe8, 0xe8, 0xf2),
    deep: Color32::from_rgb(0xdc, 0xdc, 0xe8),
    button: Color32::from_rgb(0xf4, 0xf4, 0xfa),
    border: Color32::from_rgb(0x7a, 0x7a, 0x96),
    text: Color32::from_rgb(0x20, 0x20, 0x2e),
    strong: Color32::from_rgb(0x0a, 0x0a, 0x14),
    on_accent: Color32::from_rgb(0xf4, 0xf4, 0xfa),
    accent: Color32::from_rgb(0x00, 0x7a, 0x8a),
    accent2: Color32::from_rgb(0xd4, 0x00, 0x4f),
    warn: Color32::from_rgb(0x8a, 0x6a, 0x00),
    muted: Color32::from_rgb(0x7a, 0x7a, 0x96),
    error: Color32::from_rgb(0xc8, 0x1e, 0x1e),
    ok: Color32::from_rgb(0x1a, 0x8a, 0x00),
    off: Color32::from_rgb(0xc8, 0xc8, 0xd8),
    hover: Color32::from_rgba_unmultiplied_const(0x00, 0x00, 0x00, 0x14),
};

const BUILTIN: &[(&str, &str)] = &[
    ("opensynapse", include_str!("../../../themes/opensynapse.css")),
    ("synapse", include_str!("../../../themes/synapse.css")),
    ("adwaita", include_str!("../../../themes/adwaita.css")),
    ("breeze", include_str!("../../../themes/breeze.css")),
    ("catppuccin", include_str!("../../../themes/catppuccin.css")),
    ("kanagawa", include_str!("../../../themes/kanagawa.css")),
    ("dracula", include_str!("../../../themes/dracula.css")),
];

static CURRENT: RwLock<Palette> = RwLock::new(FALLBACK_DARK);
static RADIUS: AtomicU8 = AtomicU8::new(0);

pub fn t() -> Palette {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

/// Phosphor icons, look the codepoints up on https://phosphoricons.com
pub fn icon(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}

pub fn radius() -> CornerRadius {
    CornerRadius::same(RADIUS.load(Ordering::Relaxed))
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    System,
    Light,
    Dark,
}

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    mode: Mode,
    theme: String,
    /// a font family that replaces the theme's, None keeps the theme's
    #[serde(default)]
    font: Option<String>,
}

static SETTINGS: RwLock<Settings> = RwLock::new(Settings { mode: Mode::System, theme: String::new(), font: None });

fn settings() -> Settings {
    SETTINGS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

struct ThemeFile {
    id: String,
    css: String,
    /// None for the built-in ones
    path: Option<PathBuf>,
}

static THEMES: RwLock<Vec<ThemeFile>> = RwLock::new(Vec::new());
/// Read the first time the settings page opens.
static FONTS: RwLock<Vec<(String, PathBuf)>> = RwLock::new(Vec::new());
/// The fonts the theme uses, without previews, so we can put them back.
static BASE_FONTS: RwLock<Option<egui::FontDefinitions>> = RwLock::new(None);
/// Fonts loaded to draw their own name in the font list, with the rows they
/// cover. Loading all of them would read hundreds of MB, so we only keep the
/// rows around the view and drop them when they scroll away or the list closes.
static PREVIEWS: RwLock<(std::ops::Range<usize>, Vec<Preview>)> = RwLock::new((0..0, Vec::new()));
type Preview = (String, Arc<FontData>);
/// Rows load in blocks this big, so a rebuild doesn't happen on every scroll step.
const PREVIEW_BLOCK: usize = 20;

fn config(name: &str) -> Option<PathBuf> {
    device::config_dir().map(|c| c.join("opensynapse").join(name))
}

fn scan() -> Vec<ThemeFile> {
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

pub fn start(ctx: &egui::Context) {
    let saved = config("settings.json")
        .and_then(|f| fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok());
    if let Some(s) = saved {
        *SETTINGS.write().unwrap_or_else(|e| e.into_inner()) = s;
    }
    *THEMES.write().unwrap_or_else(|e| e.into_inner()) = scan();
    apply(ctx);
    follow_desktop(ctx.clone());
}

fn save(ctx: &egui::Context, s: Settings) {
    let json = serde_json::to_string_pretty(&s).unwrap_or_default();
    *SETTINGS.write().unwrap_or_else(|e| e.into_inner()) = s;
    apply(ctx);
    let Some(file) = config("settings.json") else { return };
    let written = file.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(&file, json));
    if let Err(e) = written {
        eprintln!("could not save settings to {}: {e}", file.display());
    }
}

pub fn settings_ui(ui: &mut egui::Ui) {
    let s = settings();
    super::heading(ui, "MODE");
    ui.horizontal(|ui| {
        for (m, label, tip) in [
            (Mode::System, "SYSTEM", "follow the desktop's light or dark setting"),
            (Mode::Light, "LIGHT", "always light"),
            (Mode::Dark, "DARK", "always dark"),
        ] {
            if super::toggle_button(ui, label, s.mode == m).on_hover_text(tip).clicked() && s.mode != m {
                save(ui.ctx(), Settings { mode: m, ..s.clone() });
            }
        }
    });

    ui.add_space(16.0);
    super::heading(ui, "THEME");
    let ids: Vec<(String, String)> = THEMES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|th| (th.id.clone(), th.path.as_ref().map_or("built in".into(), |p| p.display().to_string())))
        .collect();
    let current = theme_css(&s.theme).0;
    ui.horizontal_wrapped(|ui| {
        for (id, from) in ids {
            if super::toggle_button(ui, &id.to_uppercase(), id == current).on_hover_text(from).clicked() && id != current {
                save(ui.ctx(), Settings { theme: id, ..s.clone() });
            }
        }
    });
    let dir = config("themes").map(|d| d.display().to_string()).unwrap_or_default();
    ui.horizontal(|ui| {
        if super::outline_button(ui, "RELOAD", t().accent).on_hover_text("read the themes folder and theme.css again").clicked() {
            *THEMES.write().unwrap_or_else(|e| e.into_inner()) = scan();
            apply(ui.ctx());
        }
        ui.label(RichText::new(format!("add .css themes to {dir}")).color(t().muted).small());
    });

    ui.add_space(16.0);
    super::heading(ui, "FONT");
    if FONTS.read().unwrap_or_else(|e| e.into_inner()).is_empty() {
        *FONTS.write().unwrap_or_else(|e| e.into_inner()) = system_fonts();
    }
    let fonts = FONTS.read().unwrap_or_else(|e| e.into_inner()).clone();
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

/// Loads the fonts for the block of rows in view plus one block before and
/// after, so scrolling shows them right away, and drops the ones outside.
fn load_previews(ctx: &egui::Context, fonts: &[(String, PathBuf)], visible: std::ops::Range<usize>) {
    let start = (visible.start / PREVIEW_BLOCK).saturating_sub(1) * PREVIEW_BLOCK;
    let end = ((visible.end / PREVIEW_BLOCK + 2) * PREVIEW_BLOCK).min(fonts.len());
    let mut previews = PREVIEWS.write().unwrap_or_else(|e| e.into_inner());
    if previews.0 == (start..end) {
        return;
    }
    let mut loaded = Vec::new();
    for (name, file) in &fonts[start..end] {
        // fonts still in range keep their bytes, only the new ones are read from disk
        let kept = previews.1.iter().find(|(n, _)| n == name).map(|(_, data)| data.clone());
        let data = kept.or_else(|| fs::read(file).ok().map(|b| Arc::new(FontData::from_owned(b))));
        if let Some(data) = data {
            loaded.push((name.clone(), data));
        }
    }
    *previews = (start..end, loaded);
    set_fonts_with_previews(ctx, &previews.1);
}

fn unload_previews(ctx: &egui::Context) {
    let mut previews = PREVIEWS.write().unwrap_or_else(|e| e.into_inner());
    if previews.1.is_empty() {
        return;
    }
    *previews = (0..0, Vec::new());
    set_fonts_with_previews(ctx, &[]);
}

fn set_fonts_with_previews(ctx: &egui::Context, previews: &[Preview]) {
    let mut defs = BASE_FONTS.read().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default();
    for (name, data) in previews {
        let key = format!("preview {name}");
        defs.font_data.insert(key.clone(), data.clone());
        defs.families.insert(FontFamily::Name(name.as_str().into()), vec![key]);
    }
    ctx.set_fonts(defs);
}

/// Only fonts that cover English, so symbol fonts like Wingdings can't turn
/// every label into pictures.
fn system_fonts() -> Vec<(String, PathBuf)> {
    let format = "--format=%{family[0]}\t%{style[0]}\t%{file}\n";
    let Ok(out) = Command::new("fc-list").args([format, ":lang=en"]).output() else {
        return Vec::new();
    };
    // family -> (how far from regular the style is, file)
    let mut best: std::collections::BTreeMap<String, (u8, PathBuf)> = std::collections::BTreeMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut parts = line.split('\t');
        let (Some(family), Some(style), Some(file)) = (parts.next(), parts.next(), parts.next()) else { continue };
        // egui reads TrueType and OpenType, bitmap fonts like .pcf.gz can't be previewed or used
        let usable = [".ttf", ".otf", ".ttc"].iter().any(|ext| file.to_lowercase().ends_with(ext));
        if family.trim().is_empty() || !usable {
            continue;
        }
        let style = style.to_lowercase();
        let rank = match style.as_str() {
            "regular" | "book" | "normal" | "roman" => 0,
            _ if !style.contains("bold") && !style.contains("italic") && !style.contains("oblique") => 1,
            _ => 2,
        };
        let entry = best.entry(family.trim().to_string()).or_insert((u8::MAX, PathBuf::new()));
        if rank < entry.0 {
            *entry = (rank, PathBuf::from(file));
        }
    }
    let mut fonts: Vec<(String, PathBuf)> = best.into_iter().map(|(family, (_, file))| (family, file)).collect();
    fonts.sort_by_key(|(f, _)| f.to_lowercase());
    fonts
}

fn font_bytes(families: &str) -> Option<Vec<u8>> {
    let pattern: String = families.split(',').map(|f| f.trim().trim_matches(['"', '\''])).collect::<Vec<_>>().join(",");
    let out = Command::new("fc-match").args(["--format=%{file}", &pattern]).output().ok()?;
    fs::read(String::from_utf8_lossy(&out.stdout).trim()).ok()
}

/// Falls back to the first theme when `id` is gone.
fn theme_css(id: &str) -> (String, String) {
    let themes = THEMES.read().unwrap_or_else(|e| e.into_inner());
    let th = themes.iter().find(|th| th.id == id).or(themes.first());
    th.map_or((String::new(), String::new()), |th| (th.id.clone(), th.css.clone()))
}

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS_IFACE: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";

/// The portal answers 1 for dark, 2 for light and 0 for no preference. When
/// there's no portal or no preference we go dark, since that's the app's own look.
fn prefers_light() -> bool {
    let Ok(conn) = Connection::session() else { return false };
    let Ok(reply) = conn.call_method(Some(PORTAL), PORTAL_PATH, Some(SETTINGS_IFACE), "ReadOne", &(APPEARANCE, "color-scheme"))
    else {
        return false;
    };
    let Ok(value) = reply.body().deserialize::<OwnedValue>() else { return false };
    u32::try_from(value) == Ok(2)
}

struct Style {
    p: Palette,
    radius: u8,
    font: String,
    /// None until a theme sets --pane, then apply works it out from bg and panel
    pane: Option<Color32>,
}

pub fn apply(ctx: &egui::Context) {
    let s = settings();
    let user_css = config("theme.css").and_then(|f| fs::read_to_string(f).ok());
    // the settings page wins over theme.css, which wins over the desktop
    let light = match s.mode {
        Mode::Light => true,
        Mode::Dark => false,
        Mode::System => user_css.as_deref().and_then(forced_scheme).unwrap_or_else(prefers_light),
    };
    let mut style = Style { p: if light { FALLBACK_LIGHT } else { FALLBACK_DARK }, radius: 0, font: "monospace".into(), pane: None };
    parse(&theme_css(&s.theme).1, light, &mut style);
    if let Some(css) = &user_css {
        parse(css, light, &mut style);
    }
    let mut p = style.p;
    p.pane = style.pane.unwrap_or(p.bg.lerp_to_gamma(p.panel, 0.5));
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = p;
    RADIUS.store(style.radius, Ordering::Relaxed);
    let radius = radius();

    let mut v = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = radius;
    v.menu_corner_radius = radius;
    v.extreme_bg_color = p.deep;
    // no override_text_color, it would also beat the selection color and put
    // --text on top of a filled selected item
    v.override_text_color = None;
    v.selection.bg_fill = p.accent2;
    v.selection.stroke = Stroke::new(1.0, p.on_accent);
    for (w, stroke) in [
        (&mut v.widgets.noninteractive, p.border),
        (&mut v.widgets.inactive, p.border),
        (&mut v.widgets.hovered, p.accent),
        (&mut v.widgets.active, p.accent2),
        (&mut v.widgets.open, p.accent),
    ] {
        w.corner_radius = radius;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.fg_stroke = Stroke::new(1.0, p.text);
        w.weak_bg_fill = p.button;
        w.bg_fill = p.button;
    }
    // egui's own widgets, like the dropdowns, get the same hover tint as our buttons
    v.widgets.hovered.weak_bg_fill = p.button.blend(p.hover);
    v.widgets.hovered.bg_fill = p.button.blend(p.hover);
    // same visuals for both, so egui's own system theme guess can't swap them out
    ctx.set_visuals_of(Theme::Dark, v.clone());
    ctx.set_visuals_of(Theme::Light, v);

    let font = s.font.unwrap_or(style.font);
    let mut defs = egui::FontDefinitions::default();
    // its own family, Phosphor maps a-z to blank ligature glyphs and would blank out text if mixed in
    defs.font_data.insert("icons".into(), Arc::new(FontData::from_static(include_bytes!("../../../assets/phosphor.ttf"))));
    defs.families.insert(FontFamily::Name("icons".into()), vec!["icons".into()]);
    let family = match font.as_str() {
        "monospace" => FontFamily::Monospace,
        _ => {
            match font_bytes(&font) {
                Some(bytes) => {
                    defs.font_data.insert("theme".into(), Arc::new(FontData::from_owned(bytes)));
                    defs.families.entry(FontFamily::Proportional).or_default().insert(0, "theme".into());
                }
                None => eprintln!("font {font} not found, using egui's default"),
            }
            FontFamily::Proportional
        }
    };
    *BASE_FONTS.write().unwrap_or_else(|e| e.into_inner()) = Some(defs.clone());
    // keep whatever previews are loaded, the font list may be open while the theme changes
    let previews = PREVIEWS.read().unwrap_or_else(|e| e.into_inner()).1.clone();
    set_fonts_with_previews(ctx, &previews);
    ctx.all_styles_mut(|st| {
        for f in st.text_styles.values_mut() {
            *f = FontId::new(f.size, family.clone());
        }
    });
    ctx.request_repaint();
}

fn follow_desktop(ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(conn) = Connection::session() else { return };
        let Ok(proxy) = Proxy::new(&conn, PORTAL, PORTAL_PATH, SETTINGS_IFACE) else { return };
        let Ok(changes) = proxy.receive_signal("SettingChanged") else { return };
        for msg in changes {
            let Ok((ns, key, _)) = msg.body().deserialize::<(String, String, OwnedValue)>() else { continue };
            if ns == APPEARANCE && key == "color-scheme" {
                apply(&ctx);
            }
        }
    });
}

/// `color-scheme: light` or `dark` in theme.css, for desktops that don't
/// report a preference. Some(true) is light.
fn forced_scheme(css: &str) -> Option<bool> {
    for decl in strip_comments(css).split([';', '{', '}']) {
        let Some((name, value)) = decl.split_once(':') else { continue };
        if name.trim() != "color-scheme" {
            continue;
        }
        return match value.trim() {
            "light" => Some(true),
            "dark" => Some(false),
            _ => None,
        };
    }
    None
}

fn parse(css: &str, light: bool, style: &mut Style) {
    // one entry per open block, Some(is light) for a prefers-color-scheme block and None for anything else
    let mut blocks: Vec<Option<bool>> = Vec::new();
    let mut buf = String::new();
    for c in strip_comments(css).chars() {
        match c {
            '{' => {
                blocks.push(buf.contains("prefers-color-scheme").then(|| buf.contains("light")));
                buf.clear();
            }
            ';' | '}' => {
                if blocks.iter().flatten().all(|&l| l == light) {
                    set(style, buf.trim());
                }
                buf.clear();
                if c == '}' {
                    blocks.pop();
                }
            }
            _ => buf.push(c),
        }
    }
}

fn set(style: &mut Style, decl: &str) {
    let Some((name, value)) = decl.split_once(':') else { return };
    let (name, value) = (name.trim(), value.trim());
    let p = &mut style.p;
    let slot = match name {
        "--radius" => {
            match value.trim_end_matches("px").trim().parse() {
                Ok(r) => style.radius = r,
                Err(_) => eprintln!("theme: --radius: {value} is not a number of pixels from 0 to 255"),
            }
            return;
        }
        "--font" => {
            style.font = value.to_string();
            return;
        }
        "--pane" => {
            match Color32::from_hex(value) {
                Ok(color) => style.pane = Some(color),
                Err(_) => eprintln!("theme: --pane: {value} is not a #hex color"),
            }
            return;
        }
        "--bg" => &mut p.bg,
        "--panel" => &mut p.panel,
        "--card" => &mut p.card,
        "--deep" => &mut p.deep,
        "--button" => &mut p.button,
        "--border" => &mut p.border,
        "--text" => &mut p.text,
        "--strong" => &mut p.strong,
        "--on-accent" => &mut p.on_accent,
        "--accent" => &mut p.accent,
        "--accent-2" => &mut p.accent2,
        "--warn" => &mut p.warn,
        "--muted" => &mut p.muted,
        "--error" => &mut p.error,
        "--ok" => &mut p.ok,
        "--off" => &mut p.off,
        "--hover" => &mut p.hover,
        "color-scheme" => return, // read by forced_scheme
        _ => {
            eprintln!("theme: unknown property {name}");
            return;
        }
    };
    match Color32::from_hex(value) {
        Ok(color) => *slot = color,
        Err(_) => eprintln!("theme: {name}: {value} is not a #hex color"),
    }
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("*/") else { return out };
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = "/* mine */ :root { --accent: #112233; --bogus: #000; --radius: 6px; --font: \"Noto Sans\"; }
        @media (prefers-color-scheme: light) { :root { --accent: #445566; --bg: #fff; } }
        @media (prefers-color-scheme: dark) { :root { --ok: #010203; } }";

    fn parsed(css: &str, light: bool) -> Style {
        let mut style = Style { p: if light { FALLBACK_LIGHT } else { FALLBACK_DARK }, radius: 0, font: "monospace".into(), pane: None };
        parse(css, light, &mut style);
        style
    }

    #[test]
    fn dark_skips_light_block() {
        let s = parsed(CSS, false);
        assert_eq!(s.p.accent, Color32::from_rgb(0x11, 0x22, 0x33));
        assert_eq!(s.p.ok, Color32::from_rgb(1, 2, 3));
        assert_eq!(s.p.bg, FALLBACK_DARK.bg);
        assert_eq!(s.radius, 6);
        assert_eq!(s.font, "\"Noto Sans\"");
    }

    #[test]
    fn light_block_wins_over_root() {
        let s = parsed(CSS, true);
        assert_eq!(s.p.accent, Color32::from_rgb(0x44, 0x55, 0x66));
        assert_eq!(s.p.bg, Color32::WHITE);
        assert_eq!(s.p.ok, FALLBACK_LIGHT.ok);
    }

    #[test]
    fn color_scheme_forces_mode() {
        assert_eq!(forced_scheme(CSS), None);
        assert_eq!(forced_scheme(":root { color-scheme: light; --bg: #fff; }"), Some(true));
        assert_eq!(forced_scheme("/* color-scheme: light; */ :root { color-scheme: dark }"), Some(false));
    }

    #[test]
    fn opensynapse_css_matches_fallback() {
        let css = BUILTIN[0].1;
        assert_eq!(parsed(css, false).p, FALLBACK_DARK);
        assert_eq!(parsed(css, true).p, FALLBACK_LIGHT);
    }

    #[test]
    fn builtin_themes_set_every_color() {
        // parsed onto a palette of a color no theme uses, any color a theme forgets keeps it
        let unset = Color32::from_rgb(0x12, 0x34, 0x56);
        let blank = Palette {
            bg: unset,
            panel: unset,
            card: unset,
            // pane is optional in theme files, apply works it out when it's missing
            pane: Color32::WHITE,
            deep: unset,
            button: unset,
            border: unset,
            text: unset,
            strong: unset,
            on_accent: unset,
            accent: unset,
            accent2: unset,
            warn: unset,
            muted: unset,
            error: unset,
            ok: unset,
            off: unset,
            hover: unset,
        };
        for (id, css) in BUILTIN {
            for light in [false, true] {
                let mut style = Style { p: blank, radius: 0, font: String::new(), pane: None };
                parse(css, light, &mut style);
                let p = style.p;
                let colors = [
                    p.bg, p.panel, p.card, p.deep, p.button, p.border, p.text, p.strong, p.on_accent, p.accent, p.accent2, p.warn,
                    p.muted, p.error, p.ok, p.off, p.hover,
                ];
                let missing = colors.iter().filter(|c| **c == unset).count();
                assert_eq!(missing, 0, "{id} (light: {light}) leaves {missing} colors unset");
                assert!(!style.font.is_empty(), "{id} has no --font");
            }
        }
    }
}
