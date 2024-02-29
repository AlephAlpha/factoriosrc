use egui::{ComboBox, DragValue, Grid, Ui};
use factoriosrc_lib::{
    Config, NewState, SearchOrder, Symmetry, Transformation, TranslationCondition,
};

/// A panel for factoriosrc configuration.
pub struct ConfigPanel {
    /// The configuration.
    pub config: Config,
    /// Whether the configuration panel is enabled.
    ///
    /// When disabled, the configuration panel will be read-only.
    pub enabled: bool,
}

impl ConfigPanel {
    pub fn ui(&mut self, ui: &mut Ui) {
        ui.heading("Configuration");

        ui.checkbox(&mut self.enabled, "Enabled")
            .on_hover_text("Whether the configuration panel is enabled.");

        ui.add_enabled_ui(self.enabled, |ui| {
            Grid::new("config_panel")
                .striped(true)
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("rule")
                        .on_hover_text("The rule string of the cellular automaton.");
                    ui.text_edit_singleline(&mut self.config.rule_str);
                    ui.end_row();

                    if self.config.requires_square() {
                        let mut size = self.config.width;

                        ui.label("width").on_hover_text("Width of the world.");
                        ui.add(
                            DragValue::new(&mut size)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();

                        ui.label("height").on_hover_text("Height of the world.");
                        ui.add(
                            DragValue::new(&mut size)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();

                        self.config.width = size;
                        self.config.height = size;
                    } else {
                        ui.label("width").on_hover_text("Width of the world.");
                        ui.add(
                            DragValue::new(&mut self.config.width)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();

                        ui.label("height").on_hover_text("Height of the world.");
                        ui.add(
                            DragValue::new(&mut self.config.height)
                                .speed(0.1)
                                .clamp_range(1..=u16::MAX),
                        );
                        ui.end_row();
                    }

                    ui.label("period").on_hover_text("Period of the pattern.");
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
                                .on_hover_text("Horizontal translation of the world.");
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
                                .on_hover_text("Vertical translation of the world.");
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
                                .on_hover_text("Horizontal translation of the world.");
                            ui.add(DragValue::new(&mut translation).speed(0.1));
                            ui.end_row();

                            ui.label("dy")
                                .on_hover_text("Vertical translation of the world.");
                            ui.add(DragValue::new(&mut translation).speed(0.1));
                            ui.end_row();

                            self.config.dx = translation;
                            self.config.dy = translation;
                        }
                        TranslationCondition::Antidiagonal => {
                            let mut dx: i32 = self.config.dx;
                            let mut dy: i32 = self.config.dy;

                            ui.label("dx")
                                .on_hover_text("Horizontal translation of the world.");
                            ui.add(DragValue::new(&mut dx).speed(0.1));
                            ui.end_row();

                            ui.label("dy")
                                .on_hover_text("Vertical translation of the world.");
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
                        .on_hover_text("Zoom of the world.");

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
                        .on_hover_text("Symmetry of the pattern.");
                    ComboBox::from_id_source("symmetry")
                        .selected_text(self.config.symmetry.to_string())
                        .show_ui(ui, |ui| {
                            for symmetry in Symmetry::iter() {
                                ui.selectable_value(
                                    &mut self.config.symmetry,
                                    symmetry,
                                    symmetry.to_string(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("transformation")
                        .on_hover_text("Transformation of the pattern.");
                    ComboBox::from_id_source("transformation")
                        .selected_text(self.config.transformation.to_string())
                        .show_ui(ui, |ui| {
                            for transformation in Transformation::iter() {
                                ui.selectable_value(
                                    &mut self.config.transformation,
                                    transformation,
                                    transformation.to_string(),
                                );
                            }
                        });

                    ui.end_row();

                    ui.label("search order").on_hover_text("Search order.");
                    ComboBox::from_id_source("search_order")
                        .selected_text(
                            self.config
                                .search_order
                                .map_or_else(|| "auto".to_owned(), |s| s.to_string()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.search_order, None, "auto");
                            for search_order_ in SearchOrder::iter() {
                                ui.selectable_value(
                                    &mut self.config.search_order,
                                    Some(search_order_),
                                    search_order_.to_string(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("new state")
                        .on_hover_text("How to guess the state of an unknown cell.");
                    ComboBox::from_id_source("new_state")
                        .selected_text(self.config.new_state.to_string())
                        .show_ui(ui, |ui| {
                            for new_state in NewState::iter() {
                                ui.selectable_value(
                                    &mut self.config.new_state,
                                    new_state,
                                    new_state.to_string(),
                                );
                            }
                        });
                    ui.end_row();

                    ui.label("seed")
                        .on_hover_text("Random seed for guessing the state of an unknown cell.");
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
                        .on_hover_text("Upper bound of the population of the pattern.");
                    ui.horizontal(|ui| {
                        let mut checked = self.config.max_population.is_some();
                        ui.checkbox(&mut checked, "");
                        let mut dummy = 0;
                        let max_population = if checked {
                            self.config.max_population.get_or_insert(0)
                        } else {
                            self.config.max_population = None;
                            &mut dummy
                        };
                        ui.add_enabled_ui(checked, |ui| {
                            ui.add(DragValue::new(max_population).speed(1.0));
                        });
                    });
                    ui.end_row();

                    ui.label("reduce max population").on_hover_text(
                    "Whether to reduce the upper bound of the population when a solution is found.",
                );
                    ui.checkbox(&mut self.config.reduce_max_population, "");
                });
        });
    }
}
