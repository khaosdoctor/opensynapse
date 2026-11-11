//! Asks the desktop for light or dark through xdg-desktop-portal, and follows it when it changes.

use super::apply;
use eframe::egui;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedValue;

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS_IFACE: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";

/// The portal answers 1 for dark, 2 for light and 0 for no preference. When
/// there's no portal or no preference we go dark, since that's the app's own look.
pub(super) fn prefers_light() -> bool {
    let Ok(conn) = Connection::session() else { return false };
    let Ok(reply) = conn.call_method(Some(PORTAL), PORTAL_PATH, Some(SETTINGS_IFACE), "ReadOne", &(APPEARANCE, "color-scheme"))
    else {
        return false;
    };
    let Ok(value) = reply.body().deserialize::<OwnedValue>() else { return false };
    u32::try_from(value) == Ok(2)
}

pub(super) fn follow_desktop(ctx: egui::Context) {
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
