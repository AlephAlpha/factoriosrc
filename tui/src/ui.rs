use crate::app::{App, ConfigField, Mode, ViewportOffset};
use crate::layout::{
    centered_popup_rect, clamp_scroll_offset, split_grid_scrollable_area, split_main_layout,
    split_mark_layout, split_scrollable_area, split_vertical_scrollable_area,
};
use factoriosrc_lib::{CellState, ConfigHelpField, Reason, SearchControlHelpField, Status, World};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget},
};

/// The RLE character for a dying state.
const fn dying_char(i: u8) -> char {
    char::from_u32(b'A' as u32 + i as u32 - 1).unwrap()
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    chrome: Style,
    chrome_muted: Style,
    text: Style,
    emphasis: Style,
    accent: Style,
    success: Style,
    warning: Style,
    danger: Style,
    unknown: Style,
    known_alive: Style,
    known_dead: Style,
    deduced: Style,
    guessed_dead: Style,
    border: Style,
}

impl Palette {
    const fn new() -> Self {
        Self {
            chrome: Style::new().fg(Color::Black).bg(Color::Cyan),
            chrome_muted: Style::new().fg(Color::Black).bg(Color::Rgb(165, 214, 227)),
            text: Style::new().fg(Color::White),
            emphasis: Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            accent: Style::new().fg(Color::LightCyan),
            success: Style::new().fg(Color::LightGreen),
            warning: Style::new().fg(Color::LightYellow),
            danger: Style::new().fg(Color::LightRed),
            unknown: Style::new().fg(Color::LightCyan),
            known_alive: Style::new()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
            known_dead: Style::new()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
            deduced: Style::new().fg(Color::Yellow),
            guessed_dead: Style::new().fg(Color::Gray),
            border: Style::new().fg(Color::Rgb(130, 176, 191)),
        }
    }
}

fn metric_spans(
    label: &str,
    value: impl ToString,
    label_style: Style,
    value_style: Style,
) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{label}: "), label_style),
        Span::styled(value.to_string(), value_style),
    ]
}

fn status_style(status: Status, mode: Mode, palette: Palette) -> Style {
    match status {
        Status::Solved => palette.success.add_modifier(Modifier::BOLD),
        Status::NoSolution => palette.danger.add_modifier(Modifier::BOLD),
        Status::Running if mode == Mode::Running => palette.warning.add_modifier(Modifier::BOLD),
        Status::Running => palette.accent.add_modifier(Modifier::BOLD),
        Status::NotStarted => palette.chrome_muted,
    }
}

fn config_label_width(content_width: u16) -> u16 {
    content_width
        .saturating_div(3)
        .clamp(8, 20)
        .min(content_width.saturating_sub(2))
}

const fn help_text() -> &'static str {
    "Search View\n\
     [Space]/[Enter] Start or pause the search\n\
     [Arrow Keys]    Pan across the current result\n\
     [PgUp]/[PgDn]   Pan faster by one page\n\
     [=] / [-]       Move to the next / previous generation\n\
     [n] / [p]       Browse found solutions when paused\n\
     \n\
     Configuration\n\
     [o]             Open the configuration form\n\
     [Tab]/[Shift+Tab] or [Up]/[Down]\n\
                     Move between fields in the form\n\
     [Enter]/[Space] Use the current field or button\n\
     [Left]/[Right]  Change toggles and enumerated fields\n\
     \n\
     Known Cells\n\
     [Arrows]        Move the cursor\n\
     [PgUp]/[PgDn]   Move by one visible page\n\
     [Space]         Cycle unknown -> alive -> dead -> unknown\n\
     [Enter]         Save known-cell edits and return\n\
     \n\
     General\n\
     [c]             Copy the current RLE text\n\
     [h]             Open or close this help window\n\
     [q]/[Esc]       Quit or close the current overlay"
}

