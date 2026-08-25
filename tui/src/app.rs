use crate::{
    args::{LoadArgs, NewArgs},
    event::{MouseAction, MouseInput, TermEvent},
    layout::{
        centered_popup_rect, point_in_rect, split_grid_scrollable_area, split_main_layout,
        split_mark_layout, split_vertical_scrollable_area,
    },
};
use color_eyre::Result;
use crossterm::event::KeyCode;
#[cfg(not(target_arch = "wasm32"))]
use factoriosrc_lib::save_generation;
use factoriosrc_lib::{
    CellState, Config, ExportFields, KnownCell, NewState, SearchOrder, Status, Symmetry, Template,
    Transformation, World,
};
use ratatui::{
    layout::{Margin, Rect},
    text::Text,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    path::PathBuf,
    time::{Duration, Instant},
};

const DEFAULT_STEP: usize = 100_000;

/// Application modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The search is running.
    Running,
    /// The search is not started yet, finished, or paused by the user.
    #[default]
    Paused,
    /// Configuration form is open.
    Config,
    /// Mark-known-cells mode (sub-mode of Config).
    MarkKnown,
    /// Ask the user to confirm the quit.
    Quit,
    /// Display the usage.
    Usage,
}

/// Fields in the configuration form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    RuleString,
    Width,
    Height,
    Period,
    Dx,
    Dy,
    DiagonalWidth,
    Symmetry,
    Transformation,
    SearchOrder,
    NewState,
    PhaseSaving,
    Lookahead,
    Backjump,
    Nogood,
    Seed,
    MaxPopulation,
    ReduceMaxPopulation,
    KnownCells,
    IncreaseWorldSize,
    NoStop,
    ExportResults,
    Apply,
    Cancel,
}

impl ConfigField {
    fn all() -> Vec<Self> {
        vec![
            Self::RuleString,
            Self::Width,
            Self::Height,
            Self::Period,
            Self::Dx,
            Self::Dy,
            Self::DiagonalWidth,
            Self::Symmetry,
            Self::Transformation,
            Self::SearchOrder,
            Self::NewState,
            Self::Seed,
            Self::MaxPopulation,
            Self::ReduceMaxPopulation,
            Self::KnownCells,
            Self::IncreaseWorldSize,
            Self::NoStop,
            Self::ExportResults,
            Self::PhaseSaving,
            Self::Lookahead,
            Self::Backjump,
            Self::Nogood,
            Self::Apply,
            Self::Cancel,
        ]
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::RuleString => "Rule",
            Self::Width => "Width",
            Self::Height => "Height",
            Self::Period => "Period",
            Self::Dx => "DX",
            Self::Dy => "DY",
            Self::DiagonalWidth => "Diagonal",
            Self::Symmetry => "Symmetry",
            Self::Transformation => "Transform",
            Self::SearchOrder => "Order",
            Self::NewState => "New state",
            Self::PhaseSaving => "Phase saving",
            Self::Lookahead => "Lookahead",
            Self::Backjump => "Backjump",
            Self::Nogood => "Nogood",
            Self::Seed => "Seed",
            Self::MaxPopulation => "Max pop",
            Self::ReduceMaxPopulation => "Reduce pop",
            Self::KnownCells => "Known cells",
            Self::IncreaseWorldSize => "Increase size",
            Self::NoStop => "No stop",
            Self::ExportResults => "Export results",
            Self::Apply | Self::Cancel => "",
        }
    }

    pub const fn is_text_field(self) -> bool {
        matches!(
            self,
            Self::RuleString
                | Self::Width
                | Self::Height
                | Self::Period
                | Self::Dx
                | Self::Dy
                | Self::DiagonalWidth
                | Self::Seed
                | Self::MaxPopulation
                | Self::ExportResults
        )
    }

    const fn is_direct_edit(self) -> bool {
        matches!(
            self,
            Self::Symmetry
                | Self::Transformation
                | Self::NewState
                | Self::PhaseSaving
                | Self::Lookahead
                | Self::Backjump
                | Self::Nogood
                | Self::SearchOrder
                | Self::ReduceMaxPopulation
                | Self::IncreaseWorldSize
                | Self::NoStop
        )
    }

    pub const fn is_button(self) -> bool {
        matches!(self, Self::Apply | Self::Cancel)
    }

    /// Whether this field belongs to the experimental group.
    pub const fn is_experimental(self) -> bool {
        matches!(
            self,
            Self::PhaseSaving | Self::Lookahead | Self::Backjump | Self::Nogood
        )
    }
}

/// A viewport offset for oversized content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewportOffset {
    pub x: u16,
    pub y: u16,
}

/// Ephemeral TUI state that should not be serialized into save files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiState {
    pub terminal_area: Rect,
    pub search_viewport: ViewportOffset,
    pub help_scroll: u16,
}

/// State for the mark-known-cells mode.
#[derive(Debug, Clone)]
pub struct MarkState {
    pub cursor_x: u32,
    pub cursor_y: u32,
    pub known_cells: Vec<KnownCell>,
    pub viewport: ViewportOffset,
}

/// State for the configuration form.
#[derive(Debug, Clone)]
pub struct ConfigState {
    pub working_config: Config,
    pub increase_world_size: bool,
    pub no_stop: bool,
    pub export: Option<String>,
    pub fields: Vec<ConfigField>,
    pub focus_index: usize,
    pub edit_buffer: String,
    pub error: Option<String>,
    pub show_confirm: bool,
    pub scroll_offset: Cell<usize>,
}

impl ConfigState {
    fn trim_known_cells_to_world(&mut self) {
        let width = self.working_config.width;
        let height = self.working_config.height;
        let period = self.working_config.period;
        self.working_config
            .known_cells
            .retain(|cell| cell.x < width && cell.y < height && cell.t < period);
    }

    /// Get the string representation of a config field's current value.
    pub fn field_value(&self, field: ConfigField) -> String {
        let cfg = &self.working_config;
        match field {
            ConfigField::RuleString => cfg.rule_str.clone(),
            ConfigField::Width => cfg.width.to_string(),
            ConfigField::Height => cfg.height.to_string(),
            ConfigField::Period => cfg.period.to_string(),
            ConfigField::Dx => cfg.dx.to_string(),
            ConfigField::Dy => cfg.dy.to_string(),
            ConfigField::DiagonalWidth => {
                cfg.diagonal_width.map_or(String::new(), |w| w.to_string())
            }
            ConfigField::Symmetry => cfg.symmetry.to_string(),
            ConfigField::Transformation => cfg.transformation.to_string(),
            ConfigField::SearchOrder => cfg.search_order.map_or(String::new(), |o| o.to_string()),
            ConfigField::NewState => cfg.new_state.to_string(),
            ConfigField::PhaseSaving => cfg.phase_saving.to_string(),
            ConfigField::Lookahead => cfg.lookahead.to_string(),
            ConfigField::Backjump => cfg.backjump.to_string(),
            ConfigField::Nogood => cfg.nogood.to_string(),
            ConfigField::Seed => cfg.seed.map_or(String::new(), |s| s.to_string()),
            ConfigField::MaxPopulation => {
                cfg.max_population.map_or(String::new(), |p| p.to_string())
            }
            ConfigField::ReduceMaxPopulation => cfg.reduce_max_population.to_string(),
            ConfigField::KnownCells => format!("{}", cfg.known_cells.len()),
            ConfigField::IncreaseWorldSize => self.increase_world_size.to_string(),
            ConfigField::NoStop => self.no_stop.to_string(),
            ConfigField::ExportResults => self.export.clone().unwrap_or_default(),
            ConfigField::Apply | ConfigField::Cancel => String::new(),
        }
    }

