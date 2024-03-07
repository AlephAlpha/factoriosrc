use crate::app::{App, AppConfig, Mode};
use documented::{Documented, DocumentedFields};
use egui::{Color32, ComboBox, DragValue, Grid, Label, RichText, ScrollArea, Slider, Ui};
use factoriosrc_lib::{
    Config, NewState, SearchOrder, Status, Symmetry, Transformation, TranslationCondition,
};

impl App {
    /// The configuration panel.
    pub fn config_panel(&mut self, ui: &mut Ui) {
        ui.heading("Configuration").on_hover_text(Config::DOCS);

        ui.add_enabled_ui(self.mode == Mode::Configuring, |ui| {
            Grid::new("config_panel")
                .striped(true)
                .num_columns(2)
                .show(ui, |ui| {
                    let config = &mut self.config.config;

                    ui.label("rule")
                        .on_hover_text(Config::get_field_docs("rule_str").unwrap());
                    ui.horizontal(|ui| {
                        match config.parse_rule() {
                            Ok(_) => {
                                ui.label(RichText::new("✔").color(Color32::GREEN))
                                    .on_hover_text("The rule is valid.");
                            }
                            Err(err) => {
                                ui.label(RichText::new("🗙").color(Color32::RED))
                                    .on_hover_text(err.to_string());
                            }
                        }
                        ui.text_edit_singleline(&mut config.rule_str);
                    });
                    ui.end_row();

                    if config.requires_square() {
                        let mut size = config.width;

                        ui.label("width")
                            .on_hover_text(Config::get_field_docs("width").unwrap());
                        ui.add(
                            DragValue::new(&mut size)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();

                        ui.label("height")
                            .on_hover_text(Config::get_field_docs("height").unwrap());
                        ui.add(
                            DragValue::new(&mut size)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();

                        config.width = size;
                        config.height = size;
                    } else {
                        ui.label("width")
                            .on_hover_text(Config::get_field_docs("width").unwrap());
                        ui.add(
                            DragValue::new(&mut config.width)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();

                        ui.label("height")
                            .on_hover_text(Config::get_field_docs("height").unwrap());
                        ui.add(
                            DragValue::new(&mut config.height)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();
                    }

                    ui.label("period")
                        .on_hover_text(Config::get_field_docs("period").unwrap());
                    ui.add(
                        DragValue::new(&mut config.period)
                            .speed(0.1)
                            .clamp_range(1..=u16::MAX),
                    );
                    ui.end_row();

                    let translation_condition = config.symmetry.translation_condition();
                    match translation_condition {
                        TranslationCondition::Any
                        | TranslationCondition::NoHorizontal
                        | TranslationCondition::NoVertical
                        | TranslationCondition::NoTranslation => {
                            ui.label("dx")
                                .on_hover_text(Config::get_field_docs("dx").unwrap());
                            ui.add_enabled(
                                matches!(
                                    translation_condition,
                                    TranslationCondition::Any | TranslationCondition::NoVertical
                                ),
                                DragValue::new(&mut config.dx)
                                    .speed(0.1)
                                    .clamp_range(i16::MIN..=i16::MAX),
                            );
                            ui.end_row();

                            ui.label("dy")
                                .on_hover_text(Config::get_field_docs("dy").unwrap());
                            ui.add_enabled(
                                matches!(
                                    translation_condition,
                                    TranslationCondition::Any | TranslationCondition::NoHorizontal
                                ),
                                DragValue::new(&mut config.dy)
                                    .speed(0.1)
                                    .clamp_range(i16::MIN..=i16::MAX),
                            );
                            ui.end_row();
                        }
                        TranslationCondition::Diagonal => {
                            let mut translation = config.dx;

                            ui.label("dx")
                                .on_hover_text(Config::get_field_docs("dx").unwrap());
                            ui.add(DragValue::new(&mut translation).speed(0.1));
                            ui.end_row();

                            ui.label("dy")
                                .on_hover_text(Config::get_field_docs("dy").unwrap());
                            ui.add(DragValue::new(&mut translation).speed(0.1));
                            ui.end_row();

                            config.dx = translation;
                            config.dy = translation;
                        }
                        TranslationCondition::Antidiagonal => {
                            let mut dx: i32 = config.dx;
                            let mut dy: i32 = config.dy;

                            ui.label("dx")
                                .on_hover_text(Config::get_field_docs("dx").unwrap());
                            ui.add(DragValue::new(&mut dx).speed(0.1));
                            ui.end_row();

                            ui.label("dy")
                                .on_hover_text(Config::get_field_docs("dy").unwrap());
                            ui.add(DragValue::new(&mut dy).speed(0.1));
                            ui.end_row();

                            if config.dx != dx {
                                config.dx = dx;
                                config.dy = -dx;
                            } else {
                                config.dx = -dy;
                                config.dy = dy;
                            }
                        }
                    }

                    ui.label("diagonal width")
                        .on_hover_text(Config::get_field_docs("diagonal_width").unwrap());
                    ui.add_enabled_ui(!config.requires_no_diagonal_width(), |ui| {
                        ui.horizontal(|ui| {
                            let mut checked = config.diagonal_width.is_some();
                            ui.checkbox(&mut checked, "");
                            let mut dummy = 0;
                            let diagonal_width = if checked {
                                config
                                    .diagonal_width
                                    .get_or_insert(config.width.min(config.height))
                            } else {
                                config.diagonal_width = None;
                                &mut dummy
                            };
                            ui.add_enabled_ui(checked, |ui| {
                                ui.add(
                                    DragValue::new(diagonal_width)
                                        .speed(0.1)
                                        .clamp_range(if checked { 1..=u16::MAX } else { 0..=0 }),
                                );
                            });
                        })
                    });
                    ui.end_row();

                    ui.label("symmetry")
                        .on_hover_text(Config::get_field_docs("symmetry").unwrap());
                    ComboBox::from_id_source("symmetry")
                        .selected_text(config.symmetry.to_string())
                        .show_ui(ui, |ui| {
                            for (i, symmetry) in Symmetry::iter().enumerate() {
                                ui.selectable_value(
                                    &mut config.symmetry,
                                    symmetry,
                                    symmetry.to_string(),
                                )
                                .on_hover_text(Symmetry::FIELD_DOCS[i].unwrap());
                            }
                        });
                    ui.end_row();

                    ui.label("transformation")
                        .on_hover_text(Config::get_field_docs("transformation").unwrap());
                    ComboBox::from_id_source("transformation")
                        .selected_text(config.transformation.to_string())
                        .show_ui(ui, |ui| {
                            for (i, transformation) in Transformation::iter().enumerate() {
                                ui.selectable_value(
                                    &mut config.transformation,
                                    transformation,
                                    transformation.to_string(),
                                )
                                .on_hover_text(Transformation::FIELD_DOCS[i].unwrap());
                            }
                        });
                    ui.end_row();

                    ui.label("search order")
                        .on_hover_text(Config::get_field_docs("search_order").unwrap());
                    ComboBox::from_id_source("search_order")
                        .selected_text(
                            config
                                .search_order
                                .map_or_else(|| "auto".to_owned(), |s| s.to_string()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut config.search_order, None, "auto")
                                .on_hover_text("The search order is automatically determined.");
                            for (i, search_order) in SearchOrder::iter().enumerate() {
                                ui.selectable_value(
                                    &mut config.search_order,
                                    Some(search_order),
                                    search_order.to_string(),
                                )
                                .on_hover_text(SearchOrder::FIELD_DOCS[i].unwrap());
                            }
                        });
                    ui.end_row();

                    ui.label("new state")
                        .on_hover_text(Config::get_field_docs("new_state").unwrap());
                    ComboBox::from_id_source("new_state")
                        .selected_text(config.new_state.to_string())
                        .show_ui(ui, |ui| {
                            for (i, new_state) in NewState::iter().enumerate() {
                                ui.selectable_value(
                                    &mut config.new_state,
                                    new_state,
                                    new_state.to_string(),
                                )
                                .on_hover_text(NewState::FIELD_DOCS[i].unwrap());
                            }
                        });
                    ui.end_row();

                    ui.label("seed")
                        .on_hover_text(Config::get_field_docs("seed").unwrap());
                    ui.horizontal(|ui| {
                        let mut checked = config.seed.is_some();
                        ui.checkbox(&mut checked, "");
                        let mut dummy = 0;
                        let seed = if checked {
                            config.seed.get_or_insert(0)
                        } else {
                            config.seed = None;
                            &mut dummy
                        };
                        ui.add_enabled_ui(checked, |ui| {
                            ui.add(DragValue::new(seed).speed(1.0));
                        });
                    });
                    ui.end_row();

                    ui.label("max population")
                        .on_hover_text(Config::get_field_docs("max_population").unwrap());
                    ui.horizontal(|ui| {
                        let mut checked = config.max_population.is_some();
                        ui.checkbox(&mut checked, "");
                        let mut dummy = 0;
                        let max_population = if checked {
                            config
                                .max_population
                                .get_or_insert((config.width * config.height) as usize)
                        } else {
                            config.max_population = None;
                            &mut dummy
                        };
                        ui.add_enabled_ui(checked, |ui| {
                            ui.add(DragValue::new(max_population).speed(0.1));
                        });
                    });
                    ui.end_row();

                    ui.label("reduce max")
                        .on_hover_text(Config::get_field_docs("reduce_max_population").unwrap());
                    ui.checkbox(&mut config.reduce_max_population, "");
                    ui.end_row();

                    ui.label("increase size")
                        .on_hover_text(AppConfig::get_field_docs("increase_world_size").unwrap());
                    ui.checkbox(&mut self.config.increase_world_size, "");
                    ui.end_row();

                    ui.label("no stop")
                        .on_hover_text(AppConfig::get_field_docs("no_stop").unwrap());
                    ui.checkbox(&mut self.config.no_stop, "");
                    ui.end_row();

                    ui.label("step")
                        .on_hover_text(AppConfig::get_field_docs("step").unwrap());
                    ui.add(DragValue::new(&mut self.config.step).speed(1.0));
                    ui.end_row();
                });
        });
    }

    /// The control panel.
    pub fn control_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if self.mode == Mode::Configuring {
                if ui.button("New").clicked() {
                    self.new_search();
                }
            } else {
                ui.add_enabled_ui(self.mode == Mode::Paused, |ui| {
                    let text = match self.status {
                        Status::NotStarted => "Start",
                        Status::Running => "Resume",
                        _ => "Next",
                    };

                    if ui.button(text).clicked() {
                        self.start();
                    }
                });
                ui.add_enabled_ui(self.mode == Mode::Running, |ui| {
                    if ui.button("Pause").clicked() {
                        self.pause();
                    }
                });
                if ui.button("Stop").clicked() {
                    self.stop();
                }

                ui.separator();

                ui.label("generation")
                    .on_hover_text(Self::get_field_docs("generation").unwrap());
                ui.add(Slider::new(
                    &mut self.generation,
                    0..=self.config.config.period as i32 - 1,
                ));
            }
        });
    }

