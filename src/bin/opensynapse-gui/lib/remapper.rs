//! The REMAP editor under the mouse drawing, which turns tabs and chips into an action for the selected button.

use crate::shared::widgets::{chip, note, pick, tab_row};
use crate::state::App;
use crate::theme::t;
use eframe::egui::{self, DragValue, RichText, TextEdit};
use opensynapse::action::{Action, key_name};

/// (title, grid columns, keys), each group gets its own column.
const SPECIAL_KEYS: &[(&str, usize, &[&str])] = &[
    (
        "FUNCTION",
        6,
        &[
            "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15", "f16", "f17",
            "f18", "f19", "f20", "f21", "f22", "f23", "f24",
        ],
    ),
    ("EDIT", 3, &["esc", "tab", "enter", "backspace", "space", "capslock", "insert", "delete", "menu"]),
    ("NAV", 2, &["home", "end", "pageup", "pagedown", "up", "down", "left", "right"]),
    ("SYSTEM", 1, &["printscreen", "scrolllock", "pause"]),
    (
        "NUMPAD",
        4,
        &[
            "numlock", "numslash", "numasterisk", "numminus", "num7", "num8", "num9", "numplus", "num4", "num5", "num6",
            "numenter", "num1", "num2", "num3", "numdot", "num0",
        ],
    ),
    // a modifier on its own, like holding Shift (untested on hardware)
    (
        "MODIFIER",
        2,
        &["leftctrl", "rightctrl", "leftshift", "rightshift", "leftalt", "rightalt", "leftmeta", "rightmeta"],
    ),
];

/// DPI tab. Stage jumps come from razerqdhid and are untested on the Naga.
const DPI_ACTIONS: &[(&str, Action)] = &[
    ("DPI UP", Action::Dpi(1)),
    ("DPI DOWN", Action::Dpi(2)),
    ("UP (WRAP)", Action::Dpi(6)),
    ("DOWN (WRAP)", Action::Dpi(7)),
    ("STAGE 1", Action::DpiStage(1)),
    ("STAGE 2", Action::DpiStage(2)),
    ("STAGE 3", Action::DpiStage(3)),
    ("STAGE 4", Action::DpiStage(4)),
    ("STAGE 5", Action::DpiStage(5)),
];

/// Profile tab. Jumping to a profile comes from razerqdhid and is untested on the Naga.
const PROFILE_ACTIONS: &[(&str, Action)] = &[
    ("NEXT", Action::Profile(1)),
    ("PREVIOUS", Action::Profile(2)),
    ("NEXT (WRAP)", Action::Profile(4)),
    ("PREVIOUS (WRAP)", Action::Profile(5)),
    ("P1", Action::ProfileSet(1)),
    ("P2", Action::ProfileSet(2)),
    ("P3", Action::ProfileSet(3)),
    ("P4", Action::ProfileSet(4)),
    ("P5", Action::ProfileSet(5)),
];

/// Media tab. Mute and volume are keyboard keys (verified on the Naga), the
/// rest are HID consumer usages.
const MEDIA_KEYS: &[(&str, Action)] = &[
    ("PLAY/PAUSE", Action::Media(0xcd)),
    ("PLAY", Action::Media(0xb0)),
    ("PAUSE", Action::Media(0xb1)),
    ("STOP", Action::Media(0xb7)),
    ("PREVIOUS", Action::Media(0xb6)),
    ("NEXT", Action::Media(0xb5)),
    ("MUTE", Action::Key { mods: 0, usage: 0x7f }),
    ("VOLUME -", Action::Key { mods: 0, usage: 0x81 }),
    ("VOLUME +", Action::Key { mods: 0, usage: 0x80 }),
];

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Key,
    Mouse,
    Media,
    Dpi,
    Profile,
    Hypershift,
    Disabled,
    Raw,
}

