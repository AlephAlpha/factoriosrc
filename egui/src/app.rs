use crate::search::{Event, Message, SearchThread};
use crate::theme;
use documented::{Documented, DocumentedFields};
use eframe::{App as EframeApp, Frame, glow::Context as GlowContext};
use egui::{CentralPanel, Context, Panel, Ui};
use factoriosrc_lib::{Config, Status};
#[cfg(feature = "save")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "save")]
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Configuration of the application.
#[derive(Debug, Clone, PartialEq, Eq, Documented, DocumentedFields)]
#[cfg_attr(feature = "save", derive(Serialize, Deserialize))]
pub struct AppConfig {
    /// The configuration of the search.
    pub config: Config,

    /// Display/update interval in search steps.
    pub step: usize,

    /// Restart with a slightly larger world after an exhausted search.
    ///
    /// If the diagonal width exists and is smaller than the width, it will be increased by 1.
    /// Otherwise, if the height is greater than the width, the width will increased by 1.
    /// Otherwise, the height will increased by 1.
    ///
    /// If the configuration requires a square world, both the width and the height will be
    /// increased by 1.
    ///
    /// When the world size is increased, the search will be restarted, and the current search
    /// status will be lost.
    pub increase_world_size: bool,

    /// Continue searching after the first solution.
    ///
    /// The search will continue until no more solutions exist, or paused by the user.
    pub no_stop: bool,
}

/// Application modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The user is configuring the application.
    #[default]
    Configuring,
    /// The search is running.
    Running,
    /// The search is not started yet, finished, or paused by the user.
    Paused,
}

/// Visibility state for optional UI chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChromeState {
    /// Whether the details panel is visible.
    pub show_details: bool,
    /// Whether the help window is visible.
    pub show_help: bool,
}

/// The main struct of the application.
#[derive(Debug, DocumentedFields)]
pub struct App {
    /// The configuration.
    pub config: AppConfig,
    /// Current mode of the application.
    pub mode: Mode,
    /// Visibility state for optional panels and overlays.
    pub chrome: ChromeState,
    /// A thread to run the search algorithm.
    pub search: Option<SearchThread>,
    /// The current generation to display.
    pub generation: i32,
    /// The current partial result.
    pub view: Vec<String>,
    /// Populations of each generation of the current partial result.
    pub populations: Vec<usize>,
    /// Found solutions.
    pub solutions: Vec<String>,
    /// An error message to display.
    pub error: Option<String>,
    /// Search status.
    pub status: Status,
    /// Time elapsed since the start of the search.
    pub elapsed: Duration,
    /// A proxy metric for search progress.
    pub cells_checked: usize,
    /// A path to save the search state.
    #[cfg(feature = "save")]
    pub save: Option<PathBuf>,
}

impl Default for App {
    fn default() -> Self {
        let config = AppConfig {
            config: Config::new("R3,C2,S2,B3,N+", 16, 16, 1),
            step: 100_000,
            increase_world_size: false,
            no_stop: false,
        };
        Self {
            config,
            mode: Mode::Configuring,
            chrome: ChromeState::default(),
            search: None,
            generation: 0,
            view: Vec::new(),
            populations: Vec::new(),
            solutions: Vec::new(),
            error: None,
            status: Status::NotStarted,
            elapsed: Duration::default(),
            cells_checked: 0,
            #[cfg(feature = "save")]
            save: None,
        }
    }
}

impl EframeApp for App {
    fn logic(&mut self, _ctx: &Context, _frame: &mut Frame) {
        self.receive();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        theme::apply_theme(ui.ctx());

        Panel::top("command_bar").show(ui, |ui| {
            self.command_bar(ui);
        });

        Panel::left("setup_sidebar").show(ui, |ui| {
            self.setup_panel(ui);
        });

        if self.chrome.show_details {
            Panel::right("inspector_panel").show(ui, |ui| {
                self.inspector_panel(ui);
            });
        }

        Panel::bottom("status_panel").show(ui, |ui| {
            self.status_panel(ui);
        });

        CentralPanel::default().show(ui, |ui| {
            self.workspace_panel(ui);
        });

        self.help_window(ui.ctx());
    }

    fn on_exit(&mut self, _gl: Option<&GlowContext>) {
        if self.mode == Mode::Running || self.mode == Mode::Paused {
            self.stop();
        }
    }
}

