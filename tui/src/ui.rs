use crate::app::{App, Mode};
use factoriosrc_lib::{CellState, Status, World};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Style, Stylize},
    terminal::Frame,
    text::{Line, Span, Text},
    widgets::{
        block::{Block, Title},
        Borders, Clear, Paragraph, Widget,
    },
};

impl App<'_> {
    /// Render the TUI interface.
    pub fn render(&self, frame: &mut Frame) {
        let chunks = Layout::new(
            Direction::Vertical,
            [
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ],
        )
        .split(frame.size());

        self.render_top_bar(frame, chunks[0]);
        self.render_main(frame, chunks[1]);
        self.render_bottom_bar(frame, chunks[2]);

        // Show the popup window if needed.
        match self.mode {
            Mode::Usage => self.render_help(frame, chunks[1]),
            Mode::Quit => self.render_quit(frame, chunks[1]),
            _ => {}
        }
    }

    /// Render the top bar.
    ///
    /// This includes the current generation, the population, and the elapsed time.
    fn render_top_bar(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::new(
            Direction::Horizontal,
            Constraint::from_ratios([(1, 3), (1, 3), (1, 3)]),
        )
        .split(area);

        let style = Style::new().black().on_light_blue();

        let generation = Paragraph::new(format!("Generation: {}", self.generation)).style(style);
        frame.render_widget(generation, chunks[0]);

        let population = Paragraph::new(format!(
            "Population: {}",
            self.world.population(self.generation)
        ))
        .style(style);
        frame.render_widget(population, chunks[1]);

        // Only show the elapsed time if the search not running.
        let elapsed_str = if self.mode == Mode::Running {
            String::new()
        } else {
            format!("Time: {:.3?}", self.elapsed)
        };
        let elapsed = Paragraph::new(elapsed_str).style(style);
        frame.render_widget(elapsed, chunks[2]);
    }

    /// Render the bottom bar.
    ///
    /// This includes the current status, mode, and a short help message.
    fn render_bottom_bar(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::new(
            Direction::Horizontal,
            Constraint::from_percentages([50, 50]),
        )
        .split(area);

        let style = Style::new().black().on_light_blue();

        let status_str = match self.status() {
            Status::NotStarted => "Not started yet.",
            Status::Running => {
                if self.mode == Mode::Running {
                    "Searching..."
                } else {
                    "Paused."
                }
            }
            Status::Solved => "A solution was found.",
            Status::NoSolution => {
                if self.solution.is_some() {
                    "No more solutions."
                } else {
                    "No solution found."
                }
            }
        };

        let status = Paragraph::new(status_str).style(style);
        frame.render_widget(status, chunks[0]);

        let help = Paragraph::new("Press [h] for help.").style(style);
        frame.render_widget(help, chunks[1]);
    }

    /// Render the main area.
    fn render_main(&self, frame: &mut Frame, area: Rect) {
        let rle = Rle::new(self);
        frame.render_widget(rle, area);
    }

    /// Render a popup window with some text.
    fn render_popup<'b>(
        &self,
        frame: &mut Frame,
        area: Rect,
        text: impl Into<Text<'b>>,
        title: impl Into<Title<'b>>,
        style: Style,
    ) {
        let text = text.into();

        let center_x = area.x + area.width / 2;
        let center_y = area.y + area.height / 2;

        let width = area.width.min(text.width() as u16 + 2);
        let height = area.height.min(text.height() as u16 + 2);

        let rect = Rect::new(center_x - width / 2, center_y - height / 2, width, height);

        frame.render_widget(Clear, rect);

        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(title))
            .style(style);

        frame.render_widget(paragraph, rect);
    }

    /// Render the popup window to show the help message.
    fn render_help(&self, frame: &mut Frame, area: Rect) {
        self.render_popup(
            frame,
            area,
            "[q]/[Esc]       Quit\n\
             [h]             Show or hide this help message\n\
             [Space]/[Enter] Start or pause the search\n\
             [=]             Show the next generation\n\
             [-]             Show the previous generation",
            "Help",
            Style::new().green(),
        );
    }

    /// Render the popup window to ask the user to confirm quitting.
    fn render_quit(&self, frame: &mut Frame, area: Rect) {
        self.render_popup(
            frame,
            area,
            "Are you sure you want to quit? ([y]/[n])",
            "Quit",
            Style::new().yellow(),
        );
    }
}

/// A widget to show the current generation in the RLE format.
#[derive(Debug)]
struct Rle<'a, 'b> {
    /// The current generation.
    t: isize,
    /// A reference to the world.
    world: &'b World<'a>,
}

impl<'a, 'b> Rle<'a, 'b> {
    /// Create a new RLE widget from the app.
    fn new(app: &'b App<'a>) -> Self {
        Self {
            t: app.generation,
            world: &app.world,
        }
    }
}

impl Widget for Rle<'_, '_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let w = self.world.config().width as u16;
        let h = self.world.config().height as u16;

        let header = Line::from(vec![
            Span::styled("x", Style::new().magenta()),
            Span::raw(" = "),
            Span::styled(w.to_string(), Style::new().cyan()),
            Span::raw(", "),
            Span::styled("y", Style::new().magenta()),
            Span::raw(" = "),
            Span::styled(h.to_string(), Style::new().cyan()),
            Span::raw(", "),
            Span::styled("rule", Style::new().magenta()),
            Span::raw(" = "),
            Span::styled(self.world.config().rule.name(), Style::new().cyan()),
        ]);

        buf.set_line(area.x, area.y, &header, area.width);

        if area.height > 1 {
            for y in 0..h.min(area.height - 1) {
                let buf_y = area.y + y + 1;
                for x in 0..w.min(area.width) {
                    let buf_x = area.x + x;
                    let state = self.world.get_cell_state((x as isize, y as isize, self.t));
                    match state {
                        Some(CellState::Alive) => buf
                            .get_mut(buf_x, buf_y)
                            .set_char('o')
                            .set_style(Style::new().green()),
                        Some(CellState::Dead) => buf
                            .get_mut(buf_x, buf_y)
                            .set_char('.')
                            .set_style(Style::new().red()),
                        None => buf
                            .get_mut(buf_x, buf_y)
                            .set_char('?')
                            .set_style(Style::new().blue()),
                    };
                }
                if area.width > w + 1 {
                    let buf_x: u16 = area.x + w;
                    buf.get_mut(buf_x, buf_y)
                        .set_char(if y == h - 1 { '!' } else { '$' })
                        .set_style(Style::new().dark_gray());
                }
            }
        }
    }
}
