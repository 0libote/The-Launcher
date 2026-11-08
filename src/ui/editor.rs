use super::*;
use eframe::egui::Rect;
#[derive(Clone, Copy)]
pub(super) enum Panel {
    Navigation,
    Details,
    Tools,
}

impl Launcher {
    pub(super) fn open_editor(&mut self) {
        if self.editor_original.is_none() {
            self.notice = None;
            self.draft = None;
            self.remove = false;
            self.steam_settings = false;
            self.steam_key.clear();
            self.editor_original = Some(self.workspace.clone());
        }
    }
    pub(super) fn workspace_editor(&mut self, ctx: &egui::Context) {
        if self.editor_original.is_none() {
            return;
        }
        let before = self.workspace.current().skin.clone();
        let mut save = false;
        let mut cancel = false;
        let colors = theme::palette(ctx);
        egui::Modal::new(egui::Id::new("workspace_editor"))
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .stroke(Stroke::new(1.0_f32, colors.line))
                    .corner_radius(12)
                    .inner_margin(22),
            )
            .show(ctx, |ui| {
                ui.set_width(680.0);
                ui.label(heading("Build your workspace", 26.0));
                ui.label(
                    RichText::new("Arrange panels, pick a view, and make the colors yours.")
                        .color(colors.muted),
                );
                egui::ScrollArea::vertical()
                    .id_salt("workspace_editor_scroll")
                    .max_height((ctx.content_rect().height() - 240.0).max(200.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Saved layout");
                            egui::ComboBox::from_id_salt("saved_layouts")
                                .selected_text(&self.workspace.current().name)
                                .show_ui(ui, |ui| {
                                    for index in 0..self.workspace.layouts.len() {
                                        ui.selectable_value(
                                            &mut self.workspace.active,
                                            index,
                                            &self.workspace.layouts[index].name,
                                        );
                                    }
                                });
                            if ui.button("Duplicate").clicked() {
                                let result = self.workspace.duplicate();
                                self.result(result, "Layout copied");
                            }
                            if ui
                                .add_enabled(
                                    self.workspace.layouts.len() > 1,
                                    egui::Button::new("Delete"),
                                )
                                .clicked()
                            {
                                self.workspace.layouts.remove(self.workspace.active);
                                self.workspace.active = 0;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.workspace.current_mut().name)
                                    .desired_width(260.0)
                                    .char_limit(64),
                            );
                        });
                        ui.add_space(8.0);
                        ui.label("Drag a panel onto an edge in the preview:");
                        ui.horizontal(|ui| {
                            for (panel, label) in [
                                (Panel::Navigation, "Navigation"),
                                (Panel::Details, "Game details"),
                                (Panel::Tools, "Tools"),
                            ] {
                                ui.dnd_drag_source(
                                    egui::Id::new(("panel_source", label)),
                                    panel,
                                    |ui| {
                                        ui.label(
                                            RichText::new(format!("Drag {label}"))
                                                .color(colors.selection),
                                        );
                                    },
                                );
                            }
                        });
                        layout_canvas(ui, &mut self.workspace.current_mut().layout);
                        let layout = &mut self.workspace.current_mut().layout;
                        egui::Grid::new("panel_positions")
                            .num_columns(2)
                            .spacing([16.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("Navigation");
                                egui::ComboBox::from_id_salt("navigation_position")
                                    .selected_text(format!("{:?}", layout.navigation))
                                    .show_ui(ui, |ui| {
                                        for (v, t) in [
                                            (Navigation::Left, "Left"),
                                            (Navigation::Right, "Right"),
                                            (Navigation::Hidden, "Hidden"),
                                        ] {
                                            ui.selectable_value(&mut layout.navigation, v, t);
                                        }
                                    });
                                ui.end_row();
                                ui.label("Game details");
                                egui::ComboBox::from_id_salt("details_position")
                                    .selected_text(format!("{:?}", layout.details))
                                    .show_ui(ui, |ui| {
                                        for (v, t) in [
                                            (Details::Bottom, "Bottom"),
                                            (Details::Right, "Right"),
                                            (Details::Hidden, "Hidden"),
                                        ] {
                                            ui.selectable_value(&mut layout.details, v, t);
                                        }
                                    });
                                ui.end_row();
                                ui.label("Tools");
                                egui::ComboBox::from_id_salt("tools_position")
                                    .selected_text(format!("{:?}", layout.tools))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut layout.tools, Tools::Top, "Top");
                                        ui.selectable_value(
                                            &mut layout.tools,
                                            Tools::Bottom,
                                            "Bottom",
                                        );
                                    });
                                ui.end_row();
                                ui.label("Game view");
                                ui.horizontal(|ui| {
                                    for (v, t) in [
                                        (View::Grid, "Grid"),
                                        (View::List, "List"),
                                        (View::Shelf, "Shelf"),
                                    ] {
                                        ui.selectable_value(&mut layout.view, v, t);
                                    }
                                });
                                ui.end_row();
                                ui.label("Cover width");
                                ui.add(
                                    egui::Slider::new(&mut layout.tile_width, 130.0..=320.0)
                                        .suffix(" pt"),
                                );
                                ui.end_row();
                                ui.label("Navigation width");
                                ui.add(
                                    egui::Slider::new(&mut layout.navigation_width, 160.0..=280.0)
                                        .suffix(" pt"),
                                );
                                ui.end_row();
                                ui.label("Details width");
                                ui.add(
                                    egui::Slider::new(&mut layout.details_width, 220.0..=340.0)
                                        .suffix(" pt"),
                                );
                                ui.end_row();
                                ui.label("Shortcut hints");
                                ui.checkbox(&mut layout.show_shortcuts, "Show");
                                ui.end_row();
                            });
                        ui.separator();
                        ui.label(heading("Skin", 19.0));
                        let skin = &mut self.workspace.current_mut().skin;
                        egui::Grid::new("skin_colors")
                            .num_columns(4)
                            .spacing([16.0, 8.0])
                            .show(ui, |ui| {
                                for (index, (label, value)) in [
                                    ("Canvas", &mut skin.background),
                                    ("Navigation", &mut skin.navigation),
                                    ("Panels", &mut skin.surface),
                                    ("Controls", &mut skin.raised),
                                    ("Text", &mut skin.text),
                                    ("Secondary text", &mut skin.muted),
                                    ("Quiet text", &mut skin.faint),
                                    ("Borders", &mut skin.line),
                                    ("Actions", &mut skin.accent),
                                    ("Selection", &mut skin.selection),
                                ]
                                .into_iter()
                                .enumerate()
                                {
                                    ui.label(label);
                                    ui.color_edit_button_srgb(value);
                                    if index % 2 == 1 {
                                        ui.end_row();
                                    }
                                }
                            });
                        if ui.button("Reset this layout").clicked() {
                            let name = self.workspace.current().name.clone();
                            *self.workspace.current_mut() = the_launcher::workspace::NamedLayout {
                                name,
                                ..Default::default()
                            };
                        }
                        if let Some((text, true)) = &self.notice {
                            ui.colored_label(colors.accent, text);
                        }
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    save = ui.button("Save layouts").clicked();
                    cancel = ui.button("Cancel changes").clicked();
                });
            });
        if before != self.workspace.current().skin {
            theme::apply(ctx, &self.workspace.current().skin);
        }
        if save {
            let result = self.workspace.save(&self.workspace_path);
            if result.is_ok() {
                self.editor_original = None;
            }
            self.result(result, "Workspace saved");
        }
        if (cancel || ctx.input(|i| i.key_pressed(Key::Escape))) && self.editor_original.is_some() {
            self.workspace = self.editor_original.take().unwrap();
            theme::apply(ctx, &self.workspace.current().skin);
        }
    }
    pub(super) fn steam_dialog(&mut self, ctx: &egui::Context) {
        if !self.steam_settings {
            return;
        }
        let colors = theme::palette(ctx);
        let mut scan = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("steam_connection")).frame(egui::Frame::new().fill(colors.surface).corner_radius(12).inner_margin(24)).show(ctx,|ui| {
            ui.set_width(500.0);ui.label(heading("Steam library",26.0));
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-220.0).max(220.0)).show(ui,|ui|{
                ui.label("Installed games are detected automatically. No Steam account connection is needed for them.");
                ui.collapsing("Steam location override",|ui| {
                    ui.label("Leave blank to detect Steam");
                    ui.add(egui::TextEdit::singleline(&mut self.steam_root).desired_width(f32::INFINITY));
                    if ui.button("Choose folder…").clicked() && let Some(path)=rfd::FileDialog::new().pick_folder(){self.steam_root=path.display().to_string();}
                });
                ui.separator();
                ui.label(heading("Include owned, uninstalled games",18.0));
                ui.label(RichText::new("Optional: use the Steam Web API account route also supported by Playnite. Supply both fields, or leave both empty.").size(12.0).color(colors.muted));
                ui.label("SteamID64");ui.add(egui::TextEdit::singleline(&mut self.steam_account).hint_text("7656119…").desired_width(f32::INFINITY));
                ui.label("Steam Web API key");ui.add(egui::TextEdit::singleline(&mut self.steam_key).password(true).desired_width(f32::INFINITY));
                ui.label(RichText::new("The key is used for this import, then cleared. It is not saved in your library or workspace.").size(12.0).color(colors.muted));
                ui.hyperlink_to("Steam API key page","https://steamcommunity.com/dev/apikey");
            });
            ui.horizontal(|ui|{scan=ui.button("Import library").clicked();cancel=ui.button("Cancel").clicked();});
        });
        if scan {
            self.start_steam_import(ctx);
        }
        if cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.steam_settings = false;
            self.steam_key.clear();
        }
    }
}