const KINDS: &[(&str, Kind)] = &[
    ("KEY", Kind::Key),
    ("MOUSE", Kind::Mouse),
    ("MEDIA", Kind::Media),
    ("DPI", Kind::Dpi),
    ("PROFILE", Kind::Profile),
    ("HYPERSHIFT", Kind::Hypershift),
    ("RAW", Kind::Raw),
    ("OFF", Kind::Disabled),
];

#[derive(Clone, PartialEq)]
pub(crate) struct Editor {
    kind: Kind,
    mods: u8,
    key: String,
    mouse: u8,
    /// repeat the key or mouse button while held
    turbo: bool,
    rate: u16,
    media: Action,
    dpi: Action,
    profile: Action,
    raw: String,
}

impl Editor {
    pub(crate) fn load(a: &Action) -> Self {
        let mut e = Editor {
            kind: Kind::Raw,
            mods: 0,
            key: String::new(),
            mouse: 1,
            turbo: false,
            rate: 20,
            media: MEDIA_KEYS[0].1.clone(),
            dpi: DPI_ACTIONS[0].1.clone(),
            profile: PROFILE_ACTIONS[0].1.clone(),
            raw: String::new(),
        };
        if MEDIA_KEYS.iter().any(|(_, m)| m == a) {
            (e.kind, e.media) = (Kind::Media, a.clone());
            return e;
        }
        match a {
            Action::Key { mods, usage } | Action::KeyTurbo { mods, usage, .. } => {
                e.kind = Kind::Key;
                e.mods = *mods;
                e.key = key_name(*usage).unwrap_or(format!("0x{usage:02x}"));
            }
            Action::Mouse(b) | Action::MouseTurbo { button: b, .. } => (e.kind, e.mouse) = (Kind::Mouse, *b),
            Action::Dpi(_) | Action::DpiStage(_) => (e.kind, e.dpi) = (Kind::Dpi, a.clone()),
            Action::Profile(_) | Action::ProfileSet(_) => (e.kind, e.profile) = (Kind::Profile, a.clone()),
            Action::Hypershift => e.kind = Kind::Hypershift,
            Action::Disabled => e.kind = Kind::Disabled,
            _ => e.raw = a.encode().iter().map(|b| format!("{b:02x}")).collect(),
        }
        if let Action::KeyTurbo { interval, .. } | Action::MouseTurbo { interval, .. } = a {
            (e.turbo, e.rate) = (true, *interval);
        }
        e
    }

    pub(crate) fn build(&self) -> Result<Action, String> {
        Ok(match self.kind {
            Kind::Key => {
                // the box also takes `ctrl+shift+z`, on top of the modifier chips
                let text = self.key.trim();
                let Ok(Action::Key { mods, usage }) = format!("key:{text}").parse() else {
                    return Err(format!("unknown key '{text}'"));
                };
                let mods = mods | self.mods;
                if self.turbo {
                    return Ok(Action::KeyTurbo { mods, usage, interval: self.rate });
                }
                Action::Key { mods, usage }
            }
            Kind::Mouse if self.turbo => Action::MouseTurbo { button: self.mouse, interval: self.rate },
            Kind::Mouse => Action::Mouse(self.mouse),
            Kind::Media => self.media.clone(),
            Kind::Dpi => self.dpi.clone(),
            Kind::Profile => self.profile.clone(),
            Kind::Hypershift => Action::Hypershift,
            Kind::Disabled => Action::Disabled,
            Kind::Raw => format!("raw:{}", self.raw.trim()).parse()?,
        })
    }
}

