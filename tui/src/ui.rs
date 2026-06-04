use crate::app::{App, ConfigField, Mode};
use factoriosrc_lib::{CellState, Reason, Status, World};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph, Widget},
};

impl App {
    /// Render the TUI interface.
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        match self.mode {
            Mode::Config => {
                self.render_config_form(frame, area);
                if self.config_state.as_ref().is_some_and(|s| s.show_confirm) {
                    self.render_config_confirm(frame, area);
                }
            }
            Mode::MarkKnown => {
                self.render_mark_view(frame, area);
            }
            _ => {
                let [top, main, legend, bottom] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Min(0),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .areas(area);

                self.render_top_bar(frame, top);
                self.render_main(frame, main);
                self.render_legend_bar(frame, legend);
                self.render_bottom_bar(frame, bottom);

                match self.mode {
                    Mode::Usage => self.render_help(frame, main),
                    Mode::Quit => self.render_quit(frame, main),
                    _ => {}
                }
            }
        }
    }

    /// Render the top bar.
    ///
    /// This includes the current generation, the population, the number of solutions found, and the
    /// elapsed time.
    fn render_top_bar(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::horizontal(Constraint::from_ratios([(1, 4), (1, 4), (1, 4), (1, 4)]))
            .split(area);

        let style = Style::new().black().on_light_blue();

        if let Some(i) = self.viewing_solution {
            let label =
                Paragraph::new(format!("Solution {}/{}", i + 1, self.solutions.len())).style(style);
            frame.render_widget(label, chunks[0]);

            let gen_label = Paragraph::new(format!(
                "Gen: {}/{}",
                self.generation,
                self.world.config().period - 1,
            ))
            .style(style);
            frame.render_widget(gen_label, chunks[1]);

            let count = Paragraph::new(format!("Solutions: {}", self.solutions.len())).style(style);
            frame.render_widget(count, chunks[2]);
        } else {
            let generation =
                Paragraph::new(format!("Generation: {}", self.generation)).style(style);
            frame.render_widget(generation, chunks[0]);

            let population = Paragraph::new(format!(
                "Population: {}",
                self.world.population(self.generation)
            ))
            .style(style);
            frame.render_widget(population, chunks[1]);

            let solution_count =
                Paragraph::new(format!("Solutions: {}", self.solutions.len())).style(style);
            frame.render_widget(solution_count, chunks[2]);

            // Only show the elapsed time if the search not running.
            let elapsed_str = if self.mode == Mode::Running {
                String::new()
            } else {
                format!("Time: {:.3?}", self.elapsed)
            };
            let elapsed = Paragraph::new(elapsed_str).style(style);
            frame.render_widget(elapsed, chunks[3]);
        }
    }

    /// Render the bottom bar.
    ///
    /// This includes the current status, mode, and a short help message.
    fn render_bottom_bar(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::horizontal(Constraint::from_percentages([50, 50])).split(area);

        let style = Style::new().black().on_light_blue();

        let status_str = self.viewing_solution.map_or_else(
            || match self.world.status() {
                Status::NotStarted => "Not started yet.".to_string(),
                Status::Running => {
                    if self.mode == Mode::Running {
                        "Searching...".to_string()
                    } else {
                        "Paused.".to_string()
                    }
                }
                Status::Solved => "A solution was found.".to_string(),
                Status::NoSolution => {
                    if self.solutions.is_empty() {
                        "No solution found.".to_string()
                    } else {
                        "No more solutions.".to_string()
                    }
                }
            },
            |i| format!("Viewing solution {}/{}", i + 1, self.solutions.len()),
        );

        let status = Paragraph::new(status_str).style(style);
        frame.render_widget(status, chunks[0]);

        let help = Paragraph::new("Press [h] for help.").style(style);
        frame.render_widget(help, chunks[1]);
    }

    /// Render the main area.
    fn render_main(&self, frame: &mut Frame, area: Rect) {
        if let Some(i) = self.viewing_solution {
            let rle = self.current_rle();
            let title = format!("Solution {}/{}", i + 1, self.solutions.len());
            let paragraph = Paragraph::new(rle)
                .style(Style::new().white())
                .block(Block::bordered().title(title));
            frame.render_widget(paragraph, area);
        } else {
            let rle = Rle::new(self);
            frame.render_widget(rle, area);
        }
    }

    /// Render the legend bar showing cell symbol and color meanings.
    fn render_legend_bar(&self, frame: &mut Frame, area: Rect) {
        let spans = vec![
            Span::styled("o ", Style::new().green()),
            Span::raw("Alive("),
            Span::styled("K", Style::new().green().add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("D", Style::new().yellow()),
            Span::raw("/"),
            Span::styled("G", Style::new().green()),
            Span::raw(")  "),
            Span::styled(". ", Style::new().red()),
            Span::raw("Dead("),
            Span::styled("K", Style::new().red().add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("D", Style::new().dark_gray()),
            Span::raw("/"),
            Span::styled("G", Style::new().gray()),
            Span::raw(")  "),
            Span::styled("? Unknown", Style::new().cyan()),
            Span::raw("  "),
            Span::styled("K", Style::new().green().add_modifier(Modifier::BOLD)),
            Span::raw("=Known "),
            Span::styled("D", Style::new().yellow()),
            Span::raw("=Deduced "),
            Span::styled("G", Style::new().green()),
            Span::raw("=Guessed"),
        ];

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    /// Render a popup window with some text.
    fn render_popup<'b>(
        &self,
        frame: &mut Frame,
        area: Rect,
        text: impl Into<Text<'b>>,
        title: impl Into<Line<'b>>,
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
            .block(Block::bordered().title(title))
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
             [-]             Show the previous generation\n\
             [n]             Next solution (when paused)\n\
             [p]             Previous solution (when paused)\n\
             [o]             Open configuration (when paused)\n\
             [c]             Copy RLE to clipboard",
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

    // ── Config form rendering ──

    /// Render the full-screen configuration form.
    fn render_config_form(&self, frame: &mut Frame, area: Rect) {
        let Some(ref state) = self.config_state else {
            return;
        };

        let mut lines: Vec<Line> = Vec::new();

        for (i, field) in state.fields.iter().enumerate() {
            let is_focused = i == state.focus_index;

            if field.is_button() {
                let btn_text = match field {
                    ConfigField::Apply => "[ Apply ]".to_string(),
                    ConfigField::Cancel => "[ Cancel ]".to_string(),
                    _ => unreachable!(),
                };
                let style = if is_focused {
                    Style::new().green().add_modifier(Modifier::BOLD)
                } else {
                    Style::new().cyan()
                };
                if matches!(field, ConfigField::Apply) {
                    lines.push(Line::from(""));
                }
                let prefix = if is_focused { "▸ " } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(btn_text, style),
                    Span::raw("          "),
                ]));
                if matches!(field, ConfigField::Cancel) {
                    lines.push(Line::from(""));
                }
            } else {
                let label = format!("{}:", field.label());
                let value_str = if is_focused && field.is_text_field() {
                    state.edit_buffer.clone()
                } else {
                    state.field_value(*field)
                };
                let display_val = if value_str.is_empty() {
                    "—".to_string()
                } else {
                    value_str
                };
                let prefix = if is_focused { "▸ " } else { "  " };

                let value_style = if is_focused {
                    Style::new().green()
                } else {
                    match field {
                        ConfigField::KnownCells => Style::new().gray(),
                        _ => Style::new().white(),
                    }
                };

                lines.push(Line::from(vec![
                    Span::styled(prefix, value_style),
                    Span::styled(format!("{label:<18}"), Style::new().white()),
                    Span::styled(display_val, value_style),
                ]));
            }
        }

        // Error message.
        if let Some(ref error) = state.error {
            lines.push(Line::from(Span::styled(
                format!("! {error}"),
                Style::new().red(),
            )));
        }

        // Short help line (only when no error).
        if state.error.is_none() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Tab/Shift+Tab: navigate  |  Enter: apply  |  Esc: cancel",
                Style::new().gray(),
            )));
        }

        // Auto-scroll: keep the focused field visible in small terminals.
        let inner_h = area.height.saturating_sub(2) as usize;
        let total = lines.len();
        if total > inner_h {
            let max_offset = total.saturating_sub(inner_h);
            let cur = state.scroll_offset.get().min(max_offset);
            let new_offset = if state.focus_index < cur {
                state.focus_index
            } else if state.focus_index >= cur + inner_h {
                state.focus_index.saturating_add(1).saturating_sub(inner_h)
            } else {
                cur
            };
            state.scroll_offset.set(new_offset.min(max_offset));
        } else {
            state.scroll_offset.set(0);
        }

        let offset = state.scroll_offset.get();
        let visible_lines: Vec<Line> = lines.iter().skip(offset).take(inner_h).cloned().collect();

        let paragraph = Paragraph::new(Text::from(visible_lines))
            .block(Block::bordered().title("Configuration"))
            .style(Style::new().white());

        frame.render_widget(paragraph, area);

        // Cursor position for text editing.
        if let Some(field) = state.fields.get(state.focus_index)
            && field.is_text_field()
        {
            let label_width = 20u16;
            let inner_x = area.x + 1 + label_width;
            let inner_y = area.y + 1 + (state.focus_index - offset) as u16;
            let cursor_x = inner_x + state.edit_buffer.len() as u16;
            frame.set_cursor_position((cursor_x, inner_y));
        }
    }

    /// Render the confirm dialog for applying config changes.
    fn render_config_confirm(&self, frame: &mut Frame, area: Rect) {
        self.render_popup(
            frame,
            area,
            "Changing the configuration will reset all search progress.\n\nAre you sure? ([y]/[n])",
            "Confirm",
            Style::new().yellow(),
        );
    }

    /// Render the mark-known-cells view.
    fn render_mark_view(&self, frame: &mut Frame, area: Rect) {
        let Some(ref state) = self.mark_state else {
            return;
        };

        let [top, main, bottom] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(area);

        // Top bar: status.
        let top_style = Style::new().black().on_light_blue();
        let known_count = state.known_cells.len();
        let gen_str = format!("{}/{}", self.generation, self.world.config().period - 1);
        let top_text = format!(
            "Known: {} cell(s)  |  Position: ({}, {})  |  Gen: {}",
            known_count, state.cursor_x, state.cursor_y, gen_str,
        );
        frame.render_widget(Paragraph::new(top_text).style(top_style), top);

        // Bottom bar: keybindings.
        let bottom_style = Style::new().black().on_light_blue();
        let bottom_text =
            "[Space] cycle  [a] alive  [d] dead  [u] unset  [Enter] save  [Esc] cancel";
        frame.render_widget(Paragraph::new(bottom_text).style(bottom_style), bottom);

        // Grid.
        let w = self.world.config().width as u16;
        let h = self.world.config().height as u16;
        let buf = frame.buffer_mut();

        // Header: x = W, y = H, rule = RULE
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
            Span::styled(&self.world.config().rule_str, Style::new().cyan()),
        ]);
        buf.set_line(main.x, main.y, &header, main.width);

        if main.height > 1 {
            for y in 0..h.min(main.height - 1) {
                let buf_y = main.y + y + 1;
                for x in 0..w.min(main.width) {
                    let buf_x = main.x + x;
                    let cursor_here = x as u32 == state.cursor_x && y as u32 == state.cursor_y;

                    let coord = (x as u32, y as u32, self.generation as u32);
                    let known = state.known_cells.iter().find(|k| (k.x, k.y, k.t) == coord);

                    let (ch, base_style) = match known {
                        Some(k) if k.state == CellState::Alive => {
                            ('o', Style::new().green().add_modifier(Modifier::BOLD))
                        }
                        Some(_) => ('.', Style::new().red().add_modifier(Modifier::BOLD)),
                        None => ('?', Style::new().cyan()),
                    };

                    let style = if cursor_here {
                        base_style.add_modifier(Modifier::REVERSED)
                    } else {
                        base_style
                    };

                    buf.cell_mut((buf_x, buf_y))
                        .unwrap()
                        .set_char(ch)
                        .set_style(style);
                }
                if main.width > w + 1 {
                    let sep_x = main.x + w;
                    buf.cell_mut((sep_x, buf_y))
                        .unwrap()
                        .set_char(if y == h - 1 { '!' } else { '$' })
                        .set_style(Style::new().dark_gray());
                }
            }
        }
    }
}

