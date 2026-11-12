//! Turns a traced command and its reply into words, using the layouts in
//! PROTOCOL.md and the button names from the device file. Commands nobody has
//! decoded yet get None and stay raw bytes.

use crate::action::Action;
use crate::device::{CLASS_DPI, CLASS_INFO, CLASS_KEYMAP, CLASS_LIGHT, CLASS_PROFILE, DeviceDef};
use crate::hid::Exchange;

pub fn describe(def: &DeviceDef, e: &Exchange) -> Option<String> {
    let at = |b: &[u8], i: usize| b.get(i).copied().unwrap_or(0);
    let arg = |i: usize| at(e.args, i);
    let be = |b: &[u8], i: usize| u16::from_be_bytes([at(b, i), at(b, i + 1)]);
    let button = |id: u8| def.buttons.iter().find(|b| b.id == id).map_or(format!("button {id:02x}"), |b| b.name.clone());
    let layer = |l: u8| def.layers.get(l as usize).cloned().unwrap_or(format!("layer {l}"));
    let target = || format!("P{} {} {}", arg(0), button(arg(1)), layer(arg(2)));
    let stages = |b: &[u8], count: usize, from: usize| {
        (0..count.min(5)).map(|i| be(b, from + i * 7).to_string()).collect::<Vec<_>>().join("/")
    };

    let request = match (e.class, e.id) {
        (CLASS_INFO, 0x81) => "get firmware".to_string(),
        (CLASS_INFO, 0x82) => "get serial".to_string(),
        (CLASS_KEYMAP, 0x8c) => format!("get button {}", target()),
        (CLASS_KEYMAP, 0x0c) => format!("set button {} > {}", target(), Action::decode(&e.args[3.min(e.args.len())..])),
        (CLASS_DPI, 0x85) => "get DPI".to_string(),
        (CLASS_DPI, 0x05) => format!("set DPI {} x {}", be(e.args, 1), be(e.args, 3)),
        (CLASS_DPI, 0x86) => "get DPI stages".to_string(),
        (CLASS_DPI, 0x06) => format!("set DPI stages {} active {}", stages(e.args, arg(2) as usize, 4), arg(1)),
        (CLASS_PROFILE, 0x80) => "get profile count".to_string(),
        (CLASS_PROFILE, 0x81) => "get profile list".to_string(),
        (CLASS_PROFILE, 0x84) => "get active profile".to_string(),
        (CLASS_PROFILE, 0x8a) => "get max profiles".to_string(),
        (CLASS_PROFILE, 0x02) => format!("create profile P{}", arg(0)),
        (CLASS_PROFILE, 0x03) => format!("delete profile P{}", arg(0)),
        (CLASS_PROFILE, 0x04) => format!("set active profile P{}", arg(0)),
        (CLASS_LIGHT, 0x82) => format!("get LED {:02x} effect", arg(1)),
        (CLASS_LIGHT, 0x84) => format!("get LED {:02x} brightness", arg(1)),
        _ => return None,
    };
    let Ok(data) = &e.reply else { return Some(request) };
    // writes only echo their arguments, so only reads have something to add
    let answer = match (e.class, e.id) {
        (CLASS_INFO, 0x81) => format!("v{}.{}", at(data, 0), at(data, 1)),
        (CLASS_INFO, 0x82) => String::from_utf8_lossy(data).trim_end_matches('\0').to_string(),
        (CLASS_KEYMAP, 0x8c) => Action::decode(data.get(3..).unwrap_or_default()).to_string(),
        (CLASS_DPI, 0x85) => format!("{} x {}", be(data, 1), be(data, 3)),
        (CLASS_DPI, 0x86) => format!("{} active {}", stages(data, at(data, 2) as usize, 4), at(data, 1)),
        (CLASS_PROFILE, 0x80 | 0x84 | 0x8a) => at(data, 0).to_string(),
        (CLASS_PROFILE, 0x81) => {
            let ids: Vec<String> = data.iter().skip(1).map(|p| format!("P{p}")).collect();
            format!("max {}, {}", at(data, 0), ids.join(" "))
        }
        (CLASS_LIGHT, 0x82) => format!("{:?}", crate::rgb::Effect::from_reply(data)),
        (CLASS_LIGHT, 0x84) => at(data, 2).to_string(),
        _ => return Some(request),
    };
    Some(format!("{request} = {answer}"))
}

