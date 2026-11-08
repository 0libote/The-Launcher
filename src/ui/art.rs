use super::theme::*;
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

pub fn color(name: &str) -> Color32 {
    [
        MINT,
        LILAC,
        CORAL,
        Color32::from_rgb(131, 183, 223),
        Color32::from_rgb(229, 200, 133),
    ][seed(name) as usize % 5]
}
fn seed(name: &str) -> u32 {
    name.bytes().fold(0_u32, |hash, b| {
        hash.wrapping_mul(31).wrapping_add(b as u32)
    })
}

/// Original vector cover labels. Their colors stay tied to the title, not its shelf index.
pub fn cover(p: &egui::Painter, rect: Rect, name: &str) {
    let colors = palette(p.ctx());
    let p = p.with_clip_rect(rect);
    let accent = color(name);
    let s = rect.width() / 180.0;
    let at = |x, y| rect.min + Vec2::new(x, y) * s;
    p.rect_filled(rect, 8.0, accent.gamma_multiply(0.23));
    let center = at(100.0, 105.0);
    match seed(name) % 3 {
        0 => {
            p.circle_filled(center, 54.0 * s, accent.gamma_multiply(0.3));
            for i in 0..5 {
                p.circle_stroke(
                    center,
                    (26.0 + i as f32 * 13.0) * s,
                    Stroke::new(1.4 * s, accent.gamma_multiply(0.7)),
                );
            }
            p.add(egui::Shape::convex_polygon(
                vec![at(0.0, 210.0), at(80.0, 105.0), at(170.0, 220.0)],
                accent.gamma_multiply(0.7),
                Stroke::NONE,
            ));
        }
        1 => {
            p.circle_filled(at(132.0, 65.0), 38.0 * s, accent);
            for i in 0..6 {
                let y = 130.0 + i as f32 * 20.0;
                p.line_segment(
                    [at(0.0, y), at(180.0, y - 60.0)],
                    Stroke::new(6.0 * s, accent.gamma_multiply(0.35)),
                );
            }
            p.add(egui::Shape::convex_polygon(
                vec![at(0.0, 190.0), at(70.0, 80.0), at(150.0, 230.0)],
                colors.surface,
                Stroke::NONE,
            ));
        }
        _ => {
            for i in 0..7 {
                let r = Rect::from_center_size(center, Vec2::splat((25.0 + i as f32 * 20.0) * s));
                p.rect_stroke(
                    r,
                    14.0 * s,
                    Stroke::new(3.0 * s, accent.gamma_multiply(0.25 + i as f32 * 0.08)),
                    StrokeKind::Inside,
                );
            }
            p.circle_filled(center, 12.0 * s, accent);
        }
    }
    p.rect_filled(
        Rect::from_min_size(rect.min, Vec2::new(9.0 * s, rect.height())),
        2.0,
        colors.background.gamma_multiply(0.5),
    );
    p.line_segment(
        [at(17.0, 20.0), at(42.0, 20.0)],
        Stroke::new(3.0 * s, accent),
    );
    let mut job = egui::text::LayoutJob::simple(
        name.into(),
        display(21.0 * s),
        colors.text,
        rect.width() - 38.0 * s,
    );
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = true;
    let galley = p.layout_job(job);
    let base = rect.left_bottom() + Vec2::new(20.0 * s, -20.0 * s - galley.size().y);
    // Dark strip keeps titles readable over each geometric composition.
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left(), base.y - 12.0 * s),
            rect.right_bottom(),
        ),
        0.0,
        colors.background.gamma_multiply(0.9),
    );
    p.galley(base, galley, colors.text);
}

pub fn welcome(p: &egui::Painter, rect: Rect) {
    let colors = palette(p.ctx());
    let p = p.with_clip_rect(rect);
    let s = (rect.width() / 420.0).min(rect.height() / 320.0);
    let origin = rect.center() - Vec2::new(210.0, 160.0) * s;
    let at = |x, y| origin + Vec2::new(x, y) * s;
    let ground = Rect::from_center_size(at(218.0, 276.0), Vec2::new(330.0, 26.0) * s);
    p.rect_filled(ground, 13.0 * s, colors.background.gamma_multiply(0.55));
    for (x, y, w, h, col) in [
        (42.0, 81.0, 100.0, 161.0, LILAC),
        (261.0, 95.0, 103.0, 163.0, colors.accent),
        (133.0, 39.0, 137.0, 220.0, colors.selection),
    ] {
        let r = Rect::from_min_size(at(x, y), Vec2::new(w, h) * s);
        p.add(egui::Shape::convex_polygon(
            vec![
                r.left_top(),
                r.left_top() + Vec2::new(12.0, -8.0) * s,
                r.right_top() + Vec2::new(12.0, -8.0) * s,
                r.right_bottom() + Vec2::new(12.0, -8.0) * s,
                r.right_bottom(),
            ],
            col.gamma_multiply(0.38),
            Stroke::NONE,
        ));
        p.rect_filled(
            r.translate(Vec2::new(0.0, 7.0) * s),
            8.0 * s,
            colors.background.gamma_multiply(0.4),
        );
        p.rect_filled(r, 7.0 * s, col.gamma_multiply(0.5));
        p.rect_stroke(
            r,
            7.0 * s,
            Stroke::new(1.0 * s, col.gamma_multiply(0.8)),
            StrokeKind::Inside,
        );
        let label = r.shrink2(Vec2::new(10.0, 21.0) * s);
        p.rect_filled(label, 4.0 * s, col);
        let c = label.center() - Vec2::new(0.0, 14.0) * s;
        p.circle_stroke(
            c,
            w * 0.24 * s,
            Stroke::new(2.0 * s, colors.background.gamma_multiply(0.35)),
        );
        p.circle_stroke(
            c,
            w * 0.17 * s,
            Stroke::new(2.0 * s, colors.background.gamma_multiply(0.35)),
        );
        p.add(egui::Shape::convex_polygon(
            vec![
                c + Vec2::new(-8.0, -13.0) * s,
                c + Vec2::new(15.0, 0.0) * s,
                c + Vec2::new(-8.0, 13.0) * s,
            ],
            colors.background,
            Stroke::NONE,
        ));
        p.rect_filled(
            Rect::from_min_size(
                label.left_bottom() - Vec2::new(0.0, 31.0) * s,
                Vec2::new(label.width(), 31.0 * s),
            ),
            0.0,
            colors.text.gamma_multiply(0.85),
        );
        for i in 0..4 {
            let x = r.left() + (16.0 + i as f32 * 7.0) * s;
            p.line_segment(
                [
                    Pos2::new(x, r.bottom() - 10.0 * s),
                    Pos2::new(x + 3.0 * s, r.bottom() - 10.0 * s),
                ],
                Stroke::new(2.0 * s, col),
            );
        }
    }
    // A small four-point sparkle, cut from the same hardware language.
    for (x, y, size) in [(330.0, 47.0, 9.0), (71.0, 36.0, 5.0), (380.0, 185.0, 5.0)] {
        let c = at(x, y);
        p.line_segment(
            [c - Vec2::X * size * s, c + Vec2::X * size * s],
            Stroke::new(1.5 * s, colors.muted),
        );
        p.line_segment(
            [c - Vec2::Y * size * s, c + Vec2::Y * size * s],
            Stroke::new(1.5 * s, colors.muted),
        );
    }
    if s >= 0.75 {
        p.text(
            at(208.0, 300.0),
            egui::Align2::CENTER_CENTER,
            "A little shelf. A lot of possibilities.",
            FontId::proportional(11.0 * s),
            colors.muted,
        );
    }
}
