use crate::{
    app::{App, Mode},
    args::Args,
    event::EventHandler,
};
use color_eyre::Result;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::{backend::CrosstermBackend, terminal::Terminal};
use std::io::{stdout, Stdout};

/// The terminal user interface.
#[derive(Debug)]
pub struct Tui {
    /// The terminal.
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// The application state.
    app: App,
    /// The event handler.
    event_handler: EventHandler,
}

impl Tui {
    /// Create a new `Tui` from the command line arguments.
    pub fn new(args: Args) -> Result<Self> {
        let backend = CrosstermBackend::new(stdout());
        let terminal = Terminal::new(backend)?;

        let app = App::new(args)?;
        let event_handler = EventHandler::new();

        let mut tui = Self {
            terminal,
            app,
            event_handler,
        };

        tui.init()?;

        Ok(tui)
    }

    /// Initialize the terminal.
    fn init(&mut self) -> Result<()> {
        enable_raw_mode()?;
        crossterm::execute!(self.terminal.backend_mut(), EnterAlternateScreen)?;
        self.terminal.clear()?;
        self.terminal.hide_cursor()?;

        self.draw()?;
        Ok(())
    }

    /// Cleanup the terminal.
    fn cleanup(&mut self) -> Result<()> {
        disable_raw_mode()?;
        crossterm::execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// Draw the terminal user interface.
    fn draw(&mut self) -> Result<()> {
        self.terminal.draw(|f| self.app.render(f))?;
        Ok(())
    }

    /// Exit the terminal user interface.
    fn exit(&mut self) -> Result<()> {
        self.cleanup()?;
        self.app.print_solution();
        Ok(())
    }

    /// The main loop.
    pub fn run(&mut self) -> Result<()> {
        while !self.app.should_quit {
            // If the application is running, do not block on the event handler.
            if self.app.mode == Mode::Running {
                if let Some(event) = self.event_handler.try_recv()? {
                    self.app.update(event);
                }
                self.app.step();
            } else {
                let event = self.event_handler.recv()?;
                self.app.update(event);
            };

            self.draw()?;
        }

        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        self.exit().ok();
    }
}