    /// Commit the edit buffer for the currently focused text field.
    fn commit_edit(&mut self) {
        let field = self.fields[self.focus_index];
        match field {
            ConfigField::RuleString => {
                self.working_config.rule_str = self.edit_buffer.clone();
            }
            ConfigField::Width => {
                if let Ok(v) = self.edit_buffer.parse::<u32>()
                    && v > 0
                {
                    self.working_config.width = v;
                } else {
                    self.error = Some("width must be a positive integer".to_string());
                    return;
                }
            }
            ConfigField::Height => {
                if let Ok(v) = self.edit_buffer.parse::<u32>()
                    && v > 0
                {
                    self.working_config.height = v;
                } else {
                    self.error = Some("height must be a positive integer".to_string());
                    return;
                }
            }
            ConfigField::Period => {
                if let Ok(v) = self.edit_buffer.parse::<u32>()
                    && v > 0
                {
                    self.working_config.period = v;
                } else {
                    self.error = Some("period must be a positive integer".to_string());
                    return;
                }
            }
            ConfigField::Dx => {
                let Ok(v) = self.edit_buffer.parse::<i32>() else {
                    self.error = Some("dx must be an integer".to_string());
                    return;
                };
                self.working_config.dx = v;
            }
            ConfigField::Dy => {
                let Ok(v) = self.edit_buffer.parse::<i32>() else {
                    self.error = Some("dy must be an integer".to_string());
                    return;
                };
                self.working_config.dy = v;
            }
            ConfigField::DiagonalWidth => {
                if self.edit_buffer.is_empty() {
                    self.working_config.diagonal_width = None;
                } else {
                    let Ok(v) = self.edit_buffer.parse::<u32>() else {
                        self.error = Some("diagonal width must be a positive integer".to_string());
                        return;
                    };
                    self.working_config.diagonal_width = Some(v);
                }
            }
            ConfigField::Seed => {
                if self.edit_buffer.is_empty() {
                    self.working_config.seed = None;
                } else {
                    let Ok(v) = self.edit_buffer.parse::<u64>() else {
                        self.error = Some("seed must be a non-negative integer".to_string());
                        return;
                    };
                    self.working_config.seed = Some(v);
                }
            }
            ConfigField::MaxPopulation => {
                if self.edit_buffer.is_empty() {
                    self.working_config.max_population = None;
                } else {
                    let Ok(v) = self.edit_buffer.parse::<usize>() else {
                        self.error =
                            Some("max population must be a non-negative integer".to_string());
                        return;
                    };
                    self.working_config.max_population = Some(v);
                }
            }
            ConfigField::ExportResults => {
                self.export = Some(self.edit_buffer.clone());
            }
            _ => {}
        }

        self.error = None;
    }

    /// Cycle an enum/option field forward or backward.
    fn cycle_field(&mut self, field: ConfigField, forward: bool) {
        match field {
            ConfigField::Symmetry => {
                const VARIANTS: &[Symmetry] = &[
                    Symmetry::C1,
                    Symmetry::C2,
                    Symmetry::C4,
                    Symmetry::D2H,
                    Symmetry::D2V,
                    Symmetry::D2D,
                    Symmetry::D2A,
                    Symmetry::D4O,
                    Symmetry::D4X,
                    Symmetry::D8,
                ];
                let cur = &self.working_config.symmetry;
                let pos = VARIANTS.iter().position(|v| v == cur).unwrap_or(0);
                let next = if forward {
                    (pos + 1) % VARIANTS.len()
                } else {
                    (pos + VARIANTS.len() - 1) % VARIANTS.len()
                };
                self.working_config.symmetry = VARIANTS[next];
            }
            ConfigField::Transformation => {
                const VARIANTS: &[Transformation] = &[
                    Transformation::R0,
                    Transformation::R1,
                    Transformation::R2,
                    Transformation::R3,
                    Transformation::S0,
                    Transformation::S1,
                    Transformation::S2,
                    Transformation::S3,
                ];
                let cur = &self.working_config.transformation;
                let pos = VARIANTS.iter().position(|v| v == cur).unwrap_or(0);
                let next = if forward {
                    (pos + 1) % VARIANTS.len()
                } else {
                    (pos + VARIANTS.len() - 1) % VARIANTS.len()
                };
                self.working_config.transformation = VARIANTS[next];
            }
            ConfigField::NewState => {
                const VARIANTS: &[NewState] = &[NewState::Dead, NewState::Alive, NewState::Random];
                let cur = &self.working_config.new_state;
                let pos = VARIANTS.iter().position(|v| v == cur).unwrap_or(0);
                let next = if forward {
                    (pos + 1) % VARIANTS.len()
                } else {
                    (pos + VARIANTS.len() - 1) % VARIANTS.len()
                };
                self.working_config.new_state = VARIANTS[next];
            }
            ConfigField::SearchOrder => {
                const VARIANTS: &[Option<SearchOrder>] = &[
                    None,
                    Some(SearchOrder::RowFirst),
                    Some(SearchOrder::ColumnFirst),
                    Some(SearchOrder::Diagonal),
                ];
                let cur = self.working_config.search_order;
                let pos = VARIANTS.iter().position(|v| *v == cur).unwrap_or(0);
                let next = if forward {
                    (pos + 1) % VARIANTS.len()
                } else {
                    (pos + VARIANTS.len() - 1) % VARIANTS.len()
                };
                self.working_config.search_order = VARIANTS[next];
            }
            ConfigField::ReduceMaxPopulation => {
                self.working_config.reduce_max_population =
                    !self.working_config.reduce_max_population;
            }
            ConfigField::PhaseSaving => {
                self.working_config.phase_saving = !self.working_config.phase_saving;
            }
            ConfigField::Lookahead => {
                self.working_config.lookahead = !self.working_config.lookahead;
            }
            ConfigField::Backjump => {
                self.working_config.backjump = !self.working_config.backjump;
            }
            ConfigField::Nogood => {
                self.working_config.nogood = !self.working_config.nogood;
                // The nogood database enables backjumping in `Config::check`;
                // keep the form in sync when it is toggled on.
                if self.working_config.nogood {
                    self.working_config.backjump = true;
                }
            }
            ConfigField::IncreaseWorldSize => {
                self.increase_world_size = !self.increase_world_size;
            }
            ConfigField::NoStop => {
                self.no_stop = !self.no_stop;
            }
            ConfigField::DiagonalWidth => {
                match self.working_config.diagonal_width {
                    Some(_) => self.working_config.diagonal_width = None,
                    None => self.working_config.diagonal_width = Some(0),
                }
                self.edit_buffer = self.field_value(field);
            }
            ConfigField::Seed => {
                match self.working_config.seed {
                    Some(_) => self.working_config.seed = None,
                    None => self.working_config.seed = Some(0),
                }
                self.edit_buffer = self.field_value(field);
            }
            ConfigField::MaxPopulation => {
                match self.working_config.max_population {
                    Some(_) => self.working_config.max_population = None,
                    None => self.working_config.max_population = Some(0),
                }
                self.edit_buffer = self.field_value(field);
            }
            _ => {}
        }
    }
}