impl App {
    pub(crate) fn editor_ui(&mut self, ui: &mut egui::Ui, name: &str) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("> REMAP:").color(t().accent2).strong());
            ui.label(RichText::new(name.to_uppercase()).color(t().accent).strong().underline());
            if matches!(self.editor.kind, Kind::Key | Kind::Mouse) {
                ui.add_space(32.0);
                turbo_toggle(ui, &mut self.editor);
            }
        });
        egui::ScrollArea::vertical().show(ui, |ui| self.editor_fields(ui));
        self.queue_edits();
    }

    fn editor_fields(&mut self, ui: &mut egui::Ui) {
        // a tab is only a view, the button changes type when you pick or type something in it.
        // Otherwise picking another button while on MOUSE would queue it as a left click
        let shown = self.tab.get_or_insert(self.editor.kind);
        tab_row(ui, KINDS, shown);
        let shown = *shown;
        ui.separator();
        let e = &mut self.editor;
        let before = e.clone();
        // nothing highlights in a tab the button isn't using, its fields hold defaults
        let on = e.kind == shown;
        // re-picking a value that equals the default doesn't change any field, so clicks count too
        let mut picked = false;
        ui.horizontal_wrapped(|ui| match shown {
            Kind::Key => {} // laid out as columns below
            Kind::Mouse => {
                for &(n, v) in opensynapse::action::MOUSE {
                    // wheelleft/right is what the wheel tilt sends (horizontal scroll)
                    let label = match n {
                        "wheelleft" => "TILT LEFT".to_string(),
                        "wheelright" => "TILT RIGHT".to_string(),
                        _ => n.to_uppercase(),
                    };
                    if chip(ui, &label, on && e.mouse == v).clicked() {
                        (e.mouse, picked) = (v, true);
                    }
                }
            }
            Kind::Media => picked = pick(ui, MEDIA_KEYS, &mut e.media, on, chip),
            Kind::Hypershift => {
                picked = chip(ui, "USE AS HYPERSHIFT", on).clicked();
                note(
                    ui,
                    "Hold this button and the other buttons do what their HYPERSHIFT layer says \
                     (switch LAYER at the top to edit it). Let go and they go back to normal.",
                );
            }
            Kind::Dpi => picked = pick(ui, DPI_ACTIONS, &mut e.dpi, on, chip),
            Kind::Profile => picked = pick(ui, PROFILE_ACTIONS, &mut e.profile, on, chip),
            Kind::Disabled => picked = chip(ui, "TURN OFF", on).clicked(),
            Kind::Raw => {
                ui.add(TextEdit::singleline(&mut e.raw).hint_text("hex bytes").desired_width(200.0));
            }
        });
        if shown == Kind::Key {
            key_fields(ui, e, on);
        }
        if picked || *e != before {
            e.kind = shown;
        }
    }
}

