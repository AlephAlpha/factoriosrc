use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::Text,
};

#[derive(Debug, Clone, Copy)]
pub struct MainLayout {
    pub top: Rect,
    pub main: Rect,
    pub legend: Rect,
    pub bottom: Rect,
}

#[derive(Debug, Clone, Copy)]
pub struct MarkLayout {
    pub top: Rect,
    pub main: Rect,
    pub bottom: Rect,
}

#[derive(Debug, Clone, Copy)]
pub struct ScrollableArea {
    pub viewport: Rect,
    pub vertical: Option<Rect>,
    pub horizontal: Option<Rect>,
}

#[derive(Debug, Clone, Copy)]
pub struct GridScrollableArea {
    pub grid: Rect,
    pub body: Rect,
    pub vertical: Option<Rect>,
    pub horizontal: Option<Rect>,
}

pub fn split_main_layout(area: Rect) -> MainLayout {
    let legend_height = if area.height >= 8 { 1 } else { 0 };
    let [top, main, legend, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(legend_height),
        Constraint::Length(1),
    ])
    .areas(area);

    MainLayout {
        top,
        main,
        legend,
        bottom,
    }
}

pub fn split_mark_layout(area: Rect) -> MarkLayout {
    let [top, main, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    MarkLayout { top, main, bottom }
}

pub fn centered_popup_rect(area: Rect, text: &Text<'_>) -> Rect {
    let center_x = area.x + area.width / 2;
    let center_y = area.y + area.height / 2;

    let width = area.width.min(text.width() as u16 + 2);
    let height = area.height.min(text.height() as u16 + 2);

    Rect::new(center_x - width / 2, center_y - height / 2, width, height)
}

pub fn clamp_scroll_offset(offset: u16, content_len: u16, viewport_len: u16) -> u16 {
    if viewport_len == 0 {
        0
    } else {
        offset.min(content_len.saturating_sub(viewport_len))
    }
}

pub const fn split_scrollable_area(
    area: Rect,
    content_width: u16,
    content_height: u16,
) -> ScrollableArea {
    if area.width == 0 || area.height == 0 {
        return ScrollableArea {
            viewport: area,
            vertical: None,
            horizontal: None,
        };
    }

    let mut show_horizontal = false;
    let mut show_vertical = false;

    loop {
        let viewport_width = area.width.saturating_sub(if show_vertical { 1 } else { 0 });
        let viewport_height = area
            .height
            .saturating_sub(if show_horizontal { 1 } else { 0 });
        let next_horizontal = content_width > viewport_width && viewport_width > 0;
        let next_vertical = content_height > viewport_height && viewport_height > 0;

        if next_horizontal == show_horizontal && next_vertical == show_vertical {
            let viewport = Rect::new(area.x, area.y, viewport_width, viewport_height);
            let vertical = if show_vertical && viewport.height > 0 {
                Some(Rect::new(
                    area.x + viewport.width,
                    area.y,
                    1,
                    viewport.height,
                ))
            } else {
                None
            };
            let horizontal = if show_horizontal && viewport.width > 0 {
                Some(Rect::new(
                    area.x,
                    area.y + viewport.height,
                    viewport.width,
                    1,
                ))
            } else {
                None
            };
            return ScrollableArea {
                viewport,
                vertical,
                horizontal,
            };
        }

        show_horizontal = next_horizontal;
        show_vertical = next_vertical;
    }
}

pub const fn split_vertical_scrollable_area(area: Rect, content_height: u16) -> ScrollableArea {
    if area.width == 0 || area.height == 0 {
        return ScrollableArea {
            viewport: area,
            vertical: None,
            horizontal: None,
        };
    }

    let show_vertical = content_height > area.height;
    let viewport_width = area.width.saturating_sub(if show_vertical { 1 } else { 0 });
    let viewport = Rect::new(area.x, area.y, viewport_width, area.height);
    let vertical = if show_vertical && viewport.height > 0 {
        Some(Rect::new(
            area.x + viewport.width,
            area.y,
            1,
            viewport.height,
        ))
    } else {
        None
    };

    ScrollableArea {
        viewport,
        vertical,
        horizontal: None,
    }
}

pub fn split_grid_scrollable_area(
    area: Rect,
    content_width: u16,
    content_height: u16,
) -> GridScrollableArea {
    let header_height = area.height.min(1);
    let body_outer = Rect::new(
        area.x,
        area.y.saturating_add(header_height),
        area.width,
        area.height.saturating_sub(header_height),
    );
    let scroll = split_scrollable_area(body_outer, content_width, content_height);
    let grid = Rect::new(
        area.x,
        area.y,
        scroll.viewport.width,
        header_height.saturating_add(scroll.viewport.height),
    );

    GridScrollableArea {
        grid,
        body: scroll.viewport,
        vertical: scroll.vertical,
        horizontal: scroll.horizontal,
    }
}

pub const fn point_in_rect(column: u16, row: u16, rect: Rect) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}