pub fn raw(e: &Exchange) -> String {
    format!("{} > {}", e.sent, e.got)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::offline_mouse;
    use crate::hid::hex;

    fn exchange<'a>(class: u8, id: u8, args: &'a [u8], reply: &'a [u8]) -> Exchange<'a> {
        Exchange { class, id, args, reply: Ok(reply), sent: String::new(), got: format!("ok [{}]", hex(reply)) }
    }

    /// The bytes are the ones the Naga sent back while the profile commands were worked out.
    #[test]
    fn known_commands_read_as_words() {
        let def = offline_mouse().def;
        let read = exchange(0x02, 0x8c, &[0x01, 0x01, 0x00], &[0x01, 0x01, 0x00, 0x01, 0x01, 0x02, 0x00]);
        assert_eq!(describe(&def, &read).unwrap(), "get button P1 left normal = mouse:right");
        let list = exchange(0x05, 0x81, &[], &[0x05, 0x01, 0x02]);
        assert_eq!(describe(&def, &list).unwrap(), "get profile list = max 5, P1 P2");
        let switch = exchange(0x05, 0x04, &[0x02], &[0x02]);
        assert_eq!(describe(&def, &switch).unwrap(), "set active profile P2");
        let unknown = exchange(0x0f, 0x80, &[], &[]);
        assert_eq!(describe(&def, &unknown), None);
    }

    #[test]
    fn set_button_decodes_the_action_it_writes() {
        let def = offline_mouse().def;
        let set = exchange(0x02, 0x0c, &[0x01, 0x40, 0x01, 0x01, 0x01, 0x04], &[]);
        assert_eq!(describe(&def, &set).unwrap(), "set button P1 thumb1 hypershift > mouse:back");
        // a button or layer the device file doesn't name still reads as something
        let unnamed = exchange(0x02, 0x8c, &[0x02, 0x77, 0x05], &[0x02, 0x77, 0x05, 0x00]);
        assert_eq!(describe(&def, &unnamed).unwrap(), "get button P2 button 77 layer 5 = disabled");
    }

    #[test]
    fn dpi_and_stages_read_the_x_and_y_values() {
        let def = offline_mouse().def;
        let dpi = exchange(0x04, 0x85, &[0x01], &[0x01, 0x06, 0x40, 0x0c, 0x80]);
        assert_eq!(describe(&def, &dpi).unwrap(), "get DPI = 1600 x 3200");
        let set = exchange(0x04, 0x05, &[0x01, 0x03, 0x20, 0x03, 0x20], &[]);
        assert_eq!(describe(&def, &set).unwrap(), "set DPI 800 x 800");
        // each stage is index, x, y and two reserved bytes, after the variable store, active and count
        let table = [0x01, 0x02, 0x03, 0, 0x03, 0x20, 0x03, 0x20, 0, 0, 1, 0x06, 0x40, 0x06, 0x40, 0, 0, 2, 0x0c, 0x80, 0x0c, 0x80, 0, 0];
        let read = exchange(0x04, 0x86, &[0x01], &table);
        assert_eq!(describe(&def, &read).unwrap(), "get DPI stages = 800/1600/3200 active 2");
        let write = exchange(0x04, 0x06, &table, &[]);
        assert_eq!(describe(&def, &write).unwrap(), "set DPI stages 800/1600/3200 active 2");
    }

    #[test]
    fn identity_and_lighting_reads_show_their_answer() {
        let def = offline_mouse().def;
        assert_eq!(describe(&def, &exchange(0x00, 0x81, &[], &[1, 2])).unwrap(), "get firmware = v1.2");
        // the serial is padded with zeros up to the reply size
        assert_eq!(describe(&def, &exchange(0x00, 0x82, &[], b"PM123\0\0")).unwrap(), "get serial = PM123");
        let effect = exchange(0x0f, 0x82, &[0x00, 0x04], &[0x00, 0x04, 0x01, 0, 0, 1, 0, 0xff, 0]);
        assert_eq!(describe(&def, &effect).unwrap(), "get LED 04 effect = Static([0, 255, 0])");
        let brightness = exchange(0x0f, 0x84, &[0x00, 0x04], &[0x00, 0x04, 0x80]);
        assert_eq!(describe(&def, &brightness).unwrap(), "get LED 04 brightness = 128");
    }

    #[test]
    fn profile_reads_and_writes_name_the_profile() {
        let def = offline_mouse().def;
        assert_eq!(describe(&def, &exchange(0x05, 0x80, &[], &[2])).unwrap(), "get profile count = 2");
        assert_eq!(describe(&def, &exchange(0x05, 0x84, &[], &[1])).unwrap(), "get active profile = 1");
        assert_eq!(describe(&def, &exchange(0x05, 0x8a, &[], &[5])).unwrap(), "get max profiles = 5");
        assert_eq!(describe(&def, &exchange(0x05, 0x02, &[0x03], &[0x03])).unwrap(), "create profile P3");
        assert_eq!(describe(&def, &exchange(0x05, 0x03, &[0x03], &[0x03])).unwrap(), "delete profile P3");
    }

    #[test]
    fn a_failed_reply_returns_just_the_request() {
        let def = offline_mouse().def;
        let failed = Exchange { class: 0x04, id: 0x85, args: &[0x01], reply: Err("busy".into()), sent: String::new(), got: "error: busy".into() };
        assert_eq!(describe(&def, &failed).unwrap(), "get DPI");
    }

    #[test]
    fn raw_joins_what_was_sent_and_what_came_back() {
        let e = exchange(0x04, 0x85, &[0x01], &[0x01]);
        let e = Exchange { sent: "04/85 size 07 args [01]".into(), ..e };
        assert_eq!(raw(&e), "04/85 size 07 args [01] > ok [01]");
    }

    #[test]
    fn unknown_class_has_no_translation() {
        let def = offline_mouse().def;
        assert_eq!(describe(&def, &exchange(0x7f, 0x81, &[], &[1])), None);
    }
}
