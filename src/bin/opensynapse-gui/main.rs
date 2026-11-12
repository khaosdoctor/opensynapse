// rustc expects a lib.rs to be a crate root, ours is just the GUI's own lib/ folder
#![allow(special_module_name)]

use eframe::egui;
use lib::instance_lock::{Launcher, Lock, take_lock};
use state::App;

mod gui;
mod lib;
mod shared;
mod state;
mod theme;

fn main() -> eframe::Result {
    let (lock, busy) = match take_lock() {
        Lock::Free(file) => (file, None),
        Lock::Busy(pid) => (None, Some(pid)),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 760.0])
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon.png")).unwrap_or_default()),
        ..Default::default()
    };
    eframe::run_native(
        "opensynapse",
        options,
        Box::new(move |cc| {
            theme::start(&cc.egui_ctx);
            // while the alert shows we don't touch the mouse, the other GUI still has it
            if busy.is_none() {
                lib::debug::start();
            }
            let app = busy.is_none().then(|| App::new(cc.egui_ctx.clone()));
            Ok(Box::new(Launcher { app, busy, _lock: lock }))
        }),
    )
}
