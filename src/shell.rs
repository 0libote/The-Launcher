//! The Launcher's Rune frontend: sidebar, game views, game details modal,
//! search, add/edit/remove dialogs, Steam import, metadata & artwork, cover art,
//! and the workspace editor.
//!
//! This is the whole UI layer. Data and behavior stay in the library crates:
//! [`crate::library`] owns persistence, [`crate::launch`] owns dispatch, and the
//! Steam/metadata/workspace modules own their domains. Network and decoding
//! work happens on worker threads; the UI thread only merges results.
//!
//! shortcut: keyboard navigation, gamepad input, and television-distance
//! tuning are future work; the shell is pointer-first with a searchable shelf.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rune::tok::{motion, radius, type_};
use rune::{
    AppSpec, Color, Cx, El, FileDialogOptions, Pixels, ScrollSpec, SurfaceFit, TextEdit, Tx, col,
    el, row, spacer, text, text_input,
};
use the_launcher::{
    launch,
    library::{Game, Library, Target},
    metadata::{self, ArtworkProvider, Fetched, ImageKind, Settings},
    steam,
    workspace::{Details, Navigation, View, Workspace},
};

/// Accent presets for the workspace editor: coral is the default.
const ACCENTS: [([u8; 3], &str); 6] = [
    ([255, 151, 124], "Coral"),
    ([159, 222, 204], "Mint"),
    ([183, 175, 224], "Lilac"),
    ([150, 190, 255], "Sky"),
    ([255, 200, 120], "Gold"),
    ([255, 150, 180], "Rose"),
];
/// Background presets for the workspace editor.
const BACKGROUNDS: [([u8; 3], &str); 4] = [
    ([16, 20, 32], "Midnight"),
    ([10, 12, 20], "Abyss"),
    ([24, 28, 40], "Slate"),
    ([28, 22, 26], "Ember"),
];
/// Navigation-rail presets: a step above the canvas.
const NAVS: [([u8; 3], &str); 4] = [
    ([21, 26, 40], "Rail"),
    ([14, 17, 28], "Deep"),
    ([30, 34, 50], "Slate"),
    ([34, 28, 32], "Ember"),
];
/// Panel-surface presets for cards, details, and dialogs.
const SURFACES: [([u8; 3], &str); 4] = [
    ([29, 35, 51], "Panel"),
    ([20, 24, 38], "Low"),
    ([38, 46, 64], "Raised"),
    ([40, 32, 36], "Warm"),
];
/// Text presets: warm ivory default, plus muted variants for secondary text.
const TEXTS: [([u8; 3], &str); 3] = [
    ([243, 240, 232], "Ivory"),
    ([225, 230, 240], "Frost"),
    ([255, 255, 255], "White"),
];
const MUTEDS: [([u8; 3], &str); 3] = [
    ([160, 171, 190], "Slate"),
    ([140, 150, 170], "Cool"),
    ([180, 175, 165], "Warm"),
];
/// Hairline presets for borders and rules.
const LINES: [([u8; 3], &str); 3] = [
    ([49, 59, 80], "Slate"),
    ([36, 44, 62], "Deep"),
    ([64, 72, 96], "Bright"),
];
/// Selection presets: mint is the default.
const SELECTIONS: [([u8; 3], &str); 4] = [
    ([159, 222, 204], "Mint"),
    ([255, 151, 124], "Coral"),
    ([150, 190, 255], "Sky"),
    ([183, 175, 224], "Lilac"),
];

/// Every color the shell paints, resolved from the active workspace skin.
/// Computed per frame from [`Shell::workspace`] so skins apply live to the
/// titlebar, rail, shelf, cards, details, and dialogs — not just the canvas.
#[derive(Clone, Copy)]
struct Look {
    bg: Color,
    nav: Color,
    surface: Color,
    raised: Color,
    text: Color,
    muted: Color,
    faint: Color,
    line: Color,
    accent: Color,
    ink: Color,
    select: Color,
    err: Color,
}

impl Look {
    fn of(workspace: &Workspace) -> Self {
        let skin = &workspace.current().skin;
        let accent = rgb8(skin.accent);
        let select = if skin.accent == [159, 222, 204] {
            rgb8([255, 151, 124])
        } else {
            rgb8(skin.selection)
        };
        Look {
            bg: rgb8(skin.background),
            nav: rgb8(skin.navigation),
            surface: rgb8(skin.surface),
            raised: rgb8(skin.raised),
            text: rgb8(skin.text),
            muted: rgb8(skin.muted),
            faint: rgb8(skin.faint),
            line: rgb8(skin.line),
            accent,
            ink: ink_for(skin.accent),
            select,
            err: Color {
                r: 1.0,
                g: 0.45,
                b: 0.42,
                a: 1.0,
            },
        }
    }

    /// Subtle hover wash that works on any dark skin.
    fn hover(self) -> Color {
        self.text.alpha(0.07)
    }

    /// Pressed wash, a touch stronger than hover.
    fn pressed(self) -> Color {
        self.text.alpha(0.12)
    }

    /// Tinted wash for the active collection row.
    fn active_wash(self) -> Color {
        self.accent.alpha(0.14)
    }

    /// Selected-row wash for list rows.
    fn select_wash(self) -> Color {
        self.select.alpha(0.12)
    }

    /// Top-lit rim highlight for elevated panels and cards.
    fn rim_top(self) -> Color {
        self.text.alpha(0.14)
    }
}