    /// The status panel.
    pub fn status_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some(err) = &self.error {
                ui.label(RichText::new(err.to_string()).color(Color32::RED));
            } else {
                let status = if self.status == Status::Running && self.mode == Mode::Paused {
                    "Paused."
                } else {
                    Status::get_field_docs(self.status.to_string()).unwrap()
                };

                ui.label(status)
                    .on_hover_text(Self::get_field_docs("status").unwrap());
            }

            ui.separator();

            ui.label("Solution count:")
                .on_hover_text("The number of solutions found so far.");
            ui.label(self.solutions.len().to_string());

            if !self.populations.is_empty() {
                ui.separator();

                ui.label("Population:")
                    .on_hover_text("Populations of the current partial result.");
                ui.label(self.populations[self.generation as usize].to_string());
            }

            if self.mode == Mode::Paused {
                ui.separator();

                ui.label("Search time:")
                    .on_hover_text(Self::get_field_docs("elapsed").unwrap());
                ui.label(format!("{:?}", self.elapsed));
            }
        });
    }

    /// The main panel.
    pub fn main_panel(&mut self, ui: &mut Ui) {
        match self.mode {
            Mode::Configuring => {
                for view in self.solutions.iter().rev() {
                    ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        ui.add(Label::new(view.clone()).wrap(false));
                    });

                    if self.mode == Mode::Running {
                        ui.ctx().request_repaint();
                    }
                }
            }
            _ => {
                if !self.view.is_empty() {
                    ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        ui.add(Label::new(self.view[self.generation as usize].clone()).wrap(false));
                    });

                    if self.mode == Mode::Running {
                        ui.ctx().request_repaint();
                    }
                }
            }
        };
    }
}
