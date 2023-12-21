use crate::{args::Args, event::TermEvent};
use color_eyre::Result;
use crossterm::event::KeyCode;
use factoriosrc_lib::{Status, World};
use std::{
    ops::{Deref, DerefMut},
    time::{Duration, Instant},
};

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
pub struct App {
    /// The main struct of the search algorithm.
    pub world: World,
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
    /// Whether the application should quit.
    pub should_quit: bool,
}

impl App {
    /// Create a new `App` from the command line arguments.
    pub fn new(args: Args) -> Result<Self> {
        let world = World::new(args.config)?;
        let step = args.step;
        let mode = Mode::Paused;
        let generation = 0;
        let start = None;
        let elapsed = Duration::from_secs(0);
        let solution = None;
        let should_quit = false;

        Ok(Self {
            world,
            step,
            mode,
            generation,
            start,
            elapsed,
            solution,
            should_quit,
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
    pub fn start(&mut self) {
        if self.mode == Mode::Paused {
            self.start = Some(Instant::now());
            self.mode = Mode::Running;
        }
    }

    /// Pause the search.
    pub fn pause(&mut self) {
        if self.mode == Mode::Running {
            self.elapsed += self.start.take().unwrap().elapsed();
            self.mode = Mode::Paused;
        }
    }

    /// Run the search for the given number of steps.
    pub fn step(&mut self) {
        let status = self.world.search(self.step);
        if status == Status::Solved {
            self.solution = Some(self.rle(self.generation));
        }
        if status != Status::Running {
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

impl Deref for App {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

impl DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.world
    }
}
