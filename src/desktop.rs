//! Secure Tauri 2 bridge over the existing library/launch/store/metadata APIs.
//!
//! The always-compiled top half is pure service logic: handle-based library
//! mutations, preservation-aware import merges, and backup validation. It has
//! no GUI or Tauri types, so `cargo test --lib` exercises it without any
//! desktop dependencies. The Tauri commands and shared [`AppState`] live
//! behind `cfg(feature = "desktop")`.
//!
//! Rules the bridge follows:
//! - Games resolve through [`Game::handle`] (store identity or executable
//!   path), never through vector indices.
//! - Mutations clone under a short lock, persist inside `spawn_blocking`,
//!   then swap the in-memory state. The mutex is never held over IO/network.
//! - Executable arguments travel as `argv` vectors. There is no shell
//!   expansion and no command exposes arbitrary shell invocation.
//! - Artwork decodes only through the bounded `metadata` helpers
//!   (PNG/JPEG, 16 MiB, 4096 px, 64 MiB allocation budget).
//! - Store API keys are session-only inputs. They are never stored and never
//!   appear in errors or files.

use crate::{
    epic,
    launch::{self},
    library::{Game, Library, Target, validate_game},
    metadata::{self, ImageKind},
    steam,
    workspace::Workspace,
    xbox,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Shared shapes (frontend contract)
// ---------------------------------------------------------------------------

/// Outcome of an import scan after it merged into the library.
#[derive(Clone, Debug, Serialize)]
pub struct ImportReport {
    pub added: usize,
    pub warnings: Vec<String>,
    pub total: usize,
}

/// Outcome of removing an entry. Store-backed entries hide instead of
/// deleting, exactly like the previous shell behavior.
#[derive(Clone, Debug, Serialize)]
pub struct RemoveOutcome {
    pub removed: bool,
    pub hidden: bool,
}

/// Outcome of a launch dispatch. Success means dispatch was accepted, not
/// that the store client started the game.
#[derive(Clone, Debug, Serialize)]
pub struct LaunchReport {
    pub launched: bool,
    pub dispatched: String,
}

/// Title-search hit from the metadata provider.
#[derive(Clone, Debug, Serialize)]
pub struct MetadataMatch {
    pub app_id: u32,
    pub name: String,
}

/// One downloadable artwork candidate. The frontend passes the whole choice
/// back to `metadata_apply_image`; the backend re-validates the URL host.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageChoiceDto {
    pub label: String,
    pub url: String,
    pub source: String,
}

/// Bounded PNG thumbnail (480x720 box) encoded as base64 for `<img>` use.
#[derive(Clone, Debug, Serialize)]
pub struct ThumbnailDto {
    pub width: u32,
    pub height: u32,
    pub png_base64: String,
}

/// Result of a metadata fetch after its baseline-preserving merge.
#[derive(Clone, Debug, Serialize)]
pub struct MetadataFetchResult {
    pub game: Game,
    pub warnings: Vec<String>,
}

