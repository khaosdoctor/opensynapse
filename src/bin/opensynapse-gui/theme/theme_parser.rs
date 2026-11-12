use super::palette::Palette;
use eframe::egui::Color32;

pub(super) struct Style {
    pub(super) p: Palette,
    pub(super) radius: u8,
    pub(super) font: String,
    /// None until a theme sets --pane, then apply works it out from bg and panel
    pub(super) pane: Option<Color32>,
    /// None until a theme sets --hypershift, then apply uses --warn
    pub(super) hypershift: Option<Color32>,
}

/// `color-scheme: light` or `dark` in theme.css, for desktops that don't
/// report a preference. Some(true) is light.
pub(super) fn forced_scheme(css: &str) -> Option<bool> {
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

pub(super) fn parse(css: &str, light: bool, style: &mut Style) {
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
    let color = || Color32::from_hex(value).map_err(|_| eprintln!("theme: {name}: {value} is not a #hex color")).ok();
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
            style.pane = color().or(style.pane);
            return;
        }
        "--hypershift" => {
            style.hypershift = color().or(style.hypershift);
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
    if let Some(c) = color() {
        *slot = c;
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
    use super::super::palette::{BUILTIN, base, uniform};
    use super::*;

    const CSS: &str = "/* mine */ :root { --accent: #112233; --bogus: #000; --radius: 6px; --font: \"Noto Sans\"; }
        @media (prefers-color-scheme: light) { :root { --accent: #445566; --bg: #fff; } }
        @media (prefers-color-scheme: dark) { :root { --ok: #010203; } }";

    fn parsed(css: &str, light: bool) -> Style {
        let mut style = base(light);
        parse(css, light, &mut style);
        style
    }

    #[test]
    fn dark_skips_light_block() {
        let s = parsed(CSS, false);
        assert_eq!(s.p.accent, Color32::from_rgb(0x11, 0x22, 0x33));
        assert_eq!(s.p.ok, Color32::from_rgb(1, 2, 3));
        assert_eq!(s.p.bg, Color32::from_rgb(0x07, 0x07, 0x0d));
        assert_eq!(s.radius, 6);
        assert_eq!(s.font, "\"Noto Sans\"");
    }

    #[test]
    fn light_block_wins_over_root() {
        let s = parsed(CSS, true);
        assert_eq!(s.p.accent, Color32::from_rgb(0x44, 0x55, 0x66));
        assert_eq!(s.p.bg, Color32::WHITE);
        assert_eq!(s.p.ok, Color32::from_rgb(0x1a, 0x8a, 0x00));
    }

    #[test]
    fn color_scheme_forces_mode() {
        assert_eq!(forced_scheme(CSS), None);
        assert_eq!(forced_scheme(":root { color-scheme: light; --bg: #fff; }"), Some(true));
        assert_eq!(forced_scheme("/* color-scheme: light; */ :root { color-scheme: dark }"), Some(false));
    }

    #[test]
    fn builtin_themes_set_every_color() {
        // parsed onto a palette of a color no theme uses, any color a theme forgets keeps it
        let unset = Color32::from_rgb(0x12, 0x34, 0x56);
        for (id, css) in BUILTIN {
            for light in [false, true] {
                let mut style = Style { p: uniform(unset), radius: 0, font: String::new(), pane: None, hypershift: None };
                parse(css, light, &mut style);
                let p = style.p;
                let colors = [
                    p.bg, p.panel, p.card, p.deep, p.button, p.border, p.text, p.strong, p.on_accent, p.accent, p.accent2, p.warn,
                    p.muted, p.error, p.ok, p.off, p.hover,
                ];
                let missing = colors.iter().filter(|c| **c == unset).count();
                assert_eq!(missing, 0, "{id} (light: {light}) leaves {missing} colors unset");
                assert!(style.hypershift.is_some_and(|h| h != p.accent && h != p.accent2), "{id} (light: {light}) has no distinct --hypershift");
                assert!(!style.font.is_empty(), "{id} has no --font");
            }
        }
    }

    /// A user's theme file can be anything, a broken one keeps the defaults instead of panicking.
    #[test]
    fn malformed_css_keeps_the_defaults() {
        let dark = base(false);
        let broken = [
            "",
            "{",
            "}}}",
            ":root { --accent: }",
            ":root { --accent: red; --radius: 300px; --radius: wide; --pane: nope }",
            ":root { --accent #fff }",
            ":root { : ; ;; --: #fff }",
            ":root { --accent: #010203",
            "/* never closed :root { --accent: #010203; }",
            ":root { --ACCENT: #010203; }",
        ];
        for css in broken {
            let s = parsed(css, false);
            assert!(s.p == dark.p && s.radius == dark.radius && s.pane == dark.pane && s.hypershift == dark.hypershift, "{css}");
        }
        assert_eq!(forced_scheme(":root { color-scheme: Light }"), None);
        assert_eq!(forced_scheme("color-scheme"), None);
    }

    #[test]
    fn builtin_theme_ids_are_unique_file_names() {
        for (i, (id, _)) in BUILTIN.iter().enumerate() {
            // user files are matched by lowercased file name, so a built-in id has to be one
            assert_eq!(opensynapse::device::check_name(id).as_deref(), Ok(*id));
            for (other, _) in &BUILTIN[..i] {
                assert_ne!(other, id, "{id} is listed twice");
            }
        }
    }
}
