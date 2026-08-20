use crate::{
    app::AppConfig,
    snapshot::{GenerationSnapshot, SearchSnapshot},
};
use factoriosrc_lib::{Status, World};
#[cfg(feature = "save")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "save")]
use serde_json::Error as SerdeError;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::{
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::JoinHandle,
};
// `std::time::Instant` panics on `wasm32-unknown-unknown`; `web_time::Instant`
// uses `performance.now()` there and re-exports `std::time` on native targets.
use web_time::Instant;

/// Events that the main thread can send to the search thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(Serialize, Deserialize))]
pub enum Event {
    /// Start or resume the search.
    Start,
    /// Pause the search.
    Pause,
    /// Stop the search and quit the search thread.
    Stop,
    /// Save the search state to a JSON string.
    #[cfg(feature = "save")]
    Save,
}

/// Messages that the search thread can send to the main thread.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "save", derive(Serialize, Deserialize))]
pub enum Message {
    /// A snapshot to display the current partial result.
    Snapshot(SearchSnapshot),

    /// A JSON string to save the search state.
    #[cfg(feature = "save")]
    Save(String),
}

impl From<SearchSnapshot> for Message {
    fn from(snapshot: SearchSnapshot) -> Self {
        Self::Snapshot(snapshot)
    }
}

impl Message {
    /// Whether the message is a frame.
    pub const fn is_frame(&self) -> bool {
        matches!(self, Self::Snapshot(_))
    }
}

/// The main struct of the search algorithm.
#[derive(Debug)]
#[cfg_attr(feature = "save", derive(Serialize, Deserialize))]
pub(crate) struct Search {
    /// The main struct of the search algorithm.
    pub(crate) world: World,
    /// Number of steps between each display of the current partial result.
    pub(crate) step: usize,
    /// Whether to increase the world size when the search fails.
    pub(crate) increase_world_size: bool,
    /// Whether not to stop the search when a solution is found.
    pub(crate) no_stop: bool,
    /// A file-name template for exporting found solutions to RLE files.
    ///
    /// [`None`] or an empty string means that result export is disabled.
    /// Only used on native platforms; on the web it is ignored.
    #[cfg_attr(feature = "save", serde(default))]
    pub(crate) export: Option<String>,
    /// The number of solutions that have been exported.
    #[cfg_attr(feature = "save", serde(skip))]
    exported: usize,
    /// Whether the search is running.
    #[cfg_attr(feature = "save", serde(skip))]
    running: bool,
    /// Whether the search should quit.
    #[cfg_attr(feature = "save", serde(skip))]
    should_quit: bool,
    /// Start time of the current search.
    #[cfg_attr(feature = "save", serde(skip))]
    start: Option<Instant>,
    /// Search status.
    status: Status,
    /// Time elapsed since the start of the search.
    elapsed: Duration,
}

impl Search {
    /// Create a new [`Search`] from a [`AppConfig`].
    pub(crate) fn new(config: AppConfig) -> Self {
        Self {
            world: World::new(config.config).unwrap(),
            step: config.step,
            increase_world_size: config.increase_world_size,
            no_stop: config.no_stop,
            export: config.export,
            exported: 0,
            running: false,
            should_quit: false,
            start: None,
            status: Status::NotStarted,
            elapsed: Duration::default(),
        }
    }

    /// Load the search state from a JSON string.
    #[cfg(feature = "save")]
    pub(crate) fn load(s: &str) -> Result<Self, SerdeError> {
        serde_json::from_str(s)
    }

    /// Save the search state to a JSON string.
    #[cfg(feature = "save")]
    pub(crate) fn save(&self) -> String {
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

    /// Run the search for the given number of steps.
    fn step(&mut self) {
        self.status = self.world.search(self.step);

        if self.status == Status::Solved {
            self.export_solution();
        }

        if self.status == Status::NoSolution && self.increase_world_size {
            log::info!("Increasing world size.");
            self.world.increase_world_size();
            self.status = Status::Running;
        }

        if self.status != Status::Running && !self.no_stop || self.status == Status::NoSolution {
            log::info!("Search status: {:?}", self.status);
            self.pause();
        }
    }

    /// Save the solution that was just found to files, if export is enabled.
    ///
    /// Each generation of the solution is written to its own compact RLE
    /// file, using the export template. The solution index is 1-based. On
    /// the web there is no file system, so this does nothing there.
    fn export_solution(&mut self) {
        self.exported += 1;
        #[cfg(not(target_arch = "wasm32"))]
        {
            use factoriosrc_lib::{ExportFields, Template, save_generation};

            let Some(template_str) = self.export.as_deref().filter(|s| !s.is_empty()) else {
                return;
            };
            let Ok(template) = Template::parse(template_str) else {
                log::error!("Invalid export template: {template_str}");
                return;
            };
            let config = self.world.config().clone();
            let index = self.exported;
            for t in 0..config.period {
                let fields =
                    ExportFields::from_config(&config, index, t, self.world.population(t as i32));
                let rle = self.world.rle(t as i32, true);
                match save_generation(&template, &fields, &rle) {
                    Ok(path) => log::info!("Saved result to {}", path.display()),
                    Err(e) => log::error!("Failed to save result: {e}"),
                }
            }
        }
    }

    /// Whether the search is currently running.
    #[cfg(target_arch = "wasm32")]
    pub(crate) const fn is_running(&self) -> bool {
        self.running
    }

    /// Run one step batch, and return a snapshot of the result.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn step_batch(&mut self) -> Message {
        self.step();
        self.snapshot().into()
    }

