use super::*;
impl Launcher {
    fn paint_cover(&mut self, ui: &egui::Ui, rect: egui::Rect, game: &Game) {
        if let Some(texture) = game
            .cover
            .as_ref()
            .and_then(|path| self.covers.texture(ui.ctx(), path))
        {
            egui::Image::new(&texture)
                .uv(covers::crop_uv(texture.size_vec2(), rect.size()))
                .corner_radius(8)
                .paint_at(ui, rect);
        } else {
            art::cover(ui.painter(), rect, &game.name);
        }
    }
    pub(super) fn list_view(&mut self, ui: &mut egui::Ui, visible: &[usize]) {
        let colors = theme::palette(ui.ctx());
        let width = ui.available_width();
        egui::ScrollArea::vertical()
            .id_salt("list_view")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for &index in visible {
                    let game = self.library.games[index].clone();
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(width - 2.0, 74.0), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &game.name)
                    });
                    let selected = self.selected == Some(index);
                    if selected || response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            8.0,
                            if selected {
                                colors.selection.gamma_multiply(0.12)
                            } else {
                                colors.surface
                            },
                        );
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            rect,
                            8.0,
                            Stroke::new(1.0_f32, colors.selection),
                            StrokeKind::Inside,
                        );
                    }
                    self.shelf_focused |= response.has_focus();
                    if selected && self.scroll_selection {
                        response.scroll_to_me_animation(
                            Some(Align::Center),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                    if ui.is_rect_visible(rect) {
                        let cover = egui::Rect::from_min_size(
                            rect.min + Vec2::new(8.0, 7.0),
                            Vec2::new(44.0, 60.0),
                        );
                        self.paint_cover(ui, cover, &game);
                        let title = egui::Rect::from_min_size(
                            rect.min + Vec2::new(66.0, 12.0),
                            Vec2::new((width - 100.0).max(40.0), 23.0),
                        );
                        fitted_text(
                            ui.painter(),
                            title,
                            &game.name,
                            FontId::proportional(15.0),
                            colors.text,
                        );
                        ui.painter().text(
                            title.left_bottom() + Vec2::new(0.0, 15.0),
                            Align2::LEFT_CENTER,
                            format!(
                                "{} / {}",
                                game.source(),
                                match game.installed {
                                    Some(false) => "Not installed",
                                    Some(true) => "Installed",
                                    None if matches!(game.target, Target::Steam { .. }) =>
                                        "Not checked",
                                    None => "Executable",
                                }
                            ),
                            FontId::proportional(11.0),
                            colors.muted,
                        );
                        if game.favorite {
                            icon(
                                ui.painter(),
                                rect.right_center() - Vec2::new(20.0, 0.0),
                                Icon::Heart,
                                16.0,
                                colors.selection,
                            );
                        }
                    }
                    if response.clicked() || response.gained_focus() {
                        self.selected = Some(index);
                    }
                    if response.double_clicked() {
                        self.selected = Some(index);
                        self.play();
                    }
                    response.on_hover_text(&game.name);
                }
            });
        self.scroll_selection = false;
    }
    pub(super) fn shelf_view(&mut self, ui: &mut egui::Ui, visible: &[usize]) {
        let colors = theme::palette(ui.ctx());
        let width = self.workspace.current().layout.tile_width;
        let height = (ui.available_height() - 72.0)
            .clamp(120.0, 320.0)
            .min(width * 1.45);
        egui::ScrollArea::horizontal()
            .id_salt("horizontal_shelf")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for &index in visible {
                        let game = self.library.games[index].clone();
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(width, height + 56.0), Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &game.name)
                        });
                        let selected = self.selected == Some(index);
                        self.shelf_focused |= response.has_focus();
                        if selected && self.scroll_selection {
                            response.scroll_to_me_animation(
                                Some(Align::Center),
                                egui::style::ScrollAnimation::none(),
                            );
                        }
                        if ui.is_rect_visible(rect) {
                            let cover = egui::Rect::from_min_size(
                                rect.min + Vec2::new(6.0, 4.0),
                                Vec2::new(width - 12.0, height - 8.0),
                            );
                            self.paint_cover(ui, cover, &game);
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    cover.left_bottom() + Vec2::new(-6.0, 7.0),
                                    Vec2::new(width, 7.0),
                                ),
                                2.0,
                                colors.raised,
                            );
                            if selected || response.has_focus() || response.hovered() {
                                ui.painter().rect_stroke(
                                    cover.expand(2.0),
                                    10.0,
                                    Stroke::new(2.0_f32, colors.selection),
                                    StrokeKind::Outside,
                                );
                            }
                            fitted_text(
                                ui.painter(),
                                egui::Rect::from_min_size(
                                    rect.min + Vec2::new(6.0, height + 23.0),
                                    Vec2::new(width - 12.0, 24.0),
                                ),
                                &game.name,
                                FontId::proportional(13.0),
                                colors.text,
                            );
                        }
                        if response.clicked() || response.gained_focus() {
                            self.selected = Some(index);
                        }
                        if response.double_clicked() {
                            self.selected = Some(index);
                            self.play();
                        }
                        response.on_hover_text(&game.name);
                    }
                });
            });
        self.scroll_selection = false;
    }
    pub(super) fn details_right(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let Some(index) = self.selected else { return };
        let game = self.library.games[index].clone();
        egui::SidePanel::right("game_details_right")
            .exact_width(
                self.workspace
                    .current()
                    .layout
                    .details_width
                    .min(ctx.available_rect().width() * 0.35),
            )
            .resizable(false)
            .frame(egui::Frame::new().fill(colors.surface).inner_margin(20))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height((ui.available_height() - 116.0).max(80.0))
                    .show(ui, |ui| {
                        let width = ui.available_width();
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(width, width * 1.3), Sense::hover());
                        self.paint_cover(ui, rect, &game);
                        ui.add_space(8.0);
                        ui.label(heading(&game.name, 23.0));
                        badge(ui, game.source(), colors.selection);
                        ui.label(
                            RichText::new(if game.installed == Some(false) {
                                "Not installed"
                            } else {
                                "Ready to play"
                            })
                            .color(colors.muted),
                        );
                    });
                ui.horizontal(|ui| {
                    if button(
                        ui,
                        if game.installed == Some(false) {
                            "Install"
                        } else {
                            "Play"
                        },
                        Icon::Play,
                        true,
                    )
                    .clicked()
                    {
                        self.play();
                    }
                    let favorite = icon_button(
                        ui,
                        Icon::Heart,
                        if game.favorite {
                            "Remove favorite"
                        } else {
                            "Add favorite"
                        },
                    );
                    if game.favorite {
                        icon(
                            ui.painter(),
                            favorite.rect.center(),
                            Icon::Heart,
                            18.0,
                            colors.selection,
                        );
                    }
                    if favorite.clicked() {
                        let result = self.library.change(&self.path, |l| {
                            l.games[index].favorite = !l.games[index].favorite;
                            Ok(())
                        });
                        self.result(result, "Collection updated");
                    }
                });
                self.game_options(ui, index, &game);
            });
    }
}