const fn config_field_help(field: ConfigField) -> &'static str {
    match field {
        ConfigField::RuleString => ConfigHelpField::RuleString.short_help(),
        ConfigField::Width => ConfigHelpField::Width.short_help(),
        ConfigField::Height => ConfigHelpField::Height.short_help(),
        ConfigField::Period => ConfigHelpField::Period.short_help(),
        ConfigField::Dx => ConfigHelpField::Dx.short_help(),
        ConfigField::Dy => ConfigHelpField::Dy.short_help(),
        ConfigField::DiagonalWidth => ConfigHelpField::DiagonalWidth.short_help(),
        ConfigField::Symmetry => ConfigHelpField::Symmetry.short_help(),
        ConfigField::Transformation => ConfigHelpField::Transformation.short_help(),
        ConfigField::SearchOrder => ConfigHelpField::SearchOrder.short_help(),
        ConfigField::NewState => ConfigHelpField::NewState.short_help(),
        ConfigField::Seed => ConfigHelpField::Seed.short_help(),
        ConfigField::MaxPopulation => ConfigHelpField::MaxPopulation.short_help(),
        ConfigField::ReduceMaxPopulation => ConfigHelpField::ReduceMaxPopulation.short_help(),
        ConfigField::KnownCells => ConfigHelpField::KnownCells.short_help(),
        ConfigField::IncreaseWorldSize => SearchControlHelpField::IncreaseWorldSize.short_help(),
        ConfigField::NoStop => SearchControlHelpField::NoStop.short_help(),
        ConfigField::Apply => "Validate the current settings and rebuild the search world.",
        ConfigField::Cancel => "Discard configuration edits and return to the search view.",
    }
}

fn wrap_text_lines(text: &str, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }

    let mut wrapped = Vec::new();
    let width = width as usize;

    for line in text.lines() {
        if line.is_empty() {
            wrapped.push(Line::from(""));
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        for chunk in chars.chunks(width.max(1)) {
            let chunk: String = chunk.iter().collect();
            wrapped.push(Line::from(chunk));
        }
    }

    wrapped
}