/// Application state.
#[derive(Debug, Serialize, Deserialize)]
pub struct App {
    /// The main struct of the search algorithm.
    pub world: World,
    /// Number of steps between each display of the current partial result.
    pub step: usize,
    /// Current mode of the application.
    #[serde(skip)]
    pub mode: Mode,
    /// Generation to display.
    pub generation: i32,
    /// Start time of the current search.
    #[serde(skip)]
    pub start: Option<Instant>,
    /// Time elapsed since the start of the search.
    pub elapsed: Duration,
    /// All found solutions, each with one RLE string per generation.
    #[serde(default)]
    pub solutions: Vec<Vec<String>>,
    /// Index of the solution being viewed, if any.
    #[serde(skip)]
    pub viewing_solution: Option<usize>,
    /// Whether to set the clipboard content on the next iteration.
    #[serde(skip)]
    pub should_copy: bool,
    /// Whether the application should quit.
    #[serde(skip)]
    pub should_quit: bool,
    /// Whether to increase the world size when the search fails.
    pub increase_world_size: bool,
    /// Whether not to stop the search when a solution is found.
    pub no_stop: bool,
    /// A file-name template for exporting found solutions to RLE files.
    ///
    /// [`None`] or an empty string means that result export is disabled.
    #[serde(default)]
    pub export: Option<String>,
    /// The most recent result-export message, shown in the status bar.
    #[serde(skip)]
    pub export_message: Option<String>,
    /// A path to save the application state.
    #[serde(skip)]
    pub save: Option<PathBuf>,
    /// Configuration form state.
    #[serde(skip)]
    pub config_state: Option<ConfigState>,
    /// Mark-known-cells mode state.
    #[serde(skip)]
    pub mark_state: Option<MarkState>,
    /// Ephemeral UI state used for layout and future pointer interactions.
    #[serde(skip)]
    pub ui_state: UiState,
}

impl App {
    /// Create a new [`App`] from the command line arguments.
    pub fn new(args: NewArgs) -> Result<Self> {
        let needs_config = args.config.width == 0 || args.config.height == 0;

        let mut world_cfg = args.config;
        if needs_config {
            if world_cfg.width == 0 {
                world_cfg.width = 16;
            }
            if world_cfg.height == 0 {
                world_cfg.height = 16;
            }
        }

        let world = World::new(world_cfg)?;
        let step = args.step.unwrap_or(DEFAULT_STEP);
        let mode = Mode::Paused;
        let generation = 0;
        let start = None;
        let elapsed = Duration::from_secs(0);
        let solutions = Vec::new();
        let viewing_solution = None;
        let should_copy = false;
        let should_quit = false;
        let increase_world_size = args.increase_world_size;
        let no_stop = args.no_stop;
        let export = args.export;
        let export_message = None;
        let save = args.save;
        let config_state = None;
        let mark_state = None;
        let ui_state = UiState::default();

        let mut app = Self {
            world,
            step,
            mode,
            generation,
            start,
            elapsed,
            solutions,
            viewing_solution,
            should_copy,
            should_quit,
            increase_world_size,
            no_stop,
            export,
            export_message,
            save,
            config_state,
            mark_state,
            ui_state,
        };

        if needs_config {
            app.enter_config_mode();
        }

        Ok(app)
    }

    /// Load the [`App`] from the path given in the command line arguments.
    pub fn load(args: LoadArgs) -> Result<Self> {
        let path = args.load;
        let json = std::fs::read_to_string(path)?;
        let mut app: Self = serde_json::from_str(&json)?;
        app.save = args.save;

        // Apply command-line overrides.
        if let Some(step) = args.step {
            app.step = step;
        }
        if let Some(no_stop) = args.no_stop {
            app.no_stop = no_stop;
        }
        if let Some(increase_world_size) = args.increase_world_size {
            app.increase_world_size = increase_world_size;
        }
        if let Some(export) = args.export {
            app.export = Some(export);
        }

        Ok(app)
    }

    /// Save the application state.
    pub fn save(&self) -> Result<()> {
        if let Some(save) = &self.save {
            let json = serde_json::to_string(self)?;
            std::fs::write(save, json)?;
        }
        Ok(())
    }

    /// Synchronize transient terminal metrics used by the TUI.
    pub const fn sync_terminal_area(&mut self, area: Rect) {
        self.ui_state.terminal_area = area;
    }

