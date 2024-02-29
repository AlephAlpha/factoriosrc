#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::{NativeOptions, Result};
use factoriosrc_egui::App;

fn main() -> Result<()> {
    env_logger::init();

    eframe::run_native(
        "factoriosrc",
        NativeOptions::default(),
        Box::new(|_cc| Box::<App>::default()),
    )
}
