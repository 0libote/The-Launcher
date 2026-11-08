use super::theme::*;
use eframe::egui::{self, Align, Layout, Rect, Sense, Stroke, Vec2};

/// System operations go through winit/Windows, not simulated movement or resizing.
pub fn titlebar(ctx: &egui::Context, name: &str, fullscreen: bool) -> bool {
    if fullscreen {
        return false;
    }
    let colors = palette(ctx);
    let mut edit = false;
    egui::TopBottomPanel::top("launcher_frame")
        .exact_height(36.0)
        .frame(
            egui::Frame::new()
                .fill(colors.navigation)
                .inner_margin(egui::Margin::symmetric(12, 3)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().interact_size.y = 26.0;
            ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for (label, help, command) in [
                    ("×", "Close", egui::ViewportCommand::Close),
                    (
                        "□",
                        "Maximize or restore",
                        egui::ViewportCommand::Maximized(
                            !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                        ),
                    ),
                    ("−", "Minimize", egui::ViewportCommand::Minimized(true)),
                ] {
                    if ui
                        .add_sized([38.0, 26.0], egui::Button::new(label).frame(false))
                        .on_hover_text(help)
                        .clicked()
                    {
                        ctx.send_viewport_cmd(command);
                    }
                }
                edit = ui
                    .button("Customize")
                    .on_hover_text("Build your layout")
                    .clicked();
                let rect = ui.available_rect_before_wrap();
                let response = ui.allocate_rect(rect, Sense::click_and_drag());
                fitted_text(
                    ui.painter(),
                    rect,
                    &format!("The Launcher / {name}"),
                    egui::FontId::proportional(12.0),
                    colors.muted,
                );
                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if response.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                        !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                    ));
                }
            });
        });
    edit
}

pub fn resize_edges(ctx: &egui::Context, fullscreen: bool) {
    if fullscreen || ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let r = ctx.content_rect();
    let t = 5.0;
    let c = 12.0;
    use egui::ResizeDirection::*;
    let edges = [
        (Rect::from_min_size(r.min, Vec2::splat(c)), NorthWest),
        (
            Rect::from_min_size(r.right_top() - Vec2::new(c, 0.0), Vec2::splat(c)),
            NorthEast,
        ),
        (
            Rect::from_min_size(r.left_bottom() - Vec2::new(0.0, c), Vec2::splat(c)),
            SouthWest,
        ),
        (
            Rect::from_min_size(r.max - Vec2::splat(c), Vec2::splat(c)),
            SouthEast,
        ),
        (
            Rect::from_min_max(r.min + Vec2::new(c, 0.0), r.right_top() + Vec2::new(-c, t)),
            North,
        ),
        (
            Rect::from_min_max(
                r.left_bottom() + Vec2::new(c, -t),
                r.max - Vec2::new(c, 0.0),
            ),
            South,
        ),
        (
            Rect::from_min_max(
                r.min + Vec2::new(0.0, c),
                r.left_bottom() + Vec2::new(t, -c),
            ),
            West,
        ),
        (
            Rect::from_min_max(r.right_top() + Vec2::new(-t, c), r.max - Vec2::new(0.0, c)),
            East,
        ),
    ];
    for (index, (rect, direction)) in edges.into_iter().enumerate() {
        egui::Area::new(egui::Id::new(("resize_edge", index)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let (_, response) = ui.allocate_exact_size(rect.size(), Sense::drag());
                if response.hovered() {
                    ctx.set_cursor_icon(match direction {
                        North | South => egui::CursorIcon::ResizeVertical,
                        East | West => egui::CursorIcon::ResizeHorizontal,
                        NorthWest | SouthEast => egui::CursorIcon::ResizeNwSe,
                        _ => egui::CursorIcon::ResizeNeSw,
                    });
                }
                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            });
    }
    ctx.layer_painter(egui::LayerId::background()).rect_stroke(
        r,
        0.0,
        Stroke::new(1.0_f32, palette(ctx).line),
        egui::StrokeKind::Inside,
    );
}
