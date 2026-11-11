//! Colors for the GUI: a dark and a light palette that follow the desktop's
//! color scheme (xdg-desktop-portal), overridable from
//! ~/.config/opensynapse/theme.css with CSS custom properties:
//!
//! ```css
//! :root { --accent: #00f0ff; }
//! @media (prefers-color-scheme: light) { :root { --accent: #007a8a; } }
//! ```
//!
//! Only `--name: #hex;` declarations are read. Selectors are ignored, so
//! anything outside a `prefers-color-scheme` block applies to both schemes.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, Theme};
use opensynapse::device;
use std::sync::RwLock;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    /// text fields and other sunken areas
    pub deep: Color32,
    pub text: Color32,
    /// text that has to stand out from `text`
    pub strong: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub warn: Color32,
    pub muted: Color32,
    pub error: Color32,
    pub ok: Color32,
    /// edge of a button whose action is OFF
    pub off: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x07, 0x07, 0x0d),
    panel: Color32::from_rgb(0x0e, 0x0e, 0x1a),
    card: Color32::from_rgb(0x14, 0x14, 0x24),
    deep: Color32::from_rgb(0x03, 0x03, 0x08),
    text: Color32::from_rgb(0xd0, 0xd0, 0xe8),
    strong: Color32::WHITE,
    accent: Color32::from_rgb(0x00, 0xf0, 0xff),
    accent2: Color32::from_rgb(0xff, 0x2a, 0x6d),
    warn: Color32::from_rgb(0xfc, 0xee, 0x0a),
    muted: Color32::from_rgb(0x5a, 0x5a, 0x7a),
    error: Color32::from_rgb(0xff, 0x30, 0x30),
    ok: Color32::from_rgb(0x39, 0xff, 0x14),
    off: Color32::from_rgb(0x2a, 0x2a, 0x3a),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf4, 0xf4, 0xfa),
    panel: Color32::from_rgb(0xe8, 0xe8, 0xf2),
    card: Color32::from_rgb(0xff, 0xff, 0xff),
    deep: Color32::from_rgb(0xdc, 0xdc, 0xe8),
    text: Color32::from_rgb(0x20, 0x20, 0x2e),
    strong: Color32::from_rgb(0x0a, 0x0a, 0x14),
    accent: Color32::from_rgb(0x00, 0x7a, 0x8a),
    accent2: Color32::from_rgb(0xd4, 0x00, 0x4f),
    warn: Color32::from_rgb(0x8a, 0x6a, 0x00),
    muted: Color32::from_rgb(0x7a, 0x7a, 0x96),
    error: Color32::from_rgb(0xc8, 0x1e, 0x1e),
    ok: Color32::from_rgb(0x1a, 0x8a, 0x00),
    off: Color32::from_rgb(0xc8, 0xc8, 0xd8),
};

static CURRENT: RwLock<Palette> = RwLock::new(DARK);

