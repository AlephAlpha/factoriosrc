mod config;

pub use config::ConfigPanel;
use eframe::{App as EframeApp, Frame};
use egui::{CentralPanel, Context, SidePanel};
use factoriosrc_lib::Config;

pub struct App {
    config_panel: ConfigPanel,
}

impl Default for App {
    fn default() -> Self {
        let config = Config::new("R3,C2,S2,B3,N+", 16, 16, 1);
        Self {
            config_panel: ConfigPanel {
                config,
                enabled: true,
            },
        }
    }
}

impl EframeApp for App {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        SidePanel::left("config_panel").show(ctx, |ui| {
            self.config_panel.ui(ui);
        });

        CentralPanel::default().show(ctx, |ui| {
            ui.label("Config:");
            ui.label(format!("{:#?}", self.config_panel.config));
            if self.config_panel.config.requires_square() {
                ui.label("This config requires the world to be square.");
            }
            if self.config_panel.config.requires_no_diagonal_width() {
                ui.label("This config requires the world to have no diagonal width.");
            }
        });
    }
}
