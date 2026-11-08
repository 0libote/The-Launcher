use eframe::egui::{
    self, Align2, Color32, FontFamily, FontId, Pos2, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};

pub const BG: Color32 = Color32::from_rgb(16, 20, 32);
pub const RAIL: Color32 = Color32::from_rgb(21, 26, 40);
pub const SURFACE: Color32 = Color32::from_rgb(29, 35, 51);
pub const RAISED: Color32 = Color32::from_rgb(38, 46, 64);
pub const TEXT: Color32 = Color32::from_rgb(243, 240, 232);
pub const MUTED: Color32 = Color32::from_rgb(160, 171, 190);
pub const FAINT: Color32 = Color32::from_rgb(132, 145, 165);
pub const LINE: Color32 = Color32::from_rgb(49, 59, 80);
pub const CORAL: Color32 = Color32::from_rgb(255, 151, 124);
pub const MINT: Color32 = Color32::from_rgb(159, 222, 204);
pub const LILAC: Color32 = Color32::from_rgb(183, 175, 224);

pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display".into()))
}
pub fn heading(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(display(size)).color(TEXT)
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "body".into(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Manrope.ttf")).into(),
    );
    fonts.font_data.insert(
        "display".into(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Oxanium.ttf")).into(),
    );
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "body".into());
    let mut display_fonts = vec!["display".into()];
    display_fonts.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts
        .families
        .insert(FontFamily::Name("display".into()), display_fonts);
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = RAIL;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = MINT.gamma_multiply(0.25);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, MINT);
    style.visuals.window_corner_radius = 18.into();
    style.visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = 8.into();
        widget.fg_stroke = Stroke::new(1.0_f32, TEXT);
    }
    style.visuals.widgets.inactive.bg_fill = RAISED;
    style.visuals.widgets.inactive.weak_bg_fill = RAISED;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
    style.visuals.widgets.hovered.bg_fill = LINE;
    style.visuals.widgets.hovered.weak_bg_fill = LINE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MINT);
    style.visuals.widgets.active.bg_fill = RAISED;
    style.visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, MINT);
    style.spacing.item_spacing = Vec2::new(12.0, 12.0);
    style.spacing.button_padding = Vec2::new(16.0, 12.0);
    style.spacing.interact_size.y = 40.0;
    style.spacing.text_edit_width = 460.0;
    style.spacing.window_margin = 24.into();
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, display(24.0));
    ctx.set_style(style);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Library,
    Heart,
    Pc,
    Steam,
    Folder,
    Plus,
    Search,
    Play,
    Close,
    Image,
}

