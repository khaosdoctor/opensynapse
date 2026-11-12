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

mod palette;
mod settings_parser;
mod theme_detector;
mod theme_parser;

pub use palette::Palette;
pub use settings_parser::settings_ui;

use eframe::egui::{self, Color32, CornerRadius, FontData, FontFamily, FontId, Stroke, Theme};
use palette::{base, uniform};
use settings_parser::{Mode, SETTINGS, THEMES, config, scan, theme_css};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use theme_detector::{follow_desktop, prefers_light};
use theme_parser::{forced_scheme, parse};

// `start` fills it in before the first frame
static CURRENT: RwLock<Palette> = RwLock::new(uniform(Color32::BLACK));
static RADIUS: AtomicU8 = AtomicU8::new(0);

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

/// A poisoned lock still holds usable data, a panic elsewhere shouldn't take the theme down.
fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| e.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| e.into_inner())
}

pub fn t() -> Palette {
    *read(&CURRENT)
}

/// Runs `f` with both accents set to --hypershift, so the mouse and the editor tell
/// the Hypershift layer apart from the normal one at a glance.
pub fn tinted<R>(on: bool, f: impl FnOnce() -> R) -> R {
    if !on {
        return f();
    }
    let saved = t();
    {
        let mut p = write(&CURRENT);
        (p.accent, p.accent2) = (p.hypershift, p.hypershift);
    }
    let r = f();
    *write(&CURRENT) = saved;
    r
}

/// Phosphor icons, look the codepoints up on https://phosphoricons.com
pub fn icon(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}

pub fn radius() -> CornerRadius {
    CornerRadius::same(RADIUS.load(Ordering::Relaxed))
}

pub fn start(ctx: &egui::Context) {
    if let Some(s) = config("settings.json").and_then(|f| opensynapse::device::load_json(&f).ok()) {
        *write(&SETTINGS) = s;
    }
    *write(&THEMES) = scan();
    apply(ctx);
    follow_desktop(ctx.clone());
}

pub fn apply(ctx: &egui::Context) {
    let s = read(&SETTINGS).clone();
    let user_css = config("theme.css").and_then(|f| fs::read_to_string(f).ok());
    // the settings page wins over theme.css, which wins over the desktop
    let light = match s.mode {
        Mode::Light => true,
        Mode::Dark => false,
        Mode::System => user_css.as_deref().and_then(forced_scheme).unwrap_or_else(prefers_light),
    };
    let mut style = base(light);
    parse(&theme_css(&s.theme).1, light, &mut style);
    if let Some(css) = &user_css {
        parse(css, light, &mut style);
    }
    let mut p = style.p;
    p.pane = style.pane.unwrap_or(p.bg.lerp_to_gamma(p.panel, 0.5));
    p.hypershift = style.hypershift.unwrap_or(p.warn);
    *write(&CURRENT) = p;
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
    defs.font_data.insert("icons".into(), Arc::new(FontData::from_static(include_bytes!("../../../../assets/phosphor.ttf"))));
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
    *write(&BASE_FONTS) = Some(defs.clone());
    // keep whatever previews are loaded, the font list may be open while the theme changes
    let previews = read(&PREVIEWS).1.clone();
    set_fonts_with_previews(ctx, &previews);
    ctx.all_styles_mut(|st| {
        for f in st.text_styles.values_mut() {
            *f = FontId::new(f.size, family.clone());
        }
    });
    ctx.request_repaint();
}

/// Loads the fonts for the block of rows in view plus one block before and
/// after, so scrolling shows them right away, and drops the ones outside.
fn load_previews(ctx: &egui::Context, fonts: &[(String, PathBuf)], visible: std::ops::Range<usize>) {
    let start = (visible.start / PREVIEW_BLOCK).saturating_sub(1) * PREVIEW_BLOCK;
    let end = ((visible.end / PREVIEW_BLOCK + 2) * PREVIEW_BLOCK).min(fonts.len());
    let mut previews = write(&PREVIEWS);
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
    let mut previews = write(&PREVIEWS);
    if previews.1.is_empty() {
        return;
    }
    *previews = (0..0, Vec::new());
    set_fonts_with_previews(ctx, &[]);
}

fn set_fonts_with_previews(ctx: &egui::Context, previews: &[Preview]) {
    let mut defs = read(&BASE_FONTS).clone().unwrap_or_default();
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
    let Some(out) = fontconfig("fc-list", &["--format=%{family[0]}\t%{style[0]}\t%{file}\n", ":lang=en"]) else {
        return Vec::new();
    };
    // family -> (how far from regular the style is, file)
    let mut best: BTreeMap<String, (u8, PathBuf)> = BTreeMap::new();
    for line in out.lines() {
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
    fs::read(fontconfig("fc-match", &["--format=%{file}", &pattern])?.trim()).ok()
}

fn fontconfig(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}