/// The palette in use right now.
pub fn t() -> Palette {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";

/// The portal answers 1 for dark, 2 for light and 0 for no preference.
/// No portal or no preference means dark, the app's own look.
fn prefers_light() -> bool {
    let Ok(conn) = Connection::session() else { return false };
    let Ok(reply) = conn.call_method(Some(PORTAL), PORTAL_PATH, Some(SETTINGS), "ReadOne", &(APPEARANCE, "color-scheme")) else {
        return false;
    };
    let Ok(value) = reply.body().deserialize::<OwnedValue>() else { return false };
    u32::try_from(value) == Ok(2)
}

/// Reads the color scheme and theme.css, then restyles the whole UI.
pub fn apply(ctx: &egui::Context) {
    let light = prefers_light();
    let base = if light { LIGHT } else { DARK };
    let css = device::config_dir().and_then(|c| std::fs::read_to_string(c.join("opensynapse/theme.css")).ok());
    let p = css.map_or(base, |css| parse(&css, light, base));
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = p;

    let mut v = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.deep;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.accent2;
    v.selection.stroke = Stroke::new(1.0, p.bg);
    for (w, stroke) in [
        (&mut v.widgets.noninteractive, p.muted),
        (&mut v.widgets.inactive, p.muted),
        (&mut v.widgets.hovered, p.accent),
        (&mut v.widgets.active, p.accent2),
        (&mut v.widgets.open, p.accent),
    ] {
        w.corner_radius = CornerRadius::ZERO;
        w.bg_stroke = Stroke::new(1.0, stroke);
        w.weak_bg_fill = p.card;
        w.bg_fill = p.card;
    }
    // same visuals for both, so egui's own system theme guess can't swap them out
    ctx.set_visuals_of(Theme::Dark, v.clone());
    ctx.set_visuals_of(Theme::Light, v);
    ctx.all_styles_mut(|s| {
        for font in s.text_styles.values_mut() {
            *font = FontId::new(font.size, FontFamily::Monospace);
        }
    });
    ctx.request_repaint();
}

/// Restyles the UI whenever the desktop switches between light and dark.
pub fn follow_desktop(ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(conn) = Connection::session() else { return };
        let Ok(proxy) = Proxy::new(&conn, PORTAL, PORTAL_PATH, SETTINGS) else { return };
        let Ok(changes) = proxy.receive_signal("SettingChanged") else { return };
        for msg in changes {
            let Ok((ns, key, _)) = msg.body().deserialize::<(String, String, OwnedValue)>() else { continue };
            if ns == APPEARANCE && key == "color-scheme" {
                apply(&ctx);
            }
        }
    });
}

/// Applies the `--name: #hex;` declarations in `css` that hold for this scheme on top of `base`.
fn parse(css: &str, light: bool, base: Palette) -> Palette {
    let mut p = base;
    // one entry per open block: Some(light?) for a prefers-color-scheme block, None for anything else
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
                    set(&mut p, buf.trim());
                }
                buf.clear();
                if c == '}' {
                    blocks.pop();
                }
            }
            _ => buf.push(c),
        }
    }
    p
}

fn set(p: &mut Palette, decl: &str) {
    let Some((name, value)) = decl.split_once(':') else { return };
    let (name, value) = (name.trim(), value.trim());
    let slot = match name {
        "--bg" => &mut p.bg,
        "--panel" => &mut p.panel,
        "--card" => &mut p.card,
        "--deep" => &mut p.deep,
        "--text" => &mut p.text,
        "--strong" => &mut p.strong,
        "--accent" => &mut p.accent,
        "--accent-2" => &mut p.accent2,
        "--warn" => &mut p.warn,
        "--muted" => &mut p.muted,
        "--error" => &mut p.error,
        "--ok" => &mut p.ok,
        "--off" => &mut p.off,
        _ => {
            eprintln!("theme.css: unknown property {name}");
            return;
        }
    };
    match Color32::from_hex(value) {
        Ok(color) => *slot = color,
        Err(_) => eprintln!("theme.css: {name}: {value} is not a #hex color"),
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

    const CSS: &str = "/* mine */ :root { --accent: #112233; --bogus: #000; }
        @media (prefers-color-scheme: light) { :root { --accent: #445566; --bg: #fff; } }
        @media (prefers-color-scheme: dark) { :root { --ok: #010203; } }";

    #[test]
    fn dark_skips_light_block() {
        let p = parse(CSS, false, DARK);
        assert_eq!(p.accent, Color32::from_rgb(0x11, 0x22, 0x33));
        assert_eq!(p.ok, Color32::from_rgb(1, 2, 3));
        assert_eq!(p.bg, DARK.bg);
    }

    #[test]
    fn light_block_wins_over_root() {
        let p = parse(CSS, true, LIGHT);
        assert_eq!(p.accent, Color32::from_rgb(0x44, 0x55, 0x66));
        assert_eq!(p.bg, Color32::WHITE);
        assert_eq!(p.ok, LIGHT.ok);
    }
}
