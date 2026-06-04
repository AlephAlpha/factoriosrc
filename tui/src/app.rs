use crate::{
    args::{LoadArgs, NewArgs},
    event::TermEvent,
};
use color_eyre::Result;
use crossterm::event::KeyCode;
use factoriosrc_lib::{
    CellState, Config, KnownCell, NewState, SearchOrder, Status, Symmetry, Transformation, World,
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
    Seed,
    MaxPopulation,
    ReduceMaxPopulation,
    KnownCells,
    IncreaseWorldSize,
    NoStop,
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
            Self::Seed => "Seed",
            Self::MaxPopulation => "Max pop",
            Self::ReduceMaxPopulation => "Reduce pop",
            Self::KnownCells => "Known cells",
            Self::IncreaseWorldSize => "Increase size",
            Self::NoStop => "No stop",
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
        )
    }

    const fn is_direct_edit(self) -> bool {
        matches!(
            self,
            Self::Symmetry
                | Self::Transformation
                | Self::NewState
                | Self::SearchOrder
                | Self::ReduceMaxPopulation
                | Self::IncreaseWorldSize
                | Self::NoStop
        )
    }

    pub const fn is_button(self) -> bool {
        matches!(self, Self::Apply | Self::Cancel)
    }
}

/// State for the mark-known-cells mode.
#[derive(Debug, Clone)]
pub struct MarkState {
    pub cursor_x: u32,
    pub cursor_y: u32,
    pub known_cells: Vec<KnownCell>,
}

/// State for the configuration form.
#[derive(Debug, Clone)]
pub struct ConfigState {
    pub working_config: Config,
    pub increase_world_size: bool,
    pub no_stop: bool,
    pub fields: Vec<ConfigField>,
    pub focus_index: usize,
    pub edit_buffer: String,
    pub error: Option<String>,
    pub show_confirm: bool,
    pub scroll_offset: Cell<usize>,
}

