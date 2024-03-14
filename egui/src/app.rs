use crate::search::{Event, Message, SearchThread};
use documented::{Documented, DocumentedFields};
use eframe::{glow::Context as GlowContext, App as EframeApp, Frame};
use egui::{text::LayoutJob, CentralPanel, Context, SidePanel, TopBottomPanel};
use factoriosrc_lib::{Config, Status};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// Configuration of the application.
#[derive(Debug, Clone, PartialEq, Eq, Documented, DocumentedFields, Serialize, Deserialize)]
pub struct AppConfig {
    /// The configuration of the search.
    pub config: Config,

    /// Number of steps between each display of the current partial result.
    pub step: usize,

    /// Whether to increase the world size when the search fails.
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

    /// Do not stop the search when a solution is found.
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

/// The main struct of the application.
#[derive(Debug, DocumentedFields)]
pub struct App {
    /// The configuration.
    pub config: AppConfig,
    /// Current mode of the application.
    pub mode: Mode,
    /// A thread to run the search algorithm.
    pub search: Option<SearchThread>,
    /// The current generation to display.
    pub generation: i32,
    /// The current partial result.
    pub view: Vec<LayoutJob>,
    /// Populations of each generation of the current partial result.
    pub populations: Vec<usize>,
    /// Found solutions.
    pub solutions: Vec<LayoutJob>,
    /// An error message to display.
    pub error: Option<String>,
    /// Search status.
    pub status: Status,
    /// Time elapsed since the start of the search.
    pub elapsed: Duration,
    /// A path to save the search state.
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
            search: None,
            generation: 0,
            view: Vec::new(),
            populations: Vec::new(),
            solutions: Vec::new(),
            error: None,
            status: Status::NotStarted,
            elapsed: Duration::default(),
            save: None,
        }
    }
}

impl EframeApp for App {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        SidePanel::left("config_panel").show(ctx, |ui| {
            self.config_panel(ui);
        });

        TopBottomPanel::top("control_panel").show(ctx, |ui| {
            self.control_panel(ui);
        });

        TopBottomPanel::bottom("status_panel").show(ctx, |ui| {
            self.status_panel(ui);
        });

        CentralPanel::default().show(ctx, |ui| {
            self.main_panel(ui);
        });

        self.receive();
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
    }

    /// Send an event to the search thread to save the current state.
    pub fn save(&mut self) {
        assert!(self.mode == Mode::Running || self.mode == Mode::Paused);

        if let Some(search) = &mut self.search {
            search.send(Event::Save);
        }
    }

    /// Handle a message from the search thread and update the application state.
    pub fn handle(&mut self, message: Message) {
        match message {
            Message::Frame(frame) => {
                self.status = frame.status;
                self.view = frame.view;
                self.populations = frame.populations;
                self.elapsed = frame.elapsed;
                if frame.status == Status::Solved {
                    // Choose the generation with the smallest population.
                    let solution = self
                        .view
                        .iter()
                        .zip(&self.populations)
                        .min_by_key(|(_, &p)| p)
                        .unwrap()
                        .0
                        .clone();

                    self.solutions.push(solution);
                }

                if frame.running {
                    self.mode = Mode::Running;
                } else {
                    log::debug!("Search paused.");
                    self.mode = Mode::Paused;
                }
            }
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
        if let Some(search) = &mut self.search {
            if let Some(message) = search.try_recv() {
                self.handle(message);
            }
        }
    }
}
