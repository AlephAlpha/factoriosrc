#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![warn(clippy::nursery)]

use eframe::Result;
use factoriosrc_egui::{APP_TITLE, App, native_options};

fn main() -> Result<()> {
    env_logger::init();

    eframe::run_native(
        APP_TITLE,
        native_options(),
        Box::new(|_cc| Ok(Box::<App>::default())),
    )
}
