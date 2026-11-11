//! Polling rate through the OpenRazer daemon. Its driver already knows how to
//! set it on every mouse it supports, so we don't send our own reports for it.

use crate::rgb::{daemon_device, reply};
use serde::Serialize;
use std::io;
use zbus::blocking::Connection;
use zbus::zvariant::{DynamicType, Type};

/// The rates the daemon takes for every mouse unless it lists its own.
pub const POLL_RATES: [u16; 3] = [125, 500, 1000];

/// The mouse on the OpenRazer daemon. There's none without the daemon.
pub struct Power {
    conn: Connection,
    path: String,
}

pub fn find(serial: &str) -> Option<Power> {
    let (conn, path) = daemon_device(serial)?;
    Some(Power { conn, path })
}

impl Power {
    /// None when the method is missing, which is how the daemon says a mouse doesn't have it.
    fn get<T: serde::de::DeserializeOwned + Type>(&self, iface: &str, method: &str) -> Option<T> {
        reply(self.conn.call_method(Some("org.razer"), self.path.as_str(), Some(iface), method, &()).ok())
    }

    fn set<B: Serialize + DynamicType>(&self, iface: &str, method: &str, body: &B) -> io::Result<()> {
        self.conn.call_method(Some("org.razer"), self.path.as_str(), Some(iface), method, body).map_err(io::Error::other)?;
        Ok(())
    }

    /// In Hz.
    pub fn poll_rate(&self) -> Option<u16> {
        let hz: i32 = self.get("razer.device.misc", "getPollRate")?;
        u16::try_from(hz).ok()
    }

    pub fn set_poll_rate(&self, hz: u16) -> io::Result<()> {
        if !POLL_RATES.contains(&hz) {
            return Err(io::Error::other(format!("polling rate is one of {POLL_RATES:?}")));
        }
        self.set("razer.device.misc", "setPollRate", &(hz,))
    }
}