/// A widget to show the current generation in the RLE format.
#[derive(Debug)]
struct Rle<'b> {
    /// The current generation.
    t: i32,
    /// A reference to the world.
    world: &'b World,
}

impl<'b> Rle<'b> {
    /// Create a new RLE widget from the app.
    const fn new(app: &'b App) -> Self {
        Self {
            t: app.generation,
            world: &app.world,
        }
    }
}

impl Widget for Rle<'_> {
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
            Span::styled(&self.world.config().rule_str, Style::new().cyan()),
        ]);

        buf.set_line(area.x, area.y, &header, area.width);

        if area.height > 1 {
            for y in 0..h.min(area.height - 1) {
                let buf_y = area.y + y + 1;
                for x in 0..w.min(area.width) {
                    let buf_x = area.x + x;
                    let coord = (x as i32, y as i32, self.t);
                    let state = self.world.get_cell_state(coord);
                    let reason = self.world.get_cell_reason(coord);
                    let (ch, style) = match (state, reason) {
                        (Some(CellState::Alive), Some(Reason::Known)) => {
                            ('o', Style::new().green().add_modifier(Modifier::BOLD))
                        }
                        (Some(CellState::Alive), Some(Reason::Deduced)) => {
                            ('o', Style::new().yellow())
                        }
                        (Some(CellState::Alive), _) => ('o', Style::new().green()),
                        (Some(CellState::Dead), Some(Reason::Known)) => {
                            ('.', Style::new().red().add_modifier(Modifier::BOLD))
                        }
                        (Some(CellState::Dead), Some(Reason::Deduced)) => {
                            ('.', Style::new().dark_gray())
                        }
                        (Some(CellState::Dead), _) => ('.', Style::new().gray()),
                        (None, _) => ('?', Style::new().cyan()),
                    };
                    buf.cell_mut((buf_x, buf_y))
                        .unwrap()
                        .set_char(ch)
                        .set_style(style);
                }
                if area.width > w + 1 {
                    let buf_x: u16 = area.x + w;
                    buf.cell_mut((buf_x, buf_y))
                        .unwrap()
                        .set_char(if y == h - 1 { '!' } else { '$' })
                        .set_style(Style::new().dark_gray());
                }
            }
        }
    }
}
