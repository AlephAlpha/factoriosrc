use crate::{
    app::{App, AppConfig, Mode},
    help,
    theme::{Palette, badge_text, muted, rle_layout_job, section_title},
};
use egui::{
    Button, CollapsingHeader, Color32, ComboBox, Context, DragValue, Grid, Label, RichText,
    ScrollArea, Sense, Slider, Stroke, StrokeKind, TextEdit, Ui, Window, vec2,
};
use factoriosrc_lib::{
    CellState, Config, ConfigHelpField, KnownCell, NewState, SearchControlHelpField, SearchOrder,
    Status, Symmetry, Transformation, TranslationCondition,
};

#[derive(Debug, Clone, Default)]
struct ConfigPreview {
    rule_error: Option<String>,
    config_error: Option<String>,
    auto_search_order: Option<String>,
}

fn preview_config(config: &Config) -> ConfigPreview {
    let rule_error = config.parse_rule().err().map(|error| error.to_string());

    let mut preview = config.clone();
    let config_error = preview.check().err().map(|error| error.to_string());
    let auto_search_order = if config.search_order.is_none() && config_error.is_none() {
        preview
            .search_order
            .map(|search_order| search_order.to_string())
    } else {
        None
    };

    ConfigPreview {
        rule_error,
        config_error,
        auto_search_order,
    }
}

fn config_section(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(section_title(title));
        ui.add_space(4.0);
        add_contents(ui);
    });
    ui.add_space(6.0);
}

fn translation_condition_note(condition: TranslationCondition) -> Option<&'static str> {
    match condition {
        TranslationCondition::Any => None,
        TranslationCondition::NoHorizontal => Some("Horizontal translation is locked by symmetry."),
        TranslationCondition::NoVertical => Some("Vertical translation is locked by symmetry."),
        TranslationCondition::NoTranslation => Some("Translation is locked by symmetry."),
        TranslationCondition::Diagonal => Some("Diagonal translation forces dx = dy."),
        TranslationCondition::Antidiagonal => Some("Anti-diagonal translation forces dx = -dy."),
    }
}

fn known_cell_state(known_cells: &[KnownCell], x: u32, y: u32, t: u32) -> Option<CellState> {
    known_cells
        .iter()
        .find(|cell| (cell.x, cell.y, cell.t) == (x, y, t))
        .map(|cell| cell.state)
}

fn set_known_cell(
    known_cells: &mut Vec<KnownCell>,
    x: u32,
    y: u32,
    t: u32,
    state: Option<CellState>,
) {
    known_cells.retain(|cell| (cell.x, cell.y, cell.t) != (x, y, t));
    if let Some(state) = state {
        known_cells.push(KnownCell::new(x, y, t, state));
    }
}

fn cycle_known_cell(known_cells: &mut Vec<KnownCell>, x: u32, y: u32, t: u32) -> Option<CellState> {
    let next = match known_cell_state(known_cells, x, y, t) {
        None => Some(CellState::Alive),
        Some(CellState::Alive) => Some(CellState::Dead),
        Some(CellState::Dead | CellState::Dying(_)) => None,
    };
    set_known_cell(known_cells, x, y, t, next);
    next
}

fn normalize_help_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '`' => {}
            '[' => {
                let mut label = String::new();
                for next in chars.by_ref() {
                    if next == ']' {
                        break;
                    }
                    if next != '`' {
                        label.push(next);
                    }
                }
                out.push_str(&label);

                if chars.peek() == Some(&'(') {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(ch),
        }
    }

    out.replace("<sub>", "").replace("</sub>", "")
}