fn render_vertical_scrollbar(
    frame: &mut Frame,
    area: Option<Rect>,
    content_length: u16,
    viewport_length: u16,
    position: u16,
    palette: Palette,
) {
    let Some(area) = area else {
        return;
    };
    if area.width == 0 || area.height == 0 || content_length <= viewport_length {
        return;
    }

    let scroll_range = content_length
        .saturating_sub(viewport_length)
        .saturating_add(1);
    let mut state = ScrollbarState::new(scroll_range.max(1) as usize)
        .position(position as usize)
        .viewport_content_length(viewport_length.max(1) as usize);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_style(palette.border)
        .thumb_style(palette.accent.add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

fn render_horizontal_scrollbar(
    frame: &mut Frame,
    area: Option<Rect>,
    content_length: u16,
    viewport_length: u16,
    position: u16,
    palette: Palette,
) {
    let Some(area) = area else {
        return;
    };
    if area.width == 0 || area.height == 0 || content_length <= viewport_length {
        return;
    }

    let scroll_range = content_length
        .saturating_sub(viewport_length)
        .saturating_add(1);
    let mut state = ScrollbarState::new(scroll_range.max(1) as usize)
        .position(position as usize)
        .viewport_content_length(viewport_length.max(1) as usize);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
        .begin_symbol(None)
        .end_symbol(None)
        .track_style(palette.border)
        .thumb_style(palette.accent.add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(scrollbar, area, &mut state);
}

impl App {
    /// Render the TUI interface.
    pub fn render(&mut self, frame: &mut Frame) {
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
                let layout = split_main_layout(area);

                self.render_top_bar(frame, layout.top);
                self.render_main(frame, layout.main);
                self.render_legend_bar(frame, layout.legend);
                self.render_bottom_bar(frame, layout.bottom);

                match self.mode {
                    Mode::Usage => self.render_help(frame, layout.main),
                    Mode::Quit => self.render_quit(frame, layout.main),
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
        let palette = Palette::new();
        let chunks = Layout::horizontal([
            Constraint::Length(18),
            Constraint::Length(22),
            Constraint::Length(18),
            Constraint::Min(16),
        ])
        .split(area);

        if let Some(i) = self.viewing_solution {
            let label = Paragraph::new(Line::from(metric_spans(
                "Solution",
                format!("{}/{}", i + 1, self.solutions.len()),
                palette.chrome,
                palette.emphasis,
            )))
            .style(palette.chrome);
            frame.render_widget(label, chunks[0]);

            let gen_label = Paragraph::new(Line::from(metric_spans(
                "Generation",
                format!("{}/{}", self.generation, self.world.config().period - 1),
                palette.chrome,
                palette.emphasis,
            )))
            .style(palette.chrome);
            frame.render_widget(gen_label, chunks[1]);

            let count = Paragraph::new(Line::from(metric_spans(
                "Stored",
                self.solutions.len(),
                palette.chrome,
                palette.emphasis,
            )))
            .style(palette.chrome);
            frame.render_widget(count, chunks[2]);

            let badge = Paragraph::new(Line::from(vec![
                Span::styled("VIEWING ", palette.chrome),
                Span::styled("SOLUTION", palette.warning.add_modifier(Modifier::BOLD)),
            ]))
            .style(palette.chrome_muted);
            frame.render_widget(badge, chunks[3]);
        } else {
            let generation = Paragraph::new(Line::from(metric_spans(
                "Generation",
                self.generation,
                palette.chrome,
                palette.emphasis,
            )))
            .style(palette.chrome);
            frame.render_widget(generation, chunks[0]);

            let population = Paragraph::new(Line::from(metric_spans(
                "Population",
                self.world.population(self.generation),
                palette.chrome,
                palette.emphasis,
            )))
            .style(palette.chrome);
            frame.render_widget(population, chunks[1]);

            let solution_count = Paragraph::new(Line::from(metric_spans(
                "Solutions",
                self.solutions.len(),
                palette.chrome,
                palette.emphasis,
            )))
            .style(palette.chrome);
            frame.render_widget(solution_count, chunks[2]);

            let status = self.world.status();
            let status_text = match status {
                Status::NotStarted => "READY",
                Status::Running if self.mode == Mode::Running => "RUNNING",
                Status::Running => "PAUSED",
                Status::Solved => "SOLVED",
                Status::NoSolution => "EXHAUSTED",
            };
            let elapsed = if self.mode == Mode::Running {
                String::from("live")
            } else {
                format!("{:.3?}", self.elapsed)
            };
            let badge = Paragraph::new(Line::from(vec![
                Span::styled("STATE ", palette.chrome),
                Span::styled(status_text, status_style(status, self.mode, palette)),
                Span::styled("  TIME ", palette.chrome),
                Span::styled(elapsed, palette.emphasis),
            ]))
            .style(palette.chrome_muted);
            frame.render_widget(badge, chunks[3]);
        }
    }

    /// Render the bottom bar.
    ///
    /// This includes the current status, mode, and a short help message.
    fn render_bottom_bar(&self, frame: &mut Frame, area: Rect) {
        let palette = Palette::new();
        let chunks = Layout::horizontal([Constraint::Min(24), Constraint::Length(34)]).split(area);

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

        let status = Paragraph::new(Line::from(vec![
            Span::styled("Status: ", palette.chrome),
            Span::styled(status_str, palette.emphasis),
        ]))
        .style(palette.chrome);
        frame.render_widget(status, chunks[0]);

        let help = Paragraph::new("Pan: arrows/PgUp/PgDn  |  [h] help").style(palette.chrome_muted);
        frame.render_widget(help, chunks[1]);
    }

    /// Render the main area.
    fn render_main(&mut self, frame: &mut Frame, area: Rect) {
        let palette = Palette::new();
        if let Some(i) = self.viewing_solution {
            let rle = self.current_rle();
            let title = format!("Solution {}/{}", i + 1, self.solutions.len());
            let text = Text::from(rle.as_str());
            let content_width = text.width() as u16;
            let content_height = text.height() as u16;
            let block = Block::bordered().border_style(palette.border).title(title);
            frame.render_widget(block, area);

            let inner = area.inner(Margin {
                vertical: 1,
                horizontal: 1,
            });
            let scroll = split_scrollable_area(inner, content_width, content_height);
            let scroll_x = clamp_scroll_offset(
                self.ui_state.search_viewport.x,
                content_width,
                scroll.viewport.width,
            );
            let scroll_y = clamp_scroll_offset(
                self.ui_state.search_viewport.y,
                content_height,
                scroll.viewport.height,
            );
            self.ui_state.search_viewport = ViewportOffset {
                x: scroll_x,
                y: scroll_y,
            };

            let paragraph = Paragraph::new(rle)
                .style(palette.text)
                .scroll((scroll_y, scroll_x));
            frame.render_widget(paragraph, scroll.viewport);
            render_vertical_scrollbar(
                frame,
                scroll.vertical,
                content_height,
                scroll.viewport.height,
                scroll_y,
                palette,
            );
            render_horizontal_scrollbar(
                frame,
                scroll.horizontal,
                content_width,
                scroll.viewport.width,
                scroll_x,
                palette,
            );
        } else {
            let content_width = self.world.config().width as u16;
            let content_height = self.world.config().height as u16;
            let scroll = split_grid_scrollable_area(area, content_width, content_height);
            let viewport_x = clamp_scroll_offset(
                self.ui_state.search_viewport.x,
                content_width,
                scroll.body.width,
            );
            let viewport_y = clamp_scroll_offset(
                self.ui_state.search_viewport.y,
                content_height,
                scroll.body.height,
            );
            self.ui_state.search_viewport = ViewportOffset {
                x: viewport_x,
                y: viewport_y,
            };

            let rle = Rle {
                t: self.generation,
                world: &self.world,
                viewport_x,
                viewport_y,
            };
            frame.render_widget(rle, scroll.grid);
            render_vertical_scrollbar(
                frame,
                scroll.vertical,
                content_height,
                scroll.body.height,
                viewport_y,
                palette,
            );
            render_horizontal_scrollbar(
                frame,
                scroll.horizontal,
                content_width,
                scroll.body.width,
                viewport_x,
                palette,
            );
        }
    }

    /// Render the legend bar showing cell symbol and color meanings.
    fn render_legend_bar(&self, frame: &mut Frame, area: Rect) {
        let palette = Palette::new();
        let spans = vec![
            Span::styled("Alive ", palette.emphasis),
            Span::styled("o", palette.success),
            Span::raw(" ("),
            Span::styled("K", palette.known_alive),
            Span::raw("/"),
            Span::styled("D", palette.deduced),
            Span::raw("/"),
            Span::styled("G", palette.success),
            Span::raw(")   "),
            Span::styled("Dead ", palette.emphasis),
            Span::styled(".", palette.danger),
            Span::raw(" ("),
            Span::styled("K", palette.known_dead),
            Span::raw("/"),
            Span::styled("D", palette.guessed_dead),
            Span::raw("/"),
            Span::styled("G", palette.guessed_dead),
            Span::raw(")   "),
            Span::styled("? Unknown   ", palette.unknown),
            Span::styled("K", palette.known_alive),
            Span::raw(" known   "),
            Span::styled("D", palette.deduced),
            Span::raw(" deduced   "),
            Span::styled("G", palette.success),
            Span::raw(" guessed"),
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
        let palette = Palette::new();
        let rect = centered_popup_rect(area, &text);

        frame.render_widget(Clear, rect);

        let paragraph = Paragraph::new(text)
            .block(Block::bordered().border_style(palette.border).title(title))
            .style(style);

        frame.render_widget(paragraph, rect);
    }

    /// Render the popup window to show the help message.
    fn render_help(&mut self, frame: &mut Frame, area: Rect) {
        let palette = Palette::new();
        let rect = area.inner(Margin {
            vertical: if area.height > 4 { 1 } else { 0 },
            horizontal: if area.width > 6 { 2 } else { 0 },
        });
        let rect = if rect.width < 3 || rect.height < 3 {
            area
        } else {
            rect
        };

        frame.render_widget(Clear, rect);
        frame.render_widget(
            Block::bordered().border_style(palette.border).title("Help"),
            rect,
        );

        let inner = rect.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });
        let lines = wrap_text_lines(help_text(), inner.width.max(1));
        let scroll = split_vertical_scrollable_area(inner, lines.len() as u16);
        let offset = clamp_scroll_offset(
            self.ui_state.help_scroll,
            lines.len() as u16,
            scroll.viewport.height,
        ) as usize;
        self.ui_state.help_scroll = offset as u16;
        let visible_lines: Vec<Line> = lines
            .iter()
            .skip(offset)
            .take(scroll.viewport.height as usize)
            .cloned()
            .collect();

        frame.render_widget(
            Paragraph::new(Text::from(visible_lines)).style(palette.success),
            scroll.viewport,
        );
        render_vertical_scrollbar(
            frame,
            scroll.vertical,
            lines.len() as u16,
            scroll.viewport.height,
            offset as u16,
            palette,
        );
    }

    /// Render the popup window to ask the user to confirm quitting.
    fn render_quit(&self, frame: &mut Frame, area: Rect) {
        self.render_popup(
            frame,
            area,
            "Are you sure you want to quit? ([y]/[n])",
            "Quit",
            Palette::new().warning,
        );
    }

    // ── Config form rendering ──

    /// Render the full-screen configuration form.
    fn render_config_form(&self, frame: &mut Frame, area: Rect) {
        let Some(ref state) = self.config_state else {
            return;
        };
        let palette = Palette::new();
        let block = Block::bordered()
            .border_style(palette.border)
            .title("Configuration");
        frame.render_widget(block, area);

        let inner = area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });
        let label_width = config_label_width(inner.width);
        let label_text_width = label_width.saturating_sub(2) as usize;

        let mut lines: Vec<Line> = Vec::new();
        let mut field_line_indices = vec![0usize; state.fields.len()];

        for (i, field) in state.fields.iter().enumerate() {
            let is_focused = i == state.focus_index;

            if field.is_button() {
                let btn_text = match field {
                    ConfigField::Apply => "[ Apply ]".to_string(),
                    ConfigField::Cancel => "[ Cancel ]".to_string(),
                    _ => unreachable!(),
                };
                let style = if is_focused {
                    palette.emphasis
                } else {
                    palette.accent
                };
                if matches!(field, ConfigField::Apply) {
                    lines.push(Line::from(""));
                }
                field_line_indices[i] = lines.len();
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
                field_line_indices[i] = lines.len();
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
                    palette.success
                } else {
                    match field {
                        ConfigField::KnownCells => palette.guessed_dead,
                        _ => palette.text,
                    }
                };

                lines.push(Line::from(vec![
                    Span::styled(prefix, value_style),
                    Span::styled(
                        format!("{label:<width$}", width = label_text_width),
                        palette.text,
                    ),
                    Span::styled(display_val, value_style),
                ]));
            }
        }

        // Error message.
        if let Some(ref error) = state.error {
            lines.push(Line::from(Span::styled(
                format!("! {error}"),
                palette.danger,
            )));
        }

        // Short help line (only when no error).
        if state.error.is_none() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Tab/Shift+Tab: navigate  |  Enter/Space: use field  |  Left/Right: change  |  Esc: cancel",
                palette.guessed_dead,
            )));
            lines.push(Line::from(Span::styled(
                config_field_help(state.fields[state.focus_index]),
                palette.chrome_muted,
            )));
        }

        // Auto-scroll: keep the focused field visible in small terminals.
        let total = lines.len();
        let scroll = split_vertical_scrollable_area(inner, total as u16);
        let viewport_height = scroll.viewport.height as usize;
        let focused_line_index = field_line_indices[state.focus_index];
        if total > viewport_height {
            let max_offset = total.saturating_sub(viewport_height);
            let cur = state.scroll_offset.get().min(max_offset);
            let new_offset = if focused_line_index < cur {
                focused_line_index
            } else if focused_line_index >= cur + viewport_height {
                focused_line_index
                    .saturating_add(1)
                    .saturating_sub(viewport_height)
            } else {
                cur
            };
            state.scroll_offset.set(new_offset.min(max_offset));
        } else {
            state.scroll_offset.set(0);
        }

        let offset = state.scroll_offset.get();
        let visible_lines: Vec<Line> = lines
            .iter()
            .skip(offset)
            .take(viewport_height)
            .cloned()
            .collect();

        let paragraph = Paragraph::new(Text::from(visible_lines)).style(palette.text);

        frame.render_widget(paragraph, scroll.viewport);
        render_vertical_scrollbar(
            frame,
            scroll.vertical,
            total as u16,
            scroll.viewport.height,
            offset as u16,
            palette,
        );

        // Cursor position for text editing.
        if let Some(field) = state.fields.get(state.focus_index)
            && field.is_text_field()
            && focused_line_index >= offset
            && focused_line_index < offset.saturating_add(viewport_height)
        {
            let inner_x = scroll.viewport.x + label_width;
            let inner_y = scroll.viewport.y + (focused_line_index - offset) as u16;
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
            Palette::new().warning,
        );
    }

    /// Render the mark-known-cells view.
    fn render_mark_view(&mut self, frame: &mut Frame, area: Rect) {
        let Some(_) = self.mark_state else {
            return;
        };
        let palette = Palette::new();
        let layout = split_mark_layout(area);
        let (w, h, period, rule_str) = self
            .config_state
            .as_ref()
            .map(|state| {
                (
                    state.working_config.width as u16,
                    state.working_config.height as u16,
                    state.working_config.period,
                    state.working_config.rule_str.clone(),
                )
            })
            .unwrap_or_else(|| {
                (
                    self.world.config().width as u16,
                    self.world.config().height as u16,
                    self.world.config().period,
                    self.world.config().rule_str.clone(),
                )
            });
        let scroll = split_grid_scrollable_area(layout.main, w, h);
        let (viewport_x, viewport_y) = {
            let state = self
                .mark_state
                .as_ref()
                .expect("mark state must exist while rendering mark view");
            (
                clamp_scroll_offset(state.viewport.x, w, scroll.body.width),
                clamp_scroll_offset(state.viewport.y, h, scroll.body.height),
            )
        };
        if let Some(state) = &mut self.mark_state {
            state.viewport = ViewportOffset {
                x: viewport_x,
                y: viewport_y,
            };
        }
        let state = self
            .mark_state
            .as_ref()
            .expect("mark state must exist while rendering mark view");

        // Top bar: status.
        let known_count = state.known_cells.len();
        let gen_str = format!("{}/{}", self.generation, period.saturating_sub(1));
        let top_text = format!(
            "Known: {} cell(s)  |  Position: ({}, {})  |  Gen: {}",
            known_count, state.cursor_x, state.cursor_y, gen_str,
        );
        frame.render_widget(Paragraph::new(top_text).style(palette.chrome), layout.top);

        // Bottom bar: keybindings.
        let bottom_text =
            "[Arrows] move  [PgUp/PgDn] page  [=/-] gen  [Space] cycle  [Enter] save  [Esc] cancel";
        frame.render_widget(
            Paragraph::new(bottom_text).style(palette.chrome_muted),
            layout.bottom,
        );

        // Grid.
        let buf = frame.buffer_mut();

        // Header: x = W, y = H, rule = RULE
        let header = Line::from(vec![
            Span::styled("x", palette.accent),
            Span::raw(" = "),
            Span::styled(w.to_string(), palette.unknown),
            Span::raw(", "),
            Span::styled("y", palette.accent),
            Span::raw(" = "),
            Span::styled(h.to_string(), palette.unknown),
            Span::raw(", "),
            Span::styled("rule", palette.accent),
            Span::raw(" = "),
            Span::styled(rule_str, palette.unknown),
        ]);
        buf.set_line(scroll.grid.x, scroll.grid.y, &header, scroll.grid.width);

        if scroll.body.height > 0 {
            for local_y in 0..h.saturating_sub(viewport_y).min(scroll.body.height) {
                let buf_y = scroll.body.y + local_y;
                let world_y = local_y + viewport_y;
                for local_x in 0..w.saturating_sub(viewport_x).min(scroll.body.width) {
                    let buf_x = scroll.body.x + local_x;
                    let world_x = local_x + viewport_x;
                    let cursor_here =
                        world_x as u32 == state.cursor_x && world_y as u32 == state.cursor_y;

                    let coord = (world_x as u32, world_y as u32, self.generation as u32);
                    let known = state.known_cells.iter().find(|k| (k.x, k.y, k.t) == coord);

                    let (ch, base_style) =
                        known.map_or(('?', palette.unknown), |k| match k.state {
                            CellState::Alive => ('o', palette.known_alive),
                            CellState::Dying(i) => (dying_char(i), palette.known_alive),
                            CellState::Dead => ('.', palette.known_dead),
                        });

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
                if scroll.body.width > w.saturating_sub(viewport_x) + 1 {
                    let sep_x = scroll.body.x + w.saturating_sub(viewport_x);
                    buf.cell_mut((sep_x, buf_y))
                        .unwrap()
                        .set_char(if world_y == h - 1 { '!' } else { '$' })
                        .set_style(palette.guessed_dead);
                }
            }
        }

        render_vertical_scrollbar(
            frame,
            scroll.vertical,
            h,
            scroll.body.height,
            viewport_y,
            palette,
        );
        render_horizontal_scrollbar(
            frame,
            scroll.horizontal,
            w,
            scroll.body.width,
            viewport_x,
            palette,
        );
    }
}