    fn current_view_content_size(&self) -> (u16, u16) {
        if self.viewing_solution.is_some() {
            let rle = self.current_rle();
            let height = rle.lines().count().max(1).min(u16::MAX as usize) as u16;
            let width = rle
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0)
                .min(u16::MAX as usize) as u16;
            (width, height)
        } else {
            (
                self.world.config().width as u16,
                self.world.config().height as u16,
            )
        }
    }

    fn pan_search_viewport(&mut self, dx: i16, dy: i16) {
        let (content_width, content_height) = self.current_view_content_size();
        self.ui_state.search_viewport.x = self
            .ui_state
            .search_viewport
            .x
            .saturating_add_signed(dx)
            .min(content_width.saturating_sub(1));
        self.ui_state.search_viewport.y = self
            .ui_state
            .search_viewport
            .y
            .saturating_add_signed(dy)
            .min(content_height.saturating_sub(1));
    }

    fn reset_search_viewport(&mut self) {
        self.ui_state.search_viewport = ViewportOffset::default();
    }

    const fn scroll_help(&mut self, delta: i16) {
        self.ui_state.help_scroll = self.ui_state.help_scroll.saturating_add_signed(delta);
    }

    const fn reset_help_scroll(&mut self) {
        self.ui_state.help_scroll = 0;
    }

    fn search_page_step(&self) -> i16 {
        self.ui_state.terminal_area.height.saturating_sub(4).max(1) as i16
    }

    fn config_page_step(&self) -> usize {
        self.ui_state.terminal_area.height.saturating_sub(4).max(1) as usize
    }

    fn mark_viewport_span(&self) -> ViewportOffset {
        let area = self.ui_state.terminal_area;
        let main_height = area.height.saturating_sub(2);
        let visible_height = main_height.saturating_sub(1).max(1);
        let visible_width = area.width.max(1);
        ViewportOffset {
            x: visible_width,
            y: visible_height,
        }
    }

    fn sync_mark_viewport_to_cursor(&mut self) {
        let span = self.mark_viewport_span();
        let Some(state) = &mut self.mark_state else {
            return;
        };

        let cursor_x = state.cursor_x as u16;
        let cursor_y = state.cursor_y as u16;

        if cursor_x < state.viewport.x {
            state.viewport.x = cursor_x;
        } else if cursor_x >= state.viewport.x.saturating_add(span.x) {
            state.viewport.x = cursor_x.saturating_add(1).saturating_sub(span.x);
        }

        if cursor_y < state.viewport.y {
            state.viewport.y = cursor_y;
        } else if cursor_y >= state.viewport.y.saturating_add(span.y) {
            state.viewport.y = cursor_y.saturating_add(1).saturating_sub(span.y);
        }
    }

    fn move_config_focus(&mut self, delta: isize) {
        let field = self
            .config_state
            .as_ref()
            .map(|state| state.fields[state.focus_index]);
        let Some(field) = field else {
            return;
        };

        if field.is_text_field()
            && let Some(state) = &mut self.config_state
        {
            state.commit_edit();
        }
        if self
            .config_state
            .as_ref()
            .is_some_and(|state| state.error.is_some())
        {
            return;
        }

        if let Some(state) = &mut self.config_state {
            let len = state.fields.len() as isize;
            let next = (state.focus_index as isize + delta).rem_euclid(len) as usize;
            state.focus_index = next;
            state.edit_buffer = state.field_value(state.fields[state.focus_index]);
        }
    }

    fn config_field_line_indices(&self) -> Option<Vec<usize>> {
        let state = self.config_state.as_ref()?;
        let mut lines = 0usize;
        let mut indices = vec![0usize; state.fields.len()];
        let mut prev_experimental = false;

        for (i, field) in state.fields.iter().enumerate() {
            if field.is_button() {
                if matches!(field, ConfigField::Apply) {
                    lines += 1;
                }
                indices[i] = lines;
                lines += 1;
                if matches!(field, ConfigField::Cancel) {
                    lines += 1;
                }
                prev_experimental = false;
            } else {
                // Keep in sync with the caption inserted in `render_config_form`:
                // a blank line plus the group title before the first experimental
                // field of a contiguous run.
                let experimental = field.is_experimental();
                if experimental && !prev_experimental {
                    lines += 2;
                }
                indices[i] = lines;
                lines += 1;
                prev_experimental = experimental;
            }
        }

        Some(indices)
    }

    fn handle_config_mouse_event(&mut self, mouse: MouseInput) {
        if self.config_state.is_none() {
            return;
        }
        if self
            .config_state
            .as_ref()
            .is_some_and(|state| state.show_confirm)
        {
            if matches!(mouse.action, MouseAction::LeftDown) {
                let rect = centered_popup_rect(
                    self.ui_state.terminal_area,
                    &Text::from(
                        "Changing the configuration will reset all search progress.\n\nAre you sure? ([y]/[n])",
                    ),
                );
                if point_in_rect(mouse.column, mouse.row, rect) {
                    self.apply_config_with_confirmation(true);
                } else if let Some(state) = &mut self.config_state {
                    state.show_confirm = false;
                }
            }
            return;
        }

        let area = self.ui_state.terminal_area;
        let inner = area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });
        let field_lines = match self.config_field_line_indices() {
            Some(indices) => indices,
            None => return,
        };
        let scroll_offset = self
            .config_state
            .as_ref()
            .map(|state| state.scroll_offset.get())
            .unwrap_or(0);
        let total_lines = field_lines.last().copied().unwrap_or(0).saturating_add(3);
        let scroll = split_vertical_scrollable_area(inner, total_lines as u16);

        match mouse.action {
            MouseAction::ScrollUp => {
                if let Some(state) = &mut self.config_state {
                    state.scroll_offset.set(scroll_offset.saturating_sub(1));
                }
            }
            MouseAction::ScrollDown => {
                let viewport = scroll.viewport.height.max(1) as usize;
                let max_offset = total_lines.saturating_sub(viewport);
                if let Some(state) = &mut self.config_state {
                    state
                        .scroll_offset
                        .set(scroll_offset.saturating_add(1).min(max_offset));
                }
            }
            MouseAction::LeftDown | MouseAction::LeftDrag => {
                if !point_in_rect(mouse.column, mouse.row, scroll.viewport) {
                    return;
                }
                let line = scroll_offset + mouse.row.saturating_sub(scroll.viewport.y) as usize;
                let Some((focus_index, _)) = field_lines
                    .iter()
                    .enumerate()
                    .find(|(_, field_line)| **field_line == line)
                else {
                    return;
                };

                if let Some(state) = &mut self.config_state {
                    state.focus_index = focus_index;
                    let field = state.fields[focus_index];
                    state.edit_buffer = state.field_value(field);
                }

                let field = self
                    .config_state
                    .as_ref()
                    .map(|state| state.fields[focus_index])
                    .expect("config state should exist after setting focus");
                match field {
                    ConfigField::Apply => self.apply_config(),
                    ConfigField::Cancel => self.cancel_config(),
                    ConfigField::KnownCells => {
                        self.handle_config_event(KeyCode::Enter);
                    }
                    _ if field.is_direct_edit() => {
                        if let Some(state) = &mut self.config_state {
                            state.cycle_field(field, true);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_mark_mouse_event(&mut self, mouse: MouseInput) {
        let Some(_) = self.mark_state else {
            return;
        };

        let area = self.ui_state.terminal_area;
        let layout = split_mark_layout(area);
        let (width, height) = self
            .config_state
            .as_ref()
            .map(|state| {
                (
                    state.working_config.width as u16,
                    state.working_config.height as u16,
                )
            })
            .unwrap_or_else(|| {
                (
                    self.world.config().width as u16,
                    self.world.config().height as u16,
                )
            });
        let scroll = split_grid_scrollable_area(layout.main, width, height);

        match mouse.action {
            MouseAction::ScrollUp => {
                if let Some(state) = &mut self.mark_state {
                    state.viewport.y = state.viewport.y.saturating_sub(1);
                }
            }
            MouseAction::ScrollDown => {
                if let Some(state) = &mut self.mark_state {
                    state.viewport.y = state.viewport.y.saturating_add(1);
                }
            }
            MouseAction::LeftDown | MouseAction::LeftDrag => {
                if !point_in_rect(mouse.column, mouse.row, scroll.body) {
                    return;
                }
                let (viewport_x, viewport_y) = self
                    .mark_state
                    .as_ref()
                    .map(|state| (state.viewport.x, state.viewport.y))
                    .unwrap_or_default();
                let world_x = viewport_x + mouse.column.saturating_sub(scroll.body.x);
                let world_y = viewport_y + mouse.row.saturating_sub(scroll.body.y);
                if world_x >= width || world_y >= height {
                    return;
                }

                if let Some(state) = &mut self.mark_state {
                    state.cursor_x = world_x as u32;
                    state.cursor_y = world_y as u32;
                }

                if matches!(mouse.action, MouseAction::LeftDown) {
                    let coord = (world_x as u32, world_y as u32, self.generation as u32);
                    if let Some(state) = &mut self.mark_state {
                        let pos = state
                            .known_cells
                            .iter()
                            .position(|k| (k.x, k.y, k.t) == coord);
                        match pos {
                            Some(i) if state.known_cells[i].state == CellState::Alive => {
                                state.known_cells[i].state = CellState::Dead;
                            }
                            Some(i) => {
                                state.known_cells.remove(i);
                            }
                            None => {
                                state.known_cells.push(KnownCell::new(
                                    world_x as u32,
                                    world_y as u32,
                                    self.generation as u32,
                                    CellState::Alive,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    const fn handle_usage_mouse_event(&mut self, mouse: MouseInput) {
        if matches!(mouse.action, MouseAction::ScrollUp) {
            self.scroll_help(-1);
        } else if matches!(mouse.action, MouseAction::ScrollDown) {
            self.scroll_help(1);
        } else if matches!(mouse.action, MouseAction::LeftDown) {
            self.mode = Mode::Paused;
        }
    }

    fn handle_quit_mouse_event(&mut self, mouse: MouseInput) {
        if !matches!(mouse.action, MouseAction::LeftDown) {
            return;
        }
        let rect = centered_popup_rect(
            split_main_layout(self.ui_state.terminal_area).main,
            &Text::from("Are you sure you want to quit? ([y]/[n])"),
        );
        if point_in_rect(mouse.column, mouse.row, rect) {
            self.should_quit = true;
        } else {
            self.mode = Mode::Paused;
        }
    }

    fn handle_search_view_mouse_event(&mut self, mouse: MouseInput) {
        match mouse.action {
            MouseAction::ScrollUp => self.pan_search_viewport(0, -1),
            MouseAction::ScrollDown => self.pan_search_viewport(0, 1),
            _ => {}
        }
    }

    fn handle_search_navigation_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Char('=' | '+') => {
                self.next_generation();
                true
            }
            KeyCode::Char('-' | '_') => {
                self.previous_generation();
                true
            }
            KeyCode::PageUp => {
                self.pan_search_viewport(0, -self.search_page_step());
                true
            }
            KeyCode::PageDown => {
                self.pan_search_viewport(0, self.search_page_step());
                true
            }
            KeyCode::Up => {
                self.pan_search_viewport(0, -1);
                true
            }
            KeyCode::Down => {
                self.pan_search_viewport(0, 1);
                true
            }
            KeyCode::Left => {
                self.pan_search_viewport(-1, 0);
                true
            }
            KeyCode::Right => {
                self.pan_search_viewport(1, 0);
                true
            }
            _ => false,
        }
    }

    fn handle_running_key(&mut self, key: KeyCode) {
        if self.handle_search_navigation_key(key) {
            return;
        }

        match key {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => {
                self.pause();
                self.mode = Mode::Quit;
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                self.pause();
            }
            KeyCode::Char('h' | 'H') => {
                self.pause();
                self.reset_help_scroll();
                self.mode = Mode::Usage;
            }
            KeyCode::Char('o' | 'O') => {
                self.pause();
                self.enter_config_mode();
            }
            KeyCode::Char('c' | 'C') => {
                self.should_copy = true;
            }
            _ => {}
        }
    }

    fn handle_paused_key(&mut self, key: KeyCode) {
        if self.handle_search_navigation_key(key) {
            return;
        }

        match key {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => {
                if self.viewing_solution.is_some() {
                    self.viewing_solution = None;
                } else {
                    self.mode = Mode::Quit;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                self.viewing_solution = None;
                self.start();
            }
            KeyCode::Char('h' | 'H') => {
                self.reset_help_scroll();
                self.mode = Mode::Usage;
            }
            KeyCode::Char('o' | 'O') => {
                self.enter_config_mode();
            }
            KeyCode::Char('n' | 'N') => {
                self.next_solution();
            }
            KeyCode::Char('p' | 'P') => {
                self.previous_solution();
            }
            KeyCode::Char('c' | 'C') => {
                self.should_copy = true;
            }
            _ => {}
        }
    }

    /// Display the next generation.
    ///
    /// If the current generation is the last one, do nothing.
    pub const fn next_generation(&mut self) {
        let period = self.world.config().period as i32;

        if self.generation < period - 1 {
            self.generation += 1;
        }
    }

    /// Display the previous generation.
    ///
    /// If the current generation is the first one, do nothing.
    pub const fn previous_generation(&mut self) {
        if self.generation > 0 {
            self.generation -= 1;
        }
    }

    /// Start or resume the search.
    fn start(&mut self) {
        if self.mode == Mode::Paused {
            self.start = Some(Instant::now());
            self.mode = Mode::Running;
        }
    }

    /// Pause the search.
    fn pause(&mut self) {
        if self.mode == Mode::Running {
            self.elapsed += self.start.take().unwrap().elapsed();
            self.mode = Mode::Paused;
        }
    }

    /// Run the search for the given number of steps.
    pub fn step(&mut self) {
        let mut status = self.world.search(self.step);
        if status == Status::Solved {
            let period = self.world.config().period;
            let gen_rles: Vec<String> = (0..period)
                .map(|t| self.world.rle(t as i32, true))
                .collect();
            self.solutions.push(gen_rles);
            self.export_solution();
        }
        if status == Status::NoSolution && self.increase_world_size {
            self.world.increase_world_size();
            status = self.world.status();
        }
        if status != Status::Running && !self.no_stop || status == Status::NoSolution {
            self.pause();
        }
    }

    /// Save the last found solution to files, if result export is enabled.
    ///
    /// Each generation of the solution is written to its own file, using the
    /// export template in [`App::export`]. The index of the solution is
    /// 1-based and equals the number of stored solutions. The outcome is
    /// reported in [`App::export_message`].
    fn export_solution(&mut self) {
        let Some(template_str) = self.export.as_deref().filter(|s| !s.is_empty()) else {
            return;
        };
        let Ok(template) = Template::parse(template_str) else {
            self.export_message = Some(format!("Invalid export template: {template_str}"));
            return;
        };
        let config = self.world.config().clone();
        let index = self.solutions.len();
        let mut paths = Vec::new();
        let mut error = None;
        for t in 0..config.period {
            let fields =
                ExportFields::from_config(&config, index, t, self.world.population(t as i32));
            let rle = self.world.rle(t as i32, true);
            match save_generation(&template, &fields, &rle) {
                Ok(path) => paths.push(path),
                Err(e) => error = Some(e),
            }
        }
        if let Some(error) = error {
            self.export_message = Some(format!("Failed to export solution {index}: {error}"));
        } else if let Some(first) = paths.first() {
            let count = paths.len();
            self.export_message = Some(if count > 1 {
                format!(
                    "Exported solution {index} ({count} files) to {}",
                    first.display()
                )
            } else {
                format!("Exported solution {index} to {}", first.display())
            });
        }
    }

    /// Print the last found solution in RLE format.
    ///
    /// This function is called when exiting the application.
    pub fn print_solution(&self) {
        if let Some(last) = self.solutions.last()
            && let Some(rle) = last.first()
        {
            println!("{rle}");
        }
    }

    /// Get the RLE string for the current view (world or stored solution).
    pub fn current_rle(&self) -> String {
        self.viewing_solution.map_or_else(
            || self.world.rle(self.generation, true),
            |i| {
                let g = (self.generation as usize).min(self.solutions[i].len().saturating_sub(1));
                self.solutions[i][g].clone()
            },
        )
    }

    /// Navigate to the next solution.
    pub fn next_solution(&mut self) {
        if self.solutions.is_empty() {
            return;
        }
        match self.viewing_solution {
            Some(i) if i + 1 < self.solutions.len() => {
                self.viewing_solution = Some(i + 1);
                self.reset_search_viewport();
            }
            None => {
                self.generation = 0;
                self.viewing_solution = Some(0);
                self.reset_search_viewport();
            }
            _ => {}
        }
    }

    /// Navigate to the previous solution.
    pub fn previous_solution(&mut self) {
        match self.viewing_solution {
            Some(0) => {
                self.viewing_solution = None;
                self.reset_search_viewport();
            }
            Some(i) => {
                self.viewing_solution = Some(i - 1);
                self.reset_search_viewport();
            }
            None => {
                if let Some(last) = self.solutions.len().checked_sub(1) {
                    self.generation = 0;
                    self.viewing_solution = Some(last);
                    self.reset_search_viewport();
                }
            }
        }
    }

    // ── Config form methods ──

    /// Enter the configuration form mode.
    pub fn enter_config_mode(&mut self) {
        let working_config = self.world.config().clone();
        let fields = ConfigField::all();
        let mut state = ConfigState {
            working_config,
            increase_world_size: self.increase_world_size,
            no_stop: self.no_stop,
            export: self.export.clone(),
            fields,
            focus_index: 0,
            edit_buffer: String::new(),
            error: None,
            show_confirm: false,
            scroll_offset: Cell::new(0),
        };
        state.edit_buffer = state.field_value(ConfigField::RuleString);
        self.config_state = Some(state);
        self.mode = Mode::Config;
    }

    /// Cancel config changes and return to paused mode.
    pub fn cancel_config(&mut self) {
        self.config_state = None;
        self.mode = Mode::Paused;
    }

    /// Apply the working configuration, rebuilding the World.
    pub fn apply_config(&mut self) {
        self.apply_config_with_confirmation(false);
    }

    fn apply_config_with_confirmation(&mut self, confirmed: bool) {
        let Some(ref mut state) = self.config_state else {
            return;
        };

        if state.working_config.width > 0
            && state.working_config.height > 0
            && state.working_config.period > 0
        {
            state.trim_known_cells_to_world();
        }

        // Validate config.
        if let Err(e) = state.working_config.check() {
            state.error = Some(e.to_string());
            return;
        }

        // Check if search has started (inline to avoid borrow conflict).
        let search_started =
            self.world.status() != Status::NotStarted || !self.solutions.is_empty();

        if search_started && !confirmed && !state.show_confirm {
            state.show_confirm = true;
            return;
        }

        // Confirmed (or no progress to lose): rebuild World.
        match World::new(state.working_config.clone()) {
            Ok(world) => {
                self.world = world;
                self.increase_world_size = state.increase_world_size;
                self.no_stop = state.no_stop;
                self.export = state.export.clone();
                self.generation = 0;
                self.solutions.clear();
                self.elapsed = Duration::default();
                self.start = None;
                self.config_state = None;
                self.mode = Mode::Paused;
            }
            Err(e) => {
                state.error = Some(e.to_string());
            }
        }
    }

    /// Handle a key event in config mode.
    fn handle_config_event(&mut self, key: KeyCode) {
        // Confirm dialog state.
        if self.config_state.as_ref().is_some_and(|s| s.show_confirm) {
            match key {
                KeyCode::Char('y' | 'Y') => {
                    self.apply_config_with_confirmation(true);
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                    if let Some(s) = &mut self.config_state {
                        s.show_confirm = false;
                    }
                }
                _ => {}
            }
            return;
        }

        let field = self.config_state.as_ref().map(|s| s.fields[s.focus_index]);
        let Some(field) = field else { return };

        match key {
            KeyCode::Tab => {
                self.move_config_focus(1);
            }
            KeyCode::Down => {
                self.move_config_focus(1);
            }
            KeyCode::BackTab => {
                self.move_config_focus(-1);
            }
            KeyCode::Up => {
                self.move_config_focus(-1);
            }
            KeyCode::PageDown => {
                self.move_config_focus(self.config_page_step() as isize);
            }
            KeyCode::PageUp => {
                self.move_config_focus(-(self.config_page_step() as isize));
            }
            KeyCode::Enter => {
                if field.is_button() {
                    match field {
                        ConfigField::Apply => self.apply_config(),
                        ConfigField::Cancel => self.cancel_config(),
                        _ => unreachable!(),
                    }
                } else if field.is_text_field()
                    && let Some(s) = &mut self.config_state
                {
                    s.commit_edit();
                } else if field.is_direct_edit()
                    && let Some(s) = &mut self.config_state
                {
                    s.cycle_field(field, true);
                } else if field == ConfigField::KnownCells {
                    let Some(config_state) = self.config_state.as_mut() else {
                        return;
                    };
                    if config_state.working_config.width == 0
                        || config_state.working_config.height == 0
                        || config_state.working_config.period == 0
                    {
                        config_state.error = Some(
                            "known cells editor requires positive width, height, and period"
                                .to_string(),
                        );
                        return;
                    }
                    config_state.trim_known_cells_to_world();
                    self.generation = self
                        .generation
                        .min(config_state.working_config.period.saturating_sub(1) as i32);
                    // Enter mark-known-cells mode.
                    let known_cells = config_state.working_config.known_cells.clone();
                    self.mark_state = Some(MarkState {
                        cursor_x: 0,
                        cursor_y: 0,
                        known_cells,
                        viewport: ViewportOffset::default(),
                    });
                    self.mode = Mode::MarkKnown;
                }
            }
            KeyCode::Esc => self.cancel_config(),
            KeyCode::Left => {
                if field.is_direct_edit()
                    && let Some(s) = &mut self.config_state
                {
                    s.cycle_field(field, false);
                }
            }
            KeyCode::Right => {
                if field.is_direct_edit()
                    && let Some(s) = &mut self.config_state
                {
                    s.cycle_field(field, true);
                }
            }
            KeyCode::Char(' ') => match field {
                ConfigField::Apply => self.apply_config(),
                ConfigField::Cancel => self.cancel_config(),
                _ if field.is_direct_edit()
                    && let Some(s) = &mut self.config_state =>
                {
                    s.cycle_field(field, true);
                }
                _ if field.is_text_field()
                    && let Some(s) = &mut self.config_state =>
                {
                    s.edit_buffer.push(' ');
                    s.commit_edit();
                }
                _ => {}
            },
            KeyCode::Backspace => {
                if field.is_text_field()
                    && let Some(s) = &mut self.config_state
                {
                    s.edit_buffer.pop();
                    if field != ConfigField::RuleString {
                        s.commit_edit();
                    }
                }
            }
            KeyCode::Char(c) if c.is_ascii() && field.is_text_field() => {
                if let Some(s) = &mut self.config_state {
                    s.edit_buffer.push(c);
                    if field != ConfigField::RuleString {
                        s.commit_edit();
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle a key event in mark-known-cells mode.
    fn handle_mark_event(&mut self, key: KeyCode) {
        let (w, h, period) = self
            .config_state
            .as_ref()
            .map(|state| {
                (
                    state.working_config.width,
                    state.working_config.height,
                    state.working_config.period,
                )
            })
            .unwrap_or_else(|| {
                (
                    self.world.config().width,
                    self.world.config().height,
                    self.world.config().period,
                )
            });
        let page_step = self.mark_viewport_span().y.max(1) as u32;
        let Some(ref mut state) = self.mark_state else {
            return;
        };

        match key {
            KeyCode::Up => {
                if state.cursor_y > 0 {
                    state.cursor_y -= 1;
                }
                self.sync_mark_viewport_to_cursor();
            }
            KeyCode::Down => {
                if state.cursor_y + 1 < h {
                    state.cursor_y += 1;
                }
                self.sync_mark_viewport_to_cursor();
            }
            KeyCode::Left => {
                if state.cursor_x > 0 {
                    state.cursor_x -= 1;
                }
                self.sync_mark_viewport_to_cursor();
            }
            KeyCode::Right => {
                if state.cursor_x + 1 < w {
                    state.cursor_x += 1;
                }
                self.sync_mark_viewport_to_cursor();
            }
            KeyCode::PageUp => {
                state.cursor_y = state.cursor_y.saturating_sub(page_step);
                self.sync_mark_viewport_to_cursor();
            }
            KeyCode::PageDown => {
                state.cursor_y = state
                    .cursor_y
                    .saturating_add(page_step)
                    .min(h.saturating_sub(1));
                self.sync_mark_viewport_to_cursor();
            }
            KeyCode::Char(' ') => {
                let coord = (state.cursor_x, state.cursor_y, self.generation as u32);
                let pos = state
                    .known_cells
                    .iter()
                    .position(|k| (k.x, k.y, k.t) == coord);
                match pos {
                    Some(i) if state.known_cells[i].state == CellState::Alive => {
                        state.known_cells[i].state = CellState::Dead;
                    }
                    Some(i) => {
                        state.known_cells.remove(i);
                    }
                    None => {
                        state.known_cells.push(KnownCell::new(
                            state.cursor_x,
                            state.cursor_y,
                            self.generation as u32,
                            CellState::Alive,
                        ));
                    }
                }
            }
            KeyCode::Char('a' | 'A') => {
                let coord = (state.cursor_x, state.cursor_y, self.generation as u32);
                state.known_cells.retain(|k| (k.x, k.y, k.t) != coord);
                state.known_cells.push(KnownCell::new(
                    state.cursor_x,
                    state.cursor_y,
                    self.generation as u32,
                    CellState::Alive,
                ));
            }
            KeyCode::Char('d' | 'D') => {
                let coord = (state.cursor_x, state.cursor_y, self.generation as u32);
                state.known_cells.retain(|k| (k.x, k.y, k.t) != coord);
                state.known_cells.push(KnownCell::new(
                    state.cursor_x,
                    state.cursor_y,
                    self.generation as u32,
                    CellState::Dead,
                ));
            }
            KeyCode::Char('u' | 'U' | 'x' | 'X') => {
                let coord = (state.cursor_x, state.cursor_y, self.generation as u32);
                state.known_cells.retain(|k| (k.x, k.y, k.t) != coord);
            }
            KeyCode::Char('=' | '+') => {
                let period = period as i32;
                if self.generation < period - 1 {
                    self.generation += 1;
                }
            }
            KeyCode::Char('-' | '_') => {
                if self.generation > 0 {
                    self.generation -= 1;
                }
            }
            KeyCode::Enter => {
                // Save and return to config form.
                if let Some(mark) = self.mark_state.take()
                    && let Some(ref mut s) = self.config_state
                {
                    s.working_config.known_cells = mark.known_cells;
                    s.trim_known_cells_to_world();
                }
                self.mode = Mode::Config;
                // Refresh edit buffer if KnownCells field is focused.
                if let Some(ref mut s) = self.config_state
                    && s.fields.get(s.focus_index) == Some(&ConfigField::KnownCells)
                {
                    s.edit_buffer = s.field_value(ConfigField::KnownCells);
                }
            }
            KeyCode::Esc => {
                // Discard and return to config form.
                self.mark_state = None;
                self.mode = Mode::Config;
            }
            _ => {}
        }
    }

    /// Update the application state according to the given event.
    pub fn update(&mut self, event: TermEvent) {
        match self.mode {
            Mode::Running => match event {
                TermEvent::KeyPress(key) => self.handle_running_key(key),
                TermEvent::Mouse(mouse) => self.handle_search_view_mouse_event(mouse),
                TermEvent::Resize => {}
            },
            Mode::Paused => match event {
                TermEvent::KeyPress(key) => self.handle_paused_key(key),
                TermEvent::Mouse(mouse) => self.handle_search_view_mouse_event(mouse),
                TermEvent::Resize => {}
            },
            Mode::Config => match event {
                TermEvent::KeyPress(key) => {
                    self.handle_config_event(key);
                }
                TermEvent::Mouse(mouse) => {
                    self.handle_config_mouse_event(mouse);
                }
                TermEvent::Resize => {}
            },
            Mode::MarkKnown => match event {
                TermEvent::KeyPress(key) => {
                    self.handle_mark_event(key);
                }
                TermEvent::Mouse(mouse) => {
                    self.handle_mark_mouse_event(mouse);
                }
                TermEvent::Resize => {}
            },
            Mode::Quit => match event {
                TermEvent::KeyPress(key) => match key {
                    KeyCode::Char('y' | 'Y') => {
                        self.should_quit = true;
                    }
                    KeyCode::Char('n' | 'N') => {
                        self.mode = Mode::Paused;
                    }
                    _ => {}
                },
                TermEvent::Mouse(mouse) => {
                    self.handle_quit_mouse_event(mouse);
                }
                TermEvent::Resize => {}
            },
            Mode::Usage => match event {
                TermEvent::KeyPress(key) => match key {
                    KeyCode::Char('q' | 'Q') | KeyCode::Esc => {
                        self.mode = Mode::Quit;
                    }
                    KeyCode::Char('h' | 'H' | ' ') | KeyCode::Enter => {
                        self.mode = Mode::Paused;
                    }
                    KeyCode::Up => {
                        self.scroll_help(-1);
                    }
                    KeyCode::Down => {
                        self.scroll_help(1);
                    }
                    KeyCode::PageUp => {
                        self.scroll_help(-8);
                    }
                    KeyCode::PageDown => {
                        self.scroll_help(8);
                    }
                    KeyCode::Home => {
                        self.ui_state.help_scroll = 0;
                    }
                    _ => {}
                },
                TermEvent::Mouse(mouse) => {
                    self.handle_usage_mouse_event(mouse);
                }
                TermEvent::Resize => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{NewArgs, OutputFormat};

    fn test_config_state() -> ConfigState {
        ConfigState {
            working_config: Config::new("B3/S23", 8, 6, 1),
            increase_world_size: false,
            no_stop: false,
            export: None,
            fields: ConfigField::all(),
            focus_index: 0,
            edit_buffer: String::new(),
            error: None,
            show_confirm: false,
            scroll_offset: Cell::new(0),
        }
    }

    fn test_app() -> App {
        App::new(NewArgs {
            config: Config::new("B3/S23", 8, 6, 1),
            step: None,
            increase_world_size: false,
            no_stop: false,
            export: None,
            no_tui: false,
            save: None,
            known_cells: Vec::new(),
            known_cells_file: None,
            format: OutputFormat::Rle,
            generation: 0,
        })
        .expect("test app should build")
    }

    #[test]
    fn cycle_field_toggles_boolean_config_fields() {
        let mut state = test_config_state();

        state.cycle_field(ConfigField::ReduceMaxPopulation, true);
        assert!(state.working_config.reduce_max_population);

        state.cycle_field(ConfigField::IncreaseWorldSize, true);
        assert!(state.increase_world_size);

        state.cycle_field(ConfigField::NoStop, true);
        assert!(state.no_stop);
    }

    #[test]
    fn config_field_order_groups_experimental_fields() {
        let fields = ConfigField::all();
        let position = |field| {
            fields
                .iter()
                .position(|f| *f == field)
                .expect("field exists")
        };

        // Seed belongs to the new-state strategy and comes before the group.
        assert!(position(ConfigField::Seed) < position(ConfigField::PhaseSaving));
        // The experimental toggles form a contiguous run.
        assert_eq!(
            position(ConfigField::Lookahead),
            position(ConfigField::PhaseSaving) + 1
        );
        assert_eq!(
            position(ConfigField::Backjump),
            position(ConfigField::PhaseSaving) + 2
        );
        assert_eq!(
            position(ConfigField::Nogood),
            position(ConfigField::Backjump) + 1
        );
        // The group sits at the end of the form, right before the buttons, so
        // no other field can be mistaken for part of it.
        assert!(position(ConfigField::ExportResults) < position(ConfigField::PhaseSaving));
        assert!(position(ConfigField::Nogood) < position(ConfigField::Apply));
    }

    #[test]
    fn config_field_line_indices_skip_experimental_caption() {
        let mut app = test_app();
        app.enter_config_mode();

        let field_positions = {
            let fields = &app.config_state.as_ref().expect("config state").fields;
            let position = |field| {
                fields
                    .iter()
                    .position(|f| *f == field)
                    .expect("field exists")
            };
            (
                position(ConfigField::ExportResults),
                position(ConfigField::PhaseSaving),
                position(ConfigField::Lookahead),
                position(ConfigField::Backjump),
                position(ConfigField::Nogood),
            )
        };
        let (export, phase_saving, lookahead, backjump, nogood) = field_positions;

        // The caption (a blank line plus the title) is inserted between them:
        // two extra lines plus the field's own line.
        let before = app.config_field_line_indices().expect("indices");
        assert_eq!(before[phase_saving], before[export] + 3);

        // The rest of the run is contiguous: no extra lines inside the group.
        assert_eq!(before[lookahead], before[phase_saving] + 1);
        assert_eq!(before[backjump], before[lookahead] + 1);
        assert_eq!(before[nogood], before[backjump] + 1);

        // No caption appears when the experimental fields are removed.
        if let Some(state) = app.config_state.as_mut() {
            state.fields.retain(|field| !field.is_experimental());
        }
        let fields = app
            .config_state
            .as_ref()
            .expect("config state")
            .fields
            .clone();
        let without = app.config_field_line_indices().expect("indices");
        for (i, pair) in without.windows(2).enumerate() {
            // Buttons keep their own blank-line spacing.
            if !fields[i].is_button() && !fields[i + 1].is_button() {
                assert_eq!(pair[1] - pair[0], 1);
            }
        }
    }

    #[test]
    fn enter_cycles_direct_edit_boolean_fields_in_config_mode() {
        let mut app = test_app();
        app.enter_config_mode();

        let reduce_index = app
            .config_state
            .as_ref()
            .and_then(|state| {
                state
                    .fields
                    .iter()
                    .position(|field| *field == ConfigField::ReduceMaxPopulation)
            })
            .expect("reduce pop field should exist");

        if let Some(state) = &mut app.config_state {
            state.focus_index = reduce_index;
        }

        app.handle_config_event(KeyCode::Enter);

        let state = app
            .config_state
            .as_ref()
            .expect("config state should remain active");
        assert!(state.working_config.reduce_max_population);
    }

    #[test]
    fn export_solution_writes_files() {
        let mut app = test_app();
        let dir = std::env::temp_dir().join(format!(
            "factoriosrc-tui-export-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        app.export = Some(format!("{}/{{rule}}_{{index:04}}.rle", dir.display()));
        app.solutions.push(vec![app.world.rle(0, true)]);
        app.export_solution();

        let message = app.export_message.as_deref().expect("export message");
        assert!(message.starts_with("Exported solution 1"));
        assert!(dir.join("B3_S23_0001.rle").is_file());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn export_solution_reports_invalid_template() {
        let mut app = test_app();
        app.export = Some("{unknown}".to_string());
        app.solutions.push(vec![app.world.rle(0, true)]);
        app.export_solution();
        assert!(
            app.export_message
                .as_deref()
                .expect("export message")
                .contains("Invalid export template")
        );
    }

    #[test]
    fn app_serialization_round_trip_preserves_export() {
        let mut app = test_app();
        app.export = Some("results/{index}.rle".to_string());
        let json = serde_json::to_string(&app).unwrap();
        let loaded: App = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.export, app.export);
        assert!(loaded.export_message.is_none());
    }

    #[test]
    fn app_deserializes_old_save_without_export_field() {
        let app = test_app();
        let json = serde_json::to_string(&app).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("export")
            .expect("serialized app should contain export");
        let json = serde_json::to_string(&value).unwrap();
        let loaded: App = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.export, None);
    }

    #[test]
    fn confirming_config_apply_rebuilds_world() {
        let mut app = test_app();
        app.solutions.push(vec![app.world.rle(0, true)]);
        app.enter_config_mode();

        if let Some(state) = &mut app.config_state {
            state.working_config.width = 9;
            state.show_confirm = true;
        }

        app.handle_config_event(KeyCode::Char('y'));

        assert_eq!(app.world.config().width, 9);
        assert!(app.config_state.is_none());
        assert_eq!(app.mode, Mode::Paused);
    }

    #[test]
    fn resizing_config_does_not_eagerly_prune_known_cells() {
        let mut state = test_config_state();
        state.working_config.known_cells = vec![
            KnownCell::new(1, 1, 0, CellState::Alive),
            KnownCell::new(6, 1, 0, CellState::Alive),
            KnownCell::new(1, 1, 2, CellState::Dead),
        ];

        state.focus_index = state
            .fields
            .iter()
            .position(|field| *field == ConfigField::Width)
            .expect("width field should exist");
        state.edit_buffer = "4".to_string();
        state.commit_edit();

        assert_eq!(state.working_config.known_cells.len(), 3);

        state.focus_index = state
            .fields
            .iter()
            .position(|field| *field == ConfigField::Period)
            .expect("period field should exist");
        state.edit_buffer = "1".to_string();
        state.commit_edit();

        assert_eq!(state.working_config.known_cells.len(), 3);
    }

    #[test]
    fn applying_config_prunes_out_of_bounds_known_cells() {
        let mut app = test_app();
        app.enter_config_mode();

        if let Some(state) = &mut app.config_state {
            state.working_config.width = 4;
            state.working_config.period = 1;
            state.working_config.known_cells = vec![
                KnownCell::new(1, 1, 0, CellState::Alive),
                KnownCell::new(6, 1, 0, CellState::Alive),
                KnownCell::new(1, 1, 2, CellState::Dead),
            ];
        }

        app.apply_config_with_confirmation(true);

        assert_eq!(app.world.config().known_cells.len(), 1);
        assert_eq!(app.world.config().known_cells[0].x, 1);
        assert_eq!(app.world.config().known_cells[0].t, 0);
    }

    #[test]
    fn commit_edit_rejects_zero_for_positive_dimensions() {
        let mut state = test_config_state();

        state.focus_index = state
            .fields
            .iter()
            .position(|field| *field == ConfigField::Width)
            .expect("width field should exist");
        state.edit_buffer = "0".to_string();
        state.commit_edit();
        assert_eq!(
            state.error.as_deref(),
            Some("width must be a positive integer")
        );

        state.error = None;
        state.focus_index = state
            .fields
            .iter()
            .position(|field| *field == ConfigField::Height)
            .expect("height field should exist");
        state.edit_buffer = "0".to_string();
        state.commit_edit();
        assert_eq!(
            state.error.as_deref(),
            Some("height must be a positive integer")
        );

        state.error = None;
        state.focus_index = state
            .fields
            .iter()
            .position(|field| *field == ConfigField::Period)
            .expect("period field should exist");
        state.edit_buffer = "0".to_string();
        state.commit_edit();
        assert_eq!(
            state.error.as_deref(),
            Some("period must be a positive integer")
        );
    }

    #[test]
    fn mouse_click_focuses_direct_edit_config_field() {
        let mut app = test_app();
        app.enter_config_mode();
        app.sync_terminal_area(Rect::new(0, 0, 80, 24));

        let reduce_line = {
            let state = app
                .config_state
                .as_ref()
                .expect("config state should exist");
            let reduce_index = state
                .fields
                .iter()
                .position(|field| *field == ConfigField::ReduceMaxPopulation)
                .expect("reduce pop field should exist");
            app.config_field_line_indices().expect("indices")[reduce_index]
        };

        app.update(TermEvent::Mouse(MouseInput {
            action: MouseAction::LeftDown,
            column: 3,
            row: reduce_line as u16 + 1,
        }));

        let state = app
            .config_state
            .as_ref()
            .expect("config state should exist");
        assert_eq!(
            state.fields[state.focus_index],
            ConfigField::ReduceMaxPopulation
        );
        assert!(state.working_config.reduce_max_population);
    }

    #[test]
    fn mouse_click_toggles_mark_cell() {
        let mut app = test_app();
        app.enter_config_mode();
        if let Some(state) = &mut app.config_state {
            state.focus_index = state
                .fields
                .iter()
                .position(|field| *field == ConfigField::KnownCells)
                .expect("known cells field should exist");
        }
        app.handle_config_event(KeyCode::Enter);
        app.sync_terminal_area(Rect::new(0, 0, 80, 24));

        app.update(TermEvent::Mouse(MouseInput {
            action: MouseAction::LeftDown,
            column: 1,
            row: 2,
        }));

        let state = app.mark_state.as_ref().expect("mark state should exist");
        assert_eq!(state.cursor_x, 1);
        assert_eq!(state.cursor_y, 0);
        assert_eq!(state.known_cells.len(), 1);
        assert_eq!(state.known_cells[0].state, CellState::Alive);
    }
}
