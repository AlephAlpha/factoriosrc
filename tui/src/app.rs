use crate::{args::Args, event::TermEvent};
use color_eyre::Result;
use crossterm::event::KeyCode;
use factoriosrc_lib::{Status, World, WorldAllocator};
use std::{
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

const DEFAULT_STEP: usize = 100000;

/// Application modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The search is running.
    Running,
    /// The search is not started yet, finished, or paused by the user.
    Paused,
    /// Ask the user to confirm the quit.
    Quit,
    /// Display the usage.
    Usage,
}

/// Application state.
#[derive(Debug)]
pub struct App<'a> {
    /// The main struct of the search algorithm.
    pub world: World<'a>,
    /// Number of steps between each display of the current partial result.
    pub step: usize,
    /// Current mode of the application.
    pub mode: Mode,
    /// Generation to display.
    pub generation: isize,
    /// Start time of the current search.
    pub start: Option<Instant>,
    /// Time elapsed since the start of the search.
    pub elapsed: Duration,
    /// The last found solution in RLE format.
    pub solution: Option<String>,
    /// Number of solutions found.
    pub solution_count: usize,
    /// Whether the application should quit.
    pub should_quit: bool,
    /// Whether to increase the world size when the search fails.
    pub increase_world_size: bool,
    /// Whether not to stop the search when a solution is found.
    pub no_stop: bool,
}

impl<'a> App<'a> {
    /// Create a new [`App`] from the command line arguments and the world allocator.
    pub fn new(args: Args, allocator: &'a WorldAllocator) -> Result<Self> {
        let world = allocator.new_world(args.config)?;
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
        })
    }

    /// Display the next generation.
    ///
    /// If the current generation is the last one, do nothing.
    pub fn next_generation(&mut self) {
        let period = self.config().period as isize;

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
        let mut config = self.config().clone();
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

        self.world.reset(config).unwrap();
    }

    /// Run the search for the given number of steps.
    pub fn step(&mut self) {
        let mut status = self.world.search(self.step);
        if status == Status::Solved {
            self.solution = Some(self.rle(self.generation));
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
            println!("{}", solution);
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

impl<'a> Deref for App<'a> {
    type Target = World<'a>;

    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

impl<'a> DerefMut for App<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.world
    }
}