/// KEY tab: modifiers, the key box and the special key groups.
fn key_fields(ui: &mut egui::Ui, e: &mut Editor, on: bool) {
    // one column per group, a group that doesn't fit goes to the next row
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 28.0;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            note(ui, "COMMON KEYS");
            egui::Grid::new("mods").spacing([6.0, 6.0]).show(ui, |ui| {
                for (i, (label, bit)) in opensynapse::action::MODS[..4].iter().enumerate() {
                    if chip(ui, &label.to_uppercase(), on && e.mods & bit != 0).clicked() {
                        e.mods ^= bit;
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            // an empty box just means no key picked yet
            // checked as a key even while the button is still another type
            let as_key = Editor { kind: Kind::Key, ..e.clone() };
            let err = as_key.build().err().filter(|_| !e.key.trim().is_empty());
            let color = match err {
                Some(_) => t().accent2,
                None => t().warn,
            };
            let edit = TextEdit::singleline(&mut e.key).hint_text("a, 1, ctrl+z, f13").desired_width(140.0);
            ui.add(edit.text_color(color));
            if let Some(err) = err {
                ui.label(RichText::new(err).color(t().accent2).small());
            }
        });
        for (group, cols, keys) in SPECIAL_KEYS {
            // egui only wraps single widgets, so we measure the group. A chip is its text
            // plus padding and the widest key sets the column width
            let widest = keys.iter().map(|k| k.len()).max().unwrap_or(0) as f32;
            let font = egui::TextStyle::Button.resolve(ui.style());
            let char_w = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
            let spacing = ui.spacing().item_spacing.x;
            let width = *cols as f32 * (widest * char_w + 32.0) + spacing;
            // available_width() in a wrapping layout is the whole row, so measure from the cursor
            if ui.max_rect().right() - ui.cursor().min.x < width {
                ui.end_row();
            }
            ui.vertical(|ui| {
                note(ui, *group);
                egui::Grid::new(group).spacing([6.0, 6.0]).show(ui, |ui| {
                    for (i, k) in keys.iter().enumerate() {
                        if chip(ui, &k.to_uppercase(), on && e.key == *k).clicked() {
                            e.key = k.to_string();
                        }
                        if (i + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
        }
    });
}

fn turbo_toggle(ui: &mut egui::Ui, e: &mut Editor) {
    const TIP: &str = "While you hold the button, TURBO repeats the key or click every this many milliseconds, \
                       instead of sending it once.";
    ui.checkbox(&mut e.turbo, RichText::new("TURBO").color(t().warn)).on_hover_text(TIP);
    if e.turbo {
        ui.label(RichText::new("EVERY").color(t().muted)).on_hover_text(TIP);
        // no vertical padding, so the box is as tall as the text and the row doesn't jump
        ui.spacing_mut().button_padding.y = 0.0;
        ui.add(DragValue::new(&mut e.rate).range(1..=2000).suffix(" ms")).on_hover_text(TIP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Selecting a button loads it into the editor, which queues whatever build
    /// returns if it differs, so a lossy round trip queues a write nobody asked for.
    #[test]
    fn load_then_build_gives_the_action_back() {
        let actions = [
            Action::Key { mods: 0, usage: 0x04 },
            Action::Key { mods: 0x03, usage: 0x1e },
            Action::Key { mods: 0x01, usage: 0x00 },
            Action::Key { mods: 0, usage: 0x64 },
            Action::Key { mods: 0, usage: 0x62 },
            Action::KeyTurbo { mods: 0x04, usage: 0x68, interval: 30 },
            Action::Mouse(0x01),
            Action::Mouse(0x69),
            Action::MouseTurbo { button: 0x01, interval: 50 },
            Action::Media(0xcd),
            Action::Key { mods: 0, usage: 0x7f },
            Action::Dpi(6),
            Action::DpiStage(5),
            Action::Profile(5),
            Action::ProfileSet(1),
            Action::Hypershift,
            Action::Disabled,
            Action::Raw(vec![0x0b, 0x0c]),
        ];
        for a in actions {
            assert_eq!(Editor::load(&a).build(), Ok(a.clone()), "{a}");
        }
    }

    #[test]
    fn media_outside_the_tab_round_trips() {
        // media:mute from the CLI, a different usage from the tab's MUTE key. It shows up on
        // the RAW tab, and queue_edits compares bytes, so selecting it queues nothing
        let a = Action::Media(0xe2);
        assert_eq!(Editor::load(&a).build().map(|b| b.encode()), Ok(a.encode()));
    }

    #[test]
    fn build_refuses_what_it_cant_encode() {
        let typed = |key: &str| Editor { key: key.into(), ..Editor::load(&Action::Key { mods: 0, usage: 0x04 }) }.build();
        assert_eq!(typed(" ctrl+z "), Ok(Action::Key { mods: 0x01, usage: 0x1d }));
        assert_eq!(typed("Z"), Ok(Action::Key { mods: 0, usage: 0x1d }));
        assert_eq!(typed(""), Err("unknown key ''".into()));
        assert_eq!(typed("nosuchkey"), Err("unknown key 'nosuchkey'".into()));
        // the box only takes a plain key, turbo comes from the toggle
        assert_eq!(typed("a:50"), Err("unknown key 'a:50'".into()));
        let raw = |hex: &str| Editor { raw: hex.into(), ..Editor::load(&Action::Raw(vec![0x0b])) }.build();
        assert_eq!(raw(" 0b0c "), Ok(Action::Raw(vec![0x0b, 0x0c])));
        for bad in ["", "0", "zz", "0b 0c"] {
            assert!(raw(bad).is_err(), "{bad}");
        }
    }
}
