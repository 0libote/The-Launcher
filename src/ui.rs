use anyhow::Context;
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Key, Layout, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use std::{
    path::PathBuf,
    process::Child,
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use the_launcher::{
    launch,
    library::{Game, Library, Target},
    steam,
};

// A deliberately small palette: warm hardware, orange selection, ink-black cabinet.
const INK: Color32 = Color32::from_rgb(18, 22, 23);
const PANEL: Color32 = Color32::from_rgb(26, 31, 32);
const PAPER: Color32 = Color32::from_rgb(235, 228, 211);
const MUTED: Color32 = Color32::from_rgb(155, 164, 157);
const ORANGE: Color32 = Color32::from_rgb(242, 139, 79);
const LINE: Color32 = Color32::from_rgb(57, 65, 63);
const COLORS: [Color32; 5] = [
    Color32::from_rgb(71, 100, 91),
    Color32::from_rgb(130, 76, 53),
    Color32::from_rgb(64, 86, 109),
    Color32::from_rgb(123, 109, 63),
    Color32::from_rgb(96, 79, 95),
];

#[derive(Default, PartialEq, Clone, Copy)]
enum Filter {
    #[default]
    All,
    Favorites,
    Pc,
    Steam,
}
#[derive(Default)]
struct Draft {
    name: String,
    path: String,
    arguments: String,
    directory: String,
    editing: Option<usize>,
}
type ScanResult = anyhow::Result<Vec<steam::SteamGame>>;

pub struct Launcher {
    library: Library,
    path: PathBuf,
    query: String,
    filter: Filter,
    selected: Option<usize>,
    draft: Option<Draft>,
    notice: Option<(String, bool)>,
    remove: bool,
    scan: Option<mpsc::Receiver<ScanResult>>,
    children: Vec<Child>,
    fullscreen: bool,
}
impl Launcher {
    pub fn new(cc: &eframe::CreationContext<'_>, library: Library, path: PathBuf) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = INK;
        style.visuals.window_fill = PANEL;
        style.visuals.override_text_color = Some(PAPER);
        style.visuals.selection.bg_fill = ORANGE;
        style.visuals.selection.stroke = Stroke::new(1.0_f32, INK);
        style.visuals.widgets.inactive.bg_fill = PANEL;
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.5_f32, ORANGE);
        style.visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, ORANGE);
        style.spacing.item_spacing = Vec2::new(12.0, 12.0);
        style.spacing.button_padding = Vec2::new(16.0, 10.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(16.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(16.0));
        cc.egui_ctx.set_style(style);
        let selected = (!library.games.is_empty()).then_some(0);
        Self {
            library,
            path,
            selected,
            query: String::new(),
            filter: Filter::All,
            draft: None,
            notice: None,
            remove: false,
            scan: None,
            children: vec![],
            fullscreen: false,
        }
    }
    fn result(&mut self, result: anyhow::Result<()>, success: &str) {
        self.notice = Some(match result {
            Ok(()) => (success.into(), false),
            Err(e) => (format!("{e:#}"), true),
        });
    }
    fn visible(&self) -> Vec<usize> {
        let query = self.query.to_lowercase();
        self.library
            .games
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                g.name.to_lowercase().contains(&query)
                    && match self.filter {
                        Filter::All => true,
                        Filter::Favorites => g.favorite,
                        Filter::Pc => matches!(g.target, Target::Executable { .. }),
                        Filter::Steam => matches!(g.target, Target::Steam { .. }),
                    }
            })
            .map(|(i, _)| i)
            .collect()
    }
    fn play(&mut self) {
        let Some(i) = self.selected else { return };
        let game = &self.library.games[i];
        let name = game.name.clone();
        match launch::launch(game) {
            Ok(child) => {
                if let Some(child) = child {
                    self.children.push(child);
                }
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let result = self.library.change(&self.path, |l| {
                    l.games[i].last_launched = Some(now);
                    Ok(())
                });
                self.result(result, &format!("Launch requested for {name}"));
            }
            Err(e) => self.result(Err(e), ""),
        }
    }
    fn import(&mut self, ctx: &egui::Context) {
        let Some(root) = rfd::FileDialog::new()
            .set_title("Select your Steam installation folder (contains steamapps)")
            .pick_folder()
        else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(steam::scan(&root));
            ctx.request_repaint();
        });
    }
    fn poll(&mut self) {
        let result = self.scan.as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(result)) => {
                self.scan = None;
                match result {
                    Ok(games) => {
                        let mut added = 0;
                        let result = self.library.change(&self.path, |l| {
                            for game in games {
                                if l.games.iter().any(|g| {
                                    g.target
                                        == (Target::Steam {
                                            app_id: game.app_id,
                                        })
                                }) {
                                    continue;
                                }
                                l.add(Game {
                                    name: game.name,
                                    target: Target::Steam {
                                        app_id: game.app_id,
                                    },
                                    favorite: false,
                                    last_launched: None,
                                })?;
                                added += 1;
                            }
                            Ok(())
                        });
                        self.result(
                            result,
                            &format!("Imported {added} Steam games. Existing entries kept."),
                        );
                    }
                    Err(e) => self.result(Err(e), ""),
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.scan = None;
                self.notice = Some(("Steam scan stopped unexpectedly. Try again.".into(), true));
            }
            _ => {}
        }
        self.children
            .retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(24.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("▣").size(29.0).color(ORANGE));
                    ui.label(RichText::new("The Launcher").size(25.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("+ Add game").color(INK))
                                    .fill(ORANGE),
                            )
                            .clicked()
                        {
                            self.draft = Some(Draft::default());
                        }
                        if ui
                            .add_enabled(
                                self.scan.is_none(),
                                egui::Button::new(if self.scan.is_some() {
                                    "Scanning Steam…"
                                } else {
                                    "Import Steam"
                                }),
                            )
                            .clicked()
                        {
                            self.import(ctx);
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("footer")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(12.0))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new("Left/Right Browse     Enter Play     F11 Full screen")
                            .size(13.0)
                            .color(MUTED),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new("Local library / v0.1")
                                .size(13.0)
                                .color(MUTED),
                        );
                    });
                });
            });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation")
            .exact_width(190.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(20.0))
            .show(ctx, |ui| {
                ui.add_space(14.0);
                ui.label(RichText::new("Your collection").color(MUTED).size(14.0));
                ui.add_space(16.0);
                for (filter, label) in [
                    (Filter::All, "All games"),
                    (Filter::Favorites, "Favorites"),
                    (Filter::Pc, "PC executables"),
                    (Filter::Steam, "Steam"),
                ] {
                    if ui
                        .add_sized(
                            [150.0, 42.0],
                            egui::Button::selectable(
                                self.filter == filter,
                                RichText::new(label).color(if self.filter == filter {
                                    INK
                                } else {
                                    PAPER
                                }),
                            ),
                        )
                        .clicked()
                    {
                        self.filter = filter;
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    if ui.small_button("Library folder").clicked() {
                        let result = open::that_detached(self.path.parent().unwrap())
                            .map_err(anyhow::Error::from);
                        self.result(result, "Opened library folder");
                    }
                    ui.label(
                        RichText::new("Your games.\nYour little escape.")
                            .color(MUTED)
                            .size(15.0),
                    );
                });
            });
    }
    fn shelf(&mut self, ui: &mut egui::Ui, visible: &[usize]) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("Pick up where you play.").size(30.0));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} {} on this shelf",
                    visible.len(),
                    if visible.len() == 1 { "game" } else { "games" }
                ))
                .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search your games")
                        .desired_width(220.0),
                );
            });
        });
        ui.add_space(24.0);
        if visible.is_empty() {
            ui.add_space(40.0);
            ui.label(
                RichText::new(if self.library.games.is_empty() {
                    "A new home for your games."
                } else {
                    "Nothing on this shelf yet."
                })
                .size(28.0)
                .strong(),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(if self.library.games.is_empty() {
                    "Add a PC game or import your installed Steam library to get started."
                } else {
                    "Try another search or collection."
                })
                .color(MUTED),
            );
            ui.add_space(20.0);
            if self.library.games.is_empty() && ui.button("Add your first game").clicked() {
                self.draft = Some(Draft::default());
            }
            return;
        }
        let columns = ((ui.available_width() + 18.0) / 180.0).floor().max(1.0) as usize;
        let width =
            ((ui.available_width() - (columns - 1) as f32 * 18.0) / columns as f32).min(230.0);
        egui::ScrollArea::vertical()
            .id_salt("shelf")
            .show(ui, |ui| {
                egui::Grid::new("games")
                    .spacing([18.0, 22.0])
                    .show(ui, |ui| {
                        for (position, &i) in visible.iter().enumerate() {
                            let game = &self.library.games[i];
                            let (rect, response) =
                                ui.allocate_exact_size(Vec2::new(width, 222.0), Sense::click());
                            let selected = self.selected == Some(i);
                            let p = ui.painter_at(rect);
                            let color = COLORS[i % COLORS.len()];
                            p.rect_filled(rect.shrink(3.0), 4.0, color);
                            p.rect_filled(
                                egui::Rect::from_min_max(
                                    rect.min + Vec2::new(3.0, 3.0),
                                    rect.min + Vec2::new(13.0, 219.0),
                                ),
                                0.0,
                                INK.gamma_multiply(0.6),
                            );
                            let artwork = rect.shrink2(Vec2::new(24.0, 40.0));
                            // Original geometric cartridge label; no borrowed game artwork.
                            let center = artwork.center() - Vec2::new(0.0, 22.0);
                            for n in 0..4 {
                                let radius = 20.0 + n as f32 * 13.0;
                                p.circle_stroke(
                                    center,
                                    radius,
                                    Stroke::new(1.0_f32, PAPER.gamma_multiply(0.23)),
                                );
                            }
                            p.text(
                                rect.min + Vec2::new(24.0, 22.0),
                                Align2::LEFT_TOP,
                                game.source(),
                                FontId::monospace(13.0),
                                PAPER,
                            );
                            p.text(
                                center,
                                Align2::CENTER_CENTER,
                                format!("{:02}", i + 1),
                                FontId::monospace(36.0),
                                PAPER,
                            );
                            let label = egui::Rect::from_min_max(
                                rect.min + Vec2::new(14.0, 153.0),
                                rect.max - Vec2::new(3.0, 3.0),
                            );
                            p.rect_filled(label, 0.0, PAPER);
                            let galley = p.layout(
                                game.name.clone(),
                                FontId::proportional(16.0),
                                INK,
                                width - 38.0,
                            );
                            p.with_clip_rect(label.shrink(8.0)).galley(
                                label.min + Vec2::new(10.0, 9.0),
                                galley,
                                INK,
                            );
                            if selected || response.hovered() || response.has_focus() {
                                p.rect_stroke(
                                    rect,
                                    5.0,
                                    Stroke::new(if selected { 3.0_f32 } else { 1.0_f32 }, ORANGE),
                                    StrokeKind::Inside,
                                );
                            }
                            if game.favorite {
                                p.text(
                                    rect.right_top() + Vec2::new(-18.0, 20.0),
                                    Align2::RIGHT_TOP,
                                    "★",
                                    FontId::proportional(18.0),
                                    PAPER,
                                );
                            }
                            response.clone().on_hover_text(&game.name).widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    &game.name,
                                )
                            });
                            if response.clicked() || response.gained_focus() {
                                self.selected = Some(i);
                            }
                            if response.double_clicked() {
                                self.selected = Some(i);
                                self.play();
                            }
                            if (position + 1) % columns == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
    }
    fn details(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("details")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(22.0))
            .show(ctx, |ui| {
                let Some(i) = self.selected else { return };
                let game = self.library.games[i].clone();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&game.name).size(23.0).strong());
                        ui.label(
                            RichText::new(format!(
                                "{} / {}",
                                game.source(),
                                if game.last_launched.is_some() {
                                    "Previously launched"
                                } else {
                                    "Ready when you are"
                                }
                            ))
                            .color(MUTED)
                            .size(14.0),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_sized(
                                [110.0, 46.0],
                                egui::Button::new(RichText::new("Play").color(INK).strong())
                                    .fill(ORANGE),
                            )
                            .clicked()
                        {
                            self.play();
                        }
                        if ui
                            .button(if game.favorite {
                                "Unfavorite"
                            } else {
                                "Favorite"
                            })
                            .clicked()
                        {
                            let result = self.library.change(&self.path, |l| {
                                l.games[i].favorite = !l.games[i].favorite;
                                Ok(())
                            });
                            self.result(result, "Collection updated");
                        }
                        ui.menu_button("Options", |ui| {
                            if matches!(game.target, Target::Executable { .. })
                                && ui.button("Edit launch options").clicked()
                            {
                                if let Target::Executable {
                                    path,
                                    arguments,
                                    working_directory,
                                } = &game.target
                                {
                                    self.draft = Some(Draft {
                                        name: game.name.clone(),
                                        path: path.display().to_string(),
                                        arguments: arguments.join("\n"),
                                        directory: working_directory
                                            .as_ref()
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_default(),
                                        editing: Some(i),
                                    });
                                }
                                ui.close();
                            }
                            if ui.button("Remove from library").clicked() {
                                self.remove = true;
                                ui.close();
                            }
                        });
                    });
                });
            });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        let mut save = false;
        let mut cancel = false;
        if let Some(draft) = &mut self.draft {
            egui::Modal::new(egui::Id::new("add_game")).show(ctx, |ui| {
                ui.set_width(500.0);
                ui.heading(if draft.editing.is_some() {
                    "Launch options"
                } else {
                    "Add a PC game"
                });
                if let Some((message, true)) = &self.notice {
                    ui.colored_label(ORANGE, message);
                }
                ui.label("Name");
                ui.text_edit_singleline(&mut draft.name);
                ui.label("Executable");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut draft.path).desired_width(360.0));
                    if ui.button("Browse…").clicked() {
                        let mut picker =
                            rfd::FileDialog::new().set_title("Choose a game executable");
                        if cfg!(windows) {
                            picker = picker.add_filter("Windows executable", &["exe"]);
                        }
                        if let Some(path) = picker.pick_file() {
                            if draft.name.trim().is_empty() {
                                draft.name = path
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned();
                            }
                            draft.path = path.display().to_string();
                        }
                    }
                });
                ui.label("Arguments — one per line; do not add shell quotes");
                ui.add(
                    egui::TextEdit::multiline(&mut draft.arguments)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                ui.label("Working directory — leave blank to use the executable’s folder");
                ui.text_edit_singleline(&mut draft.directory);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    save = ui.button("Save game").clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        }
        if save {
            let d = self.draft.as_ref().unwrap();
            let result = (|| {
                let path = std::fs::canonicalize(d.path.trim())
                    .with_context(|| format!("Cannot open executable: {}", d.path.trim()))?;
                let game = Game {
                    name: d.name.trim().into(),
                    target: Target::Executable {
                        path,
                        arguments: d.arguments.lines().map(str::to_owned).collect(),
                        working_directory: if d.directory.trim().is_empty() {
                            None
                        } else {
                            Some(std::fs::canonicalize(d.directory.trim()).with_context(|| {
                                format!("Cannot open working directory: {}", d.directory.trim())
                            })?)
                        },
                    },
                    favorite: false,
                    last_launched: None,
                };
                launch::plan(&game)?;
                let editing = d.editing;
                self.library.change(&self.path, |l| {
                    if let Some(i) = editing {
                        let mut remaining = l.clone();
                        remaining.games.remove(i);
                        remaining.add(game.clone())?;
                        l.games[i] = Game {
                            favorite: l.games[i].favorite,
                            last_launched: l.games[i].last_launched,
                            ..game
                        };
                    } else {
                        l.add(game)?;
                    }
                    Ok(())
                })?;
                self.selected = Some(editing.unwrap_or(self.library.games.len() - 1));
                Ok(())
            })();
            if result.is_ok() {
                self.draft = None;
            }
            self.result(result, "Game saved to your library");
        }
        if cancel {
            self.draft = None;
        }
        if self.remove {
            egui::Modal::new(egui::Id::new("remove")).show(ctx, |ui| {
                ui.heading("Remove this game?");
                ui.label("Only the library entry is removed. Your game files stay in place.");
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        if let Some(i) = self.selected {
                            let result = self.library.change(&self.path, |l| {
                                l.games.remove(i);
                                Ok(())
                            });
                            if result.is_ok() {
                                self.selected = None;
                            }
                            self.result(result, "Game removed");
                        }
                        self.remove = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.remove = false;
                    }
                });
            });
        }
    }
}
impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        let visible = self.visible();
        if !self.selected.is_some_and(|i| visible.contains(&i)) {
            self.selected = visible.first().copied();
        }
        if ctx.input(|i| i.key_pressed(Key::F11)) {
            self.fullscreen = !self.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        if self.draft.is_none() && !self.remove && !ctx.wants_keyboard_input() {
            let direction = ctx.input(|i| {
                if i.key_pressed(Key::ArrowRight) {
                    1
                } else if i.key_pressed(Key::ArrowLeft) {
                    -1
                } else {
                    0
                }
            });
            if direction != 0 && !visible.is_empty() {
                let current = visible
                    .iter()
                    .position(|i| Some(*i) == self.selected)
                    .unwrap_or(0);
                self.selected = Some(
                    visible[(current as isize + direction).rem_euclid(visible.len() as isize)
                        as usize],
                );
            }
            if ctx.input(|i| i.key_pressed(Key::Enter)) {
                self.play();
            }
        }
        self.header(ctx);
        self.sidebar(ctx);
        self.details(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(INK).inner_margin(28.0))
            .show(ctx, |ui| {
                if let Some((text, error)) = &self.notice {
                    let mut dismiss = false;
                    egui::Frame::new()
                        .fill(PANEL)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(text).color(if *error {
                                    ORANGE
                                } else {
                                    PAPER
                                }));
                                dismiss = ui.small_button("Dismiss").clicked();
                            });
                        });
                    if dismiss {
                        self.notice = None;
                    }
                    ui.add_space(12.0);
                }
                self.shelf(ui, &visible);
            });
        self.dialogs(ctx);
        if !self.children.is_empty() || self.scan.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}