/// A widget to show the current generation in the RLE format.
#[derive(Debug)]
struct Rle<'b> {
    /// The current generation.
    t: i32,
    /// A reference to the world.
    world: &'b World,
    /// Current viewport offset inside the visible world.
    viewport_x: u16,
    viewport_y: u16,
}

impl Widget for Rle<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let palette = Palette::new();
        let w = self.world.config().width as u16;
        let h = self.world.config().height as u16;

        let header = Line::from(vec![
            Span::styled("x", palette.accent),
            Span::raw(" = "),
            Span::styled(w.to_string(), palette.unknown),
            Span::raw(", "),
            Span::styled("y", palette.accent),
            Span::raw(" = "),
            Span::styled(h.to_string(), palette.unknown),
            Span::raw(", "),
            Span::styled("rule", palette.accent),
            Span::raw(" = "),
            Span::styled(&self.world.config().rule_str, palette.unknown),
        ]);

        buf.set_line(area.x, area.y, &header, area.width);

        if area.height > 1 {
            for local_y in 0..h.saturating_sub(self.viewport_y).min(area.height - 1) {
                let buf_y = area.y + local_y + 1;
                let world_y = local_y + self.viewport_y;
                for local_x in 0..w.saturating_sub(self.viewport_x).min(area.width) {
                    let buf_x = area.x + local_x;
                    let world_x = local_x + self.viewport_x;
                    let coord = (world_x as i32, world_y as i32, self.t);
                    let state = self.world.get_cell_state(coord);
                    let reason = self.world.get_cell_reason(coord);
                    let alive_char = if self.world.is_generations_rule() {
                        'A'
                    } else {
                        'o'
                    };
                    let (ch, style) = match (state, reason) {
                        (Some(CellState::Alive), Some(Reason::Known)) => {
                            (alive_char, palette.known_alive)
                        }
                        (Some(CellState::Alive), Some(Reason::Deduced)) => {
                            (alive_char, palette.deduced)
                        }
                        (Some(CellState::Alive), _) => (alive_char, palette.success),
                        (Some(CellState::Dying(i)), Some(Reason::Known)) => {
                            (dying_char(i), palette.known_alive)
                        }
                        (Some(CellState::Dying(i)), Some(Reason::Deduced)) => {
                            (dying_char(i), palette.deduced)
                        }
                        (Some(CellState::Dying(i)), _) => (dying_char(i), palette.success),
                        (Some(CellState::Dead), Some(Reason::Known)) => ('.', palette.known_dead),
                        (Some(CellState::Dead), Some(Reason::Deduced)) => {
                            ('.', palette.guessed_dead)
                        }
                        (Some(CellState::Dead), _) => ('.', palette.guessed_dead),
                        (None, _) => ('?', palette.unknown),
                    };
                    buf.cell_mut((buf_x, buf_y))
                        .unwrap()
                        .set_char(ch)
                        .set_style(style);
                }
                if area.width > w.saturating_sub(self.viewport_x) + 1 {
                    let buf_x = area.x + w.saturating_sub(self.viewport_x);
                    buf.cell_mut((buf_x, buf_y))
                        .unwrap()
                        .set_char(if world_y == h - 1 { '!' } else { '$' })
                        .set_style(palette.guessed_dead);
                }
            }
        }
    }
}