impl App {
    /// Create a new search thread from the current configuration.
    pub fn new_search(&mut self) {
        assert!(self.mode == Mode::Configuring);
        let mut config = self.config.clone();
        if let Err(e) = config.config.check() {
            self.error = Some(e.to_string());
        } else {
            self.error = None;
            self.view.clear();
            self.populations.clear();
            self.solutions.clear();
            self.search = Some(SearchThread::new(config));
            self.mode = Mode::Paused;
        }
    }

    /// Create a new search thread from a file.
    #[cfg(feature = "save")]
    pub fn load_search(&mut self, path: impl AsRef<Path>) {
        assert!(self.mode == Mode::Configuring);

        if let Ok(string) = std::fs::read_to_string(path) {
            if let Ok((search, config)) = SearchThread::load(&string) {
                self.config = config;
                self.error = None;
                self.view.clear();
                self.populations.clear();
                self.solutions.clear();
                self.search = Some(search);
                self.mode = Mode::Paused;
            } else {
                self.error = Some("Failed to load the search state.".to_string());
            }
        } else {
            self.error = Some("Failed to open the save file.".to_string());
        }
    }

    /// Start or resume the search.
    pub fn start(&mut self) {
        assert!(self.mode == Mode::Running || self.mode == Mode::Paused);

        if let Some(search) = &mut self.search {
            search.send(Event::Start);
        }
    }

    /// Pause the search.
    pub fn pause(&mut self) {
        assert!(self.mode == Mode::Running || self.mode == Mode::Paused);

        if let Some(search) = &mut self.search {
            search.send(Event::Pause);
        }
    }

    /// Stop the search and reset the application to the configuring mode.
    pub fn stop(&mut self) {
        assert!(self.mode == Mode::Running || self.mode == Mode::Paused);

        if let Some(search) = self.search.take() {
            search.send(Event::Stop);
            search.join();
        }

        self.mode = Mode::Configuring;
        self.status = Status::NotStarted;
        self.generation = 0;
        self.cells_checked = 0;
    }

    /// Send an event to the search thread to save the current state.
    #[cfg(feature = "save")]
    pub fn save(&mut self) {
        assert!(self.mode == Mode::Running || self.mode == Mode::Paused);

        if let Some(search) = &mut self.search {
            search.send(Event::Save);
        }
    }

    /// Handle a message from the search thread and update the application state.
    pub fn handle(&mut self, message: Message) {
        match message {
            Message::Snapshot(snapshot) => {
                self.status = snapshot.status;
                self.view = snapshot
                    .generations
                    .iter()
                    .map(|generation| generation.rle.clone())
                    .collect();
                self.populations = snapshot
                    .generations
                    .iter()
                    .map(|generation| generation.population)
                    .collect();
                self.elapsed = snapshot.elapsed;
                self.cells_checked = snapshot.cells_checked;
                if snapshot.status == Status::Solved {
                    // Choose the generation with the smallest population.
                    if let Some(solution) = snapshot.smallest_population() {
                        self.solutions.push(solution.rle.clone());
                    }
                }

                if snapshot.running {
                    self.mode = Mode::Running;
                } else {
                    log::debug!("Search paused.");
                    self.mode = Mode::Paused;
                }
            }
            #[cfg(feature = "save")]
            Message::Save(string) => {
                if let Some(path) = &self.save.take() {
                    if let Err(e) = std::fs::write(path, string) {
                        log::error!("Failed to save the search state: {e}");
                        self.error = Some("Failed to save the search state.".to_string());
                    } else {
                        log::info!("Search state saved to {}", path.display());
                    }
                }
            }
        }
    }

    /// Receive and handle a message from the search thread.
    pub fn receive(&mut self) {
        if let Some(search) = &mut self.search
            && let Some(message) = search.try_recv()
        {
            self.handle(message);
        }
    }

    /// A short label for the current mode.
    pub const fn mode_label(&self) -> &'static str {
        match self.mode {
            Mode::Configuring => "Setup",
            Mode::Running => "Running",
            Mode::Paused => "Paused",
        }
    }

    /// Return the population on the currently displayed generation.
    pub fn current_population(&self) -> Option<usize> {
        self.populations.get(self.generation as usize).copied()
    }

    /// Return the RLE currently shown in the workspace.
    pub fn current_rle(&self) -> Option<&str> {
        match self.mode {
            Mode::Configuring => self.solutions.last().map(String::as_str),
            _ => self.view.get(self.generation as usize).map(String::as_str),
        }
    }

    /// Copy the currently displayed RLE text to the clipboard.
    pub fn copy_current_rle(&self, ctx: &Context) {
        if let Some(rle) = self.current_rle() {
            ctx.copy_text(rle.to_owned());
        }
    }
}
