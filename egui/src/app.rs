use crate::search::{Event, SearchThread};
use documented::DocumentedFields;
use eframe::{App as EframeApp, Frame};
use egui::{text::LayoutJob, CentralPanel, Context, SidePanel, TopBottomPanel};
use factoriosrc_lib::{Config, ConfigError, Status};
use std::time::Duration;

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
    pub config: Config,
    /// Current mode of the application.
    pub mode: Mode,
    /// A thread to run the search algorithm.
    pub search: Option<SearchThread>,
    /// The current generation to display.
    pub generation: i32,
    /// An egui [`LayoutJob`] to display the current partial result.
    pub view: Option<LayoutJob>,
    /// An error message to display.
    pub error: Option<ConfigError>,
    /// Search status.
    pub status: Status,
    /// Time elapsed since the start of the search.
    pub elapsed: Duration,
}

impl Default for App {
    fn default() -> Self {
        let config = Config::new("R3,C2,S2,B3,N+", 16, 16, 1);
        Self {
            config,
            mode: Mode::Configuring,
            search: None,
            generation: 0,
            view: None,
            error: None,
            status: Status::NotStarted,
            elapsed: Duration::default(),
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
}

impl Drop for App {
    fn drop(&mut self) {
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
        if let Err(e) = config.check() {
            self.error = Some(e);
        } else {
            self.error = None;
            self.view = None;
            self.search = Some(SearchThread::new(config));
            self.mode = Mode::Paused;
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
    }

    /// Receive a message from the search thread and update the application state.
    pub fn receive(&mut self) {
        if let Some(search) = &mut self.search {
            if let Some(message) = search.try_recv() {
                self.status = message.status;
                self.view = Some(message.view[self.generation as usize].clone());
                self.elapsed = message.elapsed;

                if message.running {
                    self.mode = Mode::Running;
                } else {
                    self.mode = Mode::Paused;
                }
            }
        }
    }
}