/// Summary of a backup restore. Reported only after every file validated.
#[derive(Clone, Debug, Serialize)]
pub struct BackupSummary {
    pub games: usize,
    pub layouts: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SteamImportInput {
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EpicImportInput {
    #[serde(default)]
    pub manifest_dir: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct XboxImportInput {
    #[serde(default)]
    pub appx_json: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MetadataFetchInput {
    pub handle: String,
    pub app_id: u32,
    #[serde(default)]
    pub replace: bool,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Portable backup snapshot. Versioned so future formats fail loudly instead
/// of restoring half-understood data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackupFile {
    pub version: u32,
    pub library: Library,
    pub workspace: Workspace,
    pub settings: metadata::Settings,
}

// ---------------------------------------------------------------------------
// Pure service (always compiled, unit-tested)
// ---------------------------------------------------------------------------

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// Build a validated executable game. Paths canonicalize at entry and the
/// launch plan validates the target, mirroring the previous add/edit form.
pub fn build_executable_game(
    name: &str,
    exe_path: &str,
    args: &[String],
    workdir: Option<&str>,
) -> Result<Game> {
    if blank(name) {
        bail!("Give the game a name");
    }
    if blank(exe_path) {
        bail!("Choose an executable file");
    }
    let path = std::fs::canonicalize(exe_path.trim())
        .with_context(|| format!("Cannot open executable: {}", exe_path.trim()))?;
    let working_directory = match workdir {
        Some(dir) if !blank(dir) => Some(
            std::fs::canonicalize(dir.trim())
                .with_context(|| format!("Cannot open working directory: {}", dir.trim()))?,
        ),
        _ => None,
    };
    let game = Game {
        metadata: Default::default(),
        name: name.trim().to_string(),
        target: Target::Executable {
            path,
            arguments: args.to_vec(),
            working_directory,
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

/// Transactional add of an executable entry with duplicate protection.
pub fn add_executable_game(
    library: &mut Library,
    name: &str,
    exe_path: &str,
    args: &[String],
    workdir: Option<&str>,
) -> Result<Game> {
    let game = build_executable_game(name, exe_path, args, workdir)?;
    library.add(game.clone())?;
    Ok(game)
}

/// Manual add of a Steam entry by app ID (imports cover the scanned path).
pub fn add_steam_game(library: &mut Library, app_id: u32, name: &str) -> Result<Game> {
    if app_id == 0 {
        bail!("Steam app ID must be positive");
    }
    if blank(name) {
        bail!("Give the game a name");
    }
    let game = Game {
        metadata: Default::default(),
        name: name.trim().to_string(),
        target: Target::Steam { app_id },
        favorite: false,
        last_launched: None,
        cover: None,
        installed: None,
        hidden: false,
    };
    library.add(game.clone())?;
    Ok(game)
}

/// Transactional edit of an executable entry. Favorites, timestamps, covers,
/// install state, visibility, and metadata survive the edit.
pub fn update_executable_game(
    library: &mut Library,
    handle: &str,
    name: &str,
    exe_path: &str,
    args: &[String],
    workdir: Option<&str>,
) -> Result<Game> {
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    let slot = library.games[index].clone();
    if !matches!(slot.target, Target::Executable { .. }) {
        bail!("Only PC executables can be edited; store entries keep their launch target");
    }
    let fresh = build_executable_game(name, exe_path, args, workdir)?;
    let mut remaining = library.clone();
    remaining.games.remove(index);
    remaining.add(fresh.clone())?;
    let merged = Game {
        favorite: slot.favorite,
        last_launched: slot.last_launched,
        cover: slot.cover,
        installed: slot.installed,
        hidden: slot.hidden,
        metadata: slot.metadata,
        ..fresh
    };
    validate_game(&merged)?;
    library.games[index] = merged.clone();
    Ok(merged)
}

/// Apply only editor-owned fields; concurrent metadata/favorite edits survive.
pub fn save_editor_game(
    library: &mut Library,
    original: Option<&str>,
    edited: Game,
) -> Result<Game> {
    let mut next = library.clone();
    let mut saved = if let Some(handle) = original {
        let index = next
            .index_of(handle)
            .context("That game is no longer in the library")?;
        match &edited.target {
            Target::Executable {
                path,
                arguments,
                working_directory,
            } => update_executable_game(
                &mut next,
                handle,
                &edited.name,
                &path.to_string_lossy(),
                arguments,
                working_directory
                    .as_ref()
                    .map(|p| p.to_string_lossy())
                    .as_deref(),
            )?,
            target if *target == next.games[index].target => {
                rename_game(&mut next, handle, &edited.name)?
            }
            _ => bail!("Store launch targets cannot be changed"),
        }
    } else {
        let Target::Executable {
            path,
            arguments,
            working_directory,
        } = &edited.target
        else {
            bail!("Use a store import to add a store game");
        };
        add_executable_game(
            &mut next,
            &edited.name,
            &path.to_string_lossy(),
            arguments,
            working_directory
                .as_ref()
                .map(|p| p.to_string_lossy())
                .as_deref(),
        )?
    };
    saved.metadata.tags = edited.metadata.tags;
    saved.metadata.series = edited.metadata.series;
    let index = next
        .index_of(&saved.handle())
        .context("Game was not saved")?;
    next.games[index] = saved.clone();
    *library = next;
    Ok(saved)
}

/// Rename any entry without touching its launch target.
pub fn rename_game(library: &mut Library, handle: &str, name: &str) -> Result<Game> {
    if blank(name) {
        bail!("Give the game a name");
    }
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    library.games[index].name = name.trim().to_string();
    validate_game(&library.games[index])?;
    Ok(library.games[index].clone())
}

pub fn set_favorite(library: &mut Library, handle: &str, favorite: bool) -> Result<Game> {
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    library.games[index].favorite = favorite;
    Ok(library.games[index].clone())
}

pub fn set_hidden(library: &mut Library, handle: &str, hidden: bool) -> Result<Game> {
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    library.games[index].hidden = hidden;
    Ok(library.games[index].clone())
}

/// Remove an entry, or hide it when it is store-backed (Steam/Epic/Xbox).
pub fn remove_game(library: &mut Library, handle: &str) -> Result<RemoveOutcome> {
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    if library.games[index].hides_instead_of_removing() {
        library.games[index].hidden = true;
        return Ok(RemoveOutcome {
            removed: false,
            hidden: true,
        });
    }
    library.games.remove(index);
    Ok(RemoveOutcome {
        removed: true,
        hidden: false,
    })
}

/// Point an entry at a local cover image. The file decodes through the same
/// bounded helpers as provider artwork, so oversized or hostile files fail
/// before anything persists.
pub fn set_cover_local(library: &mut Library, handle: &str, cover: &str) -> Result<Game> {
    if blank(cover) {
        bail!("Choose a cover image file");
    }
    let path = PathBuf::from(cover.trim());
    metadata::ui_thumbnail_from_file(&path).context("Could not use that cover image")?;
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    library.games[index].cover = Some(path);
    Ok(library.games[index].clone())
}

/// Drop a custom cover and its source attribution.
pub fn clear_cover(library: &mut Library, handle: &str) -> Result<Game> {
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    library.games[index].cover = None;
    library.games[index].metadata.sources.remove("cover");
    Ok(library.games[index].clone())
}

/// Record a Steam metadata identity on a non-Steam entry without changing
/// its launch target. Steam entries use their store ID and reject this.
pub fn set_steam_id(library: &mut Library, handle: &str, app_id: Option<u32>) -> Result<Game> {
    let index = library
        .index_of(handle)
        .context("That game is no longer in the library")?;
    if matches!(library.games[index].target, Target::Steam { .. }) {
        bail!("Steam entries already use their store ID for metadata");
    }
    if app_id.is_some_and(|id| id == 0) {
        bail!("Steam app ID must be positive");
    }
    library.games[index].metadata.steam_app_id = app_id;
    Ok(library.games[index].clone())
}

/// Resolve which artwork file a thumbnail request means.
pub fn artwork_path(game: &Game, kind: &str) -> Result<Option<PathBuf>> {
    match kind.trim().to_lowercase().as_str() {
        "cover" => Ok(game.cover.clone()),
        "background" => Ok(game.metadata.background.clone()),
        "logo" => Ok(game.metadata.logo.clone()),
        _ => bail!("Unknown artwork field; use cover, background, or logo"),
    }
}

pub fn parse_image_kind(kind: &str) -> Result<ImageKind> {
    match kind.trim().to_lowercase().as_str() {
        "cover" => Ok(ImageKind::Cover),
        "background" => Ok(ImageKind::Background),
        "logo" => Ok(ImageKind::Logo),
        _ => bail!("Unknown artwork field; use cover, background, or logo"),
    }
}

pub fn parse_artwork_provider(name: &str) -> Result<metadata::ArtworkProvider> {
    match name.trim().to_lowercase().as_str() {
        "steam" => Ok(metadata::ArtworkProvider::Steam),
        "steamgriddb" | "steamgrid_db" | "steamgrid-db" | "sgdb" => {
            Ok(metadata::ArtworkProvider::SteamGridDb)
        }
        _ => bail!("Unknown artwork provider; use steam or steamgriddb"),
    }
}

// --- Preservation-aware import merges (behavior carried over from the shell) ---

fn steam_handle(app_id: u32) -> String {
    format!("steam:{app_id}")
}

/// Fold a Steam scan into the library. A clean scan refreshes installed
/// flags; a warned scan preserves them conservatively. Names, favorites,
/// and other user data are never overwritten.
pub fn merge_steam_import(library: &mut Library, imported: &steam::ImportResult) -> Result<usize> {
    let mut added = 0;
    if imported.warnings.is_empty() {
        for game in &mut library.games {
            if matches!(game.target, Target::Steam { .. }) {
                game.installed = Some(false);
            }
        }
    }
    for game in &imported.games {
        let handle = steam_handle(game.app_id);
        if let Some(existing) = library.games.iter_mut().find(|g| g.handle() == handle) {
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

/// Fold an Epic scan into the library. Matching is case-insensitive on the
/// app name so a case-variant reimport updates the entry instead of failing
/// as a duplicate.
pub fn merge_epic_import(library: &mut Library, imported: &epic::ImportResult) -> Result<usize> {
    let mut added = 0;
    if imported.warnings.is_empty() {
        for game in &mut library.games {
            if matches!(game.target, Target::Epic { .. }) {
                game.installed = Some(false);
            }
        }
    }
    for game in &imported.games {
        let handle = format!("epic:{}", game.app_name.to_lowercase());
        if let Some(existing) = library.games.iter_mut().find(|g| g.handle() == handle) {
            if imported.warnings.is_empty() {
                existing.installed = Some(true);
            }
            continue;
        }
        library.add(Game {
            metadata: Default::default(),
            name: game.display_name.clone(),
            target: Target::Epic {
                app_name: game.app_name.clone(),
                catalog_namespace: game.catalog_namespace.clone(),
                catalog_item_id: game.catalog_item_id.clone(),
            },
            favorite: false,
            last_launched: None,
            cover: None,
            installed: Some(true),
            hidden: false,
        })?;
        added += 1;
    }
    Ok(added)
}

/// Fold an Xbox scan into the library. Matching is case-insensitive on the
/// package moniker for the same reason as Epic.
pub fn merge_xbox_import(library: &mut Library, imported: &xbox::ImportResult) -> Result<usize> {
    let mut added = 0;
    if imported.warnings.is_empty() {
        for game in &mut library.games {
            if matches!(game.target, Target::Xbox { .. }) {
                game.installed = Some(false);
            }
        }
    }
    for game in &imported.games {
        let handle = format!("xbox:{}", game.package_id.to_lowercase());
        if let Some(existing) = library.games.iter_mut().find(|g| g.handle() == handle) {
            if imported.warnings.is_empty() {
                existing.installed = Some(true);
            }
            continue;
        }
        library.add(Game {
            metadata: Default::default(),
            name: game.name.clone(),
            target: Target::Xbox {
                package_id: game.package_id.clone(),
            },
            favorite: false,
            last_launched: None,
            cover: None,
            installed: Some(true),
            hidden: false,
        })?;
        added += 1;
    }
    Ok(added)
}

/// Attach a requested metadata match without changing launch identity or
/// overwriting a match chosen while the download was in flight.
pub fn apply_metadata_result(
    library: &mut Library,
    baseline: &Game,
    app_id: u32,
    fetched: metadata::Fetched,
    replace: bool,
) -> Result<Game> {
    if app_id == 0 {
        bail!("Steam app ID must be positive");
    }
    if let Target::Steam { app_id: existing } = baseline.target
        && existing != app_id
    {
        bail!("Steam entries keep their own metadata identity");
    }
    let index = library
        .index_of(&baseline.handle())
        .context("That game is no longer in the library")?;
    let game = &mut library.games[index];
    if game.target != baseline.target || metadata::app_id(game) != metadata::app_id(baseline) {
        bail!("Game metadata match changed while fetching; try again");
    }
    let mut matching_baseline = baseline.clone();
    if !matches!(game.target, Target::Steam { .. }) {
        matching_baseline.metadata.steam_app_id = Some(app_id);
        game.metadata.steam_app_id = Some(app_id);
    }
    metadata::apply(game, &matching_baseline, fetched, replace);
    Ok(game.clone())
}

// --- Backup validation (pure; file writes live in the Tauri commands) ---

/// Parse and validate a backup snapshot. Rejects damaged or future data
/// without touching any live file.
pub fn parse_backup(text: &str) -> Result<BackupFile> {
    let backup: BackupFile =
        serde_json::from_str(text).context("Backup is not a valid launcher backup")?;
    if backup.version != 1 {
        bail!(
            "Unsupported backup version {}; file has not been applied",
            backup.version
        );
    }
    if backup.library.schema_version != 1 {
        bail!("Unsupported library version in backup; file has not been applied");
    }
    let mut checked = Library::default();
    for game in &backup.library.games {
        checked.add(game.clone())?;
    }
    backup.workspace.validate()?;
    Ok(backup)
}

/// Stage every restore file before replacing any of them. On an IO failure,
/// roll back replacements already made; the library file is committed last.
pub fn restore_backup_files(data: &std::path::Path, backup: &BackupFile) -> Result<()> {
    let values = vec![
        (
            "workspace.json",
            serde_json::to_vec_pretty(&backup.workspace)?,
        ),
        (
            "metadata.json",
            serde_json::to_vec_pretty(&backup.settings)?,
        ),
        ("library.json", serde_json::to_vec_pretty(&backup.library)?),
    ];
    replace_state_files(data, values)
}

fn replace_state_files(data: &std::path::Path, values: Vec<(&str, Vec<u8>)>) -> Result<()> {
    use std::io::Write;
    let mut staged = Vec::new();
    for (name, bytes) in values {
        let path = data.join(name);
        let old = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Cannot prepare {} for restore", path.display()));
            }
        };
        let mut temp = tempfile::NamedTempFile::new_in(data)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        staged.push((path, old, temp));
    }
    let mut replaced: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
    for (path, old, temp) in staged {
        if let Err(error) = temp.persist(&path) {
            let mut rollback_errors = Vec::new();
            for (previous, bytes) in replaced.into_iter().rev() {
                let result = (|| -> Result<()> {
                    if let Some(bytes) = bytes {
                        let mut temp = tempfile::NamedTempFile::new_in(data)?;
                        temp.write_all(&bytes)?;
                        temp.as_file().sync_all()?;
                        temp.persist(&previous)?;
                    } else {
                        std::fs::remove_file(&previous)?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    rollback_errors.push(format!("{}: {error:#}", previous.display()));
                }
            }
            bail!(
                "Restore failed at {}: {error}. Rollback errors: {}",
                path.display(),
                rollback_errors.join("; ")
            );
        }
        replaced.push((path, old));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests for the desktop service (run without GUI dependencies)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn exe_fixture(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "fixture").unwrap();
        std::fs::canonicalize(&path).unwrap()
    }

    fn exe_library() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let exe = exe_fixture(dir.path(), "game.exe");
        let mut library = Library::default();
        library
            .add(Game {
                metadata: Default::default(),
                name: "Game".into(),
                target: Target::Executable {
                    path: exe,
                    arguments: vec!["--level".into(), "1".into()],
                    working_directory: None,
                },
                favorite: true,
                last_launched: Some(42),
                cover: None,
                installed: None,
                hidden: false,
            })
            .unwrap();
        (dir, library)
    }

    #[test]
    fn new_metadata_match_fills_fields_and_late_rematches_are_rejected() {
        let (_dir, mut library) = exe_library();
        let baseline = library.games[0].clone();
        let fetched = metadata::Fetched {
            metadata: metadata::GameMetadata {
                description: "Fetched description".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        library.games[0].metadata.tags = vec!["Edited while fetching".into()];
        let saved =
            apply_metadata_result(&mut library, &baseline, 620, fetched.clone(), false).unwrap();
        assert_eq!(saved.metadata.steam_app_id, Some(620));
        assert_eq!(saved.metadata.description, "Fetched description");
        assert_eq!(saved.target, baseline.target);
        assert_eq!(saved.metadata.tags, vec!["Edited while fetching"]);
        let before = library.clone();
        assert!(apply_metadata_result(&mut library, &baseline, 730, fetched, true).is_err());
        assert_eq!(library, before);
    }

    #[test]
    fn restore_preparation_failure_leaves_existing_library_untouched() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("library.json"), "original library").unwrap();
        fs::write(dir.path().join("workspace.json"), "original workspace").unwrap();
        fs::create_dir(dir.path().join("metadata.json")).unwrap();
        let backup = BackupFile {
            version: 1,
            library: Library::default(),
            workspace: Workspace::default(),
            settings: metadata::Settings::default(),
        };
        assert!(restore_backup_files(dir.path(), &backup).is_err());
        assert_eq!(
            fs::read_to_string(dir.path().join("library.json")).unwrap(),
            "original library"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("workspace.json")).unwrap(),
            "original workspace"
        );
    }

    #[test]
    fn editor_saves_preserve_concurrent_data_and_return_canonical_identity() {
        let (_dir, mut library) = exe_library();
        let original = library.games[0].clone();
        let mut edited = original.clone();
        edited.name = "Edited title".into();
        edited.metadata.tags = vec!["Backlog".into()];
        library.games[0].metadata.description = "Arrived while editing".into();
        library.games[0].favorite = false;
        let saved = save_editor_game(&mut library, Some(&original.handle()), edited).unwrap();
        assert_eq!(saved.metadata.description, "Arrived while editing");
        assert!(!saved.favorite);
        assert_eq!(saved.metadata.tags, vec!["Backlog"]);
        assert_eq!(saved.handle(), original.handle());
        assert_eq!(library.games[0], saved);
    }

    #[test]
    fn handles_are_stable_unique_and_case_aware() {
        let steam = Game {
            metadata: Default::default(),
            name: "S".into(),
            target: Target::Steam { app_id: 10 },
            favorite: false,
            last_launched: None,
            cover: None,
            installed: None,
            hidden: false,
        };
        assert_eq!(steam.handle(), "steam:10");
        let epic = |app: &str| Game {
            metadata: Default::default(),
            name: "E".into(),
            target: Target::Epic {
                app_name: app.into(),
                catalog_namespace: String::new(),
                catalog_item_id: String::new(),
            },
            favorite: false,
            last_launched: None,
            cover: None,
            installed: None,
            hidden: false,
        };
        assert_eq!(epic("Fortnite").handle(), epic("FORTNITE").handle());
        let library = Library {
            schema_version: 1,
            games: vec![steam.clone(), epic("Fortnite")],
        };
        assert_eq!(library.index_of("steam:10"), Some(0));
        assert_eq!(library.index_of("epic:fortnite"), Some(1));
        assert_eq!(library.index_of("steam:11"), None);
    }

    #[test]
    fn add_rejects_duplicates_and_bad_targets() {
        let dir = tempfile::tempdir().unwrap();
        let exe = exe_fixture(dir.path(), "game.exe");
        let mut library = Library::default();
        add_executable_game(
            &mut library,
            "Game",
            &exe.to_string_lossy(),
            &["a b".to_string()],
            None,
        )
        .unwrap();
        assert_eq!(library.games.len(), 1);
        assert!(library.games[0].handle().starts_with("exe:"));
        assert!(
            add_executable_game(&mut library, "Copy", &exe.to_string_lossy(), &[], None).is_err()
        );
        assert_eq!(library.games.len(), 1);
        assert!(
            add_executable_game(&mut library, "  ", &exe.to_string_lossy(), &[], None).is_err()
        );
        assert!(
            add_executable_game(&mut library, "Ghost", "/definitely/missing.exe", &[], None)
                .is_err()
        );
        assert!(add_steam_game(&mut library, 10, "Steam game").is_ok());
        assert!(add_steam_game(&mut library, 10, "Steam copy").is_err());
        assert!(add_steam_game(&mut library, 0, "Zero").is_err());
    }

    #[test]
    fn update_preserves_user_data_and_rejects_duplicates() {
        let (_tmp, mut library) = exe_library();
        let handle = library.games[0].handle();
        let dir = tempfile::tempdir().unwrap();
        let other = exe_fixture(dir.path(), "other.exe");
        let updated = update_executable_game(
            &mut library,
            &handle,
            "Renamed",
            &other.to_string_lossy(),
            &["--new".to_string()],
            None,
        )
        .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert!(updated.favorite, "favorites survive edits");
        assert_eq!(updated.last_launched, Some(42), "timestamps survive edits");
        assert_eq!(
            updated.target,
            Target::Executable {
                path: std::fs::canonicalize(&other).unwrap(),
                arguments: vec!["--new".to_string()],
                working_directory: None,
            }
        );
    }

    #[test]
    fn remove_hides_store_entries_and_deletes_executables() {
        let (_tmp, mut library) = exe_library();
        let exe_handle = library.games[0].handle();
        library
            .add(Game {
                metadata: Default::default(),
                name: "Steam game".into(),
                target: Target::Steam { app_id: 7 },
                favorite: true,
                last_launched: None,
                cover: None,
                installed: None,
                hidden: false,
            })
            .unwrap();
        let steam_handle = "steam:7".to_string();
        let outcome = remove_game(&mut library, &steam_handle).unwrap();
        assert!(!outcome.removed && outcome.hidden);
        assert!(
            library
                .games
                .iter()
                .any(|g| g.handle() == steam_handle && g.hidden)
        );
        assert!(
            library
                .games
                .iter()
                .find(|g| g.handle() == steam_handle)
                .unwrap()
                .favorite
        );
        let outcome = remove_game(&mut library, &exe_handle).unwrap();
        assert!(outcome.removed && !outcome.hidden);
        assert!(library.index_of(&exe_handle).is_none());
        assert!(remove_game(&mut library, &exe_handle).is_err());
    }

    #[test]
    fn steam_merge_refreshes_clean_and_preserves_warned() {
        let mut library = Library::default();
        library
            .add(Game {
                metadata: Default::default(),
                name: "My Name".into(),
                target: Target::Steam { app_id: 1 },
                favorite: true,
                last_launched: None,
                cover: None,
                installed: Some(true),
                hidden: false,
            })
            .unwrap();
        // Warned scan: existing flags stay, new games still land.
        let warned = steam::ImportResult {
            games: vec![steam::SteamGame {
                app_id: 1,
                name: "Upstream Name".into(),
                installed: false,
                cover: None,
            }],
            warnings: vec!["one manifest skipped".into()],
        };
        assert_eq!(merge_steam_import(&mut library, &warned).unwrap(), 0);
        let game = &library.games[0];
        assert_eq!(game.name, "My Name", "imports never rename");
        assert!(game.favorite);
        assert_eq!(game.installed, Some(true), "warned scans preserve flags");
        // Clean scan: flags refresh, missing covers fill, new entries arrive.
        let dir = tempfile::tempdir().unwrap();
        let cover = dir.path().join("cover.png");
        fs::write(&cover, "placeholder").unwrap();
        let clean = steam::ImportResult {
            games: vec![
                steam::SteamGame {
                    app_id: 1,
                    name: "Upstream Name".into(),
                    installed: true,
                    cover: Some(cover.clone()),
                },
                steam::SteamGame {
                    app_id: 2,
                    name: "New".into(),
                    installed: false,
                    cover: None,
                },
            ],
            warnings: vec![],
        };
        assert_eq!(merge_steam_import(&mut library, &clean).unwrap(), 1);
        let game = library
            .games
            .iter()
            .find(|g| g.handle() == "steam:1")
            .unwrap();
        assert_eq!(game.installed, Some(true));
        assert_eq!(game.cover, Some(cover));
    }

    #[test]
    fn epic_merge_matches_case_insensitively() {
        let mut library = Library::default();
        library
            .add(Game {
                metadata: Default::default(),
                name: "Fortnite".into(),
                target: Target::Epic {
                    app_name: "Fortnite".into(),
                    catalog_namespace: "fn".into(),
                    catalog_item_id: "abc".into(),
                },
                favorite: false,
                last_launched: None,
                cover: None,
                installed: Some(false),
                hidden: false,
            })
            .unwrap();
        let imported = epic::ImportResult {
            games: vec![epic::EpicGame {
                app_name: "FORTNITE".into(),
                display_name: "Fortnite".into(),
                install_location: PathBuf::from("/games/fn"),
                catalog_namespace: "fn".into(),
                catalog_item_id: "abc".into(),
            }],
            warnings: vec![],
        };
        assert_eq!(merge_epic_import(&mut library, &imported).unwrap(), 0);
        assert_eq!(library.games.len(), 1);
        assert_eq!(library.games[0].installed, Some(true));
    }

    #[test]
    fn backup_validation_rejects_damage_without_side_effects() {
        let good = serde_json::json!({
            "version": 1,
            "library": {"schema_version": 1, "games": []},
            "workspace": Workspace::default(),
            "settings": metadata::Settings::default(),
        });
        let backup = parse_backup(&serde_json::to_string(&good).unwrap()).unwrap();
        assert_eq!(backup.library.games.len(), 0);
        for text in [
            "broken",
            &serde_json::to_string(&serde_json::json!({
                "version": 2, "library": {"schema_version": 1, "games": []},
                "workspace": Workspace::default(), "settings": metadata::Settings::default(),
            }))
            .unwrap(),
            &serde_json::to_string(&serde_json::json!({
                "version": 1,
                "library": {"schema_version": 1, "games": [{"name": "", "target": {"kind": "steam", "app_id": 1}}]},
                "workspace": Workspace::default(), "settings": metadata::Settings::default(),
            }))
            .unwrap(),
        ] {
            assert!(parse_backup(text).is_err());
        }
    }

    #[test]
    fn cover_paths_reject_undecodable_files() {
        let (_tmp, mut library) = exe_library();
        let handle = library.games[0].handle();
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("cover.png");
        fs::write(&bad, "not an image").unwrap();
        assert!(set_cover_local(&mut library, &handle, bad.to_str().unwrap()).is_err());
        assert!(library.games[0].cover.is_none());
    }
}

// ---------------------------------------------------------------------------
// Tauri bridge (desktop feature only)
// ---------------------------------------------------------------------------

#[cfg(feature = "desktop")]
pub use bridge::*;
#[cfg(feature = "desktop")]
mod bridge {
    use super::*;
    use crate::launch::LaunchPlan;
    use std::{
        path::Path,
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tauri::State;

    fn fail(error: impl std::fmt::Display) -> String {
        format!("{error:#}")
    }

    /// Paths for every on-disk file. The data directory is exactly
    /// `ProjectDirs::from("", "", "TheLauncher")`, unchanged from before.
    #[derive(Clone, Debug)]
    pub struct AppPaths {
        pub data_dir: PathBuf,
        pub library: PathBuf,
        pub workspace: PathBuf,
        pub settings: PathBuf,
        pub artwork: PathBuf,
    }

    impl AppPaths {
        pub fn new(data_dir: PathBuf) -> Self {
            Self {
                library: data_dir.join("library.json"),
                workspace: data_dir.join("workspace.json"),
                settings: data_dir.join("metadata.json"),
                artwork: data_dir.join("artwork"),
                data_dir,
            }
        }
    }

    struct StateInner {
        library: Library,
        workspace: Workspace,
        settings: metadata::Settings,
        startup_warnings: Vec<String>,
    }

    /// Shared Tauri state. Every command clones under a short lock and does
    /// IO/network inside `spawn_blocking`, never while the mutex is held.
    pub struct AppState {
        pub paths: AppPaths,
        inner: Mutex<StateInner>,
        write_gate: tauri::async_runtime::Mutex<()>,
    }

    /// Load persisted state at startup. A damaged or future `library.json`
    /// is fatal and the file is never overwritten; damaged workspace or
    /// settings files fall back to temporary defaults with a warning, and
    /// their files are left unchanged until an explicit save.
    pub fn load_app_state(data_dir: &Path) -> Result<AppState> {
        let paths = AppPaths::new(data_dir.to_path_buf());
        let library = Library::load(&paths.library)
            .with_context(|| format!("Unable to load {}", paths.library.display()))?;
        let mut startup_warnings = Vec::new();
        let workspace = match Workspace::load(&paths.workspace) {
            Ok(workspace) => workspace,
            Err(error) => {
                startup_warnings.push(format!(
                    "Workspace unavailable ({error:#}); using a temporary default"
                ));
                Workspace::default()
            }
        };
        let settings = match metadata::Settings::load(&paths.settings) {
            Ok(settings) => settings,
            Err(error) => {
                startup_warnings.push(format!(
                    "Metadata settings unavailable ({error:#}); using temporary defaults"
                ));
                metadata::Settings::default()
            }
        };
        Ok(AppState {
            paths,
            write_gate: tauri::async_runtime::Mutex::new(()),
            inner: Mutex::new(StateInner {
                library,
                workspace,
                settings,
                startup_warnings,
            }),
        })
    }

    impl AppState {
        pub fn startup_warnings(&self) -> Vec<String> {
            self.inner
                .lock()
                .map(|inner| inner.startup_warnings.clone())
                .unwrap_or_default()
        }
    }

    fn snapshot(state: &AppState) -> Result<(Library, AppPaths), String> {
        let inner = state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?;
        Ok((inner.library.clone(), state.paths.clone()))
    }

    /// Apply `edit` to a clone inside blocking IO, persist it transactionally,
    /// then publish the result to memory. Returns the persisted library.
    async fn persist_library_result<T: Send + 'static>(
        state: &AppState,
        edit: impl FnOnce(&mut Library) -> Result<T> + Send + 'static,
    ) -> Result<(Library, T), String> {
        let _write = state.write_gate.lock().await;
        let (snapshot, paths) = snapshot(state)?;
        let (next, result) =
            tauri::async_runtime::spawn_blocking(move || -> Result<(Library, T)> {
                let mut next = snapshot;
                let result = next.change(&paths.library, edit)?;
                Ok((next, result))
            })
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?
            .library = next.clone();
        Ok((next, result))
    }

    async fn persist_library(
        state: &AppState,
        edit: impl FnOnce(&mut Library) -> Result<()> + Send + 'static,
    ) -> Result<Library, String> {
        persist_library_result(state, edit)
            .await
            .map(|(library, ())| library)
    }

    fn settled_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn opt_input(value: Option<String>) -> Option<String> {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    // --- Library queries (memory-only) ---

    #[tauri::command]
    pub fn library_load(state: State<'_, AppState>) -> Result<Vec<Game>, String> {
        state
            .inner
            .lock()
            .map(|inner| inner.library.games.clone())
            .map_err(|_| "Application state is unavailable".to_string())
    }

    #[tauri::command]
    pub fn startup_warnings(state: State<'_, AppState>) -> Vec<String> {
        state
            .inner
            .lock()
            .map(|inner| inner.startup_warnings.clone())
            .unwrap_or_default()
    }

    #[tauri::command]
    pub async fn library_save_game(
        state: State<'_, AppState>,
        game: Game,
        original: Option<String>,
    ) -> Result<Game, String> {
        persist_library_result(&state, move |library| {
            save_editor_game(library, original.as_deref(), game)
        })
        .await
        .map(|(_, game)| game)
    }

    // --- Validated transactional edits ---

    #[tauri::command]
    pub async fn library_set_favorite(
        state: State<'_, AppState>,
        handle: String,
        favorite: bool,
    ) -> Result<Game, String> {
        let lookup = handle.clone();
        let next = persist_library(&state, move |library| {
            set_favorite(library, &handle, favorite)?;
            Ok(())
        })
        .await?;
        next.games
            .into_iter()
            .find(|game| game.handle() == lookup)
            .context("That game is no longer in the library")
            .map_err(fail)
    }

    #[tauri::command]
    pub async fn library_set_hidden(
        state: State<'_, AppState>,
        handle: String,
        hidden: bool,
    ) -> Result<Game, String> {
        let lookup = handle.clone();
        let next = persist_library(&state, move |library| {
            set_hidden(library, &handle, hidden)?;
            Ok(())
        })
        .await?;
        next.games
            .into_iter()
            .find(|game| game.handle() == lookup)
            .context("That game is no longer in the library")
            .map_err(fail)
    }

    #[tauri::command]
    pub async fn library_remove(
        state: State<'_, AppState>,
        handle: String,
    ) -> Result<RemoveOutcome, String> {
        let _write = state.write_gate.lock().await;
        let (snapshot, paths) = snapshot(&state)?;
        let outcome =
            tauri::async_runtime::spawn_blocking(move || -> Result<(Library, RemoveOutcome)> {
                let mut next = snapshot;
                let outcome = remove_game(&mut next, &handle)?;
                next.save(&paths.library)?;
                Ok((next, outcome))
            })
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?
            .library = outcome.0.clone();
        Ok(outcome.1)
    }

    // --- Covers and artwork ---

    #[tauri::command]
    pub async fn cover_set_local(
        state: State<'_, AppState>,
        handle: String,
        path: String,
    ) -> Result<Game, String> {
        let lookup = handle.clone();
        let next = persist_library(&state, move |library| {
            set_cover_local(library, &handle, &path)?;
            Ok(())
        })
        .await?;
        next.games
            .into_iter()
            .find(|game| game.handle() == lookup)
            .context("That game is no longer in the library")
            .map_err(fail)
    }

    #[tauri::command]
    pub async fn cover_remove(state: State<'_, AppState>, handle: String) -> Result<Game, String> {
        let lookup = handle.clone();
        let next = persist_library(&state, move |library| {
            clear_cover(library, &handle)?;
            Ok(())
        })
        .await?;
        next.games
            .into_iter()
            .find(|game| game.handle() == lookup)
            .context("That game is no longer in the library")
            .map_err(fail)
    }

    /// Bounded artwork thumbnail for the frontend. Reads only the stored
    /// cover/background/logo path and decodes through the shared limits;
    /// missing art yields `None` so the UI keeps its generated fallback.
    #[tauri::command]
    pub async fn artwork_thumbnail(
        state: State<'_, AppState>,
        handle: String,
        kind: String,
    ) -> Result<Option<ThumbnailDto>, String> {
        let path = {
            let inner = state
                .inner
                .lock()
                .map_err(|_| "Application state is unavailable".to_string())?;
            let game = inner
                .library
                .games
                .iter()
                .find(|game| game.handle() == handle)
                .context("That game is no longer in the library")
                .map_err(fail)?
                .clone();
            artwork_path(&game, &kind).map_err(fail)?
        };
        let Some(path) = path else { return Ok(None) };
        tauri::async_runtime::spawn_blocking(move || -> Result<Option<ThumbnailDto>> {
            let thumb = metadata::ui_thumbnail_from_file(&path)?;
            let mut png = Vec::new();
            {
                use image::ImageEncoder;
                image::codecs::png::PngEncoder::new(&mut png)
                    .write_image(
                        &thumb.rgba,
                        thumb.width,
                        thumb.height,
                        image::ExtendedColorType::Rgba8,
                    )
                    .context("Could not encode artwork thumbnail")?;
            }
            use base64::Engine as _;
            Ok(Some(ThumbnailDto {
                width: thumb.width,
                height: thumb.height,
                png_base64: base64::engine::general_purpose::STANDARD.encode(&png),
            }))
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)
    }

    #[tauri::command]
    pub async fn artwork_preview(
        state: State<'_, AppState>,
        choice: ImageChoiceDto,
    ) -> Result<ThumbnailDto, String> {
        let cache = state.paths.artwork.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<ThumbnailDto> {
            let client = metadata::Provider::new()?;
            let path = client.download(
                &metadata::ImageChoice {
                    label: choice.label,
                    url: choice.url,
                    source: choice.source,
                },
                &cache,
            )?;
            let thumb = metadata::ui_thumbnail_from_file(&path)?;
            use base64::Engine as _;
            use image::ImageEncoder;
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png).write_image(
                &thumb.rgba,
                thumb.width,
                thumb.height,
                image::ExtendedColorType::Rgba8,
            )?;
            Ok(ThumbnailDto {
                width: thumb.width,
                height: thumb.height,
                png_base64: base64::engine::general_purpose::STANDARD.encode(&png),
            })
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)
    }

    // --- Launch ---

    #[tauri::command]
    pub async fn game_launch(
        state: State<'_, AppState>,
        handle: String,
    ) -> Result<LaunchReport, String> {
        let game = {
            let inner = state
                .inner
                .lock()
                .map_err(|_| "Application state is unavailable".to_string())?;
            inner
                .library
                .games
                .iter()
                .find(|game| game.handle() == handle)
                .cloned()
                .context("That game is no longer in the library")
                .map_err(fail)?
        };
        let installation =
            matches!(game.target, Target::Steam { .. }) && game.installed == Some(false);
        let dispatched = tauri::async_runtime::spawn_blocking(move || -> Result<String> {
            let plan = launch::plan(&game)?;
            let label = match &plan {
                LaunchPlan::Executable { path, .. } => path.to_string_lossy().into_owned(),
                LaunchPlan::Steam(uri) | LaunchPlan::Epic(uri) => uri.clone(),
                LaunchPlan::Xbox(moniker) => moniker.clone(),
            };
            // Reap executable children on a detached thread so the launcher
            // never collects zombies and never waits on games.
            // Protocol-handler launches need no reaping.
            if let Some(mut child) = launch::launch(&game)? {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Ok(label)
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        if installation {
            return Ok(LaunchReport {
                launched: false,
                dispatched,
            });
        }
        let now = settled_now();
        persist_library(&state, move |library| {
            let Some(index) = library.index_of(&handle) else {
                bail!("That game is no longer in the library");
            };
            library.games[index].last_launched = Some(now);
            Ok(())
        })
        .await
        .map_err(|error| format!("Game started, but the timestamp did not save: {error}"))?;
        Ok(LaunchReport {
            launched: true,
            dispatched,
        })
    }

    // --- Store imports (blocking scans off the async runtime) ---

    #[tauri::command]
    pub async fn import_steam(
        state: State<'_, AppState>,
        input: SteamImportInput,
    ) -> Result<ImportReport, String> {
        let options = steam::ImportOptions {
            root: opt_input(input.root).map(PathBuf::from),
            account_id: opt_input(input.account_id),
            api_key: opt_input(input.api_key),
        };
        let imported = tauri::async_runtime::spawn_blocking(|| steam::import(options))
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        let warnings = imported.warnings.clone();
        let _write = state.write_gate.lock().await;
        let (snapshot, paths) = snapshot(&state)?;
        let (next, added) =
            tauri::async_runtime::spawn_blocking(move || -> Result<(Library, usize)> {
                let mut next = snapshot;
                let added = merge_steam_import(&mut next, &imported)?;
                next.save(&paths.library)?;
                Ok((next, added))
            })
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        let total = next.games.len();
        state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?
            .library = next;
        Ok(ImportReport {
            added,
            warnings,
            total,
        })
    }

    #[tauri::command]
    pub async fn import_epic(
        state: State<'_, AppState>,
        input: EpicImportInput,
    ) -> Result<ImportReport, String> {
        let options = epic::ImportOptions {
            manifest_dir: opt_input(input.manifest_dir).map(PathBuf::from),
        };
        let imported = tauri::async_runtime::spawn_blocking(|| epic::import(options))
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        let warnings = imported.warnings.clone();
        let _write = state.write_gate.lock().await;
        let (snapshot, paths) = snapshot(&state)?;
        let (next, added) =
            tauri::async_runtime::spawn_blocking(move || -> Result<(Library, usize)> {
                let mut next = snapshot;
                let added = merge_epic_import(&mut next, &imported)?;
                next.save(&paths.library)?;
                Ok((next, added))
            })
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        let total = next.games.len();
        state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?
            .library = next;
        Ok(ImportReport {
            added,
            warnings,
            total,
        })
    }

    #[tauri::command]
    pub async fn import_xbox(
        state: State<'_, AppState>,
        input: XboxImportInput,
    ) -> Result<ImportReport, String> {
        let options = xbox::ImportOptions {
            appx_json: opt_input(input.appx_json).map(PathBuf::from),
            raw_appx_json: None,
        };
        let imported = tauri::async_runtime::spawn_blocking(|| xbox::import(options))
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        let warnings = imported.warnings.clone();
        let _write = state.write_gate.lock().await;
        let (snapshot, paths) = snapshot(&state)?;
        let (next, added) =
            tauri::async_runtime::spawn_blocking(move || -> Result<(Library, usize)> {
                let mut next = snapshot;
                let added = merge_xbox_import(&mut next, &imported)?;
                next.save(&paths.library)?;
                Ok((next, added))
            })
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)?;
        let total = next.games.len();
        state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?
            .library = next;
        Ok(ImportReport {
            added,
            warnings,
            total,
        })
    }

    // --- Metadata ---

    #[tauri::command]
    pub async fn metadata_search(query: String) -> Result<Vec<MetadataMatch>, String> {
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<MetadataMatch>> {
            let provider = metadata::Provider::new()?;
            Ok(provider
                .search(&query)?
                .into_iter()
                .map(|hit| MetadataMatch {
                    app_id: hit.app_id,
                    name: hit.name,
                })
                .collect())
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)
    }

    #[tauri::command]
    pub async fn metadata_image_choices(
        app_id: u32,
        kind: String,
        provider: String,
        api_key: Option<String>,
    ) -> Result<Vec<ImageChoiceDto>, String> {
        let kind = parse_image_kind(&kind).map_err(fail)?;
        let provider = parse_artwork_provider(&provider).map_err(fail)?;
        let key = opt_input(api_key).unwrap_or_default();
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<ImageChoiceDto>> {
            let client = metadata::Provider::new()?;
            Ok(client
                .images(app_id, kind, provider, &key)?
                .into_iter()
                .map(|choice| ImageChoiceDto {
                    label: choice.label,
                    url: choice.url,
                    source: choice.source,
                })
                .collect())
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)
    }

    #[tauri::command]
    pub async fn metadata_fetch(
        state: State<'_, AppState>,
        input: MetadataFetchInput,
    ) -> Result<MetadataFetchResult, String> {
        if input.app_id == 0 {
            return Err("Steam app ID must be positive".to_string());
        }
        let (baseline, settings, cache) = {
            let inner = state
                .inner
                .lock()
                .map_err(|_| "Application state is unavailable".to_string())?;
            let game = inner
                .library
                .games
                .iter()
                .find(|game| game.handle() == input.handle)
                .cloned()
                .context("That game is no longer in the library")
                .map_err(fail)?;
            (
                game.clone(),
                inner.settings.for_game(&game, input.replace),
                state.paths.artwork.clone(),
            )
        };
        let key = opt_input(input.api_key).unwrap_or_default();
        let app_id = input.app_id;
        let replace = input.replace;
        let fetched = tauri::async_runtime::spawn_blocking(move || -> Result<metadata::Fetched> {
            let client = metadata::Provider::new()?;
            client.fetch(app_id, &settings, &key, &cache)
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        let warnings = fetched.warnings.clone();
        let (_, game) = persist_library_result(&state, move |library| {
            apply_metadata_result(library, &baseline, app_id, fetched, replace)
        })
        .await?;
        Ok(MetadataFetchResult { game, warnings })
    }

    #[tauri::command]
    pub async fn metadata_apply_image(
        state: State<'_, AppState>,
        handle: String,
        kind: String,
        choice: ImageChoiceDto,
    ) -> Result<Game, String> {
        let kind = parse_image_kind(&kind).map_err(fail)?;
        let cache = state.paths.artwork.clone();
        let source = choice.source.clone();
        let stored = tauri::async_runtime::spawn_blocking(move || -> Result<PathBuf> {
            let client = metadata::Provider::new()?;
            client.download(
                &metadata::ImageChoice {
                    label: choice.label.clone(),
                    url: choice.url.clone(),
                    source: choice.source.clone(),
                },
                &cache,
            )
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        let lookup = handle.clone();
        let next = persist_library(&state, move |library| {
            let Some(game) = library
                .games
                .iter_mut()
                .find(|game| game.handle() == handle)
            else {
                bail!("That game is no longer in the library");
            };
            match kind {
                ImageKind::Cover => game.cover = Some(stored.clone()),
                ImageKind::Background => game.metadata.background = Some(stored.clone()),
                ImageKind::Logo => game.metadata.logo = Some(stored.clone()),
            }
            game.metadata
                .sources
                .insert(kind.field().into(), source.clone());
            Ok(())
        })
        .await?;
        next.games
            .into_iter()
            .find(|game| game.handle() == lookup)
            .context("That game is no longer in the library")
            .map_err(fail)
    }

    // --- Workspace and settings ---

    #[tauri::command]
    pub fn workspace_load(state: State<'_, AppState>) -> Result<Workspace, String> {
        state
            .inner
            .lock()
            .map(|inner| inner.workspace.clone())
            .map_err(|_| "Application state is unavailable".to_string())
    }

    #[tauri::command]
    pub async fn workspace_save(
        state: State<'_, AppState>,
        workspace: Workspace,
    ) -> Result<Workspace, String> {
        let _write = state.write_gate.lock().await;
        let path = state.paths.workspace.clone();
        let saved = tauri::async_runtime::spawn_blocking(move || -> Result<Workspace> {
            workspace.save(&path)?;
            Ok(workspace)
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?
            .workspace = saved.clone();
        Ok(saved)
    }

    #[tauri::command]
    pub async fn preferences_save(
        state: State<'_, AppState>,
        workspace: Workspace,
        settings: metadata::Settings,
    ) -> Result<(), String> {
        workspace.validate().map_err(fail)?;
        let _write = state.write_gate.lock().await;
        let data = state.paths.data_dir.clone();
        let (workspace, settings) = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
            replace_state_files(
                &data,
                vec![
                    ("workspace.json", serde_json::to_vec_pretty(&workspace)?),
                    ("metadata.json", serde_json::to_vec_pretty(&settings)?),
                ],
            )?;
            Ok((workspace, settings))
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?;
        inner.workspace = workspace;
        inner.settings = settings;
        Ok(())
    }

    #[tauri::command]
    pub fn settings_load(state: State<'_, AppState>) -> Result<metadata::Settings, String> {
        state
            .inner
            .lock()
            .map(|inner| inner.settings.clone())
            .map_err(|_| "Application state is unavailable".to_string())
    }

    #[tauri::command]
    pub async fn settings_save(
        state: State<'_, AppState>,
        settings: metadata::Settings,
    ) -> Result<metadata::Settings, String> {
        let _write = state.write_gate.lock().await;
        let path = state.paths.settings.clone();
        let saved = tauri::async_runtime::spawn_blocking(move || -> Result<metadata::Settings> {
            settings.save(&path)?;
            Ok(settings)
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?
            .settings = saved.clone();
        Ok(saved)
    }

    #[tauri::command]
    pub async fn game_link_open(
        state: State<'_, AppState>,
        handle: String,
        index: usize,
    ) -> Result<(), String> {
        let link = {
            let inner = state
                .inner
                .lock()
                .map_err(|_| "Application state is unavailable".to_string())?;
            let game = inner
                .library
                .games
                .iter()
                .find(|game| game.handle() == handle)
                .context("Game was removed")
                .map_err(fail)?;
            game.metadata
                .links
                .get(index)
                .cloned()
                .context("Link is no longer available")
                .map_err(fail)?
        };
        let url = reqwest::Url::parse(&link).map_err(|_| "Invalid store link".to_string())?;
        if url.scheme() != "https"
            || url.host_str() != Some("store.steampowered.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
        {
            return Err("Only HTTPS Steam store links can be opened".into());
        }
        tauri::async_runtime::spawn_blocking(move || open::that_detached(url.as_str()))
            .await
            .map_err(|error| format!("Background task failed: {error}"))?
            .map_err(fail)
    }

    // --- Native pickers and data folder ---

    #[tauri::command]
    pub async fn data_folder_open(state: State<'_, AppState>) -> Result<(), String> {
        let dir = state.paths.data_dir.clone();
        tauri::async_runtime::spawn_blocking(move || {
            open::that_detached(&dir).context("Cannot open the library folder")
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)
    }

    #[tauri::command]
    pub async fn pick_executable() -> Result<Option<String>, String> {
        tauri::async_runtime::spawn_blocking(|| {
            #[allow(unused_mut)]
            let mut dialog = rfd::FileDialog::new().set_title("Choose an executable");
            #[cfg(windows)]
            {
                dialog = dialog.add_filter("Programs", &["exe"]);
            }
            dialog
                .pick_file()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))
    }

    #[tauri::command]
    pub async fn pick_image() -> Result<Option<String>, String> {
        tauri::async_runtime::spawn_blocking(|| {
            rfd::FileDialog::new()
                .set_title("Choose a cover image")
                .add_filter("Images", &["png", "jpg", "jpeg"])
                .pick_file()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))
    }

    #[tauri::command]
    pub async fn pick_folder() -> Result<Option<String>, String> {
        tauri::async_runtime::spawn_blocking(|| {
            rfd::FileDialog::new()
                .set_title("Choose a folder")
                .pick_folder()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))
    }

    #[tauri::command]
    pub async fn pick_file(title: String) -> Result<Option<String>, String> {
        tauri::async_runtime::spawn_blocking(move || {
            rfd::FileDialog::new()
                .set_title(title)
                .pick_file()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))
    }

    #[tauri::command]
    pub async fn pick_save_file(
        title: String,
        file_name: String,
    ) -> Result<Option<String>, String> {
        tauri::async_runtime::spawn_blocking(move || {
            rfd::FileDialog::new()
                .set_title(title)
                .set_file_name(file_name)
                .save_file()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))
    }

    // --- Backup and restore ---

    #[tauri::command]
    pub async fn backup_export(
        state: State<'_, AppState>,
        path: String,
    ) -> Result<BackupSummary, String> {
        if blank(&path) {
            return Err("Choose a backup file location".to_string());
        }
        let destination = PathBuf::from(path.trim());
        let actual = destination
            .canonicalize()
            .or_else(|_| {
                destination
                    .parent()
                    .context("Backup path needs a parent")
                    .and_then(|parent| {
                        Ok(parent.canonicalize()?.join(
                            destination
                                .file_name()
                                .context("Backup path needs a file name")?,
                        ))
                    })
                    .map_err(std::io::Error::other)
            })
            .unwrap_or(destination.clone());
        if [
            &state.paths.library,
            &state.paths.workspace,
            &state.paths.settings,
            &state.paths.data_dir.join("library.lock"),
        ]
        .iter()
        .any(|live| live.canonicalize().unwrap_or_else(|_| (*live).clone()) == actual)
        {
            return Err("Choose a backup location outside the live library files".into());
        }
        let snapshot = {
            let inner = state
                .inner
                .lock()
                .map_err(|_| "Application state is unavailable".to_string())?;
            BackupFile {
                version: 1,
                library: inner.library.clone(),
                workspace: inner.workspace.clone(),
                settings: inner.settings.clone(),
            }
        };
        tauri::async_runtime::spawn_blocking(move || -> Result<BackupSummary> {
            let path = PathBuf::from(path.trim());
            let parent = path
                .parent()
                .context("Backup path needs a parent directory")?;
            std::fs::create_dir_all(parent)?;
            let summary = BackupSummary {
                games: snapshot.library.games.len(),
                layouts: snapshot.workspace.layouts.len(),
            };
            let mut temp = tempfile::NamedTempFile::new_in(parent)?;
            use std::io::Write;
            temp.write_all(&serde_json::to_vec_pretty(&snapshot)?)?;
            temp.as_file().sync_all()?;
            temp.persist(&path)
                .context("Could not write backup; existing file is unchanged")?;
            Ok(summary)
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)
    }

    #[tauri::command]
    pub async fn backup_import(
        state: State<'_, AppState>,
        path: String,
    ) -> Result<BackupSummary, String> {
        if blank(&path) {
            return Err("Choose a backup file to restore".to_string());
        }
        let _write = state.write_gate.lock().await;
        let paths = state.paths.clone();
        let backup = tauri::async_runtime::spawn_blocking(move || -> Result<BackupFile> {
            let text = std::fs::read_to_string(path.trim()).context("Cannot read backup file")?;
            // Validate everything before any live file changes.
            super::parse_backup(&text)
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        let summary = BackupSummary {
            games: backup.library.games.len(),
            layouts: backup.workspace.layouts.len(),
        };
        let backup = tauri::async_runtime::spawn_blocking(move || -> Result<BackupFile> {
            restore_backup_files(&paths.data_dir, &backup)?;
            Ok(backup)
        })
        .await
        .map_err(|error| format!("Background task failed: {error}"))?
        .map_err(fail)?;
        let mut inner = state
            .inner
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?;
        inner.library = backup.library;
        inner.workspace = backup.workspace;
        inner.settings = backup.settings;
        Ok(summary)
    }
}
