use crate::app::AppConfig;
use egui::{
    text::{LayoutJob, TextFormat},
    Color32, FontId,
};
use factoriosrc_lib::{Status, World};
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
}

/// Messages that the search thread can send to the main thread.
#[derive(Debug, Clone)]
pub struct Message {
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

/// The main struct of the search algorithm.
#[derive(Debug)]
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
    running: bool,
    /// Whether the search should quit.
    should_quit: bool,
    /// Start time of the current search.
    start: Option<Instant>,
    /// Search status.
    status: Status,
    /// Time elapsed since the start of the search.
    elapsed: Duration,
    /// A channel to receive events from the main thread.
    rx: Receiver<Event>,
    /// A channel to send messages to the main thread.
    tx: Sender<Message>,
}

impl Search {
    /// Create a new [`Search`] from a [`AppConfig`].
    fn new(config: AppConfig, rx: Receiver<Event>, tx: Sender<Message>) -> Self {
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
            rx,
            tx,
        }
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

    /// Send a [`Message`] to the main thread.
    fn send_message(&self) {
        let view = self.render();
        let populations = (0..self.world.config().period)
            .map(|t| self.world.population(t as i32))
            .collect();
        let message = Message {
            status: self.status,
            running: self.running,
            elapsed: self.elapsed,
            view,
            populations,
        };
        self.tx.send(message).unwrap();
    }

    /// Handle an [`Event`] from the main thread.
    fn handle_event(&mut self, event: Event) {
        log::debug!("Received event: {:?}", event);
        match event {
            Event::Start => self.start(),
            Event::Pause => self.pause(),
            Event::Stop => {
                self.pause();
                self.should_quit = true;
            }
        }
    }

    /// The main loop of the search thread.
    fn run(&mut self) {
        self.send_message();

        while !self.should_quit {
            // If the search is running, do not block on the event receiver.
            if self.running {
                self.step();
                match self.rx.try_recv() {
                    Ok(event) => self.handle_event(event),
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        log::error!("The main thread has disconnected.");
                        break;
                    }
                }
            } else {
                match self.rx.recv() {
                    Ok(event) => self.handle_event(event),
                    Err(_) => {
                        log::error!("The main thread has disconnected.");
                        break;
                    }
                }
            }
            self.send_message();
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
            let mut search = Search::new(config, rx, tx2);
            search.run();
            log::info!("Search thread stopped.");
        });

        Self {
            thread,
            tx,
            rx: rx2,
        }
    }

    /// Send an [`Event`] to the search thread.
    pub fn send(&self, event: Event) {
        self.tx.send(event).unwrap();
    }

    /// Try to receive a [`Message`] from the search thread without blocking.
    ///
    /// If there are more than one messages in the channel, only the last one
    /// will be returned.
    pub fn try_recv(&self) -> Option<Message> {
        self.rx.try_iter().last()
    }

    /// Wait for the search thread to finish.
    pub fn join(self) {
        self.thread.join().unwrap();
    }
}
