use crate::app::AppConfig;
use egui::{
    text::{LayoutJob, TextFormat},
    Color32, FontId,
};
use factoriosrc_lib::{Status, World};
use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeError;
use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Events that the main thread can send to the search thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Start or resume the search.
    Start,
    /// Pause the search.
    Pause,
    /// Stop the search and quit the search thread.
    Stop,
    /// Save the search state to a JSON string.
    Save,
}

/// Messages that the search thread can send to the main thread.
#[derive(Debug, Clone)]
pub enum Message {
    /// A frame to display the current partial result.
    Frame(Frame),

    /// A JSON string to save the search state.
    Save(String),
}

/// A frame to display the current partial result.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Search status.
    pub status: Status,
    /// Whether the search is running.
    pub running: bool,
    /// Time elapsed since the start of the search.
    pub elapsed: Duration,
    /// The current partial result.
    pub view: Vec<LayoutJob>,
    /// Populations of each generation of the current partial result.
    pub populations: Vec<usize>,
}

impl From<Frame> for Message {
    fn from(frame: Frame) -> Self {
        Self::Frame(frame)
    }
}

impl Message {
    /// Whether the message is a frame.
    pub const fn is_frame(&self) -> bool {
        matches!(self, Self::Frame(_))
    }
}

/// The main struct of the search algorithm.
#[derive(Debug, Serialize, Deserialize)]
struct Search {
    /// The main struct of the search algorithm.
    world: World,
    /// Number of steps between each display of the current partial result.
    step: usize,
    /// Whether to increase the world size when the search fails.
    increase_world_size: bool,
    /// Whether not to stop the search when a solution is found.
    no_stop: bool,
    /// Whether the search is running.
    #[serde(skip)]
    running: bool,
    /// Whether the search should quit.
    #[serde(skip)]
    should_quit: bool,
    /// Start time of the current search.
    #[serde(skip)]
    start: Option<Instant>,
    /// Search status.
    status: Status,
    /// Time elapsed since the start of the search.
    elapsed: Duration,
}

impl Search {
    /// Create a new [`Search`] from a [`AppConfig`].
    fn new(config: AppConfig) -> Self {
        Self {
            world: World::new(config.config).unwrap(),
            step: config.step,
            increase_world_size: config.increase_world_size,
            no_stop: config.no_stop,
            running: false,
            should_quit: false,
            start: None,
            status: Status::NotStarted,
            elapsed: Duration::default(),
        }
    }

    /// Load the search state from a JSON string.
    fn load(s: &str) -> Result<Self, SerdeError> {
        serde_json::from_str(s)
    }

