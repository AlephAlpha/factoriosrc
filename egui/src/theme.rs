use egui::{
    Color32, Context, FontId, RichText, Style, Theme, Visuals,
    text::{LayoutJob, TextFormat},
    vec2,
};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub canvas: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub text: Color32,
    pub subtle_text: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub unknown: Color32,
    pub dead: Color32,
    pub line_end: Color32,
}

impl Palette {
    pub const fn new() -> Self {
        Self {
            canvas: Color32::from_rgb(242, 246, 248),
            surface: Color32::from_rgb(255, 255, 255),
            surface_alt: Color32::from_rgb(227, 238, 242),
            text: Color32::from_rgb(29, 48, 55),
            subtle_text: Color32::from_rgb(96, 118, 126),
            accent: Color32::from_rgb(13, 126, 146),
            accent_soft: Color32::from_rgb(203, 233, 238),
            success: Color32::from_rgb(75, 138, 67),
            warning: Color32::from_rgb(173, 116, 20),
            danger: Color32::from_rgb(177, 61, 61),
            unknown: Color32::from_rgb(117, 94, 163),
            dead: Color32::from_rgb(184, 76, 76),
            line_end: Color32::from_rgb(130, 136, 138),
        }
    }
}

pub fn apply_theme(ctx: &Context) {
    let palette = Palette::new();
    let mut style: Style = (*ctx.style_of(Theme::Light)).clone();

    style.spacing.item_spacing = vec2(8.0, 6.0);
    style.spacing.button_padding = vec2(8.0, 5.0);
    style.spacing.indent = 10.0;

    let mut visuals = Visuals::light();
    visuals.panel_fill = palette.canvas;
    visuals.faint_bg_color = palette.surface_alt;
    visuals.extreme_bg_color = palette.surface;
    visuals.selection.bg_fill = palette.accent;
    visuals.selection.stroke.color = Color32::WHITE;
    visuals.widgets.noninteractive.bg_fill = palette.surface;
    visuals.widgets.noninteractive.fg_stroke.color = palette.text;
    visuals.widgets.inactive.bg_fill = palette.surface;
    visuals.widgets.inactive.fg_stroke.color = palette.text;
    visuals.widgets.hovered.bg_fill = palette.surface_alt;
    visuals.widgets.hovered.fg_stroke.color = palette.text;
    visuals.widgets.active.bg_fill = palette.accent_soft;
    visuals.widgets.active.fg_stroke.color = palette.text;
    visuals.override_text_color = Some(palette.text);

    style.visuals = visuals;
    ctx.set_style_of(Theme::Light, style.clone());
    ctx.set_style_of(Theme::Dark, style);
}

pub fn section_title(text: &str) -> RichText {
    RichText::new(text).size(15.0).strong()
}

pub fn muted(text: &str) -> RichText {
    RichText::new(text).color(Palette::new().subtle_text)
}

pub fn badge_text(text: &str, fill: Color32) -> RichText {
    RichText::new(format!(" {text} "))
        .color(Color32::WHITE)
        .background_color(fill)
        .strong()
}

pub fn rle_layout_job(rle: &str) -> LayoutJob {
    let palette = Palette::new();
    let mut job = LayoutJob::default();

    for ch in rle.chars() {
        let color = match ch {
            'o' => palette.success,
            '.' => palette.dead,
            '?' => palette.unknown,
            '$' | '!' => palette.line_end,
            'x' | 'y' | 'r' => palette.accent,
            '0'..='9' | '=' | ',' | ' ' | '\n' => palette.subtle_text,
            _ => palette.text,
        };
        job.append(
            &ch.to_string(),
            0.0,
            TextFormat {
                color,
                font_id: FontId::monospace(14.0),
                ..Default::default()
            },
        );
    }

    job
}