impl ConfigState {
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
            ConfigField::Seed => cfg.seed.map_or(String::new(), |s| s.to_string()),
            ConfigField::MaxPopulation => {
                cfg.max_population.map_or(String::new(), |p| p.to_string())
            }
            ConfigField::ReduceMaxPopulation => cfg.reduce_max_population.to_string(),
            ConfigField::KnownCells => format!("{}", cfg.known_cells.len()),
            ConfigField::IncreaseWorldSize => self.increase_world_size.to_string(),
            ConfigField::NoStop => self.no_stop.to_string(),
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
                let Ok(v) = self.edit_buffer.parse::<u32>() else {
                    self.error = Some("width must be a positive integer".to_string());
                    return;
                };
                self.working_config.width = v;
            }
            ConfigField::Height => {
                let Ok(v) = self.edit_buffer.parse::<u32>() else {
                    self.error = Some("height must be a positive integer".to_string());
                    return;
                };
                self.working_config.height = v;
            }
            ConfigField::Period => {
                let Ok(v) = self.edit_buffer.parse::<u32>() else {
                    self.error = Some("period must be a positive integer".to_string());
                    return;
                };
                self.working_config.period = v;
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
    /// A path to save the application state.
    #[serde(skip)]
    pub save: Option<PathBuf>,
    /// Configuration form state.
    #[serde(skip)]
    pub config_state: Option<ConfigState>,
    /// Mark-known-cells mode state.
    #[serde(skip)]
    pub mark_state: Option<MarkState>,
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
        let save = args.save;
        let config_state = None;
        let mark_state = None;

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
            save,
            config_state,
            mark_state,
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
        }
        if status == Status::NoSolution && self.increase_world_size {
            self.world.increase_world_size();
            status = self.world.status();
        }
        if status != Status::Running && !self.no_stop || status == Status::NoSolution {
            self.pause();
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
    pub const fn next_solution(&mut self) {
        if self.solutions.is_empty() {
            return;
        }
        match self.viewing_solution {
            Some(i) if i + 1 < self.solutions.len() => {
                self.viewing_solution = Some(i + 1);
            }
            None => {
                self.generation = 0;
                self.viewing_solution = Some(0);
            }
            _ => {}
        }
    }

    /// Navigate to the previous solution.
    pub const fn previous_solution(&mut self) {
        match self.viewing_solution {
            Some(0) => {
                self.viewing_solution = None;
            }
            Some(i) => {
                self.viewing_solution = Some(i - 1);
            }
            None => {
                if let Some(last) = self.solutions.len().checked_sub(1) {
                    self.generation = 0;
                    self.viewing_solution = Some(last);
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
        let Some(ref mut state) = self.config_state else {
            return;
        };

        // Validate config.
        if let Err(e) = state.working_config.check() {
            state.error = Some(e.to_string());
            return;
        }

        // Check if search has started (inline to avoid borrow conflict).
        let search_started =
            self.world.status() != Status::NotStarted || !self.solutions.is_empty();

        if search_started && !state.show_confirm {
            state.show_confirm = true;
            return;
        }

        // Confirmed (or no progress to lose): rebuild World.
        match World::new(state.working_config.clone()) {
            Ok(world) => {
                self.world = world;
                self.increase_world_size = state.increase_world_size;
                self.no_stop = state.no_stop;
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
                    if let Some(s) = &mut self.config_state {
                        s.show_confirm = false;
                    }
                    self.apply_config();
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
                if field.is_text_field()
                    && let Some(s) = &mut self.config_state
                {
                    s.commit_edit();
                }
                if self
                    .config_state
                    .as_ref()
                    .is_some_and(|s| s.error.is_some())
                {
                    return;
                }
                if let Some(s) = &mut self.config_state {
                    s.focus_index = (s.focus_index + 1) % s.fields.len();
                    s.edit_buffer = s.field_value(s.fields[s.focus_index]);
                }
            }
            KeyCode::BackTab => {
                if field.is_text_field()
                    && let Some(s) = &mut self.config_state
                {
                    s.commit_edit();
                }
                if self
                    .config_state
                    .as_ref()
                    .is_some_and(|s| s.error.is_some())
                {
                    return;
                }
                if let Some(s) = &mut self.config_state {
                    let len = s.fields.len();
                    s.focus_index = (s.focus_index + len - 1) % len;
                    s.edit_buffer = s.field_value(s.fields[s.focus_index]);
                }
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
                } else if field == ConfigField::KnownCells {
                    // Enter mark-known-cells mode.
                    let known_cells = self
                        .config_state
                        .as_ref()
                        .map(|s| s.working_config.known_cells.clone())
                        .unwrap_or_default();
                    self.mark_state = Some(MarkState {
                        cursor_x: 0,
                        cursor_y: 0,
                        known_cells,
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
                ConfigField::ReduceMaxPopulation => {
                    if let Some(s) = &mut self.config_state {
                        s.working_config.reduce_max_population ^= true;
                    }
                }
                ConfigField::IncreaseWorldSize => {
                    if let Some(s) = &mut self.config_state {
                        s.increase_world_size ^= true;
                    }
                }
                ConfigField::NoStop => {
                    if let Some(s) = &mut self.config_state {
                        s.no_stop ^= true;
                    }
                }
                ConfigField::Apply => self.apply_config(),
                ConfigField::Cancel => self.cancel_config(),
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
        let Some(ref mut state) = self.mark_state else {
            return;
        };
        let w = self.world.config().width;
        let h = self.world.config().height;

        match key {
            KeyCode::Up => {
                if state.cursor_y > 0 {
                    state.cursor_y -= 1;
                }
            }
            KeyCode::Down => {
                if state.cursor_y + 1 < h {
                    state.cursor_y += 1;
                }
            }
            KeyCode::Left => {
                if state.cursor_x > 0 {
                    state.cursor_x -= 1;
                }
            }
            KeyCode::Right => {
                if state.cursor_x + 1 < w {
                    state.cursor_x += 1;
                }
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
                let period = self.world.config().period as i32;
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
                TermEvent::KeyPress(key) => match key {
                    KeyCode::Char('q' | 'Q') | KeyCode::Esc => {
                        self.pause();
                        self.mode = Mode::Quit;
                    }
                    KeyCode::Char(' ') | KeyCode::Enter => {
                        self.pause();
                    }
                    KeyCode::Char('=' | '+') => {
                        self.next_generation();
                    }
                    KeyCode::Char('-' | '_') => {
                        self.previous_generation();
                    }
                    KeyCode::Char('h' | 'H') => {
                        self.pause();
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
                },
                TermEvent::Resize => {}
            },
            Mode::Paused => match event {
                TermEvent::KeyPress(key) => match key {
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
                    KeyCode::Char('=' | '+') => {
                        self.next_generation();
                    }
                    KeyCode::Char('-' | '_') => {
                        self.previous_generation();
                    }
                    KeyCode::Char('h' | 'H') => {
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
                },
                TermEvent::Resize => {}
            },
            Mode::Config => match event {
                TermEvent::KeyPress(key) => {
                    self.handle_config_event(key);
                }
                TermEvent::Resize => {}
            },
            Mode::MarkKnown => match event {
                TermEvent::KeyPress(key) => {
                    self.handle_mark_event(key);
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
                    _ => {}
                },
                TermEvent::Resize => {}
            },
        }
    }
}
