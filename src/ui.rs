use anyhow::Context;
use eframe::egui::{
    self, Align, Align2, FontId, Key, Layout, RichText, Sense, Stroke, StrokeKind, Vec2,
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
    workspace::{Details, Navigation, Tools, View, Workspace},
};

mod art;
mod covers;
mod editor;
mod frame;
mod metadata_ui;
mod theme;
mod views;
use theme::*;

#[derive(Default, PartialEq, Clone, Copy)]
enum Filter {
    #[default]
    All,
    Favorites,
    Pc,
    Steam,
    Hidden,
}
#[derive(Default)]
struct Draft {
    name: String,
    path: String,
    arguments: String,
    directory: String,
    editing: Option<usize>,
}
type ScanResult = anyhow::Result<steam::ImportResult>;

pub struct Launcher {
    library: Library,
    path: PathBuf,
    query: String,
    filter: Filter,
    installed_only: bool,
    selected: Option<usize>,
    draft: Option<Draft>,
    notice: Option<(String, bool)>,
    remove: bool,
    scan: Option<mpsc::Receiver<ScanResult>>,
    children: Vec<Child>,
    fullscreen: bool,
    covers: covers::Covers,
    scroll_selection: bool,
    shelf_focused: bool,
    workspace: Workspace,
    workspace_path: PathBuf,
    editor_original: Option<Workspace>,