impl App {
    /// The setup sidebar shell.
    pub fn setup_panel(&mut self, ui: &mut Ui) {
        let palette = Palette::new();

        ui.heading(section_title("Config"))
            .on_hover_text(help::CONFIG_PANEL_TOOLTIP);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Mode").strong().color(palette.accent));
            let badge = match self.mode {
                Mode::Configuring => badge_text("SETUP", palette.accent),
                Mode::Running => badge_text("RUNNING", palette.warning),
                Mode::Paused => badge_text("PAUSED", palette.subtle_text),
            };
            ui.label(badge);
            ui.separator();
            ui.label(format!(
                "{} x {}  p{}",
                self.config.config.width, self.config.config.height, self.config.config.period,
            ));
        });
        ui.separator();

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.config_panel(ui);
            });
    }

    /// The top command bar shell.
    pub fn command_bar(&mut self, ui: &mut Ui) {
        let palette = Palette::new();

        ui.horizontal_wrapped(|ui| {
            ui.heading(section_title("factoriosrc"));
            ui.separator();

            let badge = match self.mode {
                Mode::Configuring => badge_text("SETUP", palette.accent),
                Mode::Running => badge_text("RUNNING", palette.warning),
                Mode::Paused => match self.status {
                    Status::Solved => badge_text("SOLVED", palette.success),
                    Status::NoSolution => badge_text("EXHAUSTED", palette.danger),
                    _ => badge_text("PAUSED", palette.subtle_text),
                },
            };
            ui.label(badge);

            ui.separator();
            self.control_panel(ui);

            ui.separator();
            ui.add_enabled_ui(self.current_rle().is_some(), |ui| {
                if ui
                    .button("Copy RLE")
                    .on_hover_text(help::COPY_RLE_TOOLTIP)
                    .clicked()
                {
                    self.copy_current_rle(ui.ctx());
                }
            });

            if self.mode != Mode::Configuring {
                let config_label = if self.chrome.show_config {
                    "Hide Config"
                } else {
                    "Config"
                };
                if ui
                    .button(config_label)
                    .on_hover_text(help::CONFIG_TOGGLE_TOOLTIP)
                    .clicked()
                {
                    self.chrome.show_config = !self.chrome.show_config;
                }
            }

            let details_label = if self.chrome.show_details {
                "Hide Details"
            } else {
                "Details"
            };
            if ui
                .button(details_label)
                .on_hover_text(help::DETAILS_TOGGLE_TOOLTIP)
                .clicked()
            {
                self.chrome.show_details = !self.chrome.show_details;
            }

            if ui
                .button("Help")
                .on_hover_text(help::HELP_TOOLTIP)
                .clicked()
            {
                self.chrome.show_help = true;
            }
        });
    }

    /// The optional details panel.
    pub fn inspector_panel(&self, ui: &mut Ui) {
        let palette = Palette::new();

        ui.heading(section_title("Details"));
        ui.separator();

        Grid::new("details_grid").num_columns(2).show(ui, |ui| {
            ui.label(RichText::new("Status").strong().color(palette.accent));
            ui.label(self.status.to_string());
            ui.end_row();

            ui.label(RichText::new("Mode").strong().color(palette.accent));
            ui.label(self.mode_label());
            ui.end_row();

            ui.label(RichText::new("Rule").strong().color(palette.accent));
            ui.label(&self.config.config.rule_str);
            ui.end_row();

            ui.label(RichText::new("World").strong().color(palette.accent));
            ui.label(format!(
                "{} x {}  p{}",
                self.config.config.width, self.config.config.height, self.config.config.period,
            ));
            ui.end_row();

            ui.label(RichText::new("Generation").strong().color(palette.accent));
            ui.label(self.generation.to_string());
            ui.end_row();

            ui.label(RichText::new("Solutions").strong().color(palette.accent));
            ui.label(self.solutions.len().to_string());
            ui.end_row();

            if let Some(population) = self.current_population() {
                ui.label(RichText::new("Population").strong().color(palette.accent));
                ui.label(population.to_string());
                ui.end_row();
            }

            ui.label(RichText::new("Checked").strong().color(palette.accent));
            ui.label(self.cells_checked.to_string());
            ui.end_row();

            ui.label(RichText::new("Elapsed").strong().color(palette.accent));
            ui.label(format!("{:?}", self.elapsed));
            ui.end_row();
        });
    }

    /// The main results workspace shell.
    pub fn workspace_panel(&mut self, ui: &mut Ui) {
        let palette = Palette::new();
        let generation_count = self.active_generation_count();
        let active_solution = self.active_solution_index();

        ui.horizontal_wrapped(|ui| {
            ui.heading(section_title("Results"));
            ui.separator();
            let source_badge = if self.current_view_is_live() {
                badge_text("LIVE", palette.accent)
            } else if active_solution.is_some() {
                badge_text("STORED", palette.success)
            } else {
                badge_text("EMPTY", palette.subtle_text)
            };
            ui.label(source_badge);
            ui.label(RichText::new(self.current_result_source_label()).color(palette.subtle_text));

            if self.has_live_snapshot() {
                ui.separator();
                ui.add_enabled_ui(!self.current_view_is_live(), |ui| {
                    if ui
                        .button("Live")
                        .on_hover_text(help::LIVE_VIEW_TOOLTIP)
                        .clicked()
                    {
                        self.show_live_view();
                    }
                });
            }

            if generation_count > 0 {
                ui.separator();
                ui.label(RichText::new("Gen").strong().color(palette.accent))
                    .on_hover_text(help::GENERATION_TOOLTIP);
                ui.add(
                    Slider::new(
                        &mut self.generation,
                        0..=generation_count.saturating_sub(1) as i32,
                    )
                    .show_value(false)
                    .text("generation"),
                );
                ui.label(format!(
                    "{} / {}",
                    self.generation,
                    generation_count.saturating_sub(1)
                ));
            }

            ui.separator();
            ui.label(RichText::new("Solutions").strong().color(palette.accent));
            ui.label(self.solutions.len().to_string());
            if let Some(population) = self.current_population() {
                ui.separator();
                ui.label(RichText::new("Pop").strong().color(palette.accent));
                ui.label(population.to_string());
            }

            ui.separator();
            let history_label = if self.chrome.show_history {
                "Hide History"
            } else {
                "History"
            };
            if ui
                .button(history_label)
                .on_hover_text(help::HISTORY_TOGGLE_TOOLTIP)
                .clicked()
            {
                self.chrome.show_history = !self.chrome.show_history;
            }
        });

        ui.add_space(6.0);
        if self.chrome.show_history {
            ui.columns(2, |columns| {
                columns[0].vertical(|ui| {
                    self.main_panel(ui);
                });

                columns[1].group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(section_title("History"));
                    ui.add_space(4.0);

                    if self.has_live_snapshot() {
                        let live_selected = self.current_view_is_live();
                        if ui
                            .selectable_label(live_selected, "Live snapshot")
                            .on_hover_text(help::LIVE_HISTORY_TOOLTIP)
                            .clicked()
                        {
                            self.show_live_view();
                        }
                        ui.separator();
                    }

                    if self.solutions.is_empty() {
                        ui.label(muted("No stored solutions."));
                    } else {
                        let rows: Vec<_> = self
                            .solutions
                            .iter()
                            .enumerate()
                            .map(|(index, solution)| {
                                let best = solution.smallest_population();
                                let generation = best.map_or(0, |generation| generation.generation);
                                let population = best.map_or(0, |generation| generation.population);
                                (index, generation, population)
                            })
                            .collect();
                        let selected_solution = self.active_solution_index();

                        ScrollArea::vertical().show(ui, |ui| {
                            for (index, generation, population) in rows.into_iter().rev() {
                                let selected = !self.current_view_is_live()
                                    && selected_solution == Some(index);
                                let label =
                                    format!("S{}  g{}  pop {}", index + 1, generation, population);
                                if ui
                                    .selectable_label(selected, label)
                                    .on_hover_text(help::HISTORY_ENTRY_TOOLTIP)
                                    .clicked()
                                {
                                    self.select_solution(index);
                                }
                            }
                        });
                    }
                });
            });
        } else {
            self.main_panel(ui);
        }
    }

    /// The configuration panel.
    pub fn config_panel(&mut self, ui: &mut Ui) {
        let palette = Palette::new();
        let was_trimmed = self.trim_config_known_cells_to_world();
        self.trim_editor_known_cells_to_world();
        let preview = preview_config(&self.config.config);

        ui.heading(section_title("Configuration"))
            .on_hover_text(help::CONFIG_PANEL_TOOLTIP);

        ui.horizontal_wrapped(|ui| {
            let rule_badge = if preview.rule_error.is_none() {
                badge_text("RULE OK", palette.success)
            } else {
                badge_text("RULE ERR", palette.danger)
            };
            ui.label(rule_badge);

            let config_badge = if preview.config_error.is_none() {
                badge_text("READY", palette.accent)
            } else {
                badge_text("INVALID", palette.danger)
            };
            ui.label(config_badge);

            if let Some(search_order) = &preview.auto_search_order {
                ui.label(RichText::new(format!("auto {search_order}")).color(palette.subtle_text));
            }

            if self.config.config.requires_square() {
                ui.label(RichText::new("square").color(palette.subtle_text));
            }

            if !self.config.config.known_cells.is_empty() {
                ui.label(
                    RichText::new(format!("{} known", self.config.config.known_cells.len()))
                        .color(palette.subtle_text),
                );
            }
        });

        if let Some(error) = &preview.config_error {
            ui.label(RichText::new(error).color(palette.danger));
        }

        ui.separator();

        if was_trimmed > 0 {
            ui.label(
                RichText::new(format!("trimmed {was_trimmed} out-of-bounds known cell(s)"))
                    .color(palette.subtle_text),
            );
            ui.separator();
        }

        let mut should_open_known_cells_editor = false;

        ui.add_enabled_ui(
            self.mode == Mode::Configuring && self.known_cells_editor.is_none(),
            |ui| {
                ui.push_id("config_form", |ui| {
                    let AppConfig {
                        config,
                        step,
                        increase_world_size,
                        no_stop,
                    } = &mut self.config;

                    config_section(ui, "Rule", |ui| {
                        Grid::new("config_rule_section")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("rule")
                                    .on_hover_text(ConfigHelpField::RuleString.short_help());
                                ui.horizontal(|ui| {
                                    if let Some(error) = &preview.rule_error {
                                        ui.label(RichText::new("ERR").color(Color32::RED))
                                            .on_hover_text(error);
                                    } else {
                                        ui.label(RichText::new("OK").color(palette.success))
                                            .on_hover_text("The rule is valid.");
                                    }
                                    ui.add(
                                        TextEdit::singleline(&mut config.rule_str)
                                            .id_salt("rule_str_input"),
                                    );
                                });
                                ui.end_row();
                            });
                    });

                    config_section(ui, "World", |ui| {
                        Grid::new("config_world_section")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                if config.requires_square() {
                                    let mut size = config.width;

                                    ui.label("width")
                                        .on_hover_text(ConfigHelpField::Width.short_help());
                                    ui.add(
                                        DragValue::new(&mut size).speed(0.1).range(1..=u16::MAX),
                                    );
                                    ui.end_row();

                                    ui.label("height")
                                        .on_hover_text(ConfigHelpField::Height.short_help());
                                    ui.add(
                                        DragValue::new(&mut size).speed(0.1).range(1..=u16::MAX),
                                    );
                                    ui.end_row();

                                    config.width = size;
                                    config.height = size;
                                } else {
                                    ui.label("width")
                                        .on_hover_text(ConfigHelpField::Width.short_help());
                                    ui.add(
                                        DragValue::new(&mut config.width)
                                            .speed(0.1)
                                            .range(1..=u16::MAX),
                                    );
                                    ui.end_row();

                                    ui.label("height")
                                        .on_hover_text(ConfigHelpField::Height.short_help());
                                    ui.add(
                                        DragValue::new(&mut config.height)
                                            .speed(0.1)
                                            .range(1..=u16::MAX),
                                    );
                                    ui.end_row();
                                }

                                ui.label("period")
                                    .on_hover_text(ConfigHelpField::Period.short_help());
                                ui.add(
                                    DragValue::new(&mut config.period)
                                        .speed(0.1)
                                        .range(1..=u16::MAX),
                                );
                                ui.end_row();
                            });

                        if config.requires_square() {
                            ui.label(muted(
                                "Square world required by the current symmetry or transformation.",
                            ));
                        }
                    });

                    config_section(ui, "Motion && Symmetry", |ui| {
                        let translation_condition = config.symmetry.translation_condition();

                        Grid::new("config_motion_section")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("symmetry")
                                    .on_hover_text(ConfigHelpField::Symmetry.short_help());
                                ComboBox::from_id_salt("symmetry")
                                    .selected_text(config.symmetry.to_string())
                                    .show_ui(ui, |ui| {
                                        for symmetry in Symmetry::iter() {
                                            ui.selectable_value(
                                                &mut config.symmetry,
                                                symmetry,
                                                symmetry.to_string(),
                                            )
                                            .on_hover_text(symmetry.short_help());
                                        }
                                    });
                                ui.end_row();

                                ui.label("transformation")
                                    .on_hover_text(ConfigHelpField::Transformation.short_help());
                                ComboBox::from_id_salt("transformation")
                                    .selected_text(config.transformation.to_string())
                                    .show_ui(ui, |ui| {
                                        for transformation in Transformation::iter() {
                                            ui.selectable_value(
                                                &mut config.transformation,
                                                transformation,
                                                transformation.to_string(),
                                            )
                                            .on_hover_text(transformation.short_help());
                                        }
                                    });
                                ui.end_row();

                                match translation_condition {
                                    TranslationCondition::Any
                                    | TranslationCondition::NoHorizontal
                                    | TranslationCondition::NoVertical
                                    | TranslationCondition::NoTranslation => {
                                        ui.label("dx")
                                            .on_hover_text(ConfigHelpField::Dx.short_help());
                                        ui.add_enabled(
                                            matches!(
                                                translation_condition,
                                                TranslationCondition::Any
                                                    | TranslationCondition::NoVertical
                                            ),
                                            DragValue::new(&mut config.dx)
                                                .speed(0.1)
                                                .range(i16::MIN..=i16::MAX),
                                        );
                                        ui.end_row();

                                        ui.label("dy")
                                            .on_hover_text(ConfigHelpField::Dy.short_help());
                                        ui.add_enabled(
                                            matches!(
                                                translation_condition,
                                                TranslationCondition::Any
                                                    | TranslationCondition::NoHorizontal
                                            ),
                                            DragValue::new(&mut config.dy)
                                                .speed(0.1)
                                                .range(i16::MIN..=i16::MAX),
                                        );
                                        ui.end_row();
                                    }
                                    TranslationCondition::Diagonal => {
                                        let mut translation = config.dx;

                                        ui.label("dx")
                                            .on_hover_text(ConfigHelpField::Dx.short_help());
                                        ui.add(DragValue::new(&mut translation).speed(0.1));
                                        ui.end_row();

                                        ui.label("dy")
                                            .on_hover_text(ConfigHelpField::Dy.short_help());
                                        ui.add(DragValue::new(&mut translation).speed(0.1));
                                        ui.end_row();

                                        config.dx = translation;
                                        config.dy = translation;
                                    }
                                    TranslationCondition::Antidiagonal => {
                                        let mut dx: i32 = config.dx;
                                        let mut dy: i32 = config.dy;

                                        ui.label("dx")
                                            .on_hover_text(ConfigHelpField::Dx.short_help());
                                        ui.add(DragValue::new(&mut dx).speed(0.1));
                                        ui.end_row();

                                        ui.label("dy")
                                            .on_hover_text(ConfigHelpField::Dy.short_help());
                                        ui.add(DragValue::new(&mut dy).speed(0.1));
                                        ui.end_row();

                                        if config.dx == dx {
                                            config.dx = -dy;
                                            config.dy = dy;
                                        } else {
                                            config.dx = dx;
                                            config.dy = -dx;
                                        }
                                    }
                                }
                            });

                        if let Some(note) = translation_condition_note(translation_condition) {
                            ui.label(muted(note));
                        }
                    });

                    config_section(ui, "Search Strategy", |ui| {
                        Grid::new("config_search_section")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("search order")
                                    .on_hover_text(ConfigHelpField::SearchOrder.short_help());
                                ComboBox::from_id_salt("search_order")
                                    .selected_text(config.search_order.map_or_else(
                                        || "auto".to_owned(),
                                        |search_order| search_order.to_string(),
                                    ))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut config.search_order, None, "auto")
                                            .on_hover_text(
                                                "The search order is automatically determined.",
                                            );
                                        for search_order in SearchOrder::iter() {
                                            ui.selectable_value(
                                                &mut config.search_order,
                                                Some(search_order),
                                                search_order.to_string(),
                                            )
                                            .on_hover_text(search_order.short_help());
                                        }
                                    });
                                ui.end_row();

                                ui.label("new state")
                                    .on_hover_text(ConfigHelpField::NewState.short_help());
                                ComboBox::from_id_salt("new_state")
                                    .selected_text(config.new_state.to_string())
                                    .show_ui(ui, |ui| {
                                        for new_state in NewState::iter() {
                                            ui.selectable_value(
                                                &mut config.new_state,
                                                new_state,
                                                new_state.to_string(),
                                            )
                                            .on_hover_text(new_state.short_help());
                                        }
                                    });
                                ui.end_row();

                                ui.label("seed")
                                    .on_hover_text(ConfigHelpField::Seed.short_help());
                                ui.add_enabled_ui(config.new_state == NewState::Random, |ui| {
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
                                });
                                ui.end_row();

                                ui.label("phase saving")
                                    .on_hover_text(ConfigHelpField::PhaseSaving.short_help());
                                ui.checkbox(&mut config.phase_saving, "");
                                ui.end_row();
                            });

                        if let Some(search_order) = &preview.auto_search_order {
                            ui.label(muted(
                            "Auto search order follows the current world geometry and symmetry.",
                        ));
                            ui.label(
                                RichText::new(format!("Current auto choice: {search_order}"))
                                    .color(palette.subtle_text),
                            );
                        }

                        if config.new_state != NewState::Random {
                            ui.label(muted("Seed is only used when New state is random."));
                        }
                    });

                    config_section(ui, "Constraints", |ui| {
                        Grid::new("config_constraints_section")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("diagonal width")
                                    .on_hover_text(ConfigHelpField::DiagonalWidth.short_help());
                                ui.add_enabled_ui(!config.requires_no_diagonal_width(), |ui| {
                                    ui.horizontal(|ui| {
                                        let mut checked = config.diagonal_width.is_some();
                                        ui.checkbox(&mut checked, "");
                                        let mut dummy = 0;
                                        let diagonal_width = if checked {
                                            config.diagonal_width.get_or_insert_with(|| {
                                                config.width.min(config.height)
                                            })
                                        } else {
                                            config.diagonal_width = None;
                                            &mut dummy
                                        };
                                        ui.add_enabled_ui(checked, |ui| {
                                            ui.add(
                                                DragValue::new(diagonal_width).speed(0.1).range(
                                                    if checked { 1..=u16::MAX } else { 0..=0 },
                                                ),
                                            );
                                        });
                                    })
                                });
                                ui.end_row();

                                ui.label("max population")
                                    .on_hover_text(ConfigHelpField::MaxPopulation.short_help());
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

                                ui.label("reduce max").on_hover_text(
                                    ConfigHelpField::ReduceMaxPopulation.short_help(),
                                );
                                ui.checkbox(&mut config.reduce_max_population, "");
                                ui.end_row();
                            });

                        if config.requires_no_diagonal_width() {
                            ui.label(muted(
                            "Diagonal width is disabled by the current symmetry or transformation.",
                        ));
                        }
                    });

                    config_section(ui, "Known Cells", |ui| {
                        Grid::new("config_known_cells_section")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("known cells")
                                    .on_hover_text(ConfigHelpField::KnownCells.short_help());
                                ui.label(config.known_cells.len().to_string());
                                ui.end_row();
                            });

                        if ui
                            .button("Edit known cells")
                            .on_hover_text(help::KNOWN_CELLS_EDIT_TOOLTIP)
                            .clicked()
                        {
                            should_open_known_cells_editor = true;
                        }

                        ui.label(muted(
                            "Edit per-generation alive/dead pins in a dedicated grid window.",
                        ));
                    });

                    config_section(ui, "Runtime", |ui| {
                        Grid::new("config_runtime_section")
                            .striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                ui.label("increase size").on_hover_text(
                                    SearchControlHelpField::IncreaseWorldSize.short_help(),
                                );
                                ui.checkbox(increase_world_size, "");
                                ui.end_row();

                                ui.label("no stop")
                                    .on_hover_text(SearchControlHelpField::NoStop.short_help());
                                ui.checkbox(no_stop, "");
                                ui.end_row();

                                ui.label("step")
                                    .on_hover_text(SearchControlHelpField::Step.short_help());
                                ui.add(DragValue::new(step).speed(1.0));
                                ui.end_row();
                            });
                    });
                });
            },
        );

        if self.known_cells_editor.is_some() {
            ui.label(muted("Known-cells editor is open."));
        }

        if should_open_known_cells_editor {
            self.open_known_cells_editor();
        }
    }

    /// The control panel.
    pub fn control_panel(&mut self, ui: &mut Ui) {
        if self.mode == Mode::Configuring {
            if ui
                .button("New")
                .on_hover_text(help::SEARCH_ACTIONS[0].1)
                .clicked()
            {
                self.new_search();
            }

            #[cfg(feature = "save")]
            if ui
                .button("Load")
                .on_hover_text(help::SEARCH_ACTIONS[1].1)
                .clicked()
            {
                self.load_search_dialog();
            }
        } else {
            ui.add_enabled_ui(self.mode == Mode::Paused, |ui| {
                let text = match self.status {
                    Status::NotStarted => "Start",
                    Status::Running => "Resume",
                    _ => "Next",
                };

                let hover_text = match self.status {
                    Status::NotStarted => help::SEARCH_ACTIONS[2].1,
                    Status::Running => help::SEARCH_ACTIONS[2].1,
                    _ => help::SEARCH_ACTIONS[3].1,
                };

                if ui.button(text).on_hover_text(hover_text).clicked() {
                    self.start();
                }
            });

            ui.add_enabled_ui(self.mode == Mode::Running, |ui| {
                if ui
                    .button("Pause")
                    .on_hover_text(help::SEARCH_ACTIONS[4].1)
                    .clicked()
                {
                    self.pause();
                }
            });

            if ui
                .button("Stop")
                .on_hover_text(help::SEARCH_ACTIONS[6].1)
                .clicked()
            {
                self.stop();
            }

            #[cfg(feature = "save")]
            ui.add_enabled_ui(self.mode == Mode::Paused, |ui| {
                if ui
                    .button("Save")
                    .on_hover_text(help::SEARCH_ACTIONS[5].1)
                    .clicked()
                {
                    self.save_dialog();
                }
            });
        }
    }

    /// The status panel.
    pub fn status_panel(&self, ui: &mut Ui) {
        let palette = Palette::new();

        ui.horizontal(|ui| {
            if let Some(err) = &self.error {
                ui.label(RichText::new(err.to_string()).color(Color32::RED));
            } else {
                let status = if self.status == Status::Running && self.mode == Mode::Paused {
                    "Paused."
                } else {
                    self.status.short_help()
                };

                ui.label(RichText::new(status).strong().color(palette.text))
                    .on_hover_text(help::STATUS_TOOLTIP);
            }

            ui.separator();

            ui.label("View:");
            ui.label(self.current_result_source_label());

            ui.separator();

            ui.label("Solutions:")
                .on_hover_text(help::SOLUTIONS_TOOLTIP);
            ui.label(self.solutions.len().to_string());

            if let Some(population) = self.current_population() {
                ui.separator();

                ui.label("Pop:").on_hover_text(help::POPULATION_TOOLTIP);
                ui.label(population.to_string());
            }

            if self.mode == Mode::Paused {
                ui.separator();

                ui.label("Time:").on_hover_text(help::ELAPSED_TOOLTIP);
                ui.label(format!("{:?}", self.elapsed));

                ui.separator();

                ui.label("Checked:").on_hover_text(help::CHECKED_TOOLTIP);
                ui.label(self.cells_checked.to_string());
            }
        });
    }

    /// The main panel.
    pub fn main_panel(&self, ui: &mut Ui) {
        let palette = Palette::new();

        ui.group(|ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(self.current_result_source_label())
                        .strong()
                        .color(palette.accent),
                );

                if let Some(index) = self.active_solution_index()
                    && let Some(solution) = self.solutions.get(index)
                    && let Some(best) = solution.smallest_population()
                {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("best g{} pop {}", best.generation, best.population))
                            .color(palette.subtle_text),
                    );
                }
            });
            ui.separator();

            if let Some(rle) = self.current_rle() {
                ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                    ui.add(Label::new(rle_layout_job(rle)).extend());
                });

                if self.mode == Mode::Running && self.current_view_is_live() {
                    ui.ctx().request_repaint();
                }
            } else {
                ui.add_space(18.0);
                ui.label(muted("No snapshot yet."));
            }
        });
    }

    /// Ensure generation stays within bounds when workspace controls mutate it.
    pub fn clamp_workspace_generation(&mut self) {
        self.clamp_generation_to_active();
    }

    /// The on-demand help window.
    pub fn help_window(&mut self, ctx: &Context) {
        let mut open = self.chrome.show_help;

        Window::new("Help")
            .open(&mut open)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.label(muted(
                    "Short tooltips stay compact. Field reference below shows longer help sourced from factoriosrc-lib.",
                ));
                ui.separator();

                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new("Search actions").strong());
                    Grid::new("help_actions").num_columns(2).show(ui, |ui| {
                        for (label, description) in help::SEARCH_ACTIONS {
                            ui.label(RichText::new(label).strong());
                            ui.label(description);
                            ui.end_row();
                        }
                    });

                    ui.separator();
                    ui.label(RichText::new("Workspace").strong());
                    Grid::new("help_workspace").num_columns(2).show(ui, |ui| {
                        for (label, description) in help::WORKSPACE_ACTIONS {
                            ui.label(RichText::new(label).strong());
                            ui.label(description);
                            ui.end_row();
                        }
                    });

                    ui.separator();
                    ui.label(RichText::new("Known cells").strong());
                    Grid::new("help_known_cells").num_columns(2).show(ui, |ui| {
                        for (label, description) in help::KNOWN_CELLS_ACTIONS {
                            ui.label(RichText::new(label).strong());
                            ui.label(description);
                            ui.end_row();
                        }
                    });

                    ui.separator();
                    ui.label(RichText::new("Config notes").strong());
                    Grid::new("help_config").num_columns(2).show(ui, |ui| {
                        for (label, description) in help::CONFIG_NOTES {
                            ui.label(RichText::new(label).strong());
                            ui.label(description);
                            ui.end_row();
                        }
                    });

                    ui.separator();
                    ui.label(RichText::new("Config field reference").strong());
                    for field in ConfigHelpField::iter() {
                        CollapsingHeader::new(field.label()).show(ui, |ui| {
                            ui.label(normalize_help_text(field.long_help()));
                        });
                    }

                    ui.separator();
                    ui.label(RichText::new("Runtime options").strong());
                    Grid::new("help_runtime_options")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for field in [
                                SearchControlHelpField::IncreaseWorldSize,
                                SearchControlHelpField::NoStop,
                                SearchControlHelpField::Step,
                            ] {
                                ui.label(RichText::new(field.label()).strong());
                                ui.label(field.short_help());
                                ui.end_row();
                            }
                        });
                });
            });

        self.chrome.show_help = open;
    }

    /// The on-demand known-cells editor.
    pub fn known_cells_window(&mut self, ctx: &Context) {
        let Some(editor) = &mut self.known_cells_editor else {
            return;
        };

        let palette = Palette::new();
        let width = self.config.config.width;
        let height = self.config.config.height;
        let period = self.config.config.period.max(1);
        let viewport_size = ctx.content_rect().size();
        let window_max_size = vec2(
            (viewport_size.x * 0.95).max(520.0),
            (viewport_size.y * 0.9).max(360.0),
        );

        editor.generation = editor.generation.min(period - 1);

        let mut open = true;
        let mut should_apply = false;
        let mut should_cancel = false;

        Window::new("Known Cells")
            .open(&mut open)
            .resizable(true)
            .default_width(860.0)
            .default_height(620.0)
            .max_size(window_max_size)
            .show(ctx, |ui| {
                let content_max_height = (ui.available_height() - 40.0).max(220.0);

                ScrollArea::vertical()
                    .max_height(content_max_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Gen").strong().color(palette.accent));
                            ui.add(
                                Slider::new(&mut editor.generation, 0..=period - 1).step_by(1.0),
                            );
                            ui.separator();
                            ui.label(format!("{} x {}", width, height));
                            ui.separator();
                            ui.label(format!("total {}", editor.known_cells.len()));
                            ui.separator();
                            ui.label(format!(
                                "current {}",
                                editor
                                    .known_cells
                                    .iter()
                                    .filter(|cell| cell.t == editor.generation)
                                    .count()
                            ));
                            if editor.last_trimmed > 0 {
                                ui.separator();
                                ui.label(
                                    RichText::new(format!("trimmed {}", editor.last_trimmed))
                                        .color(palette.subtle_text),
                                );
                            }
                        });

                        ui.horizontal_wrapped(|ui| {
                            ui.label(muted(
                                "Click cycles ? -> o -> . -> ?  |  drag paints the same result across cells",
                            ));
                            if ui
                                .button("Clear Gen")
                                .on_hover_text(help::KNOWN_CELLS_CLEAR_GEN_TOOLTIP)
                                .clicked()
                            {
                                editor.known_cells.retain(|cell| cell.t != editor.generation);
                            }
                            if ui
                                .button("Clear All")
                                .on_hover_text(help::KNOWN_CELLS_CLEAR_ALL_TOOLTIP)
                                .clicked()
                            {
                                editor.known_cells.clear();
                            }
                        });

                        ui.separator();

                        ui.columns(2, |columns| {
                            ScrollArea::both()
                                .auto_shrink([false, false])
                                .show(&mut columns[0], |ui| {
                                    let cell_size = 22.0;
                                    let desired_size =
                                        vec2(width as f32 * cell_size, height as f32 * cell_size);
                                    let (response, painter) =
                                        ui.allocate_painter(desired_size, Sense::click_and_drag());
                                    let rect = response.rect;

                                    let hovered_cell = response.hover_pos().and_then(|position| {
                                        if !rect.contains(position) {
                                            return None;
                                        }
                                        let local = position - rect.min;
                                        let x = (local.x / cell_size).floor() as u32;
                                        let y = (local.y / cell_size).floor() as u32;
                                        (x < width && y < height)
                                            .then_some((x, y, editor.generation))
                                    });

                                    if let Some((x, y, t)) = hovered_cell {
                                        if response.drag_started() || response.clicked() {
                                            let next = cycle_known_cell(
                                                &mut editor.known_cells,
                                                x,
                                                y,
                                                t,
                                            );
                                            editor.drag_target = Some(next);
                                            editor.last_drag_cell = Some((x, y, t));
                                        } else if response.dragged()
                                            && editor.last_drag_cell != Some((x, y, t))
                                            && let Some(target) = editor.drag_target
                                        {
                                            set_known_cell(
                                                &mut editor.known_cells,
                                                x,
                                                y,
                                                t,
                                                target,
                                            );
                                            editor.last_drag_cell = Some((x, y, t));
                                        }
                                    }

                                    if !ui.input(|input| input.pointer.primary_down()) {
                                        editor.drag_target = None;
                                        editor.last_drag_cell = None;
                                    }

                                    for y in 0..height {
                                        for x in 0..width {
                                            let cell_rect = egui::Rect::from_min_size(
                                                rect.min
                                                    + vec2(
                                                        x as f32 * cell_size,
                                                        y as f32 * cell_size,
                                                    ),
                                                vec2(cell_size, cell_size),
                                            );
                                            let state = known_cell_state(
                                                &editor.known_cells,
                                                x,
                                                y,
                                                editor.generation,
                                            );
                                            let text = match state {
                                                Some(CellState::Alive) => "o".to_string(),
                                                Some(CellState::Dead) => ".".to_string(),
                                                Some(CellState::Dying(i)) => char::from_u32(
                                                    b'A' as u32 + i as u32 - 1,
                                                )
                                                .unwrap()
                                                .to_string(),
                                                None => "?".to_string(),
                                            };
                                            let (fill, text_color) = match state {
                                                Some(CellState::Alive) => {
                                                    (palette.accent_soft, palette.success)
                                                }
                                                Some(CellState::Dead | CellState::Dying(_)) => {
                                                    (palette.surface_alt, palette.dead)
                                                }
                                                None => (palette.surface, palette.subtle_text),
                                            };

                                            painter.rect_filled(cell_rect, 2.0, fill);
                                            painter.rect_stroke(
                                                cell_rect,
                                                2.0,
                                                Stroke::new(1.0, palette.surface_alt),
                                                StrokeKind::Inside,
                                            );
                                            painter.text(
                                                cell_rect.center(),
                                                egui::Align2::CENTER_CENTER,
                                                text,
                                                egui::FontId::monospace(13.0),
                                                text_color,
                                            );
                                        }
                                    }
                                });

                            let current_generation = editor.generation;
                            let mut generation_cells: Vec<_> = editor
                                .known_cells
                                .iter()
                                .copied()
                                .filter(|cell| cell.t == current_generation)
                                .collect();
                            generation_cells.sort_by_key(|cell| (cell.y, cell.x, cell.state));

                            columns[1].label(section_title("Summary"));
                            columns[1].label(muted("Current generation entries"));
                            columns[1].add_space(4.0);
                            ScrollArea::vertical().show(&mut columns[1], |ui| {
                                if generation_cells.is_empty() {
                                    ui.label(muted("No pinned cells on this generation."));
                                } else {
                                    for cell in generation_cells {
                                        let label = match cell.state {
                                            CellState::Alive => "alive".to_string(),
                                            CellState::Dead => "dead".to_string(),
                                            CellState::Dying(i) => format!("dying {i}"),
                                        };
                                        ui.horizontal(|ui| {
                                            ui.label(format!("({}, {})", cell.x, cell.y));
                                            ui.label(RichText::new(label).color(match cell.state {
                                                CellState::Alive => palette.success,
                                                CellState::Dead => palette.dead,
                                                CellState::Dying(_) => palette.subtle_text,
                                            }));
                                            if ui.add(Button::new("x")).clicked() {
                                                set_known_cell(
                                                    &mut editor.known_cells,
                                                    cell.x,
                                                    cell.y,
                                                    cell.t,
                                                    None,
                                                );
                                            }
                                        });
                                    }
                                }
                            });
                        });
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Apply")
                        .on_hover_text(help::KNOWN_CELLS_APPLY_TOOLTIP)
                        .clicked()
                    {
                        should_apply = true;
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text(help::KNOWN_CELLS_CANCEL_TOOLTIP)
                        .clicked()
                    {
                        should_cancel = true;
                    }
                });
            });

        if should_apply {
            self.apply_known_cells_editor();
            return;
        }

        if should_cancel || !open {
            self.known_cells_editor = None;
        }
    }
}