/// Dark ink on light accents, warm white on dark ones. Luminance is measured
/// in linear light.
fn ink_for(accent: [u8; 3]) -> Color {
    let [r, g, b] = accent.map(|v| srgb_to_linear(v as f32 / 255.0));
    if 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.35 {
        Color {
            r: 0.10,
            g: 0.08,
            b: 0.10,
            a: 1.0,
        }
    } else {
        Color {
            r: 0.95,
            g: 0.94,
            b: 0.91,
            a: 1.0,
        }
    }
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn rgb8(hex: [u8; 3]) -> Color {
    let [r, g, b] = hex.map(|v| srgb_to_linear(v as f32 / 255.0));
    Color { r, g, b, a: 1.0 }
}

/// Six jewel-tone gradients, picked by hashing the game name. Original art,
/// no downloaded images involved.
const GRADIENTS: [([f32; 3], [f32; 3]); 6] = [
    ([0.36, 0.38, 0.80], [0.15, 0.16, 0.38]),
    ([0.24, 0.66, 0.62], [0.09, 0.30, 0.30]),
    ([1.00, 0.59, 0.49], [0.52, 0.20, 0.22]),
    ([0.62, 0.55, 0.90], [0.26, 0.22, 0.50]),
    ([0.95, 0.70, 0.35], [0.48, 0.30, 0.12]),
    ([0.40, 0.55, 0.82], [0.16, 0.22, 0.40]),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Filter {
    #[default]
    All,
    Favorites,
    Pc,
    Steam,
    Hidden,
}

impl Filter {
    const ALL: [(Filter, &'static str, &'static str); 5] = [
        (Filter::All, "all", "All games"),
        (Filter::Favorites, "favorites", "Favorites"),
        (Filter::Pc, "pc", "PC"),
        (Filter::Steam, "steam", "Steam"),
        (Filter::Hidden, "hidden", "Hidden"),
    ];

    fn matches(self, game: &Game) -> bool {
        if self == Filter::Hidden {
            return game.hidden;
        }
        if game.hidden {
            return false;
        }
        match self {
            Filter::All => true,
            Filter::Favorites => game.favorite,
            Filter::Pc => game.source() == "PC",
            Filter::Steam => game.source() == "Steam",
            Filter::Hidden => false,
        }
    }
}

pub struct Shell {
    library: Library,
    path: PathBuf,
    query: TextEdit,
    filter: Filter,
    selected: Option<usize>,
    details_open: bool,
    notice: Option<String>,
    children: Vec<Child>,
    draft: Option<Draft>,
    remove: bool,
    steam: SteamPanel,
    workspace: Workspace,
    workspace_path: PathBuf,
    workspace_open: bool,
    workspace_snapshot: Option<Workspace>,
    ws_name: TextEdit,
    covers: HashMap<PathBuf, Pixels>,
    meta: MetaPanel,
}

/// Metadata & artwork overlay: settings, title matching, downloads, and
/// artwork choice. Network work runs on worker threads; the UI thread merges
/// [`MetaEvent`]s in [`poll`].
struct MetaPanel {
    open: bool,
    target: Option<Target>,
    query: TextEdit,
    id_input: TextEdit,
    key: TextEdit,
    settings: Settings,
    settings_path: PathBuf,
    cache: PathBuf,
    matches: Vec<metadata::Match>,
    images: Vec<ImagePreview>,
    /// Decoded choice thumbnails, scoped to the current browse. Cleared when
    /// the panel opens or a new browse starts, so browsing artwork cannot
    /// churn the long-lived covers cache.
    previews: HashMap<PathBuf, Pixels>,
    kind: ImageKind,
    replace: bool,
    status: String,
    completed: usize,
    failed: usize,
    total: usize,
    busy: bool,
    cancel: Arc<AtomicBool>,
    rx: Option<mpsc::Receiver<MetaEvent>>,
}

struct ImagePreview {
    label: String,
    source: String,
    path: Result<PathBuf, String>,
}

enum MetaEvent {
    Fetched(Box<Game>, Result<Fetched, String>, bool),
    Matches(Option<Target>, String, Result<Vec<metadata::Match>, String>),
    Images(
        Option<Target>,
        ImageKind,
        u32,
        Result<Vec<ImagePreview>, String>,
    ),
    Done,
}

impl MetaPanel {
    fn closed() -> Self {
        MetaPanel {
            open: false,
            target: None,
            query: TextEdit::new(""),
            id_input: TextEdit::new(""),
            key: TextEdit::new("").secure(),
            settings: Settings::default(),
            settings_path: PathBuf::new(),
            cache: PathBuf::new(),
            matches: Vec::new(),
            images: Vec::new(),
            previews: HashMap::new(),
            kind: ImageKind::Cover,
            replace: false,
            status: String::new(),
            completed: 0,
            failed: 0,
            total: 0,
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
            rx: None,
        }
    }
}

/// Add/edit form. Rune text fields borrow a [`TextEdit`] from state, so each
/// field owns one; validation errors stay on the form until it saves.
struct Draft {
    name: TextEdit,
    path: TextEdit,
    args: TextEdit,
    dir: TextEdit,
    editing: Option<usize>,
    error: Option<String>,
}

impl Draft {
    fn blank() -> Self {
        let mut name = TextEdit::new("");
        name.focused = true;
        Draft {
            name,
            path: TextEdit::new(""),
            args: TextEdit::new(""),
            dir: TextEdit::new(""),
            editing: None,
            error: None,
        }
    }

    fn for_game(game: &Game, index: usize) -> Self {
        let (path, arguments, directory) = match &game.target {
            Target::Executable {
                path,
                arguments,
                working_directory,
            } => (
                path.display().to_string(),
                arguments.join("\n"),
                working_directory
                    .as_ref()
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default(),
            ),
            Target::Steam { .. } => (String::new(), String::new(), String::new()),
        };
        let mut name = TextEdit::new(game.name.clone());
        name.focused = true;
        Draft {
            name,
            path: TextEdit::new(path),
            args: TextEdit::new(arguments),
            dir: TextEdit::new(directory),
            editing: Some(index),
            error: None,
        }
    }
}

/// Steam import overlay: fields, background scan state, and the last outcome.
struct SteamPanel {
    open: bool,
    root: TextEdit,
    account: TextEdit,
    key: TextEdit,
    scanning: bool,
    error: Option<String>,
    summary: Option<String>,
    warnings: Vec<String>,
    rx: Option<mpsc::Receiver<ScanOutcome>>,
}

type ScanOutcome = Result<steam::ImportResult>;

impl SteamPanel {
    fn closed() -> Self {
        SteamPanel {
            open: false,
            root: TextEdit::new(""),
            account: TextEdit::new(""),
            key: TextEdit::new("").secure(),
            scanning: false,
            error: None,
            summary: None,
            warnings: Vec::new(),
            rx: None,
        }
    }

    fn opened(&mut self) {
        self.open = true;
        self.error = None;
        self.summary = None;
        self.warnings.clear();
    }
}

impl Shell {
    fn state() -> Shell {
        Shell {
            library: Library::default(),
            path: PathBuf::new(),
            query: TextEdit::new(""),
            filter: Filter::All,
            selected: None,
            details_open: false,
            notice: None,
            children: Vec::new(),
            draft: None,
            remove: false,
            steam: SteamPanel::closed(),
            workspace: Workspace::default(),
            workspace_path: PathBuf::new(),
            workspace_open: false,
            workspace_snapshot: None,
            ws_name: TextEdit::new(""),
            covers: HashMap::new(),
            meta: MetaPanel::closed(),
        }
    }

    pub fn from_disk(library: Library, path: PathBuf) -> Shell {
        let mut shell = Shell::state();
        shell.library = library;
        shell.path = path;
        let workspace_path = shell.path.with_file_name("workspace.json");
        match Workspace::load(&workspace_path) {
            Ok(workspace) => shell.workspace = workspace,
            Err(error) => {
                shell.notice = Some(format!(
                    "{error:#} Using a temporary default; your workspace file is unchanged."
                ));
            }
        }
        shell.workspace_path = workspace_path;
        shell.ws_name.text = shell.workspace.current().name.clone();
        let meta_path = shell.path.with_file_name("metadata.json");
        match Settings::load(&meta_path) {
            Ok(settings) => shell.meta.settings = settings,
            Err(error) => {
                let message = format!(
                    "{error:#} Temporary defaults are active; use Save defaults to replace the file."
                );
                shell.notice = shell
                    .notice
                    .take()
                    .map(|notice| format!("{notice} {message}"))
                    .or(Some(message));
            }
        }
        shell.meta.settings_path = meta_path;
        shell.meta.cache = shell.path.with_file_name("artwork");
        shell.decode_covers();
        shell.selected = shell.visible().into_iter().next();
        shell
    }

    /// Decode every referenced cover once, bounded like the old texture
    /// cache, so cards and details paint images without UI-thread stalls.
    fn decode_covers(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let paths: Vec<PathBuf> = self
            .library
            .games
            .iter()
            .filter_map(|game| game.cover.clone())
            .filter(|path| seen.insert(path.clone()))
            .take(64)
            .collect();
        for path in paths {
            if let Ok(pixels) = decode_cover(&path) {
                self.covers.insert(path, pixels);
            }
        }
    }

    fn remember_cover(&mut self, path: &Path) {
        if let Ok(pixels) = decode_cover(path) {
            self.cache_cover(path.to_path_buf(), pixels);
        }
    }

    /// Cache decoded pixels, bounding the long-lived map.
    fn cache_cover(&mut self, path: PathBuf, pixels: Pixels) {
        if self.covers.len() >= 64 {
            self.covers.clear();
        }
        self.covers.insert(path, pixels);
    }

    /// Deterministic fixtures for headless scenes and tests.
    pub fn demo() -> Shell {
        let mut shell = Shell::state();
        let game = |name: &str, target: the_launcher::library::Target| Game {
            metadata: Default::default(),
            name: name.into(),
            target,
            favorite: false,
            last_launched: None,
            cover: None,
            installed: None,
            hidden: false,
        };
        use the_launcher::library::Target::{Executable, Steam};
        shell.library.games = vec![
            game(
                "Hollow Drift",
                Executable {
                    path: PathBuf::from("C:\\Games\\HollowDrift\\drift.exe"),
                    arguments: vec![],
                    working_directory: None,
                },
            ),
            Game {
                favorite: true,
                installed: Some(true),
                ..game("Starfall Tactics", Steam { app_id: 620 })
            },
            game(
                "Ember & Oak",
                Executable {
                    path: PathBuf::from("C:\\Games\\EmberOak\\ember.exe"),
                    arguments: vec!["--profile".into(), "Story Mode".into()],
                    working_directory: None,
                },
            ),
            Game {
                installed: Some(false),
                ..game("Neon Harbor", Steam { app_id: 4000 })
            },
            Game {
                favorite: true,
                ..game(
                    "Paper Lanterns",
                    Executable {
                        path: PathBuf::from("C:\\Games\\Lanterns\\lanterns.exe"),
                        arguments: vec![],
                        working_directory: None,
                    },
                )
            },
            game(
                "A very long game title that should still sit neatly on its card",
                Executable {
                    path: PathBuf::from("C:\\Games\\Long\\long.exe"),
                    arguments: vec![],
                    working_directory: None,
                },
            ),
        ];
        shell.selected = Some(1);
        shell
    }

    fn visible(&self) -> Vec<usize> {
        visible_indices(&self.library, self.filter, &self.query.text)
    }

    /// The game the details view and Play act on: the selection when still
    /// visible, else the first visible row.
    fn current(&self) -> Option<usize> {
        let visible = self.visible();
        self.selected
            .filter(|i| visible.contains(i))
            .or(visible.into_iter().next())
    }

    /// Select a game. When details live in a modal, selecting opens it.
    fn select_game(&mut self, index: usize) {
        self.selected = Some(index);
        if self.workspace.current().layout.details == Details::Modal {
            self.details_open = true;
        }
    }

    fn close_details(&mut self) {
        self.details_open = false;
    }

    fn reselect(&mut self) {
        self.selected = self.visible().into_iter().next();
    }

    fn failed(&mut self, context: &str, error: anyhow::Error) {
        self.notice = Some(format!("{context}: {error:#}"));
    }

    fn toggle_favorite(&mut self, index: usize) {
        let path = self.path.clone();
        let name = self
            .library
            .games
            .get(index)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        if self.path.as_os_str().is_empty() {
            if let Some(game) = self.library.games.get_mut(index) {
                game.favorite = !game.favorite;
            }
            return;
        }
        match self.library.change(&path, |library| {
            if let Some(game) = library.games.get_mut(index) {
                game.favorite = !game.favorite;
            }
            Ok(())
        }) {
            Ok(()) => {
                self.notice = Some(favorite_note(
                    self.library
                        .games
                        .get(index)
                        .map(|g| g.favorite)
                        .unwrap_or(false),
                    &name,
                ))
            }
            Err(error) => self.failed("Could not save favorite", error),
        }
    }

    fn open_add(&mut self) {
        self.notice = None;
        self.draft = Some(Draft::blank());
    }

    fn request_cover(&mut self, index: usize, tx: &mut Tx) {
        if self.library.games.get(index).is_none() {
            return;
        }
        let mut options = FileDialogOptions {
            title: "Choose a cover image".into(),
            ..Default::default()
        };
        if cfg!(windows) {
            options = options.filter("Images", &["png", "jpg", "jpeg"]);
        }
        tx.open_file(options, move |s: &mut Shell, result, _| {
            let path = match result {
                Ok(Some(path)) => path,
                Ok(None) => return,
                Err(error) => {
                    s.failed("Cover dialog failed", anyhow::anyhow!(error.0));
                    return;
                }
            };
            s.apply_cover(index, &path);
        });
    }

    fn apply_cover(&mut self, index: usize, path: &Path) {
        let pixels = match decode_cover(path) {
            Ok(pixels) => pixels,
            Err(error) => {
                self.failed("Could not use that cover", error);
                return;
            }
        };
        let path = path.to_path_buf();
        let done = if self.path.as_os_str().is_empty() {
            if let Some(game) = self.library.games.get_mut(index) {
                game.cover = Some(path.clone());
            }
            Ok(())
        } else {
            let library_path = self.path.clone();
            self.library.change(&library_path, |library| {
                if let Some(game) = library.games.get_mut(index) {
                    game.cover = Some(path.clone());
                }
                Ok(())
            })
        };
        match done {
            Ok(()) => {
                if self.covers.len() >= 64 {
                    self.covers.clear();
                }
                self.covers.insert(path, pixels);
                self.notice = Some("Cover art saved".into());
            }
            Err(error) => self.failed("Could not save cover art", error),
        }
    }

    fn remove_cover(&mut self, index: usize) {
        let done = if self.path.as_os_str().is_empty() {
            if let Some(game) = self.library.games.get_mut(index) {
                game.cover = None;
                game.metadata.sources.remove("cover");
            }
            Ok(())
        } else {
            let library_path = self.path.clone();
            self.library.change(&library_path, |library| {
                if let Some(game) = library.games.get_mut(index) {
                    game.cover = None;
                    game.metadata.sources.remove("cover");
                }
                Ok(())
            })
        };
        match done {
            Ok(()) => self.notice = Some("Original cover restored".into()),
            Err(error) => self.failed("Could not remove cover art", error),
        }
    }

    fn open_edit(&mut self, index: usize) {
        if let Some(game) = self.library.games.get(index) {
            if !matches!(game.target, Target::Executable { .. }) {
                return;
            }
            self.notice = None;
            self.draft = Some(Draft::for_game(game, index));
        }
    }

    /// Validate the form and atomically persist it, preserving favorites,
    /// timestamps, covers, and install state across edits.
    fn commit_draft(&mut self) {
        let result = self.draft.as_ref().map(build_game);
        let game = match result {
            None => return,
            Some(Err(error)) => {
                if let Some(draft) = self.draft.as_mut() {
                    draft.error = Some(format!("{error:#}"));
                }
                return;
            }
            Some(Ok(game)) => game,
        };
        let editing = self.draft.as_ref().and_then(|draft| draft.editing);
        let apply = |library: &mut Library| -> Result<()> {
            if let Some(i) = editing {
                if library.games.get(i).is_none() {
                    bail!("That game is no longer in the library");
                }
                let mut remaining = library.clone();
                remaining.games.remove(i);
                remaining.add(game.clone())?;
                let slot = &mut library.games[i];
                library.games[i] = Game {
                    favorite: slot.favorite,
                    last_launched: slot.last_launched,
                    cover: slot.cover.clone(),
                    installed: slot.installed,
                    hidden: slot.hidden,
                    metadata: slot.metadata.clone(),
                    ..game
                };
            } else {
                library.add(game)?;
            }
            Ok(())
        };
        if self.path.as_os_str().is_empty() {
            let mut next = self.library.clone();
            match apply(&mut next) {
                Ok(()) => {
                    self.library = next;
                    self.selected = Some(editing.unwrap_or(self.library.games.len() - 1));
                    self.draft = None;
                    self.notice = Some("Game saved to your library".into());
                    self.maybe_automatic();
                }
                Err(error) => self.draft_error(error),
            }
            return;
        }
        let path = self.path.clone();
        match self.library.change(&path, apply) {
            Ok(()) => {
                self.selected = Some(editing.unwrap_or(self.library.games.len() - 1));
                self.draft = None;
                self.notice = Some("Game saved to your library".into());
                self.maybe_automatic();
            }
            Err(error) => self.draft_error(error),
        }
    }

    fn draft_error(&mut self, error: anyhow::Error) {
        if let Some(draft) = self.draft.as_mut() {
            draft.error = Some(format!("{error:#}"));
        } else {
            self.failed("Could not save the game", error);
        }
    }

    /// The selected game wants hiding (Steam) or removal. Confirm first.
    fn begin_remove(&mut self) {
        if self.current().is_some() {
            self.remove = true;
        }
    }

    fn confirm_remove(&mut self) {
        let Some(index) = self.current() else {
            self.remove = false;
            return;
        };
        let Some(game) = self.library.games.get(index) else {
            self.remove = false;
            return;
        };
        let hiding = !game.hidden && matches!(game.target, Target::Steam { .. });
        let apply = |library: &mut Library| -> Result<()> {
            if library.games.get(index).is_none() {
                bail!("That game is no longer in the library");
            }
            if hiding {
                library.games[index].hidden = true;
            } else {
                library.games.remove(index);
            }
            Ok(())
        };
        let done = if self.path.as_os_str().is_empty() {
            let mut next = self.library.clone();
            apply(&mut next).map(|()| self.library = next)
        } else {
            let path = self.path.clone();
            self.library.change(&path, apply)
        };
        match done {
            Ok(()) => {
                self.selected = None;
                self.reselect();
                self.remove = false;
                self.details_open = false;
                self.notice = Some(
                    if hiding {
                        "Game hidden"
                    } else {
                        "Game removed"
                    }
                    .into(),
                );
            }
            Err(error) => {
                self.remove = false;
                self.failed("Could not update the library", error);
            }
        }
    }

    fn restore_hidden(&mut self, index: usize) {
        let path = self.path.clone();
        let done = if path.as_os_str().is_empty() {
            if let Some(game) = self.library.games.get_mut(index) {
                game.hidden = false;
            }
            Ok(())
        } else {
            self.library.change(&path, |library| {
                if let Some(game) = library.games.get_mut(index) {
                    game.hidden = false;
                }
                Ok(())
            })
        };
        match done {
            Ok(()) => self.notice = Some("Game restored to your library".into()),
            Err(error) => self.failed("Could not restore the game", error),
        }
    }

    /// Open the workspace editor, snapshotting for Cancel.
    /// The name field takes no autofocus: a programmatically focused field
    /// with an on_input_change handler swallows the next press elsewhere.
    fn open_workspace(&mut self) {
        if self.workspace_snapshot.is_none() {
            self.workspace_snapshot = Some(self.workspace.clone());
        }
        self.ws_name.text = self.workspace.current().name.clone();
        self.workspace_open = true;
    }

    fn close_workspace(&mut self) {
        self.workspace_open = false;
        self.workspace_snapshot = None;
    }

    fn cancel_workspace(&mut self) {
        if let Some(snapshot) = self.workspace_snapshot.take() {
            self.workspace = snapshot;
            self.ws_name.text = self.workspace.current().name.clone();
        }
        self.workspace_open = false;
    }

    fn save_workspace(&mut self) {
        if self.workspace_path.as_os_str().is_empty() {
            self.notice = Some("Workspace lives for this session; no library path is set.".into());
            self.close_workspace();
            return;
        }
        let path = self.workspace_path.clone();
        match self.workspace.save(&path) {
            Ok(()) => {
                self.notice = Some("Workspace saved".into());
                self.close_workspace();
            }
            Err(error) => self.failed("Could not save workspace", error),
        }
    }

    fn activate_layout(&mut self, index: usize) {
        if index < self.workspace.layouts.len() {
            self.workspace.active = index;
            self.ws_name.text = self.workspace.current().name.clone();
        }
    }

    /// Start the Steam scan on a worker thread; [`poll`] merges the result.
    /// The API key never touches disk: it is cleared before the scan starts.
    fn begin_scan(&mut self) {
        let options = steam::ImportOptions {
            root: (!self.steam.root.text.trim().is_empty())
                .then(|| PathBuf::from(self.steam.root.text.trim())),
            account_id: (!self.steam.account.text.trim().is_empty())
                .then(|| self.steam.account.text.trim().to_owned()),
            api_key: (!self.steam.key.text.trim().is_empty())
                .then(|| self.steam.key.text.trim().to_owned()),
        };
        let (tx, rx) = mpsc::channel();
        self.steam.rx = Some(rx);
        self.steam.scanning = true;
        self.steam.error = None;
        self.steam.summary = None;
        self.steam.warnings.clear();
        self.steam.key.text.clear();
        std::thread::spawn(move || {
            let _ = tx.send(steam::import(options));
        });
    }

    /// Merge a finished scan: new games are added, known entries keep custom
    /// names and favorites, and a clean scan refreshes installed flags.
    fn finish_scan(&mut self, result: ScanOutcome) {
        self.steam.scanning = false;
        self.steam.rx = None;
        match result {
            Err(error) => {
                self.steam.error = Some(format!("{error:#}"));
            }
            Ok(imported) => {
                let warnings = imported.warnings.clone();
                let outcome = self.persist_import(&imported);
                match outcome {
                    Err(error) => self.steam.error = Some(format!("{error:#}")),
                    Ok(added) => {
                        let summary = format!(
                            "Imported {added} Steam games. Existing entries kept. {}",
                            warnings.join("\n")
                        );
                        self.steam.summary = Some(summary.clone());
                        self.steam.warnings = warnings;
                        self.notice = Some(summary);
                        self.reselect();
                        self.maybe_automatic();
                    }
                }
            }
        }
    }

    fn persist_import(&mut self, imported: &steam::ImportResult) -> Result<usize> {
        if self.path.as_os_str().is_empty() {
            let mut next = self.library.clone();
            let added = merge_import(&mut next, imported)?;
            self.library = next;
            Ok(added)
        } else {
            let path = self.path.clone();
            let mut added = 0;
            self.library.change(&path, |library| {
                added = merge_import(library, imported)?;
                Ok(())
            })?;
            Ok(added)
        }
    }

    fn open_meta(&mut self, index: Option<usize>) {
        let meta = &mut self.meta;
        meta.open = true;
        meta.matches.clear();
        meta.images.clear();
        meta.previews.clear();
        meta.replace = false;
        if !meta.busy {
            meta.status.clear();
        }
        let game = index.and_then(|i| self.library.games.get(i));
        meta.target = game.map(|game| game.target.clone());
        meta.query.text = game.map(|game| game.name.clone()).unwrap_or_default();
        meta.id_input.text = game
            .and_then(metadata::app_id)
            .map(|id| id.to_string())
            .unwrap_or_default();
        meta.query.focused = game.is_some();
    }

    fn save_meta_defaults(&mut self) {
        let path = self.meta.settings_path.clone();
        if path.as_os_str().is_empty() {
            self.meta.status = "Defaults live for this session; no library path is set.".into();
            return;
        }
        match self.meta.settings.save(&path) {
            Ok(()) => self.meta.status = "Metadata defaults saved".into(),
            Err(error) => self.meta.status = format!("Could not save defaults: {error:#}"),
        }
    }

    fn begin_meta(&mut self) -> Option<mpsc::Sender<MetaEvent>> {
        if self.meta.busy {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        self.meta.rx = Some(rx);
        self.meta.busy = true;
        self.meta.cancel = Arc::new(AtomicBool::new(false));
        self.meta.status.clear();
        self.meta.completed = 0;
        self.meta.failed = 0;
        self.meta.total = 0;
        Some(tx)
    }

    /// Fetch enabled fields for games with a known Steam identity, one at a
    /// time with a pause between requests. Rate-limit responses stop the queue.
    fn begin_fetch(&mut self, games: Vec<Game>, replace: bool) {
        let settings = self.meta.settings.clone();
        let games: Vec<_> = games
            .into_iter()
            .filter(|game| {
                metadata::app_id(game).is_some() && settings.for_game(game, replace).any_fields()
            })
            .collect();
        if games.is_empty() {
            self.meta.status =
                "No matching games need the enabled fields. Match an executable to a Steam title first."
                    .into();
            return;
        }
        let total = games.len();
        let Some(tx) = self.begin_meta() else {
            return;
        };
        self.meta.total = total;
        let cache = self.meta.cache.clone();
        let key = self.meta.key.text.clone();
        let cancel = self.meta.cancel.clone();
        std::thread::spawn(move || {
            let provider = metadata::Provider::new();
            for game in games {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = match &provider {
                    Ok(provider) => provider
                        .fetch(
                            metadata::app_id(&game).unwrap_or(0),
                            &settings.for_game(&game, replace),
                            &key,
                            &cache,
                        )
                        .map_err(|error| format!("{error:#}")),
                    Err(error) => Err(format!("{error:#}")),
                };
                let rate_limited = match &result {
                    Ok(fetched) => fetched.warnings.iter().any(|w| w.contains("rate limit")),
                    Err(error) => error.contains("rate limit"),
                };
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if tx
                    .send(MetaEvent::Fetched(Box::new(game), result, replace))
                    .is_err()
                {
                    return;
                }
                if rate_limited {
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            let _ = tx.send(MetaEvent::Done);
        });
    }

    fn begin_search(&mut self) {
        let query = self.meta.query.text.clone();
        let target = self.meta.target.clone();
        let Some(tx) = self.begin_meta() else {
            return;
        };
        std::thread::spawn(move || {
            let result = metadata::Provider::new()
                .and_then(|provider| provider.search(&query))
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(MetaEvent::Matches(target, query, result));
            let _ = tx.send(MetaEvent::Done);
        });
    }

    fn begin_images(&mut self, id: u32, kind: ImageKind) {
        let target = self.meta.target.clone();
        let provider = self.meta.settings.artwork_provider;
        let key = self.meta.key.text.clone();
        let cache = self.meta.cache.clone();
        let cancel = self.meta.cancel.clone();
        let Some(tx) = self.begin_meta() else {
            return;
        };
        std::thread::spawn(move || {
            let result = metadata::Provider::new()
                .and_then(|client| {
                    let choices = client.images(id, kind, provider, &key)?;
                    let mut previews = Vec::new();
                    for choice in choices {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        let path = client
                            .download(&choice, &cache)
                            .map_err(|error| format!("{error:#}"));
                        previews.push(ImagePreview {
                            label: choice.label,
                            source: choice.source,
                            path,
                        });
                    }
                    Ok(previews)
                })
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(MetaEvent::Images(target, kind, id, result));
            let _ = tx.send(MetaEvent::Done);
        });
    }

    fn stop_meta(&mut self) {
        self.meta.cancel.store(true, Ordering::Relaxed);
        self.meta.status =
            "Stopping when the in-flight game finishes. Its unsaved result will be discarded."
                .into();
    }

    /// Record a matched Steam identity on the selected game without touching
    /// its launch target, then offer an automatic fill.
    fn bind_app_id(&mut self, id: u32) {
        let Some(target) = self.meta.target.clone() else {
            return;
        };
        let path = self.path.clone();
        let apply = |library: &mut Library| -> Result<()> {
            let Some(game) = library.games.iter_mut().find(|game| game.target == target) else {
                bail!("That game is no longer in the library");
            };
            game.metadata.steam_app_id = Some(id);
            Ok(())
        };
        let done = if path.as_os_str().is_empty() {
            let mut next = self.library.clone();
            apply(&mut next).map(|()| self.library = next)
        } else {
            self.library.change(&path, apply)
        };
        match done {
            Ok(()) => {
                self.meta.images.clear();
                self.meta.matches.clear();
                self.meta.status =
                    format!("Matched to Steam {id}. Choose Fill missing metadata to download.");
                if self.meta.settings.automatic
                    && let Some(game) = self
                        .library
                        .games
                        .iter()
                        .find(|game| game.target == target)
                        .cloned()
                {
                    self.begin_fetch(vec![game], false);
                }
            }
            Err(error) => self.failed("Could not match the title", error),
        }
    }

    fn use_app_id_input(&mut self) {
        match self.meta.id_input.text.trim().parse::<u32>() {
            Ok(id) if id > 0 => self.bind_app_id(id),
            _ => self.meta.status = "Enter a positive numeric Steam app ID.".into(),
        }
    }

    /// Apply one downloaded artwork choice to the selected game and cache its
    /// pixels for painting.
    fn apply_choice(&mut self, index: usize) {
        let kind = self.meta.kind;
        let Some(preview) = self.meta.images.get(index) else {
            return;
        };
        let path = match &preview.path {
            Ok(path) => path.clone(),
            Err(_) => return,
        };
        let source = preview.source.clone();
        let Some(target) = self.meta.target.clone() else {
            return;
        };
        let path_clone = path.clone();
        let apply = |library: &mut Library| -> Result<()> {
            let Some(game) = library.games.iter_mut().find(|game| game.target == target) else {
                bail!("That game is no longer in the library");
            };
            match kind {
                ImageKind::Cover => game.cover = Some(path_clone.clone()),
                ImageKind::Background => game.metadata.background = Some(path_clone.clone()),
                ImageKind::Logo => game.metadata.logo = Some(path_clone.clone()),
            }
            game.metadata
                .sources
                .insert(kind.field().into(), source.clone());
            Ok(())
        };
        let path_field = self.path.clone();
        let done = if path_field.as_os_str().is_empty() {
            let mut next = self.library.clone();
            apply(&mut next).map(|()| self.library = next)
        } else {
            self.library.change(&path_field, apply)
        };
        match done {
            Ok(()) => {
                if let Some(pixels) = self.meta.previews.remove(&path) {
                    self.cache_cover(path, pixels);
                } else {
                    self.remember_cover(&path);
                }
                self.meta.status = "Artwork saved".into();
            }
            Err(error) => self.meta.status = format!("Could not save artwork: {error:#}"),
        }
    }

    /// Merge one finished fetch, preserving fields edited while it ran.
    fn finish_fetched(&mut self, baseline: Game, result: Result<Fetched, String>, replace: bool) {
        self.meta.completed += 1;
        match result {
            Err(error) => {
                self.meta.failed += 1;
                self.meta.status = format!("{}: {error}", baseline.name);
            }
            Ok(fetched) => {
                let warnings = fetched.warnings.join(" ");
                let cover = fetched.cover.clone();
                let done = self.persist_fetched(&baseline, fetched, replace);
                if let Err(error) = done {
                    self.meta.failed += 1;
                    self.meta.status = format!("Could not save metadata: {error:#}");
                } else {
                    if let Some(path) = cover
                        && path.is_file()
                    {
                        self.remember_cover(&path);
                    }
                    if !warnings.is_empty() {
                        self.meta.status = format!("{}: {warnings}", baseline.name);
                    }
                }
            }
        }
    }

    fn persist_fetched(&mut self, baseline: &Game, fetched: Fetched, replace: bool) -> Result<()> {
        if self.path.as_os_str().is_empty() {
            let mut next = self.library.clone();
            merge_fetched(&mut next, baseline, fetched, replace);
            self.library = next;
            Ok(())
        } else {
            let path = self.path.clone();
            self.library.change(&path, |library| {
                merge_fetched(library, baseline, fetched, replace);
                Ok(())
            })
        }
    }

    /// Fetch enabled fields for every visible game after an import or a save,
    /// when automatic downloads are on.
    fn maybe_automatic(&mut self) {
        if !self.meta.settings.automatic || self.meta.busy || self.meta.open {
            return;
        }
        let games: Vec<Game> = self
            .library
            .games
            .iter()
            .filter(|game| !game.hidden)
            .cloned()
            .collect();
        self.begin_fetch(games, false);
    }

    /// Late title matches only apply if the panel still shows their query.
    fn finish_matches(
        &mut self,
        target: Option<Target>,
        query: String,
        result: Result<Vec<metadata::Match>, String>,
    ) {
        if self.meta.target != target || self.meta.query.text != query {
            return;
        }
        match result {
            Ok(matches) => {
                self.meta.status = if matches.is_empty() {
                    "No title matches. Try a different search or enter a Steam app ID.".into()
                } else {
                    "Choose the correct title below. Launch settings stay unchanged.".into()
                };
                self.meta.matches = matches;
            }
            Err(error) => self.meta.status = error,
        }
    }

    /// Late artwork choices only apply if the panel still shows their game
    /// and the game still carries the requested identity.
    fn finish_images(
        &mut self,
        target: Option<Target>,
        kind: ImageKind,
        id: u32,
        result: Result<Vec<ImagePreview>, String>,
    ) {
        let current_id = target.as_ref().and_then(|target| {
            self.library
                .games
                .iter()
                .find(|game| &game.target == target)
                .and_then(metadata::app_id)
        });
        if self.meta.target != target || self.meta.kind != kind || current_id != Some(id) {
            return;
        }
        match result {
            Ok(images) => {
                // Decode thumbnails into the panel-scoped map, replacing any
                // previous browse. Applied choices graduate to the covers
                // cache; the rest die with the panel.
                self.meta.previews.clear();
                for preview in &images {
                    if let Ok(path) = &preview.path
                        && let Ok(pixels) = decode_cover(path)
                    {
                        self.meta.previews.insert(path.clone(), pixels);
                    }
                }
                self.meta.images = images;
                self.meta.status = "Choose an image to replace that artwork field.".into();
            }
            Err(error) => self.meta.status = error,
        }
    }
}

/// Merge one fetch result into the library, resolving by launch target plus
/// Steam identity so removals cannot misattach data.
fn merge_fetched(library: &mut Library, baseline: &Game, fetched: Fetched, replace: bool) {
    if let Some(game) = library.games.iter_mut().find(|game| {
        game.target == baseline.target && metadata::app_id(game) == metadata::app_id(baseline)
    }) {
        metadata::apply(game, baseline, fetched, replace);
    }
}

/// Validate an add/edit form into a launchable game.
fn build_game(draft: &Draft) -> Result<Game> {
    let path = std::fs::canonicalize(draft.path.text.trim())
        .with_context(|| format!("Cannot open executable: {}", draft.path.text.trim()))?;
    let game = Game {
        metadata: Default::default(),
        name: draft.name.text.trim().into(),
        target: Target::Executable {
            path,
            arguments: draft.args.text.lines().map(str::to_owned).collect(),
            working_directory: if draft.dir.text.trim().is_empty() {
                None
            } else {
                Some(
                    std::fs::canonicalize(draft.dir.text.trim()).with_context(|| {
                        format!("Cannot open working directory: {}", draft.dir.text.trim())
                    })?,
                )
            },
        },
        favorite: false,
        last_launched: None,
        cover: None,
        installed: None,
        hidden: false,
    };
    launch::plan(&game)?;
    Ok(game)
}

/// Fold a Steam scan into the library. Returns games added.
fn merge_import(library: &mut Library, imported: &steam::ImportResult) -> Result<usize> {
    let mut added = 0;
    if imported.warnings.is_empty() {
        for game in &mut library.games {
            if matches!(game.target, Target::Steam { .. }) {
                game.installed = Some(false);
            }
        }
    }
    for game in &imported.games {
        if let Some(existing) = library.games.iter_mut().find(|g| {
            g.target
                == (Target::Steam {
                    app_id: game.app_id,
                })
        }) {
            if game.installed || imported.warnings.is_empty() {
                existing.installed = Some(game.installed);
            }
            if existing.cover.is_none() {
                existing.cover = game.cover.clone();
            }
            continue;
        }
        library.add(Game {
            metadata: Default::default(),
            name: game.name.clone(),
            target: Target::Steam {
                app_id: game.app_id,
            },
            favorite: false,
            last_launched: None,
            cover: game.cover.clone(),
            installed: Some(game.installed),
            hidden: false,
        })?;
        added += 1;
    }
    Ok(added)
}

/// Decode a local PNG/JPEG cover into GPU-ready pixels, thumbnailing large
/// files. Mirrors the old texture limits: PNG/JPEG only, 16 MiB, 4096 px,
/// 64 MiB decode budget.
fn decode_cover(path: &Path) -> Result<Pixels> {
    use image::GenericImageView;
    let file = std::fs::File::open(path).context("Cover image is unavailable")?;
    if file.metadata()?.len() > 16 * 1024 * 1024 {
        bail!("Cover must be smaller than 16 MiB");
    }
    let mut reader =
        image::ImageReader::new(std::io::BufReader::new(file)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        bail!("Choose a PNG or JPEG cover");
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .context("Could not read cover; use PNG or JPEG, up to 4096 by 4096")?
        .thumbnail(480, 720);
    let (width, height) = image.dimensions();
    let pixels = Pixels::new();
    pixels.set(width, height, image.into_rgba8().into_raw());
    Ok(pixels)
}

impl Shell {
    fn play_current(&mut self) {
        let Some(index) = self.current() else {
            self.notice = Some("Select a game first".into());
            return;
        };
        let Some(game) = self.library.games.get(index).cloned() else {
            return;
        };
        match launch::launch(&game) {
            Ok(child) => {
                if let Some(child) = child {
                    self.children.push(child);
                }
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if self.path.as_os_str().is_empty() {
                    if let Some(entry) = self.library.games.get_mut(index) {
                        entry.last_launched = Some(now);
                    }
                } else {
                    let path = self.path.clone();
                    if let Err(error) = self.library.change(&path, |library| {
                        if let Some(entry) = library.games.get_mut(index) {
                            entry.last_launched = Some(now);
                        }
                        Ok(())
                    }) {
                        self.failed("Game started, but the timestamp did not save", error);
                        return;
                    }
                }
                self.notice = Some(format!("Launched {}", game.name));
            }
            Err(error) => self.failed(&format!("Could not launch {}", game.name), error),
        }
    }
}

fn favorite_note(favorite: bool, name: &str) -> String {
    if favorite {
        format!("Favorited {name}")
    } else {
        format!("Unfavorited {name}")
    }
}

fn visible_indices(library: &Library, filter: Filter, query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    library
        .games
        .iter()
        .enumerate()
        .filter(|(_, game)| filter.matches(game))
        .filter(|(_, game)| query.is_empty() || game.name.to_lowercase().contains(&query))
        .map(|(i, _)| i)
        .collect()
}

fn played_ago(now_secs: u64, then_secs: u64) -> String {
    let elapsed = now_secs.saturating_sub(then_secs);
    if elapsed < 90 {
        "Played just now".into()
    } else if elapsed < 3600 {
        format!("Played {}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("Played {}h ago", elapsed / 3600)
    } else {
        format!("Played {}d ago", elapsed / 86400)
    }
}

fn played_line(game: &Game) -> String {
    match game.last_launched {
        None => "Never played yet".into(),
        Some(then) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(then);
            played_ago(now, then)
        }
    }
}

fn initials(name: &str) -> String {
    let mut letters = name.split_whitespace().filter_map(|w| w.chars().next());
    match (letters.next(), letters.next()) {
        (Some(a), Some(b)) => format!("{}{}", a.to_uppercase(), b.to_uppercase()),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".into(),
    }
}

fn gradient(name: &str) -> (Color, Color) {
    let hash = name
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
    let ([r1, g1, b1], [r2, g2, b2]) = GRADIENTS[hash as usize % GRADIENTS.len()];
    let linear = |v: f32| srgb_to_linear(v);
    (
        Color {
            r: linear(r1),
            g: linear(g1),
            b: linear(b1),
            a: 1.0,
        },
        Color {
            r: linear(r2),
            g: linear(g2),
            b: linear(b2),
            a: 1.0,
        },
    )
}

pub fn spec() -> AppSpec<Shell> {
    AppSpec::<Shell> {
        poll: Some(poll),
        scenes: &[
            ("empty", |s: &mut Shell| *s = Shell::state()),
            ("library", |s: &mut Shell| *s = Shell::demo()),
            ("search", |s: &mut Shell| {
                *s = Shell::demo();
                s.query.text = "ember".into();
                s.reselect();
            }),
            ("favorites", |s: &mut Shell| {
                *s = Shell::demo();
                s.filter = Filter::Favorites;
                s.reselect();
            }),
            ("hidden", |s: &mut Shell| {
                *s = Shell::demo();
                s.library.games[0].hidden = true;
                s.filter = Filter::Hidden;
                s.reselect();
            }),
            ("add-game", |s: &mut Shell| {
                *s = Shell::demo();
                let mut draft = Draft::blank();
                draft.name.text = "Hollow Drift II".into();
                draft.path.text = "C:\\Games\\HollowDrift2\\drift2.exe".into();
                s.draft = Some(draft);
            }),
            ("edit-game", |s: &mut Shell| {
                *s = Shell::demo();
                s.selected = Some(4);
                let game = s.library.games[4].clone();
                s.draft = Some(Draft::for_game(&game, 4));
            }),
            ("remove", |s: &mut Shell| {
                *s = Shell::demo();
                s.selected = Some(1);
                s.remove = true;
            }),
            ("steam", |s: &mut Shell| {
                *s = Shell::demo();
                s.steam.opened();
                s.steam.summary = Some("Imported 2 Steam games. Existing entries kept. ".into());
                s.steam.warnings = vec![
                    "steamapps/libraryfolders.vdf: line 3 is malformed; that library was skipped"
                        .into(),
                ];
            }),
            ("steam-scanning", |s: &mut Shell| {
                *s = Shell::demo();
                s.steam.opened();
                let (_, rx) = mpsc::channel();
                s.steam.rx = Some(rx);
                s.steam.scanning = true;
            }),
            ("metadata", |s: &mut Shell| {
                *s = Shell::demo();
                s.selected = Some(2);
                s.open_meta(Some(2));
                s.meta.matches = vec![
                    metadata::Match {
                        app_id: 1001,
                        name: "Ember & Oak".into(),
                    },
                    metadata::Match {
                        app_id: 1002,
                        name: "Ember and Oak: Rekindled".into(),
                    },
                ];
                s.meta.status =
                    "Choose the correct title below. Launch settings stay unchanged.".into();
            }),
            ("artwork", |s: &mut Shell| {
                *s = Shell::demo();
                s.selected = Some(1);
                s.open_meta(Some(1));
                let cover = PathBuf::from("artwork/demo-cover.png");
                let thumb = Pixels::new();
                let mut rgba = Vec::with_capacity(48 * 72 * 4);
                for y in 0..72u32 {
                    for x in 0..48u32 {
                        let t = x as f32 / 48.0;
                        rgba.extend([
                            (200.0 - 80.0 * t) as u8,
                            (120.0 - 30.0 * (y as f32 / 72.0)) as u8,
                            (140.0 + 20.0 * t) as u8,
                            255,
                        ]);
                    }
                }
                thumb.set(48, 72, rgba);
                s.meta.previews.insert(cover.clone(), thumb);
                s.meta.images = vec![
                    ImagePreview {
                        label: "Official cover".into(),
                        source: "Steam".into(),
                        path: Ok(cover),
                    },
                    ImagePreview {
                        label: "Alternate art".into(),
                        source: "Steam".into(),
                        path: Err("Provider denied access".into()),
                    },
                ];
                s.meta.status = "Choose an image to replace that artwork field.".into();
            }),
            ("cover", |s: &mut Shell| {
                *s = Shell::demo();
                let cover = PathBuf::from("artwork/demo-cover.png");
                let art = Pixels::new();
                let (w, h) = (64u32, 96u32);
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let t = x as f32 / w as f32;
                        let v = y as f32 / h as f32;
                        rgba.extend([
                            (120.0 + 80.0 * t) as u8,
                            (160.0 - 40.0 * v) as u8,
                            (220.0 - 80.0 * t) as u8,
                            255,
                        ]);
                    }
                }
                art.set(w, h, rgba);
                s.covers.insert(cover.clone(), art);
                s.library.games[0].cover = Some(cover);
                s.selected = Some(0);
            }),
            ("workspace", |s: &mut Shell| {
                *s = Shell::demo();
                s.open_workspace();
            }),
            ("list-view", |s: &mut Shell| {
                *s = Shell::demo();
                s.workspace.current_mut().layout.view = View::List;
            }),
            ("shelf-view", |s: &mut Shell| {
                *s = Shell::demo();
                s.workspace.current_mut().layout.view = View::Shelf;
            }),
            ("game-modal", |s: &mut Shell| {
                *s = Shell::demo();
                s.workspace.current_mut().layout.details = Details::Modal;
                s.selected = Some(1);
                s.details_open = true;
            }),
            ("nav-right", |s: &mut Shell| {
                *s = Shell::demo();
                s.workspace.current_mut().layout.navigation = Navigation::Right;
            }),
        ],
        ..AppSpec::new("The Launcher", (1280.0, 800.0), Shell::state, view)
    }
}

/// Reap finished game processes and merge finished Steam/metadata work.
/// A running scan re-arms a short wake so its result lands without input.
fn poll(s: &mut Shell, _now: f64, tx: &mut Tx) -> bool {
    s.children
        .retain_mut(|child| child.try_wait().ok().flatten().is_none());
    let mut changed = false;
    if s.steam.scanning {
        match s.steam.rx.as_ref().map(mpsc::Receiver::try_recv) {
            Some(Ok(result)) => {
                s.finish_scan(result);
                changed = true;
            }
            Some(Err(mpsc::TryRecvError::Empty)) => {
                tx.wake_after(Duration::from_millis(200));
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                s.finish_scan(Err(anyhow::anyhow!("Steam scan stopped unexpectedly")));
                changed = true;
            }
            None => {}
        }
    }
    if s.meta.busy {
        loop {
            match s.meta.rx.as_ref().map(mpsc::Receiver::try_recv) {
                Some(Ok(MetaEvent::Fetched(baseline, result, replace))) => {
                    s.finish_fetched(*baseline, result, replace);
                    changed = true;
                }
                Some(Ok(MetaEvent::Matches(target, query, result))) => {
                    s.finish_matches(target, query, result);
                    changed = true;
                }
                Some(Ok(MetaEvent::Images(target, kind, id, result))) => {
                    s.finish_images(target, kind, id, result);
                    changed = true;
                }
                Some(Ok(MetaEvent::Done)) => {
                    s.meta.rx = None;
                    s.meta.busy = false;
                    if s.meta.total > 0 {
                        s.meta.status = format!(
                            "Processed {} of {} games ({} failed). {} {}",
                            s.meta.completed,
                            s.meta.total,
                            s.meta.failed,
                            if s.meta.cancel.load(Ordering::Relaxed) {
                                "Stopped."
                            } else {
                                ""
                            },
                            s.meta.status
                        );
                    }
                    changed = true;
                }
                Some(Err(mpsc::TryRecvError::Empty)) => break,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    s.meta.rx = None;
                    s.meta.busy = false;
                    s.meta.status = "Metadata worker stopped unexpectedly; try again.".into();
                    changed = true;
                    break;
                }
                None => break,
            }
        }
        if s.meta.busy {
            tx.wake_after(Duration::from_millis(200));
        }
    }
    changed
}

fn view(s: &Shell, cx: &mut Cx<Shell>) -> El {
    let look = Look::of(&s.workspace);
    cx.foreground(look.text);
    cx.focus_ring(2.0, 2.0, look.select);
    cx.caption_color(look.text);
    let chrome = cx.chrome_insets();
    let mut root = el().key("root").w_full().h_full().child(
        col()
            .key("window")
            .w_full()
            .h_full()
            .fill(look.bg)
            .child(toolbar(s, look, chrome.leading, chrome.trailing))
            .child(body(s, look))
            .child(footer(s, look)),
    );
    if s.workspace.current().layout.details == Details::Modal && s.details_open {
        root = root.child(game_modal(s, look));
    }
    if s.draft.is_some() {
        root = root.child(game_dialog(s, look));
    }
    if s.remove {
        root = root.child(remove_dialog(s, look));
    }
    if s.steam.open {
        root = root.child(steam_dialog(s, look));
    }
    if s.meta.open {
        root = root.child(meta_dialog(s, look));
    }
    if s.workspace_open {
        root = root.child(workspace_dialog(s, look));
    }
    root
}

/// Full-window dim layer behind a dialog; the panel itself is modal.
fn scrim() -> El {
    el().key("scrim")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .fill(Color::BLACK.alpha(0.62))
        .a11y_hidden()
}

/// Centered dialog panel. `outside` decides whether a click outside dismisses;
/// Escape always dismisses through `dismiss`.
fn panel(
    look: Look,
    key: &str,
    title: &str,
    outside: bool,
    dismiss: impl Fn(&mut Shell) + 'static,
    body: El,
) -> El {
    let sheet = col()
        .key(format!("{key}-panel"))
        .role("dialog")
        .label(title)
        .w(560.0)
        .radius(16.0)
        .continuous_corners()
        .fill(look.surface)
        .border_gradient(1.0, look.rim_top(), look.line)
        .shadow(0.0, 32.0, Color::BLACK.alpha(0.45))
        .padding(24.0)
        .gap(12.0)
        .modal(outside, dismiss)
        .child(text(title).font(type_::TITLE).color(look.text));
    el().key(format!("{key}-layer"))
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .child(sheet.child(body))
}

fn dialog_button(
    key: &str,
    label_text: &str,
    primary: bool,
    look: Look,
    press: impl Fn(&mut Shell) + 'static,
) -> El {
    row()
        .key(key)
        .role("button")
        .label(label_text)
        .focusable(true)
        .h(40.0)
        .px(18.0)
        .radius(radius::MD)
        .continuous_corners()
        .center()
        .fill(if primary { look.accent } else { look.raised })
        .hover(if primary {
            look.accent.alpha(0.85)
        } else {
            look.hover()
        })
        .press(look.pressed())
        .press_scale(0.97)
        .focus_ring(2.0, 1.0, look.select)
        .shadow(
            0.0,
            if primary { 12.0 } else { 0.0 },
            if primary {
                look.accent.alpha(0.3)
            } else {
                Color::TRANSPARENT
            },
        )
        .on_press(press)
        .child(
            text(label_text.to_owned())
                .font(type_::LABEL)
                .color(if primary { look.ink } else { look.text }),
        )
}

/// A labeled single-line field bound to one of the draft's [`TextEdit`]s.
fn form_field(
    s: &Shell,
    look: Look,
    label_text: &str,
    key: &str,
    get: impl Fn(&Draft) -> &TextEdit + 'static,
    access: impl for<'a> Fn(&'a mut Shell) -> &'a mut TextEdit + 'static,
    placeholder: &str,
) -> El {
    let draft = s.draft.as_ref().expect("game form closed");
    let input = text_input(get(draft), type_::BODY.lh(20.0))
        .key(key)
        .label(label_text)
        .single_line()
        .w_full()
        .color(look.text)
        .placeholder(placeholder, look.muted)
        .on_edit(move |s: &mut Shell| access(s));
    col()
        .key(format!("{key}-field"))
        .w_full()
        .gap(6.0)
        .child(text(label_text).font(type_::LABEL).color(look.muted))
        .child(
            el().key(format!("{key}-box"))
                .w_full()
                .h(40.0)
                .radius(radius::MD)
                .border(1.0, look.line)
                .px(12.0)
                .center_y()
                .child(input),
        )
}

fn game_dialog(s: &Shell, look: Look) -> El {
    let editing = s.draft.as_ref().and_then(|draft| draft.editing);
    let title = if editing.is_some() {
        "Launch options"
    } else {
        "Add a PC game"
    };
    let mut form = col()
        .key("dialog-form")
        .w_full()
        .gap(12.0)
        .child(
            text("A name, an executable, and you are ready to play.")
                .font(type_::LABEL)
                .color(look.muted),
        )
        .child(form_field(
            s,
            look,
            "Name",
            "draft-name",
            |draft| &draft.name,
            |s| &mut s.draft.as_mut().expect("game form closed").name,
            "What are we playing?",
        ))
        .child(
            row()
                .key("draft-path-row")
                .w_full()
                .gap(8.0)
                .child(el().key("draft-path-grow").grow(1.0).child(form_field(
                    s,
                    look,
                    "Executable",
                    "draft-path",
                    |draft| &draft.path,
                    |s| &mut s.draft.as_mut().expect("game form closed").path,
                    "Choose a game executable",
                )))
                .child(
                    row()
                        .key("draft-browse")
                        .role("button")
                        .label("Browse for an executable")
                        .focusable(true)
                        .h(40.0)
                        .px(14.0)
                        .radius(radius::MD)
                        .center_y()
                        .fill(look.raised)
                        .hover(look.hover())
                        .on_press_tx(|_: &mut Shell, tx: &mut Tx| {
                            let mut options = FileDialogOptions {
                                title: "Choose a game executable".into(),
                                ..Default::default()
                            };
                            if cfg!(windows) {
                                options = options.filter("Windows executable", &["exe"]);
                            }
                            tx.open_file(options, |s: &mut Shell, result, _| {
                                if let Ok(Some(path)) = result
                                    && let Some(draft) = s.draft.as_mut()
                                {
                                    if draft.name.text.trim().is_empty() {
                                        draft.name.text = path
                                            .file_stem()
                                            .map(|stem| stem.to_string_lossy().into_owned())
                                            .unwrap_or_default();
                                    }
                                    draft.path.text = path.display().to_string();
                                }
                            });
                        })
                        .child(text("Browse…").font(type_::LABEL).color(look.text)),
                ),
        );
    let args = text_input(
        &s.draft.as_ref().expect("game form closed").args,
        type_::BODY.lh(20.0),
    )
    .key("draft-args")
    .label("Arguments, one per line")
    .w_full()
    .h(64.0)
    .color(look.text)
    .placeholder("One argument per line; no shell quotes", look.muted)
    .on_edit(|s: &mut Shell| &mut s.draft.as_mut().expect("game form closed").args);
    form = form
        .child(
            col()
                .key("draft-args-field")
                .w_full()
                .gap(6.0)
                .child(
                    text("Arguments — one per line; do not add shell quotes")
                        .font(type_::LABEL)
                        .color(look.muted),
                )
                .child(
                    el().key("draft-args-box")
                        .w_full()
                        .h(76.0)
                        .radius(radius::MD)
                        .border(1.0, look.line)
                        .px(12.0)
                        .py(6.0)
                        .child(args),
                ),
        )
        .child(form_field(
            s,
            look,
            "Working directory — blank uses the executable's folder",
            "draft-dir",
            |draft| &draft.dir,
            |s| &mut s.draft.as_mut().expect("game form closed").dir,
            "Use the game's folder",
        ));
    if let Some(message) = s.draft.as_ref().and_then(|draft| draft.error.clone()) {
        form = form.child(
            text(message)
                .key("dialog-error")
                .font(type_::LABEL)
                .color(look.err),
        );
    }
    form = form.child(
        row()
            .key("dialog-buttons")
            .w_full()
            .gap(12.0)
            .child(dialog_button(
                "dialog-save",
                "Save game",
                true,
                look,
                |s: &mut Shell| {
                    s.commit_draft();
                },
            ))
            .child(dialog_button(
                "dialog-cancel",
                "Cancel",
                false,
                look,
                |s: &mut Shell| {
                    s.draft = None;
                },
            )),
    );
    el().key("game-dialog")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(scrim())
        .child(panel(
            look,
            "game-dialog",
            title,
            false,
            |s: &mut Shell| {
                s.draft = None;
            },
            form,
        ))
}

fn remove_dialog(s: &Shell, look: Look) -> El {
    let Some(index) = s.current() else {
        return el().key("remove-dialog");
    };
    let hiding = s
        .library
        .games
        .get(index)
        .is_some_and(|game| !game.hidden && matches!(game.target, Target::Steam { .. }));
    let (title, body, confirm) = if hiding {
        (
            "Hide this game?",
            "It moves to Hidden. You can restore it anytime from the Hidden collection.",
            "Hide",
        )
    } else {
        (
            "Remove this game?",
            "Only the library entry is removed. Your game files stay in place.",
            "Remove",
        )
    };
    let form = col()
        .key("remove-form")
        .w_full()
        .gap(12.0)
        .child(text(body).font(type_::BODY).color(look.muted))
        .child(
            row()
                .key("dialog-buttons")
                .w_full()
                .gap(12.0)
                .child(dialog_button(
                    "remove-confirm",
                    confirm,
                    true,
                    look,
                    |s: &mut Shell| {
                        s.confirm_remove();
                    },
                ))
                .child(dialog_button(
                    "remove-cancel",
                    "Cancel",
                    false,
                    look,
                    |s: &mut Shell| {
                        s.remove = false;
                    },
                )),
        );
    el().key("remove-dialog")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(scrim())
        .child(panel(
            look,
            "remove-dialog",
            title,
            true,
            |s: &mut Shell| {
                s.remove = false;
            },
            form,
        ))
}

fn steam_dialog(s: &Shell, look: Look) -> El {
    let mut form = col().key("steam-form").w_full().gap(12.0).child(
        text("Scan your Steam libraries and merge installed games. Account details are optional, used once, and never stored.")
            .font(type_::LABEL)
            .color(look.muted),
    );
    form = steam_fields(s, look, form);
    if s.steam.scanning {
        form = form.child(
            text("Scanning Steam libraries…")
                .key("steam-status")
                .font(type_::BODY)
                .color(look.text),
        );
    }
    if let Some(error) = &s.steam.error {
        form = form.child(
            text(error.clone())
                .key("steam-error")
                .font(type_::LABEL)
                .color(look.err),
        );
    }
    if let Some(summary) = &s.steam.summary {
        form = form.child(
            text(summary.clone())
                .key("steam-summary")
                .font(type_::LABEL)
                .color(look.select),
        );
        for (i, warning) in s.steam.warnings.iter().take(5).enumerate() {
            form = form.child(
                text(format!("• {warning}"))
                    .key(format!("steam-warning-{i}"))
                    .font(type_::LABEL)
                    .color(look.muted),
            );
        }
        if s.steam.warnings.len() > 5 {
            form = form.child(
                text(format!("…and {} more", s.steam.warnings.len() - 5))
                    .font(type_::LABEL)
                    .color(look.muted),
            );
        }
    }
    let mut buttons = row().key("dialog-buttons").w_full().gap(12.0);
    if !s.steam.scanning {
        buttons = buttons.child(dialog_button(
            "steam-scan",
            "Scan",
            true,
            look,
            |s: &mut Shell| {
                s.begin_scan();
            },
        ));
    }
    buttons = buttons.child(dialog_button(
        "steam-close",
        "Close",
        false,
        look,
        |s: &mut Shell| {
            s.steam.open = false;
        },
    ));
    form = form.child(buttons);
    el().key("steam-dialog")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(scrim())
        .child(panel(
            look,
            "steam-dialog",
            "Import Steam",
            true,
            |s: &mut Shell| {
                s.steam.open = false;
            },
            form,
        ))
}

/// Steam panel fields. Separate from [`form_field`] because the panel edits
/// [`SteamPanel`], not the game [`Draft`].
fn steam_fields(s: &Shell, look: Look, mut form: El) -> El {
    let root = text_input(&s.steam.root, type_::BODY.lh(20.0))
        .key("steam-root")
        .label("Steam folder")
        .single_line()
        .w_full()
        .color(look.text)
        .placeholder("Detect automatically", look.muted)
        .on_edit(|s: &mut Shell| &mut s.steam.root);
    let account = text_input(&s.steam.account, type_::BODY.lh(20.0))
        .key("steam-account")
        .label("Steam account ID (SteamID64), optional")
        .single_line()
        .w_full()
        .color(look.text)
        .placeholder("Optional: include owned, uninstalled games", look.muted)
        .on_edit(|s: &mut Shell| &mut s.steam.account);
    let key = text_input(&s.steam.key, type_::BODY.lh(20.0))
        .key("steam-key")
        .label("Steam Web API key, optional and never stored")
        .single_line()
        .w_full()
        .color(look.text)
        .placeholder("Optional, used once for this scan", look.muted)
        .on_edit(|s: &mut Shell| &mut s.steam.key);
    let browse = row()
        .key("steam-browse")
        .role("button")
        .label("Browse for the Steam folder")
        .focusable(true)
        .h(40.0)
        .px(14.0)
        .radius(radius::MD)
        .center_y()
        .fill(look.raised)
        .hover(look.hover())
        .on_press_tx(|_: &mut Shell, tx: &mut Tx| {
            tx.open_folder(
                FileDialogOptions {
                    title: "Choose the Steam folder".into(),
                    ..Default::default()
                },
                |s: &mut Shell, result, _| {
                    if let Ok(Some(path)) = result {
                        s.steam.root.text = path.display().to_string();
                    }
                },
            );
        })
        .child(text("Browse…").font(type_::LABEL).color(look.text));
    form = form
        .child(
            col()
                .key("steam-root-field")
                .w_full()
                .gap(6.0)
                .child(
                    text("Steam folder — blank uses automatic detection")
                        .font(type_::LABEL)
                        .color(look.muted),
                )
                .child(
                    row()
                        .key("steam-root-row")
                        .w_full()
                        .gap(8.0)
                        .child(
                            el().key("steam-root-grow").grow(1.0).child(
                                el().key("steam-root-box")
                                    .w_full()
                                    .h(40.0)
                                    .radius(radius::MD)
                                    .border(1.0, look.line)
                                    .px(12.0)
                                    .center_y()
                                    .child(root),
                            ),
                        )
                        .child(browse),
                ),
        )
        .child(
            col()
                .key("steam-account-field")
                .w_full()
                .gap(6.0)
                .child(
                    text("Steam account ID (SteamID64) — optional")
                        .font(type_::LABEL)
                        .color(look.muted),
                )
                .child(
                    el().key("steam-account-box")
                        .w_full()
                        .h(40.0)
                        .radius(radius::MD)
                        .border(1.0, look.line)
                        .px(12.0)
                        .center_y()
                        .child(account),
                ),
        )
        .child(
            col()
                .key("steam-key-field")
                .w_full()
                .gap(6.0)
                .child(
                    text("Steam Web API key — optional, used once, never stored")
                        .font(type_::LABEL)
                        .color(look.muted),
                )
                .child(
                    el().key("steam-key-box")
                        .w_full()
                        .h(40.0)
                        .radius(radius::MD)
                        .border(1.0, look.line)
                        .px(12.0)
                        .center_y()
                        .child(key),
                ),
        );
    form
}

/// Labeled on/off switch row.
fn toggle(
    look: Look,
    key: &str,
    label_text: &str,
    on: bool,
    flip: impl Fn(&mut Shell) + 'static,
) -> El {
    let knob_x = if on { 18.0 } else { 2.0 };
    row()
        .key(key)
        .role("switch")
        .label(label_text)
        .checked(on)
        .focusable(true)
        .w_full()
        .h(36.0)
        .px(4.0)
        .gap(10.0)
        .center_y()
        .on_press(flip)
        .child(
            text(label_text.to_owned())
                .font(type_::BODY)
                .color(look.text)
                .grow(1.0),
        )
        .child(
            el().key("switch")
                .size(38.0, 22.0)
                .radius(11.0)
                .fill(if on { look.accent } else { look.raised })
                .motion(motion::SNAPPY)
                .child(
                    el().key("knob")
                        .size(18.0, 18.0)
                        .radius(9.0)
                        .fill(look.ink)
                        .absolute(knob_x, 2.0),
                ),
        )
}

/// Segmented option row. `options` are (id, label); `selected` is the active id.
fn segmented(
    look: Look,
    key: &str,
    selected: &str,
    options: &[(String, String)],
    pick: impl Fn(&mut Shell, String) + Clone + 'static,
) -> El {
    let mut items = row().key(key.to_owned()).w_full().gap(8.0);
    for (id, label) in options {
        let active = id == selected;
        let pick = pick.clone();
        let id = id.clone();
        items = items.child(
            row()
                .key(format!("{key}-{id}"))
                .role("button")
                .label(label.clone())
                .focusable(true)
                .h(32.0)
                .px(12.0)
                .radius(radius::MD)
                .center_y()
                .fill(if active {
                    look.accent.alpha(0.2)
                } else {
                    look.raised
                })
                .border(1.0, if active { look.accent } else { look.line })
                .hover(look.hover())
                .on_press(move |s: &mut Shell| pick(s, id.clone()))
                .child(text(label.clone()).font(type_::LABEL).color(if active {
                    look.accent
                } else {
                    look.text
                })),
        );
    }
    items
}

fn section_title(look: Look, label_text: &str) -> El {
    text(label_text.to_owned())
        .font(type_::LABEL)
        .color(look.muted)
}

fn meta_dialog(s: &Shell, look: Look) -> El {
    let game = s
        .meta
        .target
        .as_ref()
        .and_then(|target| s.library.games.iter().find(|game| &game.target == target))
        .cloned();
    let title = if game.is_some() {
        "Metadata & artwork"
    } else {
        "Metadata sources"
    };
    let mut form = col().key("meta-form").w_full().gap(10.0);
    if s.meta.busy {
        let progress = if s.meta.total > 0 {
            format!(
                "Working… {} / {} games, {} failed",
                s.meta.completed, s.meta.total, s.meta.failed
            )
        } else {
            "Loading provider data and previews…".into()
        };
        form = form.child(
            row()
                .key("meta-progress")
                .w_full()
                .gap(12.0)
                .center_y()
                .child(text(progress).font(type_::BODY).color(look.text))
                .child(dialog_button(
                    "meta-stop",
                    "Stop downloads",
                    false,
                    look,
                    |s: &mut Shell| s.stop_meta(),
                )),
        );
    }
    if !s.meta.status.is_empty() {
        form = form.child(
            text(s.meta.status.clone())
                .key("meta-status")
                .font(type_::LABEL)
                .color(look.select),
        );
    }
    form = form.child(section_title(look, "DOWNLOAD DEFAULTS & PROVIDERS"));
    let settings = s.meta.settings.clone();
    form = form.child(toggle(
        look,
        "meta-automatic",
        "Automatically fill missing fields after import or matching",
        settings.automatic,
        |s: &mut Shell| {
            s.meta.settings.automatic = !s.meta.settings.automatic;
        },
    ));
    for (field, label, on) in [
        (
            "text",
            "Description & details (Steam)",
            settings.download_text,
        ),
        ("cover", "Cover", settings.download_cover),
        ("background", "Background", settings.download_background),
        ("logo", "Logo", settings.download_logo),
    ] {
        form = form.child(toggle(
            look,
            &format!("meta-{field}"),
            label,
            on,
            move |s: &mut Shell| {
                let settings = &mut s.meta.settings;
                match field {
                    "text" => settings.download_text = !settings.download_text,
                    "cover" => settings.download_cover = !settings.download_cover,
                    "background" => settings.download_background = !settings.download_background,
                    _ => settings.download_logo = !settings.download_logo,
                }
            },
        ));
    }
    let provider = match settings.artwork_provider {
        ArtworkProvider::Steam => "steam",
        ArtworkProvider::SteamGridDb => "grid",
    };
    form = form.child(segmented(
        look,
        "meta-provider",
        provider,
        &[
            ("steam".into(), "Steam official".into()),
            ("grid".into(), "SteamGridDB first, Steam fallback".into()),
        ],
        |s: &mut Shell, id| {
            s.meta.settings.artwork_provider = if id == "grid" {
                ArtworkProvider::SteamGridDb
            } else {
                ArtworkProvider::Steam
            };
        },
    ));
    if settings.artwork_provider == ArtworkProvider::SteamGridDb {
        form = form.child(meta_key_field(s, look));
    }
    form = form.child(
        text("Automatic downloads use your preferred artwork source, then Steam as fallback. Keys stay in memory for this app session.")
            .font(type_::LABEL)
            .color(look.muted),
    );
    form = form.child(dialog_button(
        "meta-save",
        "Save defaults",
        false,
        look,
        |s: &mut Shell| s.save_meta_defaults(),
    ));
    if let Some(game) = game {
        form = meta_game_section(s, look, &game, form);
    } else {
        form = form.child(section_title(look, "LIBRARY"));
        form = form.child(
            text("Fetch enabled fields for Steam games and executables you have matched. Existing fields are preserved.")
                .font(type_::LABEL)
                .color(look.muted),
        );
        form = form.child(dialog_button(
            "meta-fill-all",
            "Fill missing metadata for library",
            true,
            look,
            |s: &mut Shell| {
                let games: Vec<Game> = s
                    .library
                    .games
                    .iter()
                    .filter(|game| !game.hidden)
                    .cloned()
                    .collect();
                s.begin_fetch(games, false);
            },
        ));
    }
    let body = col()
        .key("meta-scroll")
        .w_full()
        .max_h(560.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(form);
    let mut buttons = row().key("dialog-buttons").w_full().gap(12.0);
    buttons = buttons.child(dialog_button(
        "meta-close",
        "Close",
        false,
        look,
        |s: &mut Shell| {
            s.meta.open = false;
        },
    ));
    let form = col()
        .key("meta-panel-form")
        .w_full()
        .gap(12.0)
        .child(body)
        .child(buttons);
    el().key("meta-dialog")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(scrim())
        .child(panel(
            look,
            "meta-dialog",
            title,
            true,
            |s: &mut Shell| {
                s.meta.open = false;
            },
            form,
        ))
}

/// Session API key field with a Forget button.
fn meta_key_field(s: &Shell, look: Look) -> El {
    let input = text_input(&s.meta.key, type_::BODY.lh(20.0))
        .key("meta-key")
        .label("SteamGridDB API key, optional and never stored")
        .single_line()
        .secure()
        .w_full()
        .color(look.text)
        .placeholder("Optional, used once for this session", look.muted)
        .on_edit(|s: &mut Shell| &mut s.meta.key);
    col()
        .key("meta-key-field")
        .w_full()
        .gap(6.0)
        .child(section_title(
            look,
            "SteamGridDB key — optional, never stored",
        ))
        .child(
            row()
                .key("meta-key-row")
                .w_full()
                .gap(8.0)
                .child(
                    el().key("meta-key-grow").grow(1.0).child(
                        el().key("meta-key-box")
                            .w_full()
                            .h(40.0)
                            .radius(radius::MD)
                            .border(1.0, look.line)
                            .px(12.0)
                            .center_y()
                            .child(input),
                    ),
                )
                .child(dialog_button(
                    "meta-forget",
                    "Forget",
                    false,
                    look,
                    |s: &mut Shell| {
                        s.meta.key.text.clear();
                    },
                )),
        )
}

/// Per-game matching, fetching, artwork choice, and saved details.
fn meta_game_section(s: &Shell, look: Look, game: &Game, mut form: El) -> El {
    let app_id = metadata::app_id(game);
    form = form.child(section_title(look, "MATCH TITLE"));
    if matches!(game.target, Target::Executable { .. }) {
        let query = text_input(&s.meta.query, type_::BODY.lh(20.0))
            .key("meta-query")
            .label("Match this executable to a Steam title")
            .single_line()
            .w_full()
            .color(look.text)
            .placeholder("Search Steam titles", look.muted)
            .on_edit(|s: &mut Shell| &mut s.meta.query);
        form = form.child(
            row()
                .key("meta-query-row")
                .w_full()
                .gap(8.0)
                .child(
                    el().key("meta-query-grow").grow(1.0).child(
                        el().key("meta-query-box")
                            .w_full()
                            .h(40.0)
                            .radius(radius::MD)
                            .border(1.0, look.line)
                            .px(12.0)
                            .center_y()
                            .child(query),
                    ),
                )
                .child(dialog_button(
                    "meta-search",
                    "Search titles",
                    false,
                    look,
                    |s: &mut Shell| s.begin_search(),
                )),
        );
        for candidate in &s.meta.matches {
            let id = candidate.app_id;
            let label = format!("{} — Steam {id}", candidate.name);
            let mine = label.clone();
            form = form.child(
                row()
                    .key(format!("meta-match-{id}"))
                    .role("button")
                    .label(mine.clone())
                    .focusable(true)
                    .w_full()
                    .h(36.0)
                    .px(12.0)
                    .radius(radius::MD)
                    .center_y()
                    .fill(look.raised)
                    .hover(look.hover())
                    .on_press(move |s: &mut Shell| s.bind_app_id(id))
                    .child(text(mine).font(type_::BODY).color(look.text)),
            );
        }
        let id_input = text_input(&s.meta.id_input, type_::BODY.lh(20.0))
            .key("meta-id")
            .label("Steam app ID")
            .single_line()
            .w(120.0)
            .color(look.text)
            .placeholder("ID", look.muted)
            .on_edit(|s: &mut Shell| &mut s.meta.id_input);
        form = form.child(
            row()
                .key("meta-id-row")
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    el().key("meta-id-box")
                        .w(144.0)
                        .h(40.0)
                        .radius(radius::MD)
                        .border(1.0, look.line)
                        .px(12.0)
                        .center_y()
                        .child(id_input),
                )
                .child(dialog_button(
                    "meta-use-id",
                    "Use this ID",
                    false,
                    look,
                    |s: &mut Shell| s.use_app_id_input(),
                )),
        );
        form = form.child(
            text("Matching only selects a metadata source. Your executable remains the launch target.")
                .font(type_::LABEL)
                .color(look.muted),
        );
    }
    if let Some(id) = app_id {
        form = form.child(
            text(format!("Metadata identity: Steam {id}"))
                .key("meta-identity")
                .font(type_::BODY)
                .color(look.text),
        );
        form = form.child(toggle(
            look,
            "meta-replace",
            "Replace existing enabled fields",
            s.meta.replace,
            |s: &mut Shell| {
                s.meta.replace = !s.meta.replace;
            },
        ));
        form = form.child(dialog_button(
            "meta-fill",
            if s.meta.replace {
                "Refresh enabled metadata & images"
            } else {
                "Fill missing metadata & images"
            },
            true,
            look,
            |s: &mut Shell| {
                let replace = s.meta.replace;
                let games = s
                    .meta
                    .target
                    .as_ref()
                    .and_then(|target| {
                        s.library
                            .games
                            .iter()
                            .find(|game| &game.target == target)
                            .cloned()
                    })
                    .map(|game| vec![game])
                    .unwrap_or_default();
                s.begin_fetch(games, replace);
            },
        ));
        form = form.child(section_title(look, "CHOOSE ARTWORK"));
        let kind = match s.meta.kind {
            ImageKind::Cover => "cover",
            ImageKind::Background => "background",
            ImageKind::Logo => "logo",
        };
        form = form.child(segmented(
            look,
            "meta-kind",
            kind,
            &[
                ("cover".into(), "Cover".into()),
                ("background".into(), "Background".into()),
                ("logo".into(), "Logo".into()),
            ],
            |s: &mut Shell, id| {
                s.meta.kind = match id.as_str() {
                    "background" => ImageKind::Background,
                    "logo" => ImageKind::Logo,
                    _ => ImageKind::Cover,
                };
                s.meta.images.clear();
            },
        ));
        form = form.child(dialog_button(
            "meta-browse",
            "Browse image options",
            false,
            look,
            |s: &mut Shell| {
                if let (Some(target), Some(id)) = (
                    s.meta.target.clone(),
                    s.meta
                        .target
                        .as_ref()
                        .and_then(|target| {
                            s.library.games.iter().find(|game| &game.target == target)
                        })
                        .and_then(metadata::app_id),
                ) {
                    let _ = target;
                    s.begin_images(id, s.meta.kind);
                }
            },
        ));
        for (i, preview) in s.meta.images.iter().enumerate() {
            form = form.child(meta_choice(s, look, i, preview));
        }
        form = form.child(section_title(look, "SAVED INFORMATION"));
        form = form.child(meta_saved(look, game));
    }
    form
}

/// One downloaded artwork choice with a thumbnail.
fn meta_choice(s: &Shell, look: Look, index: usize, preview: &ImagePreview) -> El {
    let thumb = match &preview.path {
        Ok(path) => match s.meta.previews.get(path).or_else(|| s.covers.get(path)) {
            Some(pixels) => el()
                .key("choice-image")
                .w(72.0)
                .h(96.0)
                .radius(8.0)
                .clip_rounded(8.0)
                .child(
                    el().key("choice-pixels")
                        .w_full()
                        .h_full()
                        .pixels_fit(pixels, SurfaceFit::Fill),
                ),
            None => el()
                .key("choice-image")
                .w(72.0)
                .h(96.0)
                .radius(8.0)
                .fill(look.raised),
        },
        Err(_) => el()
            .key("choice-image")
            .w(72.0)
            .h(96.0)
            .radius(8.0)
            .fill(look.raised),
    };
    let status = match &preview.path {
        Ok(_) => preview.source.clone(),
        Err(error) => error.clone(),
    };
    let mut item = row()
        .key(format!("meta-choice-{index}"))
        .w_full()
        .gap(12.0)
        .center_y()
        .child(thumb)
        .child(
            col()
                .key("choice-meta")
                .grow(1.0)
                .gap(2.0)
                .child(
                    text(preview.label.clone())
                        .font(type_::BODY)
                        .color(look.text),
                )
                .child(text(status).font(type_::LABEL).color(look.muted)),
        );
    if preview.path.is_ok() {
        item = item.child(dialog_button(
            &format!("meta-use-{index}"),
            "Use image",
            false,
            look,
            move |s: &mut Shell| s.apply_choice(index),
        ));
    }
    item
}

/// Saved metadata text for the selected game.
fn meta_saved(look: Look, game: &Game) -> El {
    let m = &game.metadata;
    let mut details = col().key("meta-saved").w_full().gap(4.0);
    if let Some(background) = &m.background {
        details = details.child(
            text(format!("Background: {}", background.display()))
                .font(type_::LABEL)
                .color(look.muted),
        );
    }
    if let Some(logo) = &m.logo {
        details = details.child(
            text(format!("Logo: {}", logo.display()))
                .font(type_::LABEL)
                .color(look.muted),
        );
    }
    if !m.description.is_empty() {
        details = details.child(
            text(m.description.clone())
                .font(type_::BODY)
                .color(look.text),
        );
    }
    for (label, value) in [
        ("Released", m.release_date.clone()),
        ("Developer", m.developers.join(", ")),
        ("Publisher", m.publishers.join(", ")),
        ("Genres", m.genres.join(", ")),
    ] {
        if !value.is_empty() {
            details = details.child(
                text(format!("{label}: {value}"))
                    .font(type_::LABEL)
                    .color(look.muted),
            );
        }
    }
    if !m.sources.is_empty() {
        for (field, source) in &m.sources {
            details = details.child(
                text(format!("{field}: {source}"))
                    .font(type_::LABEL)
                    .color(look.muted),
            );
        }
    }
    if m.description.is_empty() && m.sources.is_empty() {
        details = details.child(
            text("No downloaded metadata yet.")
                .font(type_::LABEL)
                .color(look.muted),
        );
    }
    details
}

fn workspace_dialog(s: &Shell, look: Look) -> El {
    let active = s.workspace.active;
    let mut form = col().key("workspace-form").w_full().gap(10.0).child(
        text("Arrange panels, pick a view, and make the colors yours.")
            .font(type_::LABEL)
            .color(look.muted),
    );
    form = form.child(section_title(look, "SAVED LAYOUT"));
    let mut layouts = col().key("workspace-layouts").w_full().gap(4.0);
    for (i, preset) in s.workspace.layouts.iter().enumerate() {
        let current = i == active;
        layouts = layouts.child(
            row()
                .key(format!("workspace-layout-{i}"))
                .role("button")
                .label(format!("Use {}", preset.name))
                .focusable(true)
                .w_full()
                .h(36.0)
                .px(12.0)
                .gap(10.0)
                .center_y()
                .radius(radius::MD)
                .fill(if current {
                    look.surface
                } else {
                    Color::TRANSPARENT
                })
                .hover(look.hover())
                .on_press(move |s: &mut Shell| s.activate_layout(i))
                .child(
                    el().key("ws-dot")
                        .size(8.0, 8.0)
                        .radius(4.0)
                        .fill(if current { look.accent } else { look.raised }),
                )
                .child(
                    text(preset.name.clone())
                        .font(type_::BODY)
                        .color(if current { look.text } else { look.muted })
                        .grow(1.0),
                ),
        );
    }
    form = form.child(layouts);
    let name_input = text_input(&s.ws_name, type_::BODY.lh(20.0))
        .key("workspace-name")
        .label("Layout name")
        .single_line()
        .w_full()
        .color(look.text)
        .placeholder("Layout name", look.muted)
        .on_edit(|s: &mut Shell| &mut s.ws_name)
        .on_input_change(|s: &mut Shell, _| {
            let name = s.ws_name.text.clone();
            s.workspace.current_mut().name = name;
        });
    form = form.child(
        el().key("workspace-name-box")
            .w_full()
            .h(40.0)
            .radius(radius::MD)
            .border(1.0, look.line)
            .px(12.0)
            .center_y()
            .child(name_input),
    );
    let mut layout_buttons = row().key("workspace-layout-buttons").w_full().gap(8.0);
    layout_buttons = layout_buttons.child(dialog_button(
        "workspace-duplicate",
        "Duplicate",
        false,
        look,
        |s: &mut Shell| {
            if s.workspace.duplicate().is_ok() {
                s.ws_name.text = s.workspace.current().name.clone();
            }
        },
    ));
    if s.workspace.layouts.len() > 1 {
        layout_buttons = layout_buttons.child(dialog_button(
            "workspace-delete",
            "Delete",
            false,
            look,
            |s: &mut Shell| {
                s.workspace.layouts.remove(s.workspace.active);
                s.workspace.active = 0;
                s.ws_name.text = s.workspace.current().name.clone();
            },
        ));
    }
    form = form.child(layout_buttons);
    form = form.child(section_title(look, "ARRANGEMENT"));
    let layout = &s.workspace.current().layout;
    form = form.child(section_title(look, "Navigation"));
    form = form.child(segmented(
        look,
        "workspace-nav",
        match layout.navigation {
            Navigation::Left => "left",
            Navigation::Right => "right",
            Navigation::Hidden => "hidden",
        },
        &[
            ("left".into(), "Left".into()),
            ("right".into(), "Right".into()),
            ("hidden".into(), "Hidden".into()),
        ],
        |s: &mut Shell, id| {
            s.workspace.current_mut().layout.navigation = match id.as_str() {
                "right" => Navigation::Right,
                "hidden" => Navigation::Hidden,
                _ => Navigation::Left,
            };
        },
    ));
    form = form.child(section_title(look, "Details"));
    form = form.child(segmented(
        look,
        "workspace-details",
        match layout.details {
            Details::Right => "right",
            Details::Modal => "modal",
            Details::Hidden => "hidden",
        },
        &[
            ("right".into(), "Right".into()),
            ("modal".into(), "Modal".into()),
            ("hidden".into(), "Hidden".into()),
        ],
        |s: &mut Shell, id| {
            s.workspace.current_mut().layout.details = match id.as_str() {
                "modal" => Details::Modal,
                "hidden" => Details::Hidden,
                _ => Details::Right,
            };
            s.details_open = false;
        },
    ));
    form = form.child(section_title(look, "View"));
    form = form.child(segmented(
        look,
        "workspace-view",
        match layout.view {
            View::Grid => "grid",
            View::List => "list",
            View::Shelf => "shelf",
        },
        &[
            ("grid".into(), "Grid".into()),
            ("list".into(), "List".into()),
            ("shelf".into(), "Shelf".into()),
        ],
        |s: &mut Shell, id| {
            s.workspace.current_mut().layout.view = match id.as_str() {
                "list" => View::List,
                "shelf" => View::Shelf,
                _ => View::Grid,
            };
        },
    ));
    let tile = layout.tile_width;
    form = form.child(
        row()
            .key("workspace-tile")
            .w_full()
            .gap(8.0)
            .center_y()
            .child(section_title(look, "Tile width"))
            .child(spacer().grow(1.0))
            .child(dialog_button(
                "workspace-tile-less",
                "−",
                false,
                look,
                |s: &mut Shell| {
                    let layout = &mut s.workspace.current_mut().layout;
                    layout.tile_width = (layout.tile_width - 20.0).clamp(130.0, 320.0);
                },
            ))
            .child(
                text(format!("{tile:.0}"))
                    .key("workspace-tile-value")
                    .w(40.0)
                    .font(type_::BODY)
                    .color(look.text),
            )
            .child(dialog_button(
                "workspace-tile-more",
                "＋",
                false,
                look,
                |s: &mut Shell| {
                    let layout = &mut s.workspace.current_mut().layout;
                    layout.tile_width = (layout.tile_width + 20.0).clamp(130.0, 320.0);
                },
            )),
    );
    form = form.child(section_title(look, "BACKGROUND"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-bg",
        &BACKGROUNDS,
        "background",
    ));
    form = form.child(section_title(look, "NAVIGATION"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-nav",
        &NAVS,
        "navigation",
    ));
    form = form.child(section_title(look, "SURFACE"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-surface",
        &SURFACES,
        "surface",
    ));
    form = form.child(section_title(look, "TEXT"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-text",
        &TEXTS,
        "text",
    ));
    form = form.child(section_title(look, "SECONDARY TEXT"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-muted",
        &MUTEDS,
        "muted",
    ));
    form = form.child(section_title(look, "HAIRLINES"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-line",
        &LINES,
        "line",
    ));
    form = form.child(section_title(look, "ACCENT"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-accent",
        &ACCENTS,
        "accent",
    ));
    form = form.child(section_title(look, "SELECTION"));
    form = form.child(workspace_swatches(
        s,
        look,
        "workspace-selection",
        &SELECTIONS,
        "selection",
    ));
    let body = col()
        .key("workspace-scroll")
        .w_full()
        .max_h(560.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(form);
    let mut buttons = row().key("dialog-buttons").w_full().gap(12.0);
    buttons = buttons.child(dialog_button(
        "workspace-save",
        "Save layout",
        true,
        look,
        |s: &mut Shell| s.save_workspace(),
    ));
    buttons = buttons.child(dialog_button(
        "workspace-cancel",
        "Cancel",
        false,
        look,
        |s: &mut Shell| s.cancel_workspace(),
    ));
    let form = col()
        .key("workspace-panel-form")
        .w_full()
        .gap(12.0)
        .child(body)
        .child(buttons);
    el().key("workspace-dialog")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(scrim())
        .child(panel(
            look,
            "workspace-dialog",
            "Build your workspace",
            false,
            |s: &mut Shell| s.cancel_workspace(),
            form,
        ))
}

/// Color preset swatches for the workspace editor.
fn workspace_swatches(
    s: &Shell,
    look: Look,
    key: &str,
    presets: &[([u8; 3], &str)],
    field: &str,
) -> El {
    let skin = &s.workspace.current().skin;
    let current = match field {
        "background" => skin.background,
        "navigation" => skin.navigation,
        "surface" => skin.surface,
        "text" => skin.text,
        "muted" => skin.muted,
        "line" => skin.line,
        "accent" => skin.accent,
        "selection" => skin.selection,
        _ => skin.background,
    };
    let mut items = row().key(key.to_owned()).w_full().gap(8.0);
    for (preset, name) in presets {
        let color = rgb8(*preset);
        let active = *preset == current;
        let preset = *preset;
        let field = field.to_owned();
        items = items.child(
            el().key(format!("{key}-{name}"))
                .role("button")
                .label(format!("{name} preset"))
                .focusable(true)
                .size(36.0, 36.0)
                .radius(18.0)
                .fill(color)
                .border(2.0, if active { look.text } else { look.line })
                .hover(look.hover())
                .press_scale(0.92)
                .focus_ring(2.0, 1.0, look.select)
                .on_press(move |s: &mut Shell| {
                    let skin = &mut s.workspace.current_mut().skin;
                    match field.as_str() {
                        "background" => skin.background = preset,
                        "navigation" => skin.navigation = preset,
                        "surface" => skin.surface = preset,
                        "text" => skin.text = preset,
                        "muted" => skin.muted = preset,
                        "line" => skin.line = preset,
                        "accent" => skin.accent = preset,
                        "selection" => skin.selection = preset,
                        _ => {}
                    }
                }),
        );
    }
    items
}

fn toolbar(s: &Shell, look: Look, leading: f32, trailing: f32) -> El {
    let search = text_input(&s.query, type_::BODY.lh(20.0))
        .key("search")
        .label("Search games")
        .single_line()
        .w(220.0)
        .color(look.text)
        .placeholder("Search games", look.faint)
        .on_edit(|s: &mut Shell| &mut s.query)
        .on_input_change(|s: &mut Shell, _| s.reselect())
        .on_submit(|s: &mut Shell, _| s.play_current());
    let search_box = row()
        .key("search-box")
        .h(38.0)
        .radius(19.0)
        .fill(look.surface)
        .border(1.0, look.line)
        .px(14.0)
        .gap(8.0)
        .center_y()
        .focus_ring(2.0, 1.0, look.select)
        .child(text("⌕").font(type_::BODY).color(look.faint))
        .child(search);
    let current = s.current();
    let ready = current.is_some();
    let mut play = row()
        .key("play")
        .role("button")
        .label("Play selected game")
        .focusable(true)
        .h(38.0)
        .px(20.0)
        .radius(19.0)
        .gap(8.0)
        .center()
        .fill(if ready { look.accent } else { look.raised })
        .hover(if ready {
            look.accent.alpha(0.85)
        } else {
            look.hover()
        })
        .press_scale(0.96)
        .focus_ring(2.0, 2.0, look.select)
        .child(
            text("▶")
                .font(type_::BODY)
                .color(if ready { look.ink } else { look.muted }),
        )
        .child(
            text("Play")
                .font(type_::LABEL)
                .color(if ready { look.ink } else { look.muted }),
        );
    if ready {
        play = play
            .shadow(0.0, 16.0, look.accent.alpha(0.35))
            .on_press(|s: &mut Shell| s.play_current());
    }
    let mut bar = row()
        .key("toolbar")
        .h(60.0)
        .w_full()
        .px(16.0)
        .gap(12.0)
        .center_y()
        .fill(look.bg)
        .border_sides([0.0, 0.0, 1.0, 0.0], look.line)
        .window_drag()
        .child(mark(look))
        .child(
            col()
                .gap(0.0)
                .child(text("The Launcher").font(type_::TITLE).color(look.text))
                .child(
                    text("your shelf, your rules")
                        .font(type_::LABEL)
                        .color(look.faint),
                ),
        )
        .child(spacer().grow(1.0))
        .child(search_box)
        .child(play);
    if leading > 0.0 {
        bar = bar.child(spacer().w(leading));
    }
    if trailing > 0.0 {
        bar = bar.child(spacer().w(trailing));
    }
    bar
}

fn mark(look: Look) -> El {
    el().key("mark")
        .size(32.0, 32.0)
        .radius(10.0)
        .continuous_corners()
        .fill(look.accent)
        .gradient(look.accent, look.select)
        .shadow(0.0, 12.0, look.accent.alpha(0.4))
        .center()
        .child(text("L").font(type_::TITLE).color(look.ink))
}

fn body(s: &Shell, look: Look) -> El {
    let layout = &s.workspace.current().layout;
    let mut middle = row().key("body-row").w_full().grow(1.0).min_h(0.0);
    if layout.navigation == Navigation::Left {
        middle = middle.child(sidebar(s, look));
    }
    middle = middle.child(shelf(s, look));
    if layout.details == Details::Right {
        middle = middle.child(details(s, look));
    }
    if layout.navigation == Navigation::Right {
        middle = middle.child(sidebar(s, look));
    }
    middle.key("body")
}

fn sidebar(s: &Shell, look: Look) -> El {
    let width = s.workspace.current().layout.navigation_width;
    let mut list = col()
        .key("sidebar")
        .w(width)
        .h_full()
        .padding(16.0)
        .gap(4.0)
        .fill(look.nav)
        .border_sides([0.0, 1.0, 0.0, 0.0], look.line);
    list = list.child(
        text("LIBRARY")
            .key("sidebar-header")
            .font(type_::SECTION)
            .color(look.faint)
            .px(12.0),
    );
    for (filter, key, name) in Filter::ALL {
        let count = s.library.games.iter().filter(|g| filter.matches(g)).count();
        let active = s.filter == filter;
        let mut item = row()
            .key(format!("filter-{key}"))
            .role("button")
            .label(name)
            .focusable(true)
            .h(38.0)
            .w_full()
            .px(12.0)
            .gap(10.0)
            .center_y()
            .radius(radius::MD)
            .continuous_corners()
            .fill(if active {
                look.active_wash()
            } else {
                Color::TRANSPARENT
            })
            .hover(look.hover())
            .press_scale(0.98)
            .focus_ring(2.0, 1.0, look.select)
            .child(el().key("dot").size(8.0, 8.0).radius(4.0).fill(if active {
                look.accent
            } else {
                look.faint.alpha(0.5)
            }))
            .child(
                text(name)
                    .font(type_::BODY)
                    .color(if active { look.text } else { look.muted })
                    .grow(1.0),
            )
            .child(
                text(format!("{count}"))
                    .font(type_::LABEL)
                    .color(if active { look.accent } else { look.faint }),
            )
            .on_press(move |s: &mut Shell| {
                s.filter = filter;
                s.reselect();
            });
        if active {
            item =
                item.border(1.0, look.accent.alpha(0.4))
                    .shadow(0.0, 8.0, look.accent.alpha(0.12));
        }
        list = list.child(item);
    }
    list = list.child(
        text("ACTIONS")
            .key("actions-header")
            .font(type_::SECTION)
            .color(look.faint)
            .px(12.0),
    );
    for (key, name, primary) in [
        ("add-game", "＋ Add game", true),
        ("import-steam", "Import Steam", false),
        ("open-metadata", "Metadata sources", false),
        ("open-workspace", "Customize layout", false),
    ] {
        list = list.child(
            row()
                .key(key)
                .role("button")
                .label(name)
                .focusable(true)
                .h(38.0)
                .w_full()
                .px(12.0)
                .gap(10.0)
                .center_y()
                .radius(radius::MD)
                .continuous_corners()
                .fill(if primary {
                    look.accent.alpha(0.16)
                } else {
                    Color::TRANSPARENT
                })
                .border(
                    1.0,
                    if primary {
                        look.accent.alpha(0.35)
                    } else {
                        Color::TRANSPARENT
                    },
                )
                .hover(look.hover())
                .press_scale(0.98)
                .focus_ring(2.0, 1.0, look.select)
                .child(
                    text(name.to_owned())
                        .font(type_::BODY)
                        .color(if primary { look.accent } else { look.muted })
                        .grow(1.0),
                )
                .on_press(move |s: &mut Shell| match key {
                    "add-game" => s.open_add(),
                    "import-steam" => s.steam.opened(),
                    "open-metadata" => s.open_meta(None),
                    _ => s.open_workspace(),
                }),
        );
    }
    list.child(spacer().grow(1.0)).child(
        text("Gamepad support is next.")
            .key("sidebar-next")
            .font(type_::LABEL)
            .color(look.faint)
            .px(12.0),
    )
}

fn shelf(s: &Shell, look: Look) -> El {
    let visible = s.visible();
    let layout = &s.workspace.current().layout;
    let title = match s.filter {
        Filter::All => "All games",
        Filter::Favorites => "Favorites",
        Filter::Pc => "PC",
        Filter::Steam => "Steam",
        Filter::Hidden => "Hidden",
    };
    let tile = layout.tile_width;
    let mut grid = col().key("grid").gap(16.0).padding(20.0);
    if visible.is_empty() {
        grid = grid.child(
            col()
                .key("empty")
                .w_full()
                .padding(48.0)
                .gap(12.0)
                .center()
                .child(
                    el().key("empty-mark")
                        .size(56.0, 56.0)
                        .radius(18.0)
                        .continuous_corners()
                        .fill(look.surface)
                        .gradient(look.accent.alpha(0.5), look.select.alpha(0.5))
                        .shadow(0.0, 20.0, look.accent.alpha(0.25))
                        .center()
                        .child(text("◈").font(type_::TITLE).color(look.text)),
                )
                .child(text("No games found").font(type_::TITLE).color(look.text))
                .child(
                    text("Try another search or collection.")
                        .font(type_::BODY)
                        .color(look.muted),
                )
                .child(dialog_button(
                    "empty-add",
                    "＋ Add game",
                    true,
                    look,
                    |s: &mut Shell| {
                        s.open_add();
                    },
                )),
        );
    } else {
        match layout.view {
            View::Grid => {
                // One wrapping row: Taffy flows as many tiles per line as the
                // shelf fits, so wide windows fill instead of leaving a void.
                let mut flow = row().key("flow").w_full().flex_wrap().gap(16.0);
                for &i in &visible {
                    flow = flow.child(card(s, look, i, tile));
                }
                grid = grid.child(flow);
            }
            View::List => {
                for &i in &visible {
                    grid = grid.child(list_row(s, look, i));
                }
            }
            View::Shelf => {
                let mut strip = row().key("strip").gap(16.0).padding(4.0);
                for &i in &visible {
                    strip = strip.child(shelf_tile(s, look, i, tile));
                }
                grid = grid.child(
                    el().key("strip-viewport")
                        .w_full()
                        .h(tile * 1.25 + 96.0)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .scroll_x()
                        .child(strip),
                );
            }
        }
    }
    col()
        .key("shelf")
        .grow(1.0)
        .h_full()
        .min_h(0.0)
        .fill(look.bg)
        .child(
            row()
                .key("shelf-header")
                .w_full()
                .h(56.0)
                .px(20.0)
                .center_y()
                .gap(10.0)
                .child(text(title).font(type_::TITLE).color(look.text))
                .child(
                    el().key("shelf-count")
                        .px(10.0)
                        .py(3.0)
                        .radius(10.0)
                        .fill(look.surface)
                        .border(1.0, look.line)
                        .child(
                            text(format!("{}", visible.len()))
                                .font(type_::LABEL)
                                .color(look.muted),
                        ),
                ),
        )
        .child(el().key("shelf-rule").h(1.0).w_full().fill(look.line))
        .child(
            grid.scroll(ScrollSpec {
                stick_bottom: false,
            })
            .grow(1.0)
            .min_h(0.0),
        )
}

/// Game artwork: the local cover when one is set and decoded, else the
/// generated gradient with initials.
fn cover_art(s: &Shell, look: Look, game: &Game, width: f32, height: f32, rounded: f32) -> El {
    let art = el()
        .key("art")
        .w(width)
        .h(height)
        .radius(rounded)
        .clip_rounded(rounded);
    if let Some(pixels) = game.cover.as_ref().and_then(|path| s.covers.get(path)) {
        return art.child(
            el().key("art-image")
                .w_full()
                .h_full()
                .pixels_fit(pixels, SurfaceFit::Fill),
        );
    }
    let (top, bottom) = gradient(&game.name);
    art.fill(look.surface).gradient(top, bottom).center().child(
        text(initials(&game.name))
            .font(type_::TITLE)
            .color(Color::WHITE.alpha(0.92)),
    )
}

fn card(s: &Shell, look: Look, index: usize, tile: f32) -> El {
    let Some(game) = s.library.games.get(index) else {
        return el().key(format!("game-{index}")).size(tile, tile + 52.0);
    };
    let selected = s.current() == Some(index);
    let name = game.name.clone();
    let favorite = game.favorite;
    col()
        .key(format!("game-{index}"))
        .role("button")
        .label(format!("{} ({})", name, game.source()))
        .focusable(true)
        .w(tile)
        .radius(radius::LG)
        .continuous_corners()
        .fill(look.surface)
        .border_gradient(
            if selected { 2.0 } else { 1.0 },
            if selected {
                look.select
            } else {
                look.rim_top()
            },
            if selected { look.select } else { look.line },
        )
        .motion(motion::SNAPPY)
        .hover(look.hover())
        .press(look.pressed())
        .press_scale(0.97)
        .focus_ring(2.0, 2.0, look.select)
        .shadow(
            0.0,
            if selected { 20.0 } else { 12.0 },
            if selected {
                look.select.alpha(0.25)
            } else {
                Color::BLACK.alpha(0.35)
            },
        )
        .on_press(move |s: &mut Shell| s.select_game(index))
        .child(cover_art(s, look, game, tile, tile * 0.56, radius::LG))
        .child(
            col()
                .key("meta")
                .w_full()
                .padding(12.0)
                .gap(6.0)
                .child(
                    text(name.clone())
                        .font(type_::BODY)
                        .color(look.text)
                        .truncate(),
                )
                .child(
                    row()
                        .key("sub")
                        .w_full()
                        .center_y()
                        .gap(8.0)
                        .child(text(game.source()).font(type_::LABEL).color(
                            if game.source() == "Steam" {
                                look.select
                            } else {
                                look.accent
                            },
                        ))
                        .child(spacer().grow(1.0))
                        .child(
                            text(if favorite { "♥" } else { "♡" })
                                .key(format!("fav-{index}"))
                                .role("button")
                                .label(if favorite { "Unfavorite" } else { "Favorite" })
                                .font(type_::BODY)
                                .color(if favorite { look.accent } else { look.muted })
                                .on_press(move |s: &mut Shell| s.toggle_favorite(index)),
                        ),
                ),
        )
}

/// Full-width list row: small cover, name, source/status, favorite.
fn list_row(s: &Shell, look: Look, index: usize) -> El {
    let Some(game) = s.library.games.get(index) else {
        return el().key(format!("game-{index}")).w_full().h(76.0);
    };
    let selected = s.current() == Some(index);
    let status = match game.installed {
        Some(false) => "Not installed",
        Some(true) => "Installed",
        None if matches!(game.target, Target::Steam { .. }) => "Not checked",
        None => "Executable",
    };
    let status = if game.hidden {
        format!("{status} / Hidden")
    } else {
        status.into()
    };
    let favorite = game.favorite;
    let name = game.name.clone();
    row()
        .key(format!("game-{index}"))
        .role("button")
        .label(format!("{} ({})", name, game.source()))
        .focusable(true)
        .w_full()
        .h(76.0)
        .px(8.0)
        .gap(12.0)
        .center_y()
        .radius(radius::MD)
        .continuous_corners()
        .fill(if selected {
            look.select_wash()
        } else {
            Color::TRANSPARENT
        })
        .hover(look.hover())
        .press(look.pressed())
        .press_scale(0.99)
        .focus_ring(2.0, 1.0, look.select)
        .border(
            if selected { 2.0 } else { 1.0 },
            if selected { look.select } else { look.line },
        )
        .on_press(move |s: &mut Shell| s.select_game(index))
        .child(cover_art(s, look, game, 48.0, 60.0, 8.0))
        .child(
            col()
                .key("row-meta")
                .grow(1.0)
                .gap(2.0)
                .child(
                    text(name.clone())
                        .font(type_::BODY)
                        .color(look.text)
                        .truncate(),
                )
                .child(
                    text(format!("{} / {status}", game.source()))
                        .font(type_::LABEL)
                        .color(look.muted)
                        .truncate(),
                ),
        )
        .child(
            text(if favorite { "♥" } else { "♡" })
                .key(format!("fav-{index}"))
                .role("button")
                .label(if favorite { "Unfavorite" } else { "Favorite" })
                .font(type_::TITLE)
                .color(if favorite { look.accent } else { look.muted })
                .on_press(move |s: &mut Shell| s.toggle_favorite(index)),
        )
}

/// Tall shelf tile for the horizontal strip.
fn shelf_tile(s: &Shell, look: Look, index: usize, tile: f32) -> El {
    let Some(game) = s.library.games.get(index) else {
        return el().key(format!("game-{index}")).size(tile, tile * 1.25);
    };
    let selected = s.current() == Some(index);
    let name = game.name.clone();
    col()
        .key(format!("game-{index}"))
        .role("button")
        .label(format!("{} ({})", name, game.source()))
        .focusable(true)
        .w(tile)
        .gap(8.0)
        .press_scale(0.97)
        .focus_ring(2.0, 2.0, look.select)
        .on_press(move |s: &mut Shell| s.select_game(index))
        .child(
            el().key("tile-frame")
                .w_full()
                .radius(radius::LG)
                .continuous_corners()
                .border_gradient(
                    if selected { 3.0 } else { 1.0 },
                    if selected {
                        look.select
                    } else {
                        look.rim_top()
                    },
                    if selected { look.select } else { look.line },
                )
                .shadow(
                    0.0,
                    if selected { 20.0 } else { 12.0 },
                    if selected {
                        look.select.alpha(0.25)
                    } else {
                        Color::BLACK.alpha(0.35)
                    },
                )
                .child(cover_art(
                    s,
                    look,
                    game,
                    tile - 2.0,
                    tile * 1.25,
                    radius::LG,
                )),
        )
        .child(
            text(name.clone())
                .font(type_::LABEL)
                .color(look.text)
                .truncate(),
        )
}

fn details(s: &Shell, look: Look) -> El {
    let width = s.workspace.current().layout.details_width;
    let Some(index) = s.current() else {
        return details_shell(look, width)
            .child(text("Nothing selected").font(type_::TITLE).color(look.text));
    };
    let Some(game) = s.library.games.get(index).cloned() else {
        return details_shell(look, width);
    };
    details_shell(look, width)
        .child(details_cover(s, look, &game, width))
        .child(
            text(game.name.clone())
                .key("details-name")
                .font(type_::TITLE)
                .color(look.text),
        )
        .child(details_badges(look, &game, game.favorite))
        .child(
            text(target_line(&game))
                .key("target")
                .font(type_::LABEL)
                .color(look.muted),
        )
        .child(
            text(played_line(&game))
                .key("played")
                .font(type_::LABEL)
                .color(look.muted),
        )
        .child(meta_summary(look, &game))
        .child(play_button(look, &game))
        .child(favorite_button(look, index, &game))
        .child(details_manage(look, index, &game))
        .child(details_media(look, index, &game))
}

/// Details container: right column.
fn details_shell(look: Look, width: f32) -> El {
    col()
        .key("details")
        .w(width)
        .h_full()
        .padding(20.0)
        .gap(12.0)
        .fill(look.nav)
        .border_sides([0.0, 0.0, 0.0, 1.0], look.line)
}

/// Game details modal: the replacement for the old bottom strip. Selecting a
/// game opens it; Escape, outside click, or Close dismisses it without losing
/// the selection.
fn game_modal(s: &Shell, look: Look) -> El {
    let Some(index) = s.current() else {
        return el().key("game-modal");
    };
    let Some(game) = s.library.games.get(index).cloned() else {
        return el().key("game-modal");
    };
    let body = col()
        .key("game-modal-body")
        .w_full()
        .gap(12.0)
        .child(modal_cover(s, look, &game))
        .child(
            text(game.name.clone())
                .key("details-name")
                .font(type_::TITLE)
                .color(look.text),
        )
        .child(details_badges(look, &game, game.favorite))
        .child(
            text(target_line(&game))
                .key("target")
                .font(type_::LABEL)
                .color(look.muted),
        )
        .child(
            text(played_line(&game))
                .key("played")
                .font(type_::LABEL)
                .color(look.muted),
        )
        .child(meta_summary(look, &game))
        .child(play_button(look, &game))
        .child(favorite_button(look, index, &game))
        .child(details_manage(look, index, &game))
        .child(details_media(look, index, &game));
    // The scroll fills whatever room the sheet leaves it, so short windows
    // shrink the artwork list instead of clipping the dialog.
    let scroll = col()
        .key("game-modal-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(body);
    let sheet = col()
        .key("game-modal-panel")
        .role("dialog")
        .label(game.name.clone())
        .w_full()
        .max_w(600.0)
        // h_full gives the sheet a definite height (capped by max_h) so the
        // flexible scroll below has room to fill; max_h alone leaves it
        // unbounded and the scroll collapses to zero.
        .h_full()
        .max_h(560.0)
        .radius(16.0)
        .continuous_corners()
        .fill(look.surface)
        .border_gradient(1.0, look.rim_top(), look.line)
        .shadow(0.0, 32.0, Color::BLACK.alpha(0.5))
        .padding(24.0)
        .gap(12.0)
        .modal(true, |s: &mut Shell| s.close_details())
        .child(
            row()
                .key("game-modal-header")
                .w_full()
                .center_y()
                .gap(12.0)
                .child(
                    text("Game details")
                        .font(type_::LABEL)
                        .color(look.muted)
                        .grow(1.0),
                )
                .child(
                    row()
                        .key("game-modal-close")
                        .role("button")
                        .label(format!("Close details for {}", game.name))
                        .focusable(true)
                        .h(32.0)
                        .px(12.0)
                        .radius(radius::MD)
                        .center()
                        .fill(look.raised)
                        .hover(look.hover())
                        .on_press(|s: &mut Shell| s.close_details())
                        .child(text("✕").font(type_::LABEL).color(look.text)),
                ),
        )
        .child(scroll)
        .child(dialog_button(
            "game-modal-done",
            "Close",
            false,
            look,
            |s: &mut Shell| s.close_details(),
        ));
    el().key("game-modal")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .child(scrim())
        .child(
            el().key("game-modal-layer")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .center()
                .padding(16.0)
                .child(sheet),
        )
}

/// Fluid cover banner for the modal: full width up to a cap, so narrow
/// windows never clip it and wide ones keep a tidy banner.
fn modal_cover(s: &Shell, look: Look, game: &Game) -> El {
    let art = el()
        .key("art")
        .w_full()
        .max_w(520.0)
        .h(180.0)
        .radius(radius::LG)
        .clip_rounded(radius::LG);
    let art = if let Some(pixels) = game.cover.as_ref().and_then(|path| s.covers.get(path)) {
        art.child(
            el().key("art-image")
                .w_full()
                .h_full()
                .pixels_fit(pixels, SurfaceFit::Fill),
        )
    } else {
        let (top, bottom) = gradient(&game.name);
        art.fill(look.surface).gradient(top, bottom).center().child(
            text(initials(&game.name))
                .font(type_::TITLE)
                .color(Color::WHITE.alpha(0.92)),
        )
    };
    el().key("cover").w_full().center().child(art)
}

fn play_button(look: Look, game: &Game) -> El {
    let action = match (&game.target, game.installed) {
        (the_launcher::library::Target::Steam { .. }, Some(false)) => "Install",
        _ => "Play",
    };
    row()
        .key("actions")
        .w_full()
        .h(46.0)
        .radius(12.0)
        .continuous_corners()
        .center()
        .gap(8.0)
        .fill(look.accent)
        .role("button")
        .label(format!("{action} {}", game.name))
        .focusable(true)
        .hover(look.accent.alpha(0.85))
        .press(look.pressed())
        .press_scale(0.98)
        .focus_ring(2.0, 2.0, look.select)
        .shadow(0.0, 16.0, look.accent.alpha(0.35))
        .on_press(|s: &mut Shell| s.play_current())
        .child(
            text(format!("▶ {action}"))
                .font(type_::LABEL)
                .color(look.ink),
        )
}

fn favorite_button(look: Look, index: usize, game: &Game) -> El {
    let favorite = game.favorite;
    row()
        .key("favorite")
        .w_full()
        .h(40.0)
        .radius(radius::MD)
        .continuous_corners()
        .center()
        .fill(look.raised)
        .border(1.0, look.line)
        .role("button")
        .label(if favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        })
        .focusable(true)
        .hover(look.hover())
        .press(look.pressed())
        .press_scale(0.98)
        .focus_ring(2.0, 1.0, look.select)
        .on_press(move |s: &mut Shell| s.toggle_favorite(index))
        .child(
            text(if favorite {
                "♥ Favorited"
            } else {
                "♡ Favorite"
            })
            .font(type_::LABEL)
            .color(if favorite { look.accent } else { look.text }),
        )
}

fn details_cover(s: &Shell, look: Look, game: &Game, width: f32) -> El {
    el().key("cover")
        .w_full()
        .h(200.0)
        .center()
        .child(cover_art(s, look, game, width - 40.0, 200.0, radius::LG))
}

fn details_badges(look: Look, game: &Game, favorite: bool) -> El {
    row()
        .key("badges")
        .w_full()
        .gap(8.0)
        .center_y()
        .child(badge(
            game.source(),
            if game.source() == "Steam" {
                look.select
            } else {
                look.accent
            },
        ))
        .child(badge(
            if favorite {
                "♥ Favorite"
            } else {
                "Not favorited"
            },
            look.muted,
        ))
}

/// One-line saved-metadata summary for the details pane.
fn meta_summary(look: Look, game: &Game) -> El {
    let m = &game.metadata;
    let mut parts = Vec::new();
    if !m.release_date.is_empty() {
        parts.push(m.release_date.clone());
    }
    if !m.genres.is_empty() {
        parts.push(m.genres.join(", "));
    }
    if !m.developers.is_empty() {
        parts.push(m.developers.join(", "));
    }
    let line = if parts.is_empty() {
        "No downloaded metadata yet.".to_owned()
    } else {
        parts.join(" • ")
    };
    text(line)
        .key("meta-summary")
        .font(type_::LABEL)
        .color(look.muted)
}

/// Metadata and cover-art entry points under the manage row. Buttons wrap
/// onto a second line instead of squeezing on narrow dialogs.
fn details_media(look: Look, index: usize, game: &Game) -> El {
    let mut buttons = row().key("media").w_full().gap(12.0).center_y().flex_wrap();
    buttons = buttons.child(
        row()
            .key("details-metadata")
            .role("button")
            .label(format!("Metadata for {}", game.name))
            .focusable(true)
            .h(36.0)
            .grow(1.0)
            .radius(radius::MD)
            .continuous_corners()
            .center()
            .fill(look.raised)
            .border(1.0, look.line)
            .hover(look.hover())
            .press(look.pressed())
            .press_scale(0.97)
            .focus_ring(2.0, 1.0, look.select)
            .on_press(move |s: &mut Shell| s.open_meta(Some(index)))
            .child(text("Metadata").font(type_::LABEL).color(look.text)),
    );
    buttons = buttons.child(
        row()
            .key("details-cover")
            .role("button")
            .label(format!("Choose cover art for {}", game.name))
            .focusable(true)
            .h(36.0)
            .grow(1.0)
            .radius(radius::MD)
            .continuous_corners()
            .center()
            .fill(look.raised)
            .border(1.0, look.line)
            .hover(look.hover())
            .press(look.pressed())
            .press_scale(0.97)
            .focus_ring(2.0, 1.0, look.select)
            .on_press_tx(move |s: &mut Shell, tx: &mut Tx| s.request_cover(index, tx))
            .child(text("Cover art…").font(type_::LABEL).color(look.text)),
    );
    if game.cover.is_some() {
        buttons = buttons.child(
            row()
                .key("details-nocover")
                .role("button")
                .label(format!("Remove custom cover art for {}", game.name))
                .focusable(true)
                .h(36.0)
                .px(12.0)
                .radius(radius::MD)
                .continuous_corners()
                .center()
                .fill(look.raised)
                .border(1.0, look.line)
                .hover(look.hover())
                .press(look.pressed())
                .press_scale(0.97)
                .focus_ring(2.0, 1.0, look.select)
                .on_press(move |s: &mut Shell| s.remove_cover(index))
                .child(text("No cover").font(type_::LABEL).color(look.text)),
        );
    }
    buttons
}

/// Edit/remove row under the favorite toggle. Steam entries hide or restore
/// instead of editing; hidden entries offer only a restore.
fn details_manage(look: Look, index: usize, game: &Game) -> El {
    let hidden = game.hidden;
    let executable = matches!(game.target, Target::Executable { .. });
    let mut buttons = row()
        .key("manage")
        .w_full()
        .gap(12.0)
        .center_y()
        .flex_wrap();
    if !hidden && executable {
        buttons = buttons.child(
            row()
                .key("details-edit")
                .role("button")
                .label(format!("Edit {}", game.name))
                .focusable(true)
                .h(36.0)
                .grow(1.0)
                .radius(radius::MD)
                .continuous_corners()
                .center()
                .fill(look.raised)
                .border(1.0, look.line)
                .hover(look.hover())
                .press(look.pressed())
                .press_scale(0.97)
                .focus_ring(2.0, 1.0, look.select)
                .on_press(move |s: &mut Shell| s.open_edit(index))
                .child(text("Edit").font(type_::LABEL).color(look.text)),
        );
    }
    let (key, label) = if hidden {
        ("details-restore", "Restore")
    } else if executable {
        ("details-remove", "Remove")
    } else {
        ("details-hide", "Hide")
    };
    buttons = buttons.child(
        row()
            .key(key)
            .role("button")
            .label(format!("{label} {}", game.name))
            .focusable(true)
            .h(36.0)
            .grow(1.0)
            .radius(radius::MD)
            .continuous_corners()
            .center()
            .fill(look.raised)
            .border(1.0, look.line)
            .hover(look.hover())
            .press(look.pressed())
            .press_scale(0.97)
            .focus_ring(2.0, 1.0, look.select)
            .on_press(move |s: &mut Shell| {
                if hidden {
                    s.restore_hidden(index);
                } else {
                    s.begin_remove();
                }
            })
            .child(text(label.to_owned()).font(type_::LABEL).color(look.text)),
    );
    buttons
}

fn badge(label: &str, color: Color) -> El {
    el().key(format!("badge-{label}"))
        .px(10.0)
        .py(4.0)
        .radius(8.0)
        .fill(color.alpha(0.14))
        .border(1.0, color.alpha(0.35))
        .child(text(label).font(type_::LABEL).color(color))
}

fn target_line(game: &Game) -> String {
    match &game.target {
        the_launcher::library::Target::Steam { app_id } => format!("Steam app {app_id}"),
        the_launcher::library::Target::Executable { path, .. } => path.display().to_string(),
    }
}

fn footer(s: &Shell, look: Look) -> El {
    let status = match &s.notice {
        Some(notice) => notice.clone(),
        None => "Select a game, then press Play".into(),
    };
    let is_error = status.starts_with("Could not")
        || status.starts_with("Cannot")
        || status.starts_with("Game started, but");
    row()
        .key("footer")
        .h(42.0)
        .w_full()
        .px(20.0)
        .gap(12.0)
        .center_y()
        .fill(look.bg)
        .border_sides([1.0, 0.0, 0.0, 0.0], look.line)
        .child(
            el().key("footer-count")
                .px(10.0)
                .py(3.0)
                .radius(10.0)
                .fill(look.surface)
                .border(1.0, look.line)
                .child(
                    text(format!("{} games", s.library.games.len()))
                        .font(type_::LABEL)
                        .color(look.muted),
                ),
        )
        .child(spacer().grow(1.0))
        .child(
            text(status)
                .key("status")
                .font(type_::LABEL)
                .color(if is_error { look.err } else { look.faint }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn harness() -> Runtime<Shell> {
        let mut rt = Runtime::new(Shell::demo(), view, (1280.0, 800.0), Env::new(true, 2.0));
        rt.frame(0.0);
        rt
    }

    fn click(rt: &mut Runtime<Shell>, key: &str) {
        // Reveal first: hits inside an overflowing scroll viewport settle
        // once it has scrolled, even when the target is already visible.
        rt.update(|_, tx| tx.scroll_into_view(key));
        rt.frame(1.0 / 60.0);
        let (x, y) = rt
            .find(key)
            .unwrap_or_else(|| panic!("missing element {key}"));
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        // Let presses land before the next query: a pressed control commits
        // first, and newly mounted elements settle on the following frame.
        rt.frame(1.0 / 60.0);
    }

    #[test]
    fn filtering_combines_collection_and_search() {
        let library = Shell::demo().library;
        assert_eq!(visible_indices(&library, Filter::All, "").len(), 6);
        assert_eq!(visible_indices(&library, Filter::Favorites, "").len(), 2);
        assert_eq!(visible_indices(&library, Filter::Pc, "").len(), 4);
        assert_eq!(visible_indices(&library, Filter::Steam, "").len(), 2);
        assert_eq!(visible_indices(&library, Filter::All, "ember"), vec![2]);
        assert_eq!(
            visible_indices(&library, Filter::Steam, "ember"),
            Vec::<usize>::new()
        );
        assert_eq!(
            visible_indices(&library, Filter::All, "  STARFALL  ").len(),
            1
        );
    }

    #[test]
    fn clicking_a_card_selects_it_and_shows_details() {
        let mut rt = harness();
        click(&mut rt, "game-0");
        assert_eq!(rt.state.selected, Some(0));
        assert_eq!(rt.state.current(), Some(0));
        assert!(rt.state.details_open, "selection opens the game modal");
        assert!(rt.find("game-modal").is_some(), "modal is visible");
        rt.frame(1.0 / 60.0);
        let name = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("details-name"))
            .unwrap();
        let name_text = name.text.as_deref().unwrap_or("");
        assert!(
            name_text.contains("Hollow Drift"),
            "details follow selection: {name_text}"
        );
    }

    #[test]
    fn game_modal_closes_without_losing_selection() {
        let mut rt = harness();
        assert!(rt.find("game-modal").is_none(), "no modal on launch");
        click(&mut rt, "game-0");
        assert!(rt.find("game-modal").is_some());
        click(&mut rt, "game-modal-close");
        assert!(!rt.state.details_open);
        assert!(rt.find("game-modal").is_none(), "close dismisses the modal");
        assert_eq!(rt.state.selected, Some(0), "selection survives close");
    }

    #[test]
    fn game_modal_scales_to_small_windows() {
        let mut rt = Runtime::new(Shell::demo(), view, (480.0, 600.0), Env::new(true, 2.0));
        rt.frame(0.0);
        click(&mut rt, "game-0");
        let panel = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("game-modal-panel"))
            .expect("modal panel renders");
        assert!(
            panel.rect[2] <= 480.0 - 16.0,
            "panel fits a narrow window: {:?}",
            panel.rect
        );
        assert!(
            panel.rect[0] >= 8.0,
            "panel keeps a margin from the edge: {:?}",
            panel.rect
        );
        assert!(
            panel.rect[3] <= 600.0 - 16.0,
            "panel fits a short window: {:?}",
            panel.rect
        );
    }

    #[test]
    fn right_details_selects_without_modal() {
        let mut rt = harness();
        rt.update(|s, _| {
            s.workspace.current_mut().layout.details = Details::Right;
            s.details_open = false;
        });
        rt.frame(1.0 / 60.0);
        click(&mut rt, "game-0");
        assert_eq!(rt.state.selected, Some(0));
        assert!(
            !rt.state.details_open,
            "right pane follows selection without a modal"
        );
        assert!(rt.find("game-modal").is_none());
        assert!(rt.find("details").is_some(), "right pane stays visible");
    }

    #[test]
    fn favorite_toggle_persists_and_filters_follow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let mut shell = Shell::demo();
        shell.path = path.clone();
        shell.library.save(&path).unwrap();
        let mut rt = Runtime::new(shell, view, (1280.0, 800.0), Env::new(true, 2.0));
        rt.frame(0.0);
        assert!(!rt.state.library.games[0].favorite);
        click(&mut rt, "fav-0");
        assert!(rt.state.library.games[0].favorite);
        assert!(Library::load(&path).unwrap().games[0].favorite);
        click(&mut rt, "filter-favorites");
        assert_eq!(rt.state.filter, Filter::Favorites);
        assert!(rt.find("game-0").is_some());
        assert!(rt.find("game-2").is_none(), "non-favorites leave the shelf");
    }

    #[test]
    fn play_reports_missing_executables_without_losing_state() {
        let mut rt = harness();
        click(&mut rt, "game-0");
        click(&mut rt, "actions");
        let status = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("status"))
            .unwrap();
        let status_text = status.text.as_deref().unwrap_or("");
        assert!(
            status_text.contains("Could not launch"),
            "launch errors surface: {status_text}"
        );
        assert_eq!(rt.state.library.games.len(), 6);
    }

    #[test]
    fn played_ago_stays_readable() {
        assert_eq!(played_ago(1000, 990), "Played just now");
        assert_eq!(played_ago(1000, 640), "Played 6m ago");
        assert_eq!(played_ago(7200, 0), "Played 2h ago");
        assert_eq!(played_ago(200_000, 0), "Played 2d ago");
    }

    fn exe_fixture(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "fixture").unwrap();
        path
    }

    fn status_text(rt: &Runtime<Shell>) -> String {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("status"))
            .unwrap()
            .text
            .clone()
            .unwrap_or_default()
    }

    fn library_harness() -> (tempfile::TempDir, Runtime<Shell>) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let mut shell = Shell::demo();
        shell.path = path.clone();
        shell.library.save(&path).unwrap();
        // Keep worker threads out of the other tests: they drive finished
        // paths directly.
        shell.meta.settings.automatic = false;
        let mut rt = Runtime::new(shell, view, (1280.0, 800.0), Env::new(true, 2.0));
        rt.frame(0.0);
        (dir, rt)
    }

    #[test]
    fn draft_save_adds_executable_and_persists() {
        let (dir, mut rt) = library_harness();
        let exe = exe_fixture(dir.path(), "game.exe");
        click(&mut rt, "add-game");
        assert!(rt.state.draft.is_some());
        {
            let draft = rt.state.draft.as_mut().unwrap();
            draft.name.text = "New Game".into();
            draft.path.text = exe.display().to_string();
            draft.args.text = "--fullscreen".into();
        }
        click(&mut rt, "dialog-save");
        assert!(rt.state.draft.is_none(), "a save closes the form");
        assert_eq!(rt.state.library.games.len(), 7);
        let saved = Library::load(&dir.path().join("library.json")).unwrap();
        assert_eq!(saved.games.len(), 7);
        assert_eq!(saved.games[6].name, "New Game");
        assert_eq!(status_text(&rt), "Game saved to your library");
    }

    #[test]
    fn draft_save_rejects_missing_executable() {
        let (_dir, mut rt) = library_harness();
        let missing = tempfile::tempdir()
            .unwrap()
            .path()
            .join("nope.exe")
            .display()
            .to_string();
        click(&mut rt, "add-game");
        {
            let draft = rt.state.draft.as_mut().unwrap();
            draft.name.text = "Ghost".into();
            draft.path.text = missing;
        }
        click(&mut rt, "dialog-save");
        assert!(rt.state.draft.is_some(), "an error keeps the form open");
        assert_eq!(rt.state.library.games.len(), 6);
        let error = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("dialog-error"))
            .unwrap()
            .text
            .clone()
            .unwrap_or_default();
        assert!(
            error.contains("Cannot open executable"),
            "inline error: {error}"
        );
    }

    #[test]
    fn draft_save_rejects_duplicates() {
        let (dir, mut rt) = library_harness();
        let exe = exe_fixture(dir.path(), "dup.exe");
        for name in ["First", "Second"] {
            click(&mut rt, "add-game");
            {
                let draft = rt.state.draft.as_mut().unwrap();
                draft.name.text = name.into();
                draft.path.text = exe.display().to_string();
            }
            click(&mut rt, "dialog-save");
        }
        assert_eq!(rt.state.library.games.len(), 7);
        assert!(rt.state.draft.is_some(), "the duplicate stays open");
        let error = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("dialog-error"))
            .unwrap()
            .text
            .clone()
            .unwrap_or_default();
        assert!(
            error.contains("already in your library"),
            "inline error: {error}"
        );
    }

    #[test]
    fn draft_edit_preserves_favorite_and_install_state() {
        let (dir, mut rt) = library_harness();
        let exe = exe_fixture(dir.path(), "edited.exe");
        click(&mut rt, "game-4");
        click(&mut rt, "details-edit");
        {
            let draft = rt.state.draft.as_mut().unwrap();
            assert_eq!(draft.editing, Some(4));
            draft.name.text = "Paper Boats".into();
            draft.path.text = exe.display().to_string();
        }
        click(&mut rt, "dialog-save");
        let game = &rt.state.library.games[4];
        assert_eq!(game.name, "Paper Boats");
        assert!(game.favorite, "edits keep the favorite flag");
        let saved = Library::load(&dir.path().join("library.json")).unwrap();
        assert_eq!(saved.games[4].name, "Paper Boats");
        assert!(saved.games[4].favorite);
    }

    #[test]
    fn remove_confirm_deletes_entry_and_persists() {
        let (dir, mut rt) = library_harness();
        click(&mut rt, "game-0");
        click(&mut rt, "details-remove");
        assert!(rt.find("remove-dialog").is_some());
        click(&mut rt, "remove-confirm");
        assert_eq!(rt.state.library.games.len(), 5);
        assert!(!rt.state.remove);
        let saved = Library::load(&dir.path().join("library.json")).unwrap();
        assert_eq!(saved.games.len(), 5);
        assert_eq!(status_text(&rt), "Game removed");
    }

    #[test]
    fn hidden_steam_games_hide_and_restore() {
        let (_dir, mut rt) = library_harness();
        click(&mut rt, "game-1");
        click(&mut rt, "details-hide");
        assert!(rt.find("remove-dialog").is_some());
        click(&mut rt, "remove-confirm");
        assert!(rt.state.library.games[1].hidden);
        assert!(rt.find("game-1").is_none(), "hidden games leave the shelf");
        click(&mut rt, "filter-hidden");
        assert!(rt.find("game-1").is_some(), "Hidden shows them again");
        click(&mut rt, "game-1");
        click(&mut rt, "details-restore");
        assert!(!rt.state.library.games[1].hidden);
        assert_eq!(status_text(&rt), "Game restored to your library");
    }

    #[test]
    fn steam_merge_adds_new_and_refreshes_installed() {
        let mut library = Shell::demo().library;
        let imported = steam::ImportResult {
            games: vec![
                steam::SteamGame {
                    app_id: 620,
                    name: "Starfall Tactics".into(),
                    installed: false,
                    cover: None,
                },
                steam::SteamGame {
                    app_id: 4001,
                    name: "Fresh Port".into(),
                    installed: true,
                    cover: None,
                },
            ],
            warnings: vec![],
        };
        let added = merge_import(&mut library, &imported).unwrap();
        assert_eq!(added, 1);
        assert_eq!(
            library
                .games
                .iter()
                .find(|g| match &g.target {
                    Target::Steam { app_id } => *app_id == 4001,
                    _ => false,
                })
                .unwrap()
                .name,
            "Fresh Port"
        );
        let starfall = library
            .games
            .iter()
            .find(|g| match &g.target {
                Target::Steam { app_id } => *app_id == 620,
                _ => false,
            })
            .unwrap();
        assert_eq!(starfall.installed, Some(false), "clean scans refresh flags");
        assert!(starfall.favorite, "favorites survive the merge");

        let warned = steam::ImportResult {
            warnings: vec!["a manifest broke".into()],
            ..imported
        };
        let mut library = Shell::demo().library;
        merge_import(&mut library, &warned).unwrap();
        let starfall = library
            .games
            .iter()
            .find(|g| match &g.target {
                Target::Steam { app_id } => *app_id == 620,
                _ => false,
            })
            .unwrap();
        assert_eq!(
            starfall.installed,
            Some(true),
            "warnings freeze installed flags"
        );
    }

    #[test]
    fn steam_scan_outcome_surfaces_in_panel_and_notice() {
        let (_dir, mut rt) = library_harness();
        click(&mut rt, "import-steam");
        assert!(rt.state.steam.open);
        // No thread involved: drive the finished paths directly.
        rt.state.begin_scan();
        assert!(rt.state.steam.scanning);
        assert!(
            rt.state.steam.key.text.is_empty(),
            "the key is cleared at dispatch"
        );
        rt.update(|s, _| s.finish_scan(Err(anyhow::anyhow!("no Steam here"))));
        rt.frame(1.0 / 60.0);
        assert!(!rt.state.steam.scanning);
        assert!(rt.find("steam-error").is_some());
        let imported = steam::ImportResult {
            games: vec![steam::SteamGame {
                app_id: 4001,
                name: "Fresh Port".into(),
                installed: true,
                cover: None,
            }],
            warnings: vec!["a manifest broke".into()],
        };
        rt.update(|s, _| {
            s.finish_scan(Ok(imported));
        });
        rt.frame(1.0 / 60.0);
        assert!(rt.find("steam-summary").is_some());
        assert!(rt.find("steam-warning-0").is_some());
        assert!(status_text(&rt).contains("Imported 1 Steam games"));
        assert_eq!(rt.state.library.games.len(), 7);
    }

    fn png_fixture(dir: &std::path::Path, name: &str, width: u32, height: u32) -> PathBuf {
        let path = dir.join(name);
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 120, 140, 255]));
        image::DynamicImage::ImageRgba8(img).save(&path).unwrap();
        path
    }

    #[test]
    fn grid_wraps_to_fill_wide_windows() {
        let mut rt = Runtime::new(Shell::demo(), view, (1920.0, 1000.0), Env::new(true, 2.0));
        rt.frame(0.0);
        rt.frame(1.0 / 60.0);
        let mut cards: Vec<[f32; 4]> = rt
            .elements()
            .into_iter()
            .filter(|e| {
                e.key.as_deref().is_some_and(|key| {
                    key.strip_prefix("game-")
                        .is_some_and(|rest| rest.parse::<usize>().is_ok())
                })
            })
            .map(|e| e.rect)
            .collect();
        cards.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        assert_eq!(cards.len(), 6, "all demo games render a card");
        let centers: Vec<i64> = cards.iter().map(|r| (r[0] + r[2] / 2.0) as i64).collect();
        let distinct = {
            let mut sorted = centers.clone();
            sorted.sort();
            sorted.dedup();
            sorted
        };
        assert!(
            distinct.len() >= 5,
            "tiles spread across the width: {centers:?}"
        );
        assert!(
            *distinct.iter().max().unwrap() > 1200,
            "the grid reaches the right edge: {centers:?}"
        );
        let top = cards.iter().map(|r| r[1] as i64).min().unwrap();
        let bottom = cards.iter().map(|r| r[1] as i64).max().unwrap();
        assert!(
            bottom - top < 100,
            "a wide window fits one row: tops {top}..{bottom}"
        );
    }

    #[test]
    fn grid_wraps_to_rows_on_narrow_windows() {
        let rt = harness();
        let y = |rt: &Runtime<Shell>, key: &str| rt.find(key).unwrap().1;
        assert!(
            (y(&rt, "game-3") - y(&rt, "game-0")).abs() < 40.0,
            "five tiles share the first row at 1280 wide"
        );
        assert!(
            y(&rt, "game-5") - y(&rt, "game-0") > 100.0,
            "the sixth tile wraps to a second row"
        );
    }

    #[test]
    fn decode_cover_accepts_png_rejects_junk() {
        let dir = tempfile::tempdir().unwrap();
        let good = png_fixture(dir.path(), "cover.png", 32, 48);
        let pixels = decode_cover(&good).unwrap();
        let (w, h) = pixels.size();
        assert!(
            w <= 480 && h <= 720 && w * 3 == h * 2,
            "thumbnail keeps aspect"
        );
        let text = dir.path().join("cover.png");
        std::fs::write(&text, "not an image").unwrap();
        assert!(decode_cover(&text).is_err());
        let huge = png_fixture(dir.path(), "huge.png", 5000, 100);
        assert!(decode_cover(&huge).is_err(), "dimensions stay bounded");
        assert!(decode_cover(&dir.path().join("missing.png")).is_err());
    }

    #[test]
    fn choose_cover_flow_sets_and_removes_cover() {
        let (dir, mut rt) = library_harness();
        let art = png_fixture(dir.path(), "art.png", 32, 48);
        click(&mut rt, "game-0");
        click(&mut rt, "details-cover");
        assert!(rt.file_dialog().is_some(), "Browse opens a dialog");
        rt.resolve_file_dialog(Some(art.clone())).unwrap();
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);
        assert_eq!(
            rt.state.library.games[0].cover.as_deref(),
            Some(art.as_path()),
            "the chosen file becomes the cover"
        );
        assert!(rt.state.covers.contains_key(&art));
        let saved = Library::load(&dir.path().join("library.json")).unwrap();
        assert_eq!(saved.games[0].cover.as_deref(), Some(art.as_path()));
        click(&mut rt, "details-nocover");
        assert!(rt.state.library.games[0].cover.is_none());
        assert!(
            !rt.state.library.games[0]
                .metadata
                .sources
                .contains_key("cover")
        );
    }

    #[test]
    fn meta_panel_prefills_from_selection() {
        let (_dir, mut rt) = library_harness();
        click(&mut rt, "game-1");
        click(&mut rt, "details-metadata");
        assert!(rt.state.meta.open);
        assert_eq!(rt.state.meta.query.text, "Starfall Tactics");
        assert_eq!(rt.state.meta.id_input.text, "620");
        click(&mut rt, "meta-close");
        assert!(!rt.state.meta.open);
    }

    #[test]
    fn meta_bind_app_id_preserves_launch_target() {
        let (dir, mut rt) = library_harness();
        click(&mut rt, "game-0");
        click(&mut rt, "details-metadata");
        rt.update(|s, _| s.meta.id_input.text = "123".into());
        rt.frame(1.0 / 60.0);
        click(&mut rt, "meta-use-id");
        let game = &rt.state.library.games[0];
        assert_eq!(game.metadata.steam_app_id, Some(123));
        assert!(matches!(game.target, Target::Executable { .. }));
        let saved = Library::load(&dir.path().join("library.json")).unwrap();
        assert_eq!(saved.games[0].metadata.steam_app_id, Some(123));
    }

    #[test]
    fn meta_matches_bind_through_list() {
        let (_dir, mut rt) = library_harness();
        click(&mut rt, "game-0");
        click(&mut rt, "details-metadata");
        let target = rt.state.meta.target.clone();
        let query = rt.state.meta.query.text.clone();
        rt.update(|s, _| {
            s.finish_matches(
                target,
                query,
                Ok(vec![metadata::Match {
                    app_id: 777,
                    name: "Hollow Drift".into(),
                }]),
            );
        });
        rt.frame(1.0 / 60.0);
        assert!(rt.find("meta-match-777").is_some());
        click(&mut rt, "meta-match-777");
        assert_eq!(rt.state.library.games[0].metadata.steam_app_id, Some(777));
    }

    #[test]
    fn meta_images_apply_cover_choice() {
        let (dir, mut rt) = library_harness();
        let art = png_fixture(dir.path(), "choice.png", 32, 48);
        click(&mut rt, "game-1");
        click(&mut rt, "details-metadata");
        let target = rt.state.meta.target.clone();
        rt.update(|s, _| {
            s.finish_images(
                target,
                ImageKind::Cover,
                620,
                Ok(vec![ImagePreview {
                    label: "Official cover".into(),
                    source: "Steam".into(),
                    path: Ok(art.clone()),
                }]),
            );
        });
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);
        assert!(rt.find("meta-choice-0").is_some());
        click(&mut rt, "meta-use-0");
        let game = &rt.state.library.games[1];
        assert_eq!(game.cover.as_deref(), Some(art.as_path()));
        assert_eq!(
            game.metadata.sources.get("cover").map(String::as_str),
            Some("Steam")
        );
        assert!(rt.state.covers.contains_key(&art));
    }

    #[test]
    fn meta_fetched_merges_and_preserves_in_flight_edits() {
        let (_dir, mut rt) = library_harness();
        click(&mut rt, "game-1");
        click(&mut rt, "details-metadata");
        let baseline = rt.state.library.games[1].clone();
        rt.update(|s, _| {
            s.library.games[1].metadata.description = "My notes".into();
        });
        let mut fetched = Fetched::default();
        fetched.metadata.description = "Store text".into();
        fetched.metadata.genres = vec!["Tactics".into()];
        rt.update(|s, _| s.finish_fetched(baseline, Ok(fetched), false));
        let game = &rt.state.library.games[1];
        assert_eq!(game.metadata.description, "My notes", "in-flight edits win");
        assert_eq!(game.metadata.genres, vec!["Tactics".to_string()]);
    }

    #[test]
    fn meta_settings_toggle_and_save() {
        let (dir, mut rt) = library_harness();
        rt.state.meta.settings.automatic = true;
        rt.state.meta.settings_path = dir.path().join("metadata.json");
        click(&mut rt, "open-metadata");
        assert!(rt.state.meta.open);
        click(&mut rt, "meta-automatic");
        assert!(!rt.state.meta.settings.automatic);
        click(&mut rt, "meta-save");
        let saved = Settings::load(&dir.path().join("metadata.json")).unwrap();
        assert!(!saved.automatic);
    }

    #[test]
    fn maybe_automatic_skips_complete_libraries() {
        let (_dir, mut rt) = library_harness();
        rt.state.meta.settings.automatic = true;
        for game in rt.state.library.games.iter_mut() {
            game.metadata.description = "x".into();
            game.metadata.developers = vec!["d".into()];
            game.metadata.publishers = vec!["p".into()];
            game.metadata.genres = vec!["g".into()];
            game.metadata.release_date = "2020".into();
            game.cover = Some(PathBuf::from("x.png"));
            game.metadata.background = Some(PathBuf::from("y.png"));
        }
        rt.update(|s, _| s.maybe_automatic());
        assert!(!rt.state.meta.busy, "nothing to fetch starts no scan");
        assert!(rt.state.meta.status.contains("No matching games"));
    }

    #[test]
    fn ink_and_select_follow_the_skin() {
        assert_eq!(
            ink_for([255, 151, 124]),
            Color {
                r: 0.10,
                g: 0.08,
                b: 0.10,
                a: 1.0
            }
        );
        assert_eq!(
            ink_for([10, 12, 20]),
            Color {
                r: 0.95,
                g: 0.94,
                b: 0.91,
                a: 1.0
            }
        );
        let look = Look::of(&Workspace::default());
        assert_eq!(look.accent, rgb8([255, 151, 124]));
        let mut mint = Workspace::default();
        mint.current_mut().skin.accent = [159, 222, 204];
        assert_eq!(Look::of(&mint).select, rgb8([255, 151, 124]));
    }

    #[test]
    fn workspace_layout_crud_validates_and_restores() {
        let (dir, mut rt) = library_harness();
        rt.state.workspace_path = dir.path().join("workspace.json");
        click(&mut rt, "open-workspace");
        assert!(rt.state.workspace_open);
        assert!(rt.find("workspace-layout-0").is_some());
        click(&mut rt, "workspace-duplicate");
        assert_eq!(rt.state.workspace.layouts.len(), 2);
        click(&mut rt, "workspace-layout-1");
        assert_eq!(rt.state.workspace.active, 1);
        // An empty name fails validation and keeps the editor open.
        rt.state.workspace.current_mut().name.clear();
        click(&mut rt, "workspace-save");
        assert!(rt.state.workspace_open);
        assert!(
            !dir.path().join("workspace.json").exists(),
            "invalid workspaces never touch disk"
        );
        // A valid save persists and closes.
        rt.state.workspace.current_mut().name = "Second".into();
        click(&mut rt, "workspace-save");
        assert!(!rt.state.workspace_open);
        assert_eq!(
            Workspace::load(&dir.path().join("workspace.json"))
                .unwrap()
                .layouts
                .len(),
            2
        );
        // Cancel restores the snapshot.
        click(&mut rt, "open-workspace");
        click(&mut rt, "workspace-duplicate");
        assert_eq!(rt.state.workspace.layouts.len(), 3);
        click(&mut rt, "workspace-cancel");
        assert_eq!(rt.state.workspace.layouts.len(), 2);
    }

    /// Frame until the keyed element's x lands, for spring-animated layout.
    fn settle_x(rt: &mut Runtime<Shell>, key: &str, target: f32) {
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
            if rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(key))
                .is_some_and(|e| (e.rect[0] - target).abs() < 1.0)
            {
                break;
            }
        }
    }

    /// Frame until the keyed element's width lands, for spring-animated layout.
    fn settle_w(rt: &mut Runtime<Shell>, key: &str, target: f32) {
        for _ in 0..240 {
            rt.frame(1.0 / 60.0);
            if rt
                .elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(key))
                .is_some_and(|e| (e.rect[2] - target).abs() < 1.0)
            {
                break;
            }
        }
    }

    #[test]
    fn workspace_layout_applies_to_shell() {
        let (_dir, mut rt) = library_harness();
        let x = |rt: &Runtime<Shell>, key: &str| rt.find(key).unwrap().0;
        assert!(
            x(&rt, "sidebar") < x(&rt, "shelf"),
            "navigation starts left"
        );
        rt.update(|s, _| s.workspace.current_mut().layout.navigation = Navigation::Right);
        settle_x(&mut rt, "shelf", 0.0);
        assert!(
            x(&rt, "sidebar") > x(&rt, "shelf"),
            "navigation moves right"
        );
        rt.update(|s, _| s.workspace.current_mut().layout.details = Details::Hidden);
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);
        assert!(rt.find("details").is_none(), "hidden details pane is gone");
        rt.update(|s, _| s.workspace.current_mut().layout.details = Details::Right);
        settle_w(&mut rt, "game-0", 180.0);
        let width = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("game-0"))
            .unwrap()
            .rect;
        assert!(
            (width[2] - 180.0).abs() < 1.0,
            "default tile width applies: {width:?}"
        );
        rt.update(|s, _| s.workspace.current_mut().layout.tile_width = 300.0);
        settle_w(&mut rt, "game-0", 300.0);
        let width = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("game-0"))
            .unwrap()
            .rect;
        assert!(
            (width[2] - 300.0).abs() < 1.0,
            "tile width applies: {width:?}"
        );
        rt.update(|s, _| s.workspace.current_mut().layout.view = View::List);
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);
        assert!(rt.find("game-0").is_some(), "list rows keep game keys");
        rt.update(|s, _| s.workspace.current_mut().layout.view = View::Shelf);
        rt.frame(1.0 / 60.0);
        rt.frame(1.0 / 60.0);
        assert!(rt.find("strip").is_some(), "shelf renders its strip");
    }
}