fn layout_canvas(ui: &mut egui::Ui, layout: &mut the_launcher::workspace::Layout) {
    let colors = theme::palette(ui.ctx());
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 160.0), Sense::hover());
    ui.painter().rect_filled(r, 8.0, colors.background);
    let top = Rect::from_min_max(
        r.min + Vec2::splat(6.0),
        r.right_top() + Vec2::new(-6.0, 28.0),
    );
    let bottom = Rect::from_min_max(
        r.left_bottom() + Vec2::new(6.0, -28.0),
        r.max - Vec2::splat(6.0),
    );
    let left = Rect::from_min_max(
        r.min + Vec2::new(6.0, 34.0),
        r.left_bottom() + Vec2::new(108.0, -34.0),
    );
    let right = Rect::from_min_max(
        r.right_top() + Vec2::new(-108.0, 34.0),
        r.max - Vec2::new(6.0, 34.0),
    );
    for (id, rect, label) in [
        ("top", top, "Tools: top"),
        ("bottom", bottom, "Details / tools: bottom"),
        ("left", left, "Navigation: left"),
        ("right", right, "Navigation / details: right"),
    ] {
        let response = ui.interact(rect, egui::Id::new(("panel_drop", id)), Sense::hover());
        let hovered = response.dnd_hover_payload::<Panel>().is_some();
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(
                1.0_f32,
                if hovered {
                    colors.selection
                } else {
                    colors.line
                },
            ),
            StrokeKind::Inside,
        );
        fitted_text(
            ui.painter(),
            rect.shrink(6.0),
            label,
            FontId::proportional(10.0),
            colors.muted,
        );
        if let Some(panel) = response.dnd_release_payload::<Panel>() {
            match (*panel, id) {
                (Panel::Navigation, "left") => layout.navigation = Navigation::Left,
                (Panel::Navigation, "right") => layout.navigation = Navigation::Right,
                (Panel::Details, "right") => layout.details = Details::Right,
                (Panel::Details, "bottom") => layout.details = Details::Bottom,
                (Panel::Tools, "top") => layout.tools = Tools::Top,
                (Panel::Tools, "bottom") => layout.tools = Tools::Bottom,
                _ => {}
            }
        }
    }
    let center = Rect::from_min_max(
        left.right_top() + Vec2::new(8.0, 0.0),
        right.left_bottom() - Vec2::new(8.0, 0.0),
    );
    ui.painter().text(
        center.center(),
        Align2::CENTER_CENTER,
        format!(
            "{:?} view\nNavigation: {:?} / Details: {:?}",
            layout.view, layout.navigation, layout.details
        ),
        FontId::proportional(11.0),
        colors.text,
    );
}