    /// Save the search state to a JSON string.
    fn save(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    /// Start or resume the search.
    fn start(&mut self) {
        if !self.running {
            self.start = Some(Instant::now());
            self.status = Status::Running;
            self.running = true;
        }
    }

    /// Pause the search.
    fn pause(&mut self) {
        if self.running {
            self.elapsed += self.start.unwrap().elapsed();
            self.running = false;
        }
    }

    /// Increment the world size and restart the search.
    fn increase_world_size(&mut self) {
        let mut config = self.world.config().clone();
        let w = config.width;
        let h = config.height;
        let d = config.diagonal_width;
        if d.is_some_and(|d| d < w) {
            config.diagonal_width = Some(d.unwrap() + 1);
        } else if config.requires_square() {
            config.width = w + 1;
            config.height = h + 1;
        } else if h > w {
            config.width = w + 1;
        } else {
            config.height = h + 1;
        }

        self.world = World::new(config).unwrap();
    }

    /// Run the search for the given number of steps.
    fn step(&mut self) {
        self.status = self.world.search(self.step);

        if self.status == Status::NoSolution && self.increase_world_size {
            log::info!("Increasing world size.");
            self.increase_world_size();
            self.status = Status::Running;
        }

        if self.status != Status::Running && !self.no_stop || self.status == Status::NoSolution {
            log::info!("Search status: {:?}", self.status);
            self.pause();
        }
    }

    /// Generate a list of egui [`LayoutJob`]s to display each generation
    /// of the world.
    fn render(&self) -> Vec<LayoutJob> {
        let w = self.world.config().width as i32;
        let h = self.world.config().height as i32;
        let p = self.world.config().period as i32;
        let rule_str = &self.world.config().rule_str;

        let mut jobs = Vec::with_capacity(p as usize);

        for t in 0..p {
            let mut job = LayoutJob::default();

            let header = format!("x = {w}, y = {h}, rule = {rule_str}\n");
            job.append(
                &header,
                0.0,
                TextFormat {
                    color: Color32::from_rgb(153, 153, 153),
                    font_id: FontId::monospace(14.0),
                    ..Default::default()
                },
            );

            for y in 0..h {
                for x in 0..w {
                    let state = self.world.get_cell_state((x, y, t));
                    match state {
                        Some(factoriosrc_lib::CellState::Alive) => {
                            job.append(
                                "o",
                                0.0,
                                TextFormat {
                                    color: Color32::from_rgb(113, 140, 0),
                                    font_id: FontId::monospace(14.0),
                                    ..Default::default()
                                },
                            );
                        }
                        Some(factoriosrc_lib::CellState::Dead) => {
                            job.append(
                                ".",
                                0.0,
                                TextFormat {
                                    color: Color32::from_rgb(200, 40, 41),
                                    font_id: FontId::monospace(14.0),
                                    ..Default::default()
                                },
                            );
                        }
                        None => {
                            job.append(
                                "?",
                                0.0,
                                TextFormat {
                                    color: Color32::from_rgb(137, 89, 168),
                                    font_id: FontId::monospace(14.0),
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }
                job.append(
                    if y == h - 1 { "!\n" } else { "$\n" },
                    0.0,
                    TextFormat {
                        color: Color32::from_rgb(142, 144, 140),
                        font_id: FontId::monospace(14.0),
                        ..Default::default()
                    },
                )
            }

            jobs.push(job);
        }

        jobs
    }

    /// Create a [`Frame`] to send to the main thread.
    fn frame(&self) -> Frame {
        let view = self.render();
        let populations = (0..self.world.config().period)
            .map(|t| self.world.population(t as i32))
            .collect();
        Frame {
            status: self.status,
            running: self.running,
            elapsed: self.elapsed,
            view,
            populations,
        }
    }

    /// Handle an [`Event`] from the main thread, and return a [`Message`].
    fn handle_event(&mut self, event: Event) -> Message {
        log::debug!("Received event: {:?}", event);
        match event {
            Event::Start => self.start(),
            Event::Pause => self.pause(),
            Event::Stop => {
                self.pause();
                self.should_quit = true;
            }
            Event::Save => return Message::Save(self.save()),
        }
        self.frame().into()
    }

    /// The main loop of the search thread.
    fn run(&mut self, rx: Receiver<Event>, tx: Sender<Message>) {
        tx.send(self.frame().into()).unwrap();

        while !self.should_quit {
            // If the search is running, do not block on the event receiver.
            if self.running {
                self.step();
                let message = match rx.try_recv() {
                    Ok(event) => self.handle_event(event),
                    Err(TryRecvError::Empty) => self.frame().into(),
                    Err(TryRecvError::Disconnected) => {
                        log::error!("The main thread has disconnected.");
                        break;
                    }
                };

                tx.send(message).unwrap();
            } else {
                let message = match rx.recv() {
                    Ok(event) => self.handle_event(event),
                    Err(_) => {
                        log::error!("The main thread has disconnected.");
                        break;
                    }
                };
                tx.send(message).unwrap();
            }
        }
    }
}

/// A struct to run the search algorithm in a separate thread.
#[derive(Debug)]
pub struct SearchThread {
    /// The search thread.
    thread: JoinHandle<()>,
    /// A channel to send events to the search thread.
    tx: Sender<Event>,
    /// A channel to receive messages from the search thread.
    rx: Receiver<Message>,
}

impl SearchThread {
    /// Create a new [`SearchThread`] from a [`AppConfig`].
    pub fn new(config: AppConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            log::info!("Search thread started.");
            let mut search = Search::new(config);
            search.run(rx, tx2);
            log::info!("Search thread stopped.");
        });

        Self {
            thread,
            tx,
            rx: rx2,
        }
    }

    /// Create a new [`SearchThread`] from a JSON string.
    ///
    /// This also returns the [`AppConfig`] so that the main thread can
    /// update the UI with the new world configuration.
    pub fn load(s: &str) -> Result<(Self, AppConfig), SerdeError> {
        // Validate the save file by trying to load it in the main thread.
        // We need to load it again later in the search thread, because
        // [`Search`] is not `Send` and cannot be moved between threads.
        let search = Search::load(s)?;
        let config = AppConfig {
            config: search.world.config().clone(),
            step: search.step,
            increase_world_size: search.increase_world_size,
            no_stop: search.no_stop,
        };

        let (tx, rx) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();
        let s = s.to_string();
        let thread = std::thread::spawn(move || {
            log::info!("Search thread started.");
            let search = Search::load(&s).unwrap();
            let mut search = search;
            search.run(rx, tx2);
            log::info!("Search thread stopped.");
        });

        let search = Self {
            thread,
            tx,
            rx: rx2,
        };

        Ok((search, config))
    }

    /// Send an [`Event`] to the search thread.
    pub fn send(&self, event: Event) {
        self.tx.send(event).unwrap();
    }

    /// Try to receive a [`Message`] from the search thread without blocking.
    ///
    /// If there are more than one messages in the channel, it will return the
    /// first one that is not a frame, or the last one if all of them are frames.
    ///
    /// If the channel is empty, it will return `None`.
    pub fn try_recv(&self) -> Option<Message> {
        let mut message = None;
        for m in self.rx.try_iter() {
            if !m.is_frame() {
                return Some(m);
            }
            message = Some(m);
        }
        message
    }

    /// Wait for the search thread to finish.
    pub fn join(self) {
        self.thread.join().unwrap();
    }
}