    /// Create a UI-neutral snapshot to send to the main thread.
    pub(crate) fn snapshot(&self) -> SearchSnapshot {
        let generations = (0..self.world.config().period as i32)
            .map(|generation| GenerationSnapshot {
                generation,
                population: self.world.population(generation),
                rle: self.world.rle(generation, false),
            })
            .collect();

        SearchSnapshot {
            status: self.status,
            running: self.running,
            elapsed: self.elapsed,
            generations,
            cells_checked: self.world.cells_checked(),
        }
    }

    /// Handle an [`Event`] from the main thread, and return a [`Message`].
    pub(crate) fn handle_event(&mut self, event: Event) -> Message {
        log::debug!("Received event: {event:?}");
        match event {
            Event::Start => self.start(),
            Event::Pause => self.pause(),
            Event::Stop => {
                self.pause();
                self.should_quit = true;
            }
            #[cfg(feature = "save")]
            Event::Save => return Message::Save(self.save()),
        }
        self.snapshot().into()
    }

    /// The main loop of the search thread.
    #[cfg(not(target_arch = "wasm32"))]
    fn run(&mut self, rx: Receiver<Event>, tx: Sender<Message>) {
        tx.send(self.snapshot().into()).unwrap();

        while !self.should_quit {
            // If the search is running, do not block on the event receiver.
            if self.running {
                self.step();
                let message = match rx.try_recv() {
                    Ok(event) => self.handle_event(event),
                    Err(TryRecvError::Empty) => self.snapshot().into(),
                    Err(TryRecvError::Disconnected) => {
                        log::error!("The main thread has disconnected.");
                        break;
                    }
                };

                tx.send(message).unwrap();
            } else {
                let message = if let Ok(event) = rx.recv() {
                    self.handle_event(event)
                } else {
                    log::error!("The main thread has disconnected.");
                    break;
                };
                tx.send(message).unwrap();
            }
        }
    }
}

/// A struct to run the search algorithm in a separate thread.
///
/// On native platforms the search runs on this thread; on the web it runs
/// in a WebWorker instead (see [`crate::web`]).
#[derive(Debug)]
#[cfg(not(target_arch = "wasm32"))]
pub struct SearchThread {
    /// The search thread.
    thread: JoinHandle<()>,
    /// A channel to send events to the search thread.
    tx: Sender<Event>,
    /// A channel to receive messages from the search thread.
    rx: Receiver<Message>,
}

#[cfg(not(target_arch = "wasm32"))]
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
    #[cfg(feature = "save")]
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
            export: search.export.clone(),
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

/// A handle to a running search backend.
///
/// On native platforms this runs the search on a separate thread, while on the web
/// it runs the search in a WebWorker.
pub trait SearchApi: std::fmt::Debug {
    /// Send an [`Event`] to the search backend.
    fn send(&self, event: Event);

    /// Try to receive a [`Message`] from the search backend without blocking.
    ///
    /// If there are more than one messages available, it will return the
    /// first one that is not a frame, or the last one if all of them are frames.
    ///
    /// If no message is available, it will return `None`.
    fn try_recv(&self) -> Option<Message>;

    /// Stop the search backend and release its resources.
    fn terminate(self: Box<Self>);
}

#[cfg(not(target_arch = "wasm32"))]
impl SearchApi for SearchThread {
    /// Send an [`Event`] to the search thread.
    fn send(&self, event: Event) {
        SearchThread::send(self, event);
    }

    /// Try to receive a [`Message`] from the search thread without blocking.
    fn try_recv(&self) -> Option<Message> {
        SearchThread::try_recv(self)
    }

    /// Stop the search thread and wait for it to finish.
    fn terminate(self: Box<Self>) {
        self.send(Event::Stop);
        self.join();
    }
}

/// Create a new search backend from the given configuration.
#[must_use]
pub fn spawn_search(config: AppConfig) -> Box<dyn SearchApi> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Box::new(SearchThread::new(config))
    }
    #[cfg(all(target_arch = "wasm32", feature = "save"))]
    {
        Box::new(crate::web::WebSearchThread::new(config))
    }
    #[cfg(all(target_arch = "wasm32", not(feature = "save")))]
    {
        unreachable!("The web build requires the `save` feature.")
    }
}

/// Create a new search backend from a JSON string.
///
/// This also returns the [`AppConfig`] so that the main thread can
/// update the UI with the new world configuration.
#[cfg(feature = "save")]
pub fn load_search(s: &str) -> Result<(Box<dyn SearchApi>, AppConfig), SerdeError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (search, config) = SearchThread::load(s)?;
        Ok((Box::new(search), config))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let (search, config) = crate::web::WebSearchThread::load(s)?;
        Ok((Box::new(search), config))
    }
}
