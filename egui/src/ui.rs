use crate::app::{App, Mode};
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
                    ui.label("rule")
                        .on_hover_text(Config::get_field_docs("rule_str").unwrap());
                    ui.horizontal(|ui| {
                        match self.config.parse_rule() {
                            Ok(_) => {
                                ui.label(RichText::new("✔").color(Color32::GREEN))
                                    .on_hover_text("The rule is valid.");
                            }
                            Err(err) => {
                                ui.label(RichText::new("🗙").color(Color32::RED))
                                    .on_hover_text(err.to_string());
                            }
                        }
                        ui.text_edit_singleline(&mut self.config.rule_str);
                    });
                    ui.end_row();

                    if self.config.requires_square() {
                        let mut size = self.config.width;

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

                        self.config.width = size;
                        self.config.height = size;
                    } else {
                        ui.label("width")
                            .on_hover_text(Config::get_field_docs("width").unwrap());
                        ui.add(
                            DragValue::new(&mut self.config.width)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();

                        ui.label("height")
                            .on_hover_text(Config::get_field_docs("height").unwrap());
                        ui.add(
                            DragValue::new(&mut self.config.height)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();
                    }

                    ui.label("period")
                        .on_hover_text(Config::get_field_docs("period").unwrap());
                    ui.add(
                        DragValue::new(&mut self.config.period)
                            .speed(0.1)
                            .clamp_range(1..=u16::MAX),
                    );
                    ui.end_row();

                    let translation_condition = self.config.symmetry.translation_condition();
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
                                DragValue::new(&mut self.config.dx)
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
                                DragValue::new(&mut self.config.dy)
                                    .speed(0.1)
                                    .clamp_range(i16::MIN..=i16::MAX),
                            );
                            ui.end_row();
                        }
                        TranslationCondition::Diagonal => {
                            let mut translation = self.config.dx;

                            ui.label("dx")
                                .on_hover_text(Config::get_field_docs("dx").unwrap());
                            ui.add(DragValue::new(&mut translation).speed(0.1));
                            ui.end_row();

                            ui.label("dy")
                                .on_hover_text(Config::get_field_docs("dy").unwrap());
                            ui.add(DragValue::new(&mut translation).speed(0.1));
                            ui.end_row();

                            self.config.dx = translation;
                            self.config.dy = translation;
                        }
                        TranslationCondition::Antidiagonal => {
                            let mut dx: i32 = self.config.dx;
                            let mut dy: i32 = self.config.dy;

                            ui.label("dx")
                                .on_hover_text(Config::get_field_docs("dx").unwrap());
                            ui.add(DragValue::new(&mut dx).speed(0.1));
                            ui.end_row();

                            ui.label("dy")
                                .on_hover_text(Config::get_field_docs("dy").unwrap());
                            ui.add(DragValue::new(&mut dy).speed(0.1));
                            ui.end_row();

                            if self.config.dx != dx {
                                self.config.dx = dx;
                                self.config.dy = -dx;
                            } else {
                                self.config.dx = -dy;
                                self.config.dy = dy;
                            }
                        }
                    }

                    ui.label("diagonal width")
                        .on_hover_text(Config::get_field_docs("diagonal_width").unwrap());
                    ui.add_enabled_ui(!self.config.requires_no_diagonal_width(), |ui| {
                        ui.horizontal(|ui| {
                            let mut checked = self.config.diagonal_width.is_some();
                            ui.checkbox(&mut checked, "");
                            let mut dummy = 0;
                            let diagonal_width = if checked {
                                self.config
                                    .diagonal_width
                                    .get_or_insert(self.config.width.min(self.config.height))
                            } else {
                                self.config.diagonal_width = None;
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
                        .selected_text(self.config.symmetry.to_string())
                        .show_ui(ui, |ui| {
                            for (i, symmetry) in Symmetry::iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.config.symmetry,
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
                        .selected_text(self.config.transformation.to_string())
                        .show_ui(ui, |ui| {
                            for (i, transformation) in Transformation::iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.config.transformation,
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
                            self.config
                                .search_order
                                .map_or_else(|| "auto".to_owned(), |s| s.to_string()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.search_order, None, "auto")
                                .on_hover_text("The search order is automatically determined.");
                            for (i, search_order) in SearchOrder::iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.config.search_order,
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
                        .selected_text(self.config.new_state.to_string())
                        .show_ui(ui, |ui| {
                            for (i, new_state) in NewState::iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.config.new_state,
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
                        let mut checked = self.config.seed.is_some();
                        ui.checkbox(&mut checked, "");
                        let mut dummy = 0;
                        let seed = if checked {
                            self.config.seed.get_or_insert(0)
                        } else {
                            self.config.seed = None;
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
                        let mut checked = self.config.max_population.is_some();
                        ui.checkbox(&mut checked, "");
                        let mut dummy = 0;
                        let max_population = if checked {
                            self.config
                                .max_population
                                .get_or_insert((self.config.width * self.config.height) as usize)
                        } else {
                            self.config.max_population = None;
                            &mut dummy
                        };
                        ui.add_enabled_ui(checked, |ui| {
                            ui.add(DragValue::new(max_population).speed(1.0));
                        });
                    });
                    ui.end_row();

                    ui.label("reduce max")
                        .on_hover_text(Config::get_field_docs("reduce_max_population").unwrap());
                    ui.checkbox(&mut self.config.reduce_max_population, "");
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
                    0..=self.config.period as i32 - 1,
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
                ui.label(Status::get_field_docs(self.status.to_string()).unwrap())
                    .on_hover_text(Self::get_field_docs("status").unwrap());
            }

            if self.mode == Mode::Running || self.mode == Mode::Paused {
                ui.separator();

                ui.label("elapsed")
                    .on_hover_text(Self::get_field_docs("elapsed").unwrap());
                ui.label(format!("{:?}", self.elapsed));
            }
        });
    }

    /// The main panel.
    pub fn main_panel(&mut self, ui: &mut Ui) {
        if let Some(view) = &self.view {
            ScrollArea::both().show(ui, |ui| {
                ui.add(Label::new(view.clone()).wrap(false));
            });

            if self.mode == Mode::Running {
                ui.ctx().request_repaint();
            }
        }
    }
}