pub fn icon(p: &egui::Painter, center: Pos2, kind: Icon, size: f32, color: Color32) {
    let s = size / 20.0;
    let at = |x: f32, y: f32| center + Vec2::new(x, y) * s;
    let stroke = Stroke::new(1.5 * s, color);
    let line = |a, b| {
        p.line_segment([a, b], stroke);
    };
    let rect = |x, y, w, h| {
        p.rect_stroke(
            Rect::from_min_size(at(x, y), Vec2::new(w, h) * s),
            2.0 * s,
            stroke,
            StrokeKind::Inside,
        );
    };
    match kind {
        Icon::Library => {
            rect(-8.0, -7.0, 5.0, 14.0);
            rect(-1.0, -7.0, 5.0, 14.0);
            line(at(6.0, -7.0), at(9.0, 6.0));
        }
        Icon::Pc => {
            rect(-8.0, -7.0, 16.0, 11.0);
            line(at(0.0, 4.0), at(0.0, 8.0));
            line(at(-4.0, 8.0), at(4.0, 8.0));
        }
        Icon::Folder => {
            p.add(egui::Shape::closed_line(
                vec![
                    at(-8.0, -4.0),
                    at(-8.0, -7.0),
                    at(-2.0, -7.0),
                    at(1.0, -4.0),
                    at(8.0, -4.0),
                    at(8.0, 7.0),
                    at(-8.0, 7.0),
                ],
                stroke,
            ));
        }
        Icon::Heart => {
            p.add(egui::Shape::closed_line(
                vec![
                    at(0.0, 8.0),
                    at(-8.0, 0.0),
                    at(-8.0, -4.0),
                    at(-5.0, -7.0),
                    at(-2.0, -7.0),
                    at(0.0, -4.0),
                    at(2.0, -7.0),
                    at(5.0, -7.0),
                    at(8.0, -4.0),
                    at(8.0, 0.0),
                ],
                stroke,
            ));
        }
        Icon::Steam => {
            p.circle_stroke(at(4.0, -4.0), 4.0 * s, stroke);
            p.circle_stroke(at(-5.0, 5.0), 3.0 * s, stroke);
            line(at(-3.0, 3.0), at(1.0, -2.0));
            line(at(-8.0, 4.0), at(-11.0, 2.0));
        }
        Icon::Plus => {
            line(at(-6.0, 0.0), at(6.0, 0.0));
            line(at(0.0, -6.0), at(0.0, 6.0));
        }
        Icon::Search => {
            p.circle_stroke(at(-2.0, -2.0), 5.5 * s, stroke);
            line(at(2.0, 2.0), at(8.0, 8.0));
        }
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![at(-4.0, -7.0), at(7.0, 0.0), at(-4.0, 7.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Close => {
            line(at(-5.0, -5.0), at(5.0, 5.0));
            line(at(5.0, -5.0), at(-5.0, 5.0));
        }
        Icon::Image => {
            rect(-8.0, -8.0, 16.0, 16.0);
            p.circle_filled(at(3.0, -3.0), 1.5 * s, color);
            line(at(-6.0, 5.0), at(-1.0, -1.0));
            line(at(-1.0, -1.0), at(6.0, 6.0));
        }
    }
}

pub fn button(ui: &mut Ui, label: &str, kind: Icon, primary: bool) -> Response {
    let color = if primary { BG } else { TEXT };
    let response = ui.add(
        egui::Button::new(RichText::new(format!("       {label}")).color(color))
            .fill(if primary { CORAL } else { RAISED })
            .corner_radius(8)
            .min_size(Vec2::new(0.0, 44.0)),
    );
    icon(
        ui.painter(),
        response.rect.left_center() + Vec2::new(21.0, 0.0),
        kind,
        17.0,
        color,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(3.0),
            10.0,
            Stroke::new(2.0_f32, MINT),
            StrokeKind::Outside,
        );
    }
    response
}

pub fn icon_button(ui: &mut Ui, kind: Icon, label: &str) -> Response {
    let response = ui.add(egui::Button::new("").min_size(Vec2::splat(40.0)));
    icon(
        ui.painter(),
        response.rect.center(),
        kind,
        18.0,
        if response.hovered() { TEXT } else { MUTED },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response.on_hover_text(label)
}

pub fn badge(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.into(), FontId::proportional(11.0), color);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 16.0, 24.0), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    ui.painter()
        .rect_filled(rect, 5.0, color.gamma_multiply(0.12));
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
}

pub fn logo(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 10.0, CORAL);
    p.rect_stroke(
        rect.shrink(9.0),
        3.0,
        Stroke::new(2.0_f32, BG),
        StrokeKind::Inside,
    );
    p.line_segment(
        [
            rect.center() + Vec2::new(-3.0, -4.0),
            rect.center() + Vec2::new(-3.0, 4.0),
        ],
        Stroke::new(2.0_f32, BG),
    );
    p.line_segment(
        [
            rect.center() + Vec2::new(-3.0, 4.0),
            rect.center() + Vec2::new(4.0, 4.0),
        ],
        Stroke::new(2.0_f32, BG),
    );
}

pub fn key_hint(ui: &mut Ui, key: &str, label: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(key.into(), FontId::proportional(10.0), MUTED);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(galley.size().x + 12.0, 22.0), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, true, format!("{key}: {label}"))
    });
    ui.painter().rect_filled(rect, 4.0, SURFACE);
    ui.painter()
        .rect_stroke(rect, 4.0, Stroke::new(1.0_f32, LINE), StrokeKind::Inside);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, MUTED);
    ui.label(RichText::new(label).size(11.0).color(FAINT));
}

pub fn fitted_text(p: &egui::Painter, rect: Rect, text: &str, font: FontId, color: Color32) {
    let mut job = egui::text::LayoutJob::simple(text.into(), font, color, rect.width());
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = p.layout_job(job);
    p.with_clip_rect(rect).galley(
        rect.left_center() - Vec2::new(0.0, galley.size().y / 2.0),
        galley,
        color,
    );
}

pub fn source_icon(source: &str) -> Icon {
    if source == "Steam" {
        Icon::Steam
    } else {
        Icon::Pc
    }
}

pub fn empty_mark(p: &egui::Painter, rect: Rect) {
    icon(p, rect.center(), Icon::Search, 32.0, FAINT);
    p.text(
        rect.center() + Vec2::new(0.0, 45.0),
        Align2::CENTER_CENTER,
        "No games found",
        display(24.0),
        TEXT,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_text_colors_meet_normal_text_contrast() {
        let luminance = |color: Color32| {
            let linear = |value: u8| {
                let s = value as f32 / 255.0;
                if s <= 0.04045 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
        };
        for text in [TEXT, MUTED, FAINT] {
            for background in [BG, RAIL, SURFACE] {
                assert!((luminance(text) + 0.05) / (luminance(background) + 0.05) >= 4.5);
            }
        }
    }
}
