//! Polling rate, battery and the sleep timeout through the OpenRazer daemon.
//! Its driver already knows how to do these on every mouse it supports, so we
//! don't send our own reports for them. Only wireless mice have a battery and
//! a sleep timeout.

use crate::rgb::{daemon_device, reply};
use serde::Serialize;
use std::io;
use std::ops::RangeInclusive;
use zbus::blocking::Connection;
use zbus::zvariant::{DynamicType, Type};

/// The rates the daemon takes for every mouse unless it lists its own.
pub const POLL_RATES: [u16; 3] = [125, 500, 1000];

/// Seconds without movement before a wireless mouse sleeps. The driver clamps to this.
pub const IDLE_SECONDS: RangeInclusive<u16> = 60..=900;

pub struct Battery {
    pub percent: f64,
    pub charging: bool,
}

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

    pub fn battery(&self) -> Option<Battery> {
        let percent: f64 = self.get("razer.device.power", "getBattery")?;
        // the daemon answers -1 when the driver can't read the charge
        if percent < 0.0 {
            return None;
        }
        let charging = self.get("razer.device.power", "isCharging").unwrap_or(false);
        Some(Battery { percent, charging })
    }

    /// Seconds without movement before the mouse sleeps.
    pub fn idle_time(&self) -> Option<u16> {
        self.get("razer.device.power", "getIdleTime")
    }

    pub fn set_idle_time(&self, seconds: u16) -> io::Result<()> {
        if !IDLE_SECONDS.contains(&seconds) {
            return Err(io::Error::other(format!("the sleep timeout is {}-{} seconds", IDLE_SECONDS.start(), IDLE_SECONDS.end())));
        }
        self.set("razer.device.power", "setIdleTime", &(seconds,))
    }
}