    metadata_ui: metadata_ui::MetadataUi,
    steam_settings: bool,
    steam_root: String,
    steam_account: String,
    steam_key: String,
}
impl Launcher {
    pub fn new(cc: &eframe::CreationContext<'_>, library: Library, path: PathBuf) -> Self {
        Self::from_library(&cc.egui_ctx, library, path)
    }
    fn from_library(ctx: &egui::Context, library: Library, path: PathBuf) -> Self {
        theme::install(ctx);
        let selected = (!library.games.is_empty()).then_some(0);
        let workspace_path = path.with_file_name("workspace.json");
        let (workspace, notice) = match Workspace::load(&workspace_path) {
            Ok(w) => (w, None),
            Err(e) => (
                Workspace::default(),
                Some((
                    format!("{e:#}. Using a temporary default; your workspace file is unchanged."),
                    true,
                )),
            ),
        };
        let (metadata_ui, metadata_error) = metadata_ui::MetadataUi::load(&path);
        let notice = metadata_error.map(|e| (e, true)).or(notice);
        theme::apply(ctx, &workspace.current().skin);
        Self {
            library,
            path,
            selected,
            query: String::new(),
            filter: Filter::All,
            installed_only: false,
            draft: None,
            notice,
            remove: false,
            scan: None,
            children: vec![],
            fullscreen: false,
            covers: covers::Covers::default(),
            scroll_selection: false,
            shelf_focused: false,
            workspace,
            workspace_path,
            editor_original: None,
            metadata_ui,
            steam_settings: false,
            steam_root: String::new(),
            steam_account: String::new(),
            steam_key: String::new(),
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
                if self.filter == Filter::Hidden {
                    if !g.hidden {
                        return false;
                    }
                } else if g.hidden {
                    return false;
                }
                g.name.to_lowercase().contains(&query)
                    && (!self.installed_only || g.installed != Some(false))
                    && match self.filter {
                        Filter::All => true,
                        Filter::Favorites => g.favorite,
                        Filter::Pc => matches!(g.target, Target::Executable { .. }),
                        Filter::Steam => matches!(g.target, Target::Steam { .. }),
                        Filter::Hidden => true,
                    }
            })
            .map(|(i, _)| i)
            .collect()
    }
    fn play(&mut self) {
        let Some(i) = self.selected else { return };
        let game = &self.library.games[i];
        let name = game.name.clone();
        let installing =
            matches!(game.target, Target::Steam { .. }) && game.installed == Some(false);
        match launch::launch(game) {
            Ok(child) => {
                if let Some(child) = child {
                    self.children.push(child);
                }
                if installing {
                    self.notice =
                        Some((format!("Installation requested for {name} in Steam"), false));
                    return;
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
    fn import(&mut self, _ctx: &egui::Context) {
        self.steam_settings = true;
    }
    fn start_steam_import(&mut self, ctx: &egui::Context) {
        let options = steam::ImportOptions {
            root: (!self.steam_root.trim().is_empty())
                .then(|| PathBuf::from(self.steam_root.trim())),
            account_id: (!self.steam_account.trim().is_empty())
                .then(|| self.steam_account.trim().to_owned()),
            api_key: (!self.steam_key.trim().is_empty()).then(|| self.steam_key.trim().to_owned()),
        };
        let (tx, rx) = mpsc::channel();
        self.scan = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(steam::import(options));
            ctx.request_repaint();
        });
        self.steam_key.clear();
        self.steam_settings = false;
    }
    fn poll(&mut self) {
        self.poll_metadata();
        let result = self.scan.as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(result)) => {
                self.scan = None;
                match result {
                    Ok(imported) => {
                        let warnings = imported.warnings;
                        let mut added = 0;
                        let result = self.library.change(&self.path, |l| {
                            if warnings.is_empty() {
                                for game in &mut l.games {
                                    if matches!(game.target, Target::Steam { .. }) {
                                        game.installed = Some(false);
                                    }
                                }
                            }
                            for game in imported.games {
                                if let Some(existing) = l.games.iter_mut().find(|g| {
                                    g.target
                                        == (Target::Steam {
                                            app_id: game.app_id,
                                        })
                                }) {
                                    if game.installed || warnings.is_empty() {
                                        existing.installed = Some(game.installed);
                                    }
                                    if existing.cover.is_none() {
                                        existing.cover = game.cover;
                                    }
                                    continue;
                                }
                                l.add(Game {
                                    metadata: Default::default(),
                                    name: game.name,
                                    target: Target::Steam {
                                        app_id: game.app_id,
                                    },
                                    favorite: false,
                                    last_launched: None,
                                    cover: game.cover,
                                    installed: Some(game.installed),
                                    hidden: false,
                                })?;
                                added += 1;
                            }
                            Ok(())
                        });
                        let imported_ok = result.is_ok();
                        self.result(
                            result,
                            &format!(
                                "Imported {added} Steam games. Existing entries kept. {}",
                                warnings.join("\n")
                            ),
                        );
                        if imported_ok && self.metadata_ui.settings.automatic {
                            self.queue_automatic_metadata();
                        }
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
    fn add_dialog(&mut self) {
        self.notice = None;
        self.draft = Some(Draft::default());
    }
    fn header(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let panel = if self.workspace.current().layout.tools == Tools::Bottom {
            egui::TopBottomPanel::bottom("header")
        } else {
            egui::TopBottomPanel::top("header")
        };
        panel
            .exact_height(96.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(egui::Margin::symmetric(32, 22)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new((ui.available_width() - 420.0).max(80.0), 50.0),
                        Layout::top_down(Align::LEFT),
                        |ui| {
                            ui.label(heading("Library", 24.0));
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&self.workspace.current().name)
                                        .size(12.0)
                                        .color(colors.muted),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&self.workspace.current().name);
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(if self.metadata_ui.busy() {
                                "Metadata…"
                            } else {
                                "Metadata"
                            })
                            .clicked()
                        {
                            self.open_metadata(None);
                        }
                        if button(ui, "Add game", Icon::Plus, true).clicked() {
                            self.add_dialog();
                        }
                        ui.add_enabled_ui(self.scan.is_none(), |ui| {
                            if button(
                                ui,
                                if self.scan.is_some() {
                                    "Scanning…"
                                } else {
                                    "Import Steam"
                                },
                                Icon::Steam,
                                false,
                            )
                            .clicked()
                            {
                                self.import(ctx);
                            }
                        });
                    });
                });
            });
    }
    fn footer(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        egui::TopBottomPanel::bottom("footer")
            .exact_height(42.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(egui::Margin::symmetric(32, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().interact_size.y = 24.0;
                    ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
                    if ui.small_button("Customize").clicked() {
                        self.open_editor();
                    }
                    key_hint(ui, "Arrows", "Browse");
                    key_hint(ui, "Enter", "Play");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new("Full screen").min_size(Vec2::new(0.0, 28.0)))
                            .on_hover_text("Toggle full screen (F11)")
                            .clicked()
                        {
                            self.fullscreen = !self.fullscreen;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                self.fullscreen,
                            ));
                        }
                    });
                });
            });
    }
    fn sidebar(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let layout = &self.workspace.current().layout;
        if layout.navigation == Navigation::Hidden {
            return;
        }
        let panel = if layout.navigation == Navigation::Right {
            egui::SidePanel::right("navigation")
        } else {
            egui::SidePanel::left("navigation")
        };
        panel
            .exact_width(
                layout
                    .navigation_width
                    .min(ctx.content_rect().width() * 0.26),
            )
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(colors.navigation)
                    .inner_margin(egui::Margin::symmetric(16, 24)),
            )
            .show(ctx, |ui| {
                ui.label(heading("Collection", 18.0));
                ui.add_space(20.0);
                for (filter, label, kind) in [
                    (Filter::All, "All games", Icon::Library),
                    (Filter::Favorites, "Favorites", Icon::Heart),
                    (Filter::Pc, "PC games", Icon::Pc),
                    (Filter::Steam, "Steam", Icon::Steam),
                    (Filter::Hidden, "Hidden", Icon::Close),
                ] {
                    let count = self
                        .library
                        .games
                        .iter()
                        .filter(|g| match filter {
                            Filter::All => !g.hidden,
                            Filter::Favorites => !g.hidden && g.favorite,
                            Filter::Pc => {
                                !g.hidden && matches!(g.target, Target::Executable { .. })
                            }
                            Filter::Steam => !g.hidden && matches!(g.target, Target::Steam { .. }),
                            Filter::Hidden => g.hidden,
                        })
                        .count();
                    let active = self.filter == filter;
                    let (rect, response) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 46.0), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::SelectableLabel,
                            true,
                            format!("{label}, {count} games"),
                        )
                    });
                    let p = ui.painter();
                    if active || response.hovered() || response.has_focus() {
                        p.rect_filled(
                            rect,
                            8.0,
                            if active {
                                colors.selection.gamma_multiply(0.12)
                            } else {
                                colors.surface
                            },
                        );
                    }
                    if active {
                        p.rect_filled(
                            egui::Rect::from_min_size(
                                rect.left_center() - Vec2::new(0.0, 9.0),
                                Vec2::new(3.0, 18.0),
                            ),
                            2.0,
                            colors.selection,
                        );
                    }
                    if response.has_focus() {
                        p.rect_stroke(
                            rect,
                            8.0,
                            Stroke::new(1.0_f32, colors.selection),
                            StrokeKind::Inside,
                        );
                    }
                    let color = if active {
                        colors.selection
                    } else {
                        colors.muted
                    };
                    icon(
                        p,
                        rect.left_center() + Vec2::new(22.0, 0.0),
                        kind,
                        18.0,
                        color,
                    );
                    p.text(
                        rect.left_center() + Vec2::new(42.0, 0.0),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(13.0),
                        color,
                    );
                    p.text(
                        rect.right_center() - Vec2::new(14.0, 0.0),
                        Align2::RIGHT_CENTER,
                        count.to_string(),
                        FontId::proportional(11.0),
                        if active {
                            colors.selection
                        } else {
                            colors.faint
                        },
                    );
                    if response.clicked() {
                        self.filter = filter;
                        self.scroll_selection = true;
                    }
                }
            });
    }
    fn shelf(&mut self, ui: &mut egui::Ui, visible: &[usize]) {
        let colors = theme::palette(ui.ctx());
        let width = ui.available_width();
        self.shelf_focused = false;
        ui.horizontal(|ui| {
            let label = match self.filter {
                Filter::All => "All games",
                Filter::Favorites => "Favorites",
                Filter::Pc => "PC games",
                Filter::Steam => "Steam",
                Filter::Hidden => "Hidden",
            };
            ui.menu_button(heading(label, 20.0), |ui| {
                for (value, label) in [
                    (Filter::All, "All games"),
                    (Filter::Favorites, "Favorites"),
                    (Filter::Pc, "PC games"),
                    (Filter::Steam, "Steam"),
                    (Filter::Hidden, "Hidden"),
                ] {
                    if ui.selectable_label(self.filter == value, label).clicked() {
                        self.filter = value;
                        ui.close();
                    }
                }
            });
            if self.fullscreen
                && !self.workspace.current().layout.show_shortcuts
                && ui.small_button("Customize").clicked()
            {
                self.open_editor();
            }
            badge(ui, &visible.len().to_string(), colors.muted);
            if self.workspace.current().layout.details == Details::Hidden
                && let Some(index) = self.selected
            {
                let game = self.library.games[index].clone();
                self.game_options(ui, index, &game);
            }
            if self.selected.is_some()
                && self.workspace.current().layout.details == Details::Hidden
                && ui.small_button("Play / Install").clicked()
            {
                self.play();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::Frame::new()
                    .fill(colors.navigation)
                    .stroke(Stroke::new(1.0_f32, colors.line))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(12, 9))
                    .show(ui, |ui| {
                        ui.set_width((width * 0.28).clamp(130.0, 230.0) + 33.0);
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            ui.spacing_mut().interact_size.y = 20.0;
                            icon(
                                ui.painter(),
                                ui.next_widget_position() + Vec2::new(8.0, 9.0),
                                Icon::Search,
                                16.0,
                                colors.faint,
                            );
                            ui.add_space(21.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .hint_text("Search games…")
                                    .desired_width((width * 0.28).clamp(130.0, 230.0))
                                    .frame(false),
                            );
                        });
                    });
                ui.checkbox(&mut self.installed_only, "Installed only")
                    .on_hover_text("Show only installed games");
            });
        });
        ui.add_space(20.0);
        if self.library.games.is_empty() {
            let height = (ui.available_height() - 14.0).clamp(310.0, 520.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| self.welcome(ui, height));
            return;
        }
        if visible.is_empty() {
            ui.add_space(70.0);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(240.0, 90.0), Sense::hover());
                empty_mark(ui.painter(), rect);
                ui.label(
                    RichText::new(if self.filter == Filter::Hidden {
                        "Hidden Steam games live here. Hide a game to declutter without losing it."
                    } else {
                        "Try a different search or another collection."
                    })
                    .color(colors.muted),
                );
                if ui.button("Show all games").clicked() {
                    self.query.clear();
                    self.filter = Filter::All;
                    self.installed_only = false;
                }
            });
            return;
        }
        if self.workspace.current().layout.view == View::List {
            self.list_view(ui, visible);
            return;
        }
        if self.workspace.current().layout.view == View::Shelf {
            self.shelf_view(ui, visible);
            return;
        }
        let columns = columns_for(width, self.workspace.current().layout.tile_width);
        let tile_width = (width - (columns - 1) as f32 * 20.0) / columns as f32;
        let art_height = tile_width * 1.30;
        egui::ScrollArea::vertical()
            .id_salt("shelf")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("games")
                    .spacing([20.0, 24.0])
                    .show(ui, |ui| {
                        for (position, &i) in visible.iter().enumerate() {
                            let game = &self.library.games[i];
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(tile_width, art_height + 62.0),
                                Sense::click(),
                            );
                            let selected = self.selected == Some(i);
                            self.shelf_focused |= response.has_focus();
                            if selected && self.scroll_selection {
                                response.scroll_to_me_animation(
                                    Some(Align::Center),
                                    egui::style::ScrollAnimation::none(),
                                );
                            }
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    format!("{}, {}", game.name, game.source()),
                                )
                            });
                            if ui.is_rect_visible(rect) {
                                let p = ui.painter_at(rect.expand(5.0));
                                let cover = egui::Rect::from_min_size(
                                    rect.min,
                                    Vec2::new(tile_width, art_height),
                                );
                                let texture = game
                                    .cover
                                    .as_ref()
                                    .and_then(|path| self.covers.texture(ui.ctx(), path));
                                if let Some(texture) = texture {
                                    egui::Image::new(&texture)
                                        .uv(covers::crop_uv(texture.size_vec2(), cover.size()))
                                        .corner_radius(8)
                                        .paint_at(ui, cover);
                                } else {
                                    art::cover(&p, cover, &game.name);
                                }
                                if selected || response.hovered() || response.has_focus() {
                                    p.rect_stroke(
                                        cover.expand(3.0),
                                        11.0,
                                        Stroke::new(
                                            if selected { 2.0_f32 } else { 1.0_f32 },
                                            if selected {
                                                colors.selection
                                            } else {
                                                colors.text
                                            },
                                        ),
                                        StrokeKind::Outside,
                                    );
                                }
                                let title = egui::Rect::from_min_size(
                                    cover.left_bottom() + Vec2::new(0.0, 14.0),
                                    Vec2::new(tile_width, 20.0),
                                );
                                fitted_text(
                                    &p,
                                    title,
                                    &game.name,
                                    FontId::proportional(14.0),
                                    colors.text,
                                );
                                icon(
                                    &p,
                                    title.left_bottom() + Vec2::new(7.0, 16.0),
                                    source_icon(game.source()),
                                    12.0,
                                    colors.faint,
                                );
                                p.text(
                                    title.left_bottom() + Vec2::new(21.0, 16.0),
                                    Align2::LEFT_CENTER,
                                    game.source(),
                                    FontId::proportional(11.0),
                                    colors.muted,
                                );
                                if game.favorite {
                                    icon(
                                        &p,
                                        cover.right_top() + Vec2::new(-20.0, 20.0),
                                        Icon::Heart,
                                        16.0,
                                        colors.text,
                                    );
                                }
                            }
                            let mut response = response.on_hover_text(&game.name);
                            if let Some(error) =
                                game.cover.as_ref().and_then(|path| self.covers.error(path))
                            {
                                response = response.on_hover_text(error);
                            }
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
        self.scroll_selection = false;
    }
    fn welcome(&mut self, ui: &mut egui::Ui, height: f32) {
        let colors = theme::palette(ui.ctx());
        let width = ui.available_width();
        let compact = width < 750.0;
        egui::Frame::new()
            .fill(colors.surface)
            .stroke(Stroke::new(1.0_f32, colors.line))
            .corner_radius(18)
            .show(ui, |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width - 2.0, height), Sense::hover());
                let padding = if compact { 26.0 } else { 40.0 };
                let content = rect.shrink(padding);
                let copy_width = if compact {
                    content.width() * 0.57
                } else {
                    content.width() * 0.53
                };
                let copy = egui::Rect::from_min_size(
                    content.min + Vec2::new(0.0, if compact { 4.0 } else { 32.0 }),
                    Vec2::new(copy_width, content.height()),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(copy), |ui| {
                    ui.spacing_mut().item_spacing.y = if compact { 6.0 } else { 12.0 };
                    if !compact {
                        badge(ui, "Your collection starts here", colors.selection);
                        ui.add_space(24.0);
                    }
                    ui.label(heading(
                        "Good games.
Great company.",
                        if compact { 30.0 } else { 44.0 },
                    ));
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(
                            "Give your PC and Steam games a home.
Pick something you love. Press play.",
                        )
                        .size(if compact { 12.0 } else { 14.0 })
                        .color(colors.muted),
                    );
                    ui.add_space(if compact { 12.0 } else { 24.0 });
                    if button(ui, "Add your first game", Icon::Plus, true).clicked() {
                        self.add_dialog();
                    }
                    ui.add_space(3.0);
                    ui.spacing_mut().interact_size.y = 22.0;
                    ui.spacing_mut().button_padding = Vec2::new(0.0, 4.0);
                    ui.add_enabled_ui(self.scan.is_none(), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Or import from Steam")
                                        .size(12.0)
                                        .color(colors.muted),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            self.import(ui.ctx());
                        }
                    });
                });
                let illustration = egui::Rect::from_min_max(
                    egui::pos2(content.left() + copy_width, content.top() + 5.0),
                    egui::pos2(
                        content.right() + 10.0,
                        content.bottom() - if height > 430.0 { 78.0 } else { 0.0 },
                    ),
                );
                art::welcome(ui.painter(), illustration);
                if height > 430.0 {
                    let y = rect.bottom() - 91.0;
                    ui.painter().line_segment(
                        [
                            egui::pos2(content.left(), y),
                            egui::pos2(content.right(), y),
                        ],
                        Stroke::new(1.0_f32, colors.line),
                    );
                    for (n, (kind, title, subtitle)) in [
                        (Icon::Pc, "Your PC games", "Add any executable"),
                        (Icon::Steam, "Your Steam library", "Import installed games"),
                        (Icon::Image, "Your own style", "Choose local cover art"),
                    ]
                    .iter()
                    .enumerate()
                    {
                        let x = content.left() + n as f32 * (content.width() / 3.0);
                        icon(
                            ui.painter(),
                            egui::pos2(x + 9.0, y + 31.0),
                            *kind,
                            18.0,
                            colors.selection,
                        );
                        ui.painter().text(
                            egui::pos2(x + 30.0, y + 24.0),
                            Align2::LEFT_TOP,
                            title,
                            FontId::proportional(12.0),
                            colors.text,
                        );
                        ui.painter().text(
                            egui::pos2(x + 30.0, y + 44.0),
                            Align2::LEFT_TOP,
                            subtitle,
                            FontId::proportional(10.0),
                            colors.muted,
                        );
                    }
                }
            });
    }
    fn choose_cover(&mut self, index: usize) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose cover art")
            .add_filter("Cover image", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        let result = (|| {
            covers::decode(&path)?;
            let path = std::fs::canonicalize(path)?;
            self.covers.forget(&path);
            self.library.change(&self.path, |library| {
                library.games[index].cover = Some(path);
                library.games[index]
                    .metadata
                    .sources
                    .insert("cover".into(), "Local file".into());
                Ok(())
            })
        })();
        self.result(result, "Cover art updated");
    }
    fn details(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        if self.selected.is_none() || self.workspace.current().layout.details == Details::Hidden {
            return;
        }
        if self.workspace.current().layout.details == Details::Right {
            self.details_right(ctx);
            return;
        }
        egui::TopBottomPanel::bottom("details")
            .exact_height(104.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(28, 20)),
            )
            .show(ctx, |ui| {
                let i = self.selected.unwrap();
                let game = self.library.games[i].clone();
                ui.horizontal(|ui| {
                    let info_width = (ui.available_width() - 350.0).max(140.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(info_width, 62.0),
                        Layout::top_down(Align::LEFT),
                        |ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::new(info_width, 29.0), Sense::hover());
                            fitted_text(ui.painter(), rect, &game.name, display(23.0), colors.text);
                            ui.horizontal(|ui| {
                                badge(ui, game.source(), art::color(&game.name));
                                if game.hidden {
                                    badge(ui, "Hidden", colors.muted);
                                }
                                ui.label(
                                    RichText::new(if game.installed == Some(false) {
                                        "Not installed"
                                    } else if game.last_launched.is_some() {
                                        "Previously played"
                                    } else {
                                        "Ready to play"
                                    })
                                    .size(11.0)
                                    .color(colors.muted),
                                );
                            });
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
                        let favorite_button = icon_button(
                            ui,
                            Icon::Heart,
                            if game.favorite {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            },
                        );
                        if game.favorite {
                            icon(
                                ui.painter(),
                                favorite_button.rect.center(),
                                Icon::Heart,
                                18.0,
                                colors.selection,
                            );
                        }
                        if favorite_button.clicked() {
                            let result = self.library.change(&self.path, |l| {
                                l.games[i].favorite = !l.games[i].favorite;
                                Ok(())
                            });
                            self.result(result, "Collection updated");
                        }
                        self.game_options(ui, i, &game);
                    });
                });
            });
    }
    fn game_options(&mut self, ui: &mut egui::Ui, i: usize, game: &Game) {
        ui.menu_button("Options", |ui| {
            if ui.button("Metadata & artwork…").clicked() {
                self.open_metadata(Some(i));
                ui.close();
            }
            if ui.button("Choose cover art…").clicked() {
                ui.close();
                self.choose_cover(i);
            }
            if game.cover.is_some() && ui.button("Remove cover art").clicked() {
                let result = self.library.change(&self.path, |l| {
                    l.games[i].cover = None;
                    l.games[i].metadata.sources.remove("cover");
                    Ok(())
                });
                self.result(result, "Original cover restored");
                ui.close();
            }
            ui.separator();
            if matches!(game.target, Target::Executable { .. })
                && ui.button("Edit launch options").clicked()
            {
                if let Target::Executable {
                    path,
                    arguments,
                    working_directory,
                } = &game.target
                {
                    self.notice = None;
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
            if game.hidden {
                if ui.button("Restore to library").clicked() {
                    let result = self.library.change(&self.path, |l| {
                        l.games[i].hidden = false;
                        Ok(())
                    });
                    self.result(result, "Game restored to your library");
                    ui.close();
                }
            } else if matches!(game.target, Target::Steam { .. }) {
                if ui.button("Hide from library").clicked() {
                    self.remove = true;
                    ui.close();
                }
            } else if ui.button("Remove from library").clicked() {
                self.remove = true;
                ui.close();
            }
        });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        let mut save = false;
        let mut cancel = false;
        if let Some(draft) = &mut self.draft {
            egui::Modal::new(egui::Id::new("add_game"))
                .frame(
                    egui::Frame::new()
                        .fill(colors.surface)
                        .stroke(Stroke::new(1.0_f32, colors.line))
                        .corner_radius(18)
                        .inner_margin(24),
                )
                .show(ctx, |ui| {
                    ui.set_width(500.0);
                    ui.label(heading(
                        if draft.editing.is_some() {
                            "Launch options"
                        } else {
                            "Add a PC game"
                        },
                        26.0,
                    ));
                    ui.label(
                        RichText::new("A name, an executable, and you’re ready to play.")
                            .size(12.0)
                            .color(colors.muted),
                    );
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt("launch_form")
                        .max_height((ctx.content_rect().height() - 260.0).max(200.0))
                        .show(ui, |ui| {
                            if let Some((message, true)) = &self.notice {
                                ui.colored_label(colors.accent, message);
                            }
                            ui.label("Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut draft.name)
                                    .hint_text("What are we playing?")
                                    .desired_width(f32::INFINITY)
                                    .margin(Vec2::new(12.0, 10.0)),
                            );
                            ui.label("Executable");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.path)
                                        .hint_text("Choose a game executable")
                                        .desired_width(350.0)
                                        .margin(Vec2::new(12.0, 10.0)),
                                );
                                if ui.button("Browse…").clicked() {
                                    let mut picker = rfd::FileDialog::new()
                                        .set_title("Choose a game executable");
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
                                    .desired_rows(2)
                                    .margin(Vec2::new(12.0, 10.0))
                                    .desired_width(f32::INFINITY),
                            );
                            ui.label(
                                "Working directory — leave blank to use the executable’s folder",
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut draft.directory)
                                    .hint_text("Use the game’s folder")
                                    .desired_width(f32::INFINITY)
                                    .margin(Vec2::new(12.0, 10.0)),
                            );
                        });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        save = button(ui, "Save game", Icon::Plus, true).clicked();
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
                    metadata: Default::default(),
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
                    cover: None,
                    installed: None,
                    hidden: false,
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
                            cover: l.games[i].cover.clone(),
                            installed: l.games[i].installed,
                            hidden: l.games[i].hidden,
                            metadata: l.games[i].metadata.clone(),
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
                if self.metadata_ui.settings.automatic {
                    self.queue_automatic_metadata();
                }
            }
            self.result(result, "Game saved to your library");
        }
        if self.draft.is_some() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if cancel {
            self.draft = None;
        }
        if self.remove {
            let hiding = self
                .selected
                .and_then(|i| self.library.games.get(i))
                .is_some_and(|g| !g.hidden && matches!(g.target, Target::Steam { .. }));
            egui::Modal::new(egui::Id::new("remove")).show(ctx, |ui| {
                if hiding {
                    ui.label(heading("Hide this game?", 24.0));
                    ui.label("It moves to Hidden. You can restore it anytime from the Hidden collection.");
                    ui.horizontal(|ui| {
                        if ui.button("Hide").clicked() {
                            if let Some(i) = self.selected {
                                let result = self.library.change(&self.path, |l| {
                                    l.games[i].hidden = true;
                                    Ok(())
                                });
                                if result.is_ok() {
                                    self.selected = None;
                                }
                                self.result(result, "Game hidden");
                            }
                            self.remove = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.remove = false;
                        }
                    });
                } else {
                    ui.label(heading("Remove this game?", 24.0));
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
                }
            });
        }
    }
}
impl Launcher {
    fn draw(&mut self, ctx: &egui::Context) {
        let colors = theme::palette(ctx);
        self.poll();
        let visible = self.visible();
        if !self.selected.is_some_and(|i| visible.contains(&i)) {
            self.selected = visible.first().copied();
        }
        if ctx.input(|i| i.key_pressed(Key::F11)) {
            self.fullscreen = !self.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        if self.draft.is_none()
            && !self.remove
            && self.editor_original.is_none()
            && !self.steam_settings
            && !self.metadata_ui.open
            && !egui::Popup::is_any_open(ctx)
            && (!ctx.wants_keyboard_input() || self.shelf_focused)
        {
            let layout = &self.workspace.current().layout;
            let navigation_width = if layout.navigation == Navigation::Hidden {
                0.0
            } else {
                layout
                    .navigation_width
                    .min(ctx.content_rect().width() * 0.26)
            };
            let remaining = ctx.content_rect().width() - navigation_width;
            let width = remaining
                - 64.0
                - if layout.details == Details::Right {
                    layout.details_width.min(remaining * 0.35)
                } else {
                    0.0
                };
            let columns = if layout.view == View::Grid {
                columns_for(width.max(1.0), layout.tile_width)
            } else {
                1
            };
            let direction = ctx.input(|i| {
                if i.key_pressed(Key::ArrowRight) {
                    1
                } else if i.key_pressed(Key::ArrowLeft) {
                    -1
                } else if i.key_pressed(Key::ArrowDown) {
                    columns as isize
                } else if i.key_pressed(Key::ArrowUp) {
                    -(columns as isize)
                } else {
                    0
                }
            });
            if direction != 0 && !visible.is_empty() {
                self.scroll_selection = true;
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
        if frame::titlebar(ctx, &self.workspace.current().name, self.fullscreen) {
            self.open_editor();
        }
        self.sidebar(ctx);
        self.header(ctx);
        if self.workspace.current().layout.show_shortcuts {
            self.footer(ctx);
        }
        self.details(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(egui::Margin::symmetric(32, 16)),
            )
            .show(ctx, |ui| {
                if let Some((text, error)) = &self.notice
                    && self.draft.is_none()
                {
                    let mut dismiss = false;
                    egui::Frame::new()
                        .fill(colors.surface)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(text).color(if *error {
                                    colors.accent
                                } else {
                                    colors.text
                                }));
                                dismiss =
                                    icon_button(ui, Icon::Close, "Dismiss notification").clicked();
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
        self.steam_dialog(ctx);
        self.workspace_editor(ctx);
        self.metadata_dialog(ctx);
        frame::resize_edges(ctx, self.fullscreen);
        if !self.children.is_empty() || self.scan.is_some() || self.metadata_ui.busy() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

fn columns_for(width: f32, tile_width: f32) -> usize {
    (((width + 20.0) / (tile_width + 20.0)).floor() as usize).max(1)
}
#[cfg(test)]
fn grid_columns(width: f32) -> usize {
    columns_for(width, 180.0)
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.draw(ctx);
    }
}

pub fn window_icon() -> egui::IconData {
    let mut rgba = vec![0; 64 * 64 * 4];
    for y in 0..64_i32 {
        for x in 0..64_i32 {
            let dx = (x - 32).abs();
            let dy = (y - 32).abs();
            let round = (dx - 20).max(0).pow(2) + (dy - 20).max(0).pow(2) <= 11 * 11;
            let border = (17..47).contains(&x)
                && (16..48).contains(&y)
                && !((20..44).contains(&x) && (19..45).contains(&y));
            let letter = (25..28).contains(&x) && (25..39).contains(&y)
                || (25..39).contains(&x) && (36..39).contains(&y);
            let color = if !round {
                egui::Color32::TRANSPARENT
            } else if border || letter {
                BG
            } else {
                CORAL
            };
            rgba[((y * 64 + x) * 4) as usize..((y * 64 + x) * 4 + 4) as usize]
                .copy_from_slice(&color.to_array());
        }
    }
    egui::IconData {
        rgba,
        width: 64,
        height: 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(app: &mut Launcher, ctx: &egui::Context, size: Vec2, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ctx| app.draw(ctx),
        );
    }
    #[test]
    fn empty_and_populated_layouts_fit_supported_window_sizes() {
        let temp = tempfile::tempdir().unwrap();
        for count in [0, 1, 30] {
            for size in [
                Vec2::new(840.0, 600.0),
                Vec2::new(1200.0, 800.0),
                Vec2::new(1600.0, 1000.0),
            ] {
                let ctx = egui::Context::default();
                let library = Library {
                    schema_version: 1,
                    games: (0..count)
                        .map(|i| Game {
                            metadata: Default::default(),
                            name: format!("A deliberately very long game title {i}"),
                            target: Target::Steam { app_id: i + 1 },
                            favorite: false,
                            last_launched: None,
                            cover: None,
                            installed: None,
                            hidden: false,
                        })
                        .collect(),
                };
                let mut app =
                    Launcher::from_library(&ctx, library, temp.path().join("library.json"));
                frame(&mut app, &ctx, size, vec![]);
                frame(&mut app, &ctx, size, vec![]);
                assert!(
                    ctx.used_rect().width() <= size.x + 1.0,
                    "layout overflows horizontally at {size:?}, count {count}, used {:?}",
                    ctx.used_rect()
                );
                assert!(
                    ctx.used_rect().height() <= size.y + 1.0,
                    "layout overflows vertically at {size:?}, count {count}, used {:?}",
                    ctx.used_rect()
                );
            }
        }
    }
    #[test]
    fn arrows_move_by_row_and_modal_input_does_not_move_the_shelf() {
        let ctx = egui::Context::default();
        let temp = tempfile::tempdir().unwrap();
        let library = Library {
            schema_version: 1,
            games: (1..=12)
                .map(|i| Game {
                    metadata: Default::default(),
                    name: format!("Game {i}"),
                    target: Target::Steam { app_id: i },
                    favorite: false,
                    last_launched: None,
                    cover: None,
                    installed: None,
                    hidden: false,
                })
                .collect(),
        };
        let mut app = Launcher::from_library(&ctx, library, temp.path().join("library.json"));
        let size = Vec2::new(1200.0, 800.0);
        frame(&mut app, &ctx, size, vec![]);
        let press = |key| {
            vec![
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        frame(&mut app, &ctx, size, press(Key::ArrowDown));
        assert_eq!(app.selected, Some(grid_columns(928.0)));
        frame(&mut app, &ctx, size, press(Key::ArrowUp));
        assert_eq!(app.selected, Some(0));
        app.draft = Some(Draft::default());
        frame(&mut app, &ctx, size, press(Key::ArrowRight));
        assert_eq!(app.selected, Some(0));
        frame(&mut app, &ctx, size, press(Key::Escape));
        assert!(app.draft.is_none());
    }
    #[test]
    fn user_views_and_panel_placements_fit_and_keep_the_editor_reachable() {
        let dir = tempfile::tempdir().unwrap();
        for view in [View::Grid, View::List, View::Shelf] {
            for (navigation, details, tools) in [
                (Navigation::Right, Details::Right, Tools::Bottom),
                (Navigation::Hidden, Details::Hidden, Tools::Top),
                (Navigation::Left, Details::Bottom, Tools::Bottom),
            ] {
                for size in [Vec2::new(840.0, 600.0), Vec2::new(1200.0, 800.0)] {
                    let ctx = egui::Context::default();
                    let mut app = Launcher::from_library(
                        &ctx,
                        Library::default(),
                        dir.path().join("library.json"),
                    );
                    app.library.games = vec![Game {
                        metadata: Default::default(),
                        name: "Long title for a user-built layout".repeat(2),
                        target: Target::Steam { app_id: 10 },
                        favorite: true,
                        last_launched: None,
                        cover: None,
                        installed: Some(false),
                        hidden: false,
                    }];
                    let layout = &mut app.workspace.current_mut().layout;
                    layout.navigation = navigation;
                    layout.details = details;
                    layout.tools = tools;
                    layout.view = view;
                    layout.tile_width = 320.0;
                    layout.navigation_width = 280.0;
                    layout.details_width = 340.0;
                    frame(&mut app, &ctx, size, vec![]);
                    frame(&mut app, &ctx, size, vec![]);
                    assert!(
                        ctx.used_rect().width() <= size.x + 1.0,
                        "overflow {view:?} {navigation:?} {details:?} at {size:?}: {:?}",
                        ctx.used_rect()
                    );
                    app.open_editor();
                    let saved = app.workspace.clone();
                    app.workspace.current_mut().layout.view = View::Shelf;
                    let escape = egui::Event::Key {
                        key: Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    };
                    frame(&mut app, &ctx, size, vec![escape]);
                    assert_eq!(app.workspace, saved);
                }
            }
        }
    }
    #[test]
    fn partial_steam_sync_preserves_custom_metadata_and_known_installation_state() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let mut app =
            Launcher::from_library(&ctx, Library::default(), dir.path().join("library.json"));
        app.library.games.push(Game {
            metadata: Default::default(),
            name: "My custom title".into(),
            target: Target::Steam { app_id: 10 },
            favorite: true,
            last_launched: Some(100),
            cover: Some(dir.path().join("my-cover.png")),
            installed: Some(true),
            hidden: false,
        });
        let (tx, rx) = mpsc::channel();
        app.scan = Some(rx);
        tx.send(Ok(steam::ImportResult {
            games: vec![steam::SteamGame {
                app_id: 10,
                name: "Store title".into(),
                installed: false,
                cover: None,
            }],
            warnings: vec!["One manifest was unreadable".into()],
        }))
        .unwrap();
        app.poll();
        let game = &app.library.games[0];
        assert_eq!(game.name, "My custom title");
        assert!(game.favorite);
        assert!(game.cover.is_some());
        assert_eq!(game.installed, Some(true));
        assert_eq!(game.last_launched, Some(100));
        assert_eq!(Library::load(&app.path).unwrap(), app.library);
    }
    #[test]
    fn filters_keep_search_and_source_constraints_together() {
        let ctx = egui::Context::default();
        let mut app =
            Launcher::from_library(&ctx, Library::default(), PathBuf::from("unused.json"));
        app.library.games = vec![
            Game {
                metadata: Default::default(),
                name: "Moon Garden".into(),
                target: Target::Steam { app_id: 10 },
                favorite: true,
                last_launched: None,
                cover: None,
                installed: None,
                hidden: false,
            },
            Game {
                metadata: Default::default(),
                name: "Moon Racer".into(),
                target: Target::Steam { app_id: 20 },
                favorite: false,
                last_launched: None,
                cover: None,
                installed: None,
                hidden: false,
            },
        ];
        app.query = "MOON".into();
        app.filter = Filter::Favorites;
        assert_eq!(app.visible(), vec![0]);
        app.filter = Filter::Pc;
        assert!(app.visible().is_empty());
        app.filter = Filter::All;
        app.library.games[1].installed = Some(false);
        assert_eq!(app.visible(), vec![0, 1]);
        app.installed_only = true;
        assert_eq!(app.visible(), vec![0]);
    }
    #[test]
    fn hidden_steam_games_live_in_the_hidden_collection() {
        let ctx = egui::Context::default();
        let mut app =
            Launcher::from_library(&ctx, Library::default(), PathBuf::from("unused.json"));
        app.library.games = vec![
            Game {
                metadata: Default::default(),
                name: "Shown Steam".into(),
                target: Target::Steam { app_id: 10 },
                favorite: false,
                last_launched: None,
                cover: None,
                installed: Some(true),
                hidden: false,
            },
            Game {
                metadata: Default::default(),
                name: "Hidden Steam".into(),
                target: Target::Steam { app_id: 20 },
                favorite: false,
                last_launched: None,
                cover: None,
                installed: Some(true),
                hidden: true,
            },
        ];
        app.filter = Filter::All;
        assert_eq!(app.visible(), vec![0]);
        app.filter = Filter::Steam;
        assert_eq!(app.visible(), vec![0]);
        app.filter = Filter::Hidden;
        assert_eq!(app.visible(), vec![1]);
    }
    #[test]
    fn steam_resync_keeps_hidden_games_hidden() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let mut app =
            Launcher::from_library(&ctx, Library::default(), dir.path().join("library.json"));
        app.library.games.push(Game {
            metadata: Default::default(),
            name: "Hidden Steam".into(),
            target: Target::Steam { app_id: 10 },
            favorite: false,
            last_launched: None,
            cover: None,
            installed: Some(true),
            hidden: true,
        });
        let (tx, rx) = mpsc::channel();
        app.scan = Some(rx);
        tx.send(Ok(steam::ImportResult {
            games: vec![steam::SteamGame {
                app_id: 10,
                name: "Hidden Steam".into(),
                installed: true,
                cover: None,
            }],
            warnings: vec![],
        }))
        .unwrap();
        app.poll();
        assert!(app.library.games[0].hidden);
        app.filter = Filter::All;
        assert!(app.visible().is_empty());
        app.filter = Filter::Hidden;
        assert_eq!(app.visible(), vec![0]);
    }
    #[test]
    fn metadata_modal_blocks_library_arrows_and_renders_at_minimum_size() {
        let temp = tempfile::tempdir().unwrap();
        for size in [Vec2::new(840.0, 600.0), Vec2::new(1200.0, 800.0)] {
            let ctx = egui::Context::default();
            let games=(1..=2).map(|id|serde_json::from_value(serde_json::json!({"name":format!("Game {id}"),"target":{"kind":"steam","app_id":id}})).unwrap()).collect();
            let mut app = Launcher::from_library(
                &ctx,
                Library {
                    schema_version: 1,
                    games,
                },
                temp.path().join("library.json"),
            );
            app.open_metadata(Some(0));
            frame(&mut app, &ctx, size, vec![]);
            frame(
                &mut app,
                &ctx,
                size,
                vec![egui::Event::Key {
                    key: Key::ArrowRight,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert_eq!(app.selected, Some(0));
            assert!(app.metadata_ui.open);
            app.open_metadata(None);
            frame(&mut app, &ctx, size, vec![]);
        }
    }
}
