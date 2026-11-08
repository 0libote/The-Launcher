use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use the_launcher::metadata::{self, ArtworkProvider, Fetched, ImageChoice, ImageKind, Settings};

pub(super) struct Preview {
    choice: ImageChoice,
    path: Result<PathBuf, String>,
}
enum Event {
    Fetched(Box<Game>, Box<Result<Fetched, String>>, bool),
    Matches(Option<Target>, String, Result<Vec<metadata::Match>, String>),
    Images(Option<Target>, u32, ImageKind, Result<Vec<Preview>, String>),
    Done,
}
pub(super) struct MetadataUi {
    pub open: bool,
    pub settings: Settings,
    settings_path: PathBuf,
    cache: PathBuf,
    key: String,
    target: Option<Target>,
    query: String,
    id_input: String,
    matches: Vec<metadata::Match>,
    images: Vec<Preview>,
    kind: ImageKind,
    replace: bool,
    receiver: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    pending_automatic: bool,
    completed: usize,
    failed: usize,
    total: usize,
    status: String,
}
impl Drop for MetadataUi {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl MetadataUi {
    pub fn load(library_path: &std::path::Path) -> (Self, Option<String>) {
        let settings_path = library_path.with_file_name("metadata.json");
        let (settings, error) = match Settings::load(&settings_path) {
            Ok(s) => (s, None),
            Err(e) => (
                Settings::default(),
                Some(format!(
                    "{e:#}. Temporary defaults are active; use Save defaults to replace the file."
                )),
            ),
        };
        (
            Self {
                open: false,
                settings,
                settings_path,
                cache: library_path.with_file_name("artwork"),
                key: String::new(),
                target: None,
                query: String::new(),
                id_input: String::new(),
                matches: vec![],
                images: vec![],
                kind: ImageKind::Cover,
                replace: false,
                receiver: None,
                cancel: Arc::new(AtomicBool::new(false)),
                pending_automatic: false,
                completed: 0,
                failed: 0,
                total: 0,
                status: String::new(),
            },
            error,
        )
    }
    pub fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    fn begin(&mut self, _ctx: &egui::Context) -> mpsc::Sender<Event> {
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.status.clear();
        self.total = 0;
        self.completed = 0;
        self.failed = 0;
        tx
    }
}
impl Launcher {
    pub(super) fn open_metadata(&mut self, index: Option<usize>) {
        let state = &mut self.metadata_ui;
        state.open = true;
        state.images.clear();
        state.matches.clear();
        state.replace = false;
        if !state.busy() {
            state.status.clear();
        }
        state.target = index.map(|i| self.library.games[i].target.clone());
        state.query = index
            .map(|i| self.library.games[i].name.clone())
            .unwrap_or_default();
        state.id_input = index
            .and_then(|i| metadata::app_id(&self.library.games[i]))
            .map(|v| v.to_string())
            .unwrap_or_default();
        if let Some(original) = self.editor_original.take() {
            self.workspace = original;
        }
        self.draft = None;
        self.remove = false;
        self.steam_settings = false;
        self.steam_key.clear();
    }
    pub(super) fn queue_automatic_metadata(&mut self) {
        self.metadata_ui.pending_automatic = true;
    }
    fn start_metadata(&mut self, ctx: &egui::Context, games: Vec<Game>, replace: bool) {
        if self.metadata_ui.busy() {
            return;
        }
        let settings = self.metadata_ui.settings.clone();
        let games: Vec<_> = games
            .into_iter()
            .filter(|g| metadata::app_id(g).is_some() && settings.for_game(g, replace).any_fields())
            .collect();
        if games.is_empty() {
            self.metadata_ui.status="No matching games need the enabled fields. Match an executable to a Steam title first.".into();
            return;
        }
        let state = &mut self.metadata_ui;
        state.completed = 0;
        state.failed = 0;
        let total = games.len();
        let tx = state.begin(ctx);
        state.total = total;
        let cache = state.cache.clone();
        let key = state.key.clone();
        let cancel = state.cancel.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let provider = metadata::Provider::new();
            for game in games {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = match &provider {
                    Ok(p) => p
                        .fetch(
                            metadata::app_id(&game).unwrap(),
                            &settings.for_game(&game, replace),
                            &key,
                            &cache,
                        )
                        .map_err(|e| format!("{e:#}")),
                    Err(e) => Err(format!("{e:#}")),
                };
                let rate_limited = match &result {
                    Ok(fetched) => fetched.warnings.iter().any(|w| w.contains("rate limit")),
                    Err(e) => e.contains("rate limit"),
                };
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if tx
                    .send(Event::Fetched(Box::new(game), Box::new(result), replace))
                    .is_err()
                {
                    return;
                }
                ctx.request_repaint();
                if rate_limited {
                    break;
                }
                // A single sequential queue avoids burst requests to store/community services.
                std::thread::sleep(Duration::from_secs(2));
            }
            let _ = tx.send(Event::Done);
            ctx.request_repaint();
        });
    }
    fn start_metadata_search(&mut self, ctx: &egui::Context) {
        let query = self.metadata_ui.query.clone();
        let target = self.metadata_ui.target.clone();
        let tx = self.metadata_ui.begin(ctx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = metadata::Provider::new()
                .and_then(|p| p.search(&query))
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::Matches(target, query, result));
            let _ = tx.send(Event::Done);
            ctx.request_repaint();
        });
    }
    fn start_image_choices(&mut self, ctx: &egui::Context, id: u32) {
        let state = &mut self.metadata_ui;
        state.images.clear();
        let tx = state.begin(ctx);
        let ctx = ctx.clone();
        let kind = state.kind;
        let target = state.target.clone();
        let provider = state.settings.artwork_provider;
        let key = state.key.clone();
        let cache = state.cache.clone();
        let cancel = state.cancel.clone();
        std::thread::spawn(move || {
            let result = metadata::Provider::new().and_then(|p| {
                let choices = p.images(id, kind, provider, &key)?;
                let mut previews = Vec::new();
                for choice in choices {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let path = p.download(&choice, &cache).map_err(|e| format!("{e:#}"));
                    previews.push(Preview { choice, path });
                }
                Ok(previews)
            });
            let _ = tx.send(Event::Images(
                target,
                id,
                kind,
                result.map_err(|e| format!("{e:#}")),
            ));
            let _ = tx.send(Event::Done);
            ctx.request_repaint();
        });
    }
    pub(super) fn poll_metadata(&mut self) {
        loop {
            let event = self.metadata_ui.receiver.as_ref().map(|r| r.try_recv());
            match event {
                Some(Ok(Event::Fetched(baseline, result, replace))) => {
                    self.metadata_ui.completed += 1;
                    match *result {
                        Ok(fetched) => {
                            let warning = fetched.warnings.join(" ");
                            let result = self.library.change(&self.path, |l| {
                                if let Some(game) = l.games.iter_mut().find(|g| {
                                    g.target == baseline.target
                                        && metadata::app_id(g) == metadata::app_id(&baseline)
                                }) {
                                    metadata::apply(game, &baseline, fetched, replace);
                                }
                                Ok(())
                            });
                            if let Err(e) = result {
                                self.metadata_ui.failed += 1;
                                self.metadata_ui.status = format!("Could not save metadata: {e:#}");
                            } else if !warning.is_empty() {
                                self.metadata_ui.status = format!("{}: {warning}", baseline.name);
                            }
                        }
                        Err(error) => {
                            self.metadata_ui.failed += 1;
                            self.metadata_ui.status = format!("{}: {error}", baseline.name);
                        }
                    }
                }
                Some(Ok(Event::Matches(target, query, result))) => {
                    if !self.metadata_ui.cancel.load(Ordering::Relaxed)
                        && self.metadata_ui.target == target
                        && self.metadata_ui.query == query
                    {
                        match result {
                            Ok(matches) => {
                                self.metadata_ui.status = if matches.is_empty() {
                                    "No title matches. Try a different search or enter a Steam app ID.".into()
                                } else {
                                    "Choose the correct title below. Launch settings stay unchanged.".into()
                                };
                                self.metadata_ui.matches = matches;
                            }
                            Err(error) => self.metadata_ui.status = error,
                        }
                    }
                }
                Some(Ok(Event::Images(target, id, kind, result))) => {
                    let current_id = target
                        .as_ref()
                        .and_then(|t| self.library.games.iter().find(|g| &g.target == t))
                        .and_then(metadata::app_id);
                    if !self.metadata_ui.cancel.load(Ordering::Relaxed)
                        && self.metadata_ui.target == target
                        && self.metadata_ui.kind == kind
                        && current_id == Some(id)
                    {
                        match result {
                            Ok(images) => {
                                self.metadata_ui.images = images;
                                self.metadata_ui.status =
                                    "Choose an image to replace that artwork field.".into();
                            }
                            Err(error) => self.metadata_ui.status = error,
                        }
                    }
                }
                Some(Ok(Event::Done)) => {
                    self.metadata_ui.receiver = None;
                    if self.metadata_ui.total > 0 {
                        self.metadata_ui.status = format!(
                            "Processed {} of {} games ({} failed). {} {}",
                            self.metadata_ui.completed,
                            self.metadata_ui.total,
                            self.metadata_ui.failed,
                            if self.metadata_ui.cancel.load(Ordering::Relaxed) {
                                "Stopped."
                            } else {
                                ""
                            },
                            self.metadata_ui.status
                        );
                    }
                    break;
                }
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    self.metadata_ui.receiver = None;
                    self.metadata_ui.status =
                        "Metadata worker stopped unexpectedly; try again.".into();
                    break;
                }
                _ => break,
            }
        }
    }
    pub(super) fn metadata_dialog(&mut self, ctx: &egui::Context) {
        // Run import-triggered jobs here, where the native repaint context is available.
        if self.metadata_ui.pending_automatic && !self.metadata_ui.busy() {
            self.metadata_ui.pending_automatic = false;
            if self.metadata_ui.settings.automatic {
                self.start_metadata(
                    ctx,
                    self.library
                        .games
                        .iter()
                        .filter(|g| !g.hidden)
                        .cloned()
                        .collect(),
                    false,
                );
            }
        }
        if !self.metadata_ui.open {
            return;
        }
        let index = self
            .metadata_ui
            .target
            .as_ref()
            .and_then(|t| self.library.games.iter().position(|g| &g.target == t));
        let game = index.map(|i| self.library.games[i].clone());
        let busy = self.metadata_ui.busy();
        let colors = theme::palette(ctx);
        let mut search = false;
        let mut fetch = false;
        let mut browse = false;
        let mut close = false;
        let mut bind = None;
        let mut choose = None;
        let mut save_settings = false;
        egui::Modal::new(egui::Id::new("metadata"))
            .frame(egui::Frame::new().fill(colors.surface).inner_margin(24).corner_radius(16))
            .show(ctx, |ui| {
                ui.set_width(650.0_f32.min(ctx.content_rect().width() - 80.0));
                ui.label(heading(if game.is_some() { "Metadata & artwork" } else { "Metadata sources" }, 26.0));
                if let Some(game) = &game {
                    ui.label(&game.name);
                } else {
                    ui.label("Automatic information and images, with your choice of source.");
                }
                if busy {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        if self.metadata_ui.total > 0 {
                            ui.label(format!("Working… {} / {} games, {} failed", self.metadata_ui.completed, self.metadata_ui.total, self.metadata_ui.failed));
                        } else { ui.label("Loading provider data and previews…"); }
                        if ui.button("Stop downloads").clicked() {
                            self.metadata_ui.cancel.store(true, Ordering::Relaxed);
                            self.metadata_ui.status = "Stopping when the in-flight game finishes. Its unsaved result will be discarded.".into();
                        }
                    });
                }
                if !self.metadata_ui.status.is_empty() {
                    egui::ScrollArea::vertical().id_salt("metadata_status").max_height(64.0).show(ui, |ui| {
                        ui.label(RichText::new(&self.metadata_ui.status).color(colors.selection));
                    });
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("metadata_body")
                    .max_height((ctx.content_rect().height() - 320.0).max(140.0))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!busy, |ui| {
                            egui::CollapsingHeader::new(if game.is_some() { "Download defaults & providers" } else { "Automatic download & providers" })
                                .default_open(game.is_none())
                                .show(ui, |ui| self.metadata_settings(ui));
                            if game.is_none() {
                                self.metadata_settings_summary(ui);
                                ui.add_space(10.0);
                                ui.label("Fetch enabled fields for Steam games and executables you have matched. Existing fields are preserved.");
                                fetch = ui.button("Fill missing metadata for library").clicked();
                            }
                            if let Some(game) = &game {
                                if matches!(game.target, Target::Executable { .. }) {
                                    ui.label("Match this executable to a Steam title");
                                    ui.horizontal(|ui| {
                                        ui.add(egui::TextEdit::singleline(&mut self.metadata_ui.query).desired_width(420.0).char_limit(256));
                                        search = ui.button("Search titles").clicked();
                                    });
                                    for candidate in &self.metadata_ui.matches {
                                        if ui.button(format!("{} · Steam {}", candidate.name, candidate.app_id)).clicked() {
                                            bind = Some(candidate.app_id);
                                        }
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label("Steam app ID");
                                        ui.add(egui::TextEdit::singleline(&mut self.metadata_ui.id_input).desired_width(120.0).char_limit(10));
                                        if ui.button("Use this ID").clicked() {
                                            match self.metadata_ui.id_input.trim().parse::<u32>() {
                                                Ok(id) if id > 0 => bind = Some(id),
                                                _ => self.metadata_ui.status = "Enter a positive numeric Steam app ID.".into(),
                                            }
                                        }
                                    });
                                    ui.label(RichText::new("Matching only selects a metadata source. Your executable remains the launch target.").small().color(colors.muted));
                                }
                                if let Some(id) = metadata::app_id(game) {
                                    ui.label(format!("Metadata identity: Steam {id}"));
                                    ui.checkbox(&mut self.metadata_ui.replace, "Replace existing enabled fields");
                                    fetch = ui.button(if self.metadata_ui.replace { "Refresh enabled metadata & images" } else { "Fill missing metadata & images" }).clicked();
                                    ui.separator();
                                    ui.label(heading("Choose artwork", 18.0));
                                    ui.horizontal(|ui| {
                                        for (kind, label) in [(ImageKind::Cover, "Cover"), (ImageKind::Background, "Background"), (ImageKind::Logo, "Logo")] {
                                            if ui.selectable_label(self.metadata_ui.kind == kind, label).clicked() {
                                                self.metadata_ui.kind = kind;
                                                self.metadata_ui.images.clear();
                                            }
                                        }
                                    });
                                    ui.push_id("image_provider", |ui| provider_picker(ui, &mut self.metadata_ui.settings.artwork_provider));
                                    if self.metadata_ui.settings.artwork_provider == ArtworkProvider::SteamGridDb {
                                        key_input(ui, &mut self.metadata_ui.key);
                                    }
                                    browse = ui.button("Browse image options").clicked();
                                    for (i, preview) in self.metadata_ui.images.iter().enumerate() {
                                        ui.horizontal(|ui| {
                                            match &preview.path {
                                                Ok(path) => {
                                                    if let Some(texture) = self.covers.texture(ctx, path) {
                                                        ui.add(egui::Image::new(&texture).max_size(Vec2::new(90.0, 110.0)));
                                                    }
                                                    ui.vertical(|ui| {
                                                        ui.label(&preview.choice.label);
                                                        ui.label(RichText::new(&preview.choice.source).small().color(colors.muted));
                                                        if ui.button("Use image").clicked() { choose = Some(i); }
                                                    });
                                                }
                                                Err(error) => { ui.label(format!("{}: {error}", preview.choice.label)); }
                                            }
                                        });
                                        ui.separator();
                                    }
                                }
                                ui.separator();
                                ui.label(heading("Saved information", 18.0));
                                self.metadata_details(ui, game);
                            }
                        });
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!busy, |ui| { save_settings = ui.button("Save defaults").clicked(); });
                    if ui.button("Close").clicked() { close = true; }
                });
            });
        if save_settings {
            let result = self
                .metadata_ui
                .settings
                .save(&self.metadata_ui.settings_path);
            self.metadata_ui.status = match result {
                Ok(()) => "Metadata defaults saved".into(),
                Err(e) => format!("Could not save defaults: {e:#}"),
            };
        }
        if let (Some(i), Some(id)) = (index, bind) {
            let result = self.library.change(&self.path, |l| {
                l.games[i].metadata.steam_app_id = Some(id);
                Ok(())
            });
            if result.is_ok() {
                self.metadata_ui.images.clear();
                self.metadata_ui.matches.clear();
                self.metadata_ui.status =
                    format!("Matched to Steam {id}. Choose Fill missing metadata to download.");
                if self.metadata_ui.settings.automatic {
                    self.start_metadata(ctx, vec![self.library.games[i].clone()], false);
                }
            } else {
                self.result(result, "");
            }
        }
        if search {
            self.start_metadata_search(ctx);
        }
        if fetch {
            let games = game.clone().map(|g| vec![g]).unwrap_or_else(|| {
                self.library
                    .games
                    .iter()
                    .filter(|g| !g.hidden)
                    .cloned()
                    .collect()
            });
            self.start_metadata(ctx, games, game.is_some() && self.metadata_ui.replace);
        }
        if browse && let Some(id) = game.as_ref().and_then(metadata::app_id) {
            self.start_image_choices(ctx, id);
        }
        if let (Some(i), Some(choice)) = (index, choose) {
            let preview = &self.metadata_ui.images[choice];
            if let Ok(path) = &preview.path {
                let path = path.clone();
                let source = preview.choice.source.clone();
                let kind = self.metadata_ui.kind;
                let result = self.library.change(&self.path, |l| {
                    let game = &mut l.games[i];
                    match kind {
                        ImageKind::Cover => game.cover = Some(path),
                        ImageKind::Background => game.metadata.background = Some(path),
                        ImageKind::Logo => game.metadata.logo = Some(path),
                    }
                    game.metadata.sources.insert(kind.field().into(), source);
                    Ok(())
                });
                self.metadata_ui.status = match result {
                    Ok(()) => "Artwork saved".into(),
                    Err(e) => format!("Could not save artwork: {e:#}"),
                };
            }
        }
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.metadata_ui.open = false;
        }
    }
    fn metadata_settings(&mut self, ui: &mut egui::Ui) {
        let state = &mut self.metadata_ui;
        ui.checkbox(
            &mut state.settings.automatic,
            "Automatically fill missing fields after import or matching",
        );
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut state.settings.download_text,
                "Description & details (Steam)",
            );
            ui.checkbox(&mut state.settings.download_cover, "Cover");
            ui.checkbox(&mut state.settings.download_background, "Background");
            ui.checkbox(&mut state.settings.download_logo, "Logo");
        });
        provider_picker(ui, &mut state.settings.artwork_provider);
        if state.settings.artwork_provider == ArtworkProvider::SteamGridDb {
            key_input(ui, &mut state.key);
        }
        ui.label("Automatic downloads use your preferred artwork source, then Steam as fallback. Keys stay in memory for this app session.");
    }
    fn metadata_settings_summary(&mut self, ui: &mut egui::Ui) {
        ui.label(format!(
            "Artwork: {} · automatic: {}",
            self.metadata_ui.settings.artwork_provider.label(),
            if self.metadata_ui.settings.automatic {
                "on"
            } else {
                "off"
            }
        ));
    }
    pub(super) fn metadata_details(&mut self, ui: &mut egui::Ui, game: &Game) {
        if let Some(path) = &game.metadata.background
            && let Some(texture) = self.covers.texture(ui.ctx(), path)
        {
            ui.add(egui::Image::new(&texture).max_size(Vec2::new(ui.available_width(), 180.0)));
        }
        if let Some(path) = &game.metadata.logo
            && let Some(texture) = self.covers.texture(ui.ctx(), path)
        {
            ui.add(
                egui::Image::new(&texture)
                    .max_size(Vec2::new(ui.available_width().min(240.0), 80.0)),
            );
        }
        let m = &game.metadata;
        if !m.description.is_empty() {
            ui.label(&m.description);
        }
        for (label, value) in [
            ("Released", m.release_date.clone()),
            ("Developer", m.developers.join(", ")),
            ("Publisher", m.publishers.join(", ")),
            ("Genres", m.genres.join(", ")),
        ] {
            if !value.is_empty() {
                ui.label(format!("{label}: {value}"));
            }
        }
        if !m.sources.is_empty() {
            ui.collapsing("Sources", |ui| {
                for (field, source) in &m.sources {
                    ui.label(format!("{field}: {source}"));
                }
            });
        }
        if m.description.is_empty() && m.sources.is_empty() {
            ui.label("No downloaded metadata yet.");
        }
    }
}
fn provider_picker(ui: &mut egui::Ui, provider: &mut ArtworkProvider) {
    egui::ComboBox::from_id_salt("artwork_provider")
        .selected_text(provider.label())
        .show_ui(ui, |ui| {
            ui.selectable_value(provider, ArtworkProvider::Steam, "Steam official");
            ui.selectable_value(
                provider,
                ArtworkProvider::SteamGridDb,
                "SteamGridDB first, Steam fallback",
            );
        });
}
fn key_input(ui: &mut egui::Ui, key: &mut String) {
    ui.horizontal(|ui| {
        ui.label("SteamGridDB key");
        ui.add(
            egui::TextEdit::singleline(key)
                .password(true)
                .char_limit(256),
        );
        if ui.small_button("Forget").clicked() {
            key.clear();
        }
    });
    ui.hyperlink_to(
        "Get a SteamGridDB API key",
        "https://www.steamgriddb.com/profile/preferences/api",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_results_follow_identity_and_preserve_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let ctx = egui::Context::default();
        let game:Game=serde_json::from_value(serde_json::json!({"name":"Custom", "target":{"kind":"steam","app_id":620},"cover":"my-cover.png"})).unwrap();
        let mut app = Launcher::from_library(
            &ctx,
            Library {
                schema_version: 1,
                games: vec![game.clone()],
            },
            path,
        );
        let fetched = Fetched {
            metadata: metadata::GameMetadata {
                description: "Fetched".into(),
                ..Default::default()
            },
            cover: Some("download.png".into()),
            ..Default::default()
        };
        let (tx, rx) = mpsc::channel();
        app.metadata_ui.receiver = Some(rx);
        app.library.games[0].favorite = true;
        tx.send(Event::Fetched(
            Box::new(game.clone()),
            Box::new(Ok(fetched.clone())),
            false,
        ))
        .unwrap();
        tx.send(Event::Done).unwrap();
        app.poll_metadata();
        assert_eq!(app.library.games[0].metadata.description, "Fetched");
        assert!(app.library.games[0].favorite);
        assert_eq!(app.library.games[0].cover, game.cover);
        let (tx, rx) = mpsc::channel();
        app.metadata_ui.receiver = Some(rx);
        app.library.games.clear();
        tx.send(Event::Fetched(Box::new(game), Box::new(Ok(fetched)), true))
            .unwrap();
        tx.send(Event::Done).unwrap();
        app.poll_metadata();
        assert!(app.library.games.is_empty());
    }
    #[test]
    fn late_image_choices_do_not_follow_user_to_a_different_game() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app =
            Launcher::from_library(&ctx, Library::default(), dir.path().join("library.json"));
        app.metadata_ui.target = Some(Target::Steam { app_id: 20 });
        let (tx, rx) = mpsc::channel();
        app.metadata_ui.receiver = Some(rx);
        tx.send(Event::Images(
            Some(Target::Steam { app_id: 10 }),
            10,
            ImageKind::Cover,
            Ok(vec![Preview {
                choice: ImageChoice {
                    label: "Old game".into(),
                    url: "unused".into(),
                    source: "Steam".into(),
                },
                path: Ok("old.png".into()),
            }]),
        ))
        .unwrap();
        tx.send(Event::Matches(
            Some(Target::Steam { app_id: 10 }),
            "old query".into(),
            Ok(vec![metadata::Match {
                app_id: 10,
                name: "Old game".into(),
            }]),
        ))
        .unwrap();
        tx.send(Event::Done).unwrap();
        app.poll_metadata();
        assert!(app.metadata_ui.images.is_empty());
        assert!(app.metadata_ui.matches.is_empty());
    }
}
