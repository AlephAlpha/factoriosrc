use crate::{
    args::{LoadArgs, NewArgs},
    event::TermEvent,
};
use color_eyre::Result;
use crossterm::event::KeyCode;
use factoriosrc_lib::{Status, World};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const DEFAULT_STEP: usize = 100000;

/// Application modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The search is running.
    Running,
    /// The search is not started yet, finished, or paused by the user.
    #[default]
    Paused,
    /// Ask the user to confirm the quit.
    Quit,
    /// Display the usage.
    Usage,
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
    /// The last found solution in RLE format.
    pub solution: Option<String>,
    /// Number of solutions found.
    pub solution_count: usize,
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
}

impl App {
    /// Create a new [`App`] from the command line arguments.
    pub fn new(args: NewArgs) -> Result<Self> {
        let world = World::new(args.config)?;
        let step = args.step.unwrap_or(DEFAULT_STEP);
        let mode = Mode::Paused;
        let generation = 0;
        let start = None;
        let elapsed = Duration::from_secs(0);
        let solution = None;
        let solution_count = 0;
        let should_quit = false;
        let increase_world_size = args.increase_world_size;
        let no_stop = args.no_stop;
        let save = args.save;

        Ok(Self {
            world,
            step,
            mode,
            generation,
            start,
            elapsed,
            solution,
            solution_count,
            should_quit,
            increase_world_size,
            no_stop,
            save,
        })
    }

    /// Load the [`App`] from the path given in the command line arguments.
    pub fn load(args: LoadArgs) -> Result<Self> {
        let path = args.load;
        let json = std::fs::read_to_string(path)?;
        let mut app: Self = serde_json::from_str(&json)?;
        app.save = args.save;
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
    pub fn next_generation(&mut self) {
        let period = self.world.config().period as i32;

        if self.generation < period - 1 {
            self.generation += 1;
        }
    }

    /// Display the previous generation.
    ///
    /// If the current generation is the first one, do nothing.
    pub fn previous_generation(&mut self) {
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
    pub fn step(&mut self) {
        let mut status = self.world.search(self.step);
        if status == Status::Solved {
            self.solution = Some(self.world.rle(self.generation, true));
            self.solution_count += 1;
        }
        if status == Status::NoSolution && self.increase_world_size {
            self.increase_world_size();
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
        if let Some(solution) = &self.solution {
            println!("{solution}");
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
                    _ => {}
                },
                TermEvent::Resize => {}
            },
            Mode::Paused => match event {
                TermEvent::KeyPress(key) => match key {
                    KeyCode::Char('q' | 'Q') | KeyCode::Esc => {
                        self.mode = Mode::Quit;
                    }
                    KeyCode::Char(' ') | KeyCode::Enter => {
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
                    _ => {}
                },
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
