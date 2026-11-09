//! Epic Games local library scan. Mirrors the Playnite Epic reference:
//! installed `.item` manifests under
//! `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests`, filtered like
//! upstream (DLC without a launchable addon, engine plugins, missing folders).
//! No login, no account catalog, no download/install watching yet.

use anyhow::{Context, Result, anyhow, bail};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpicGame {
    pub app_name: String,
    pub display_name: String,
    pub install_location: PathBuf,
    pub catalog_namespace: String,
    pub catalog_item_id: String,
}

#[derive(Default)]
pub struct ImportOptions {
    /// Override the manifests directory. When omitted, the Windows default
    /// under `%PROGRAMDATA%` is used.
    pub manifest_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct ImportResult {
    pub games: Vec<EpicGame>,
    pub warnings: Vec<String>,
}

/// Default manifests directory on Windows. Returns `None` off Windows so
/// callers can surface "needs Windows" instead of guessing.
pub fn default_manifest_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("PROGRAMDATA")
            .map(PathBuf::from)
            .map(|base| {
                base.join("Epic")
                    .join("EpicGamesLauncher")
                    .join("Data")
                    .join("Manifests")
            })
    }
    #[cfg(not(windows))]
    {
        None
    }
}

pub fn import(options: ImportOptions) -> Result<ImportResult> {
    let dir = match options.manifest_dir {
        Some(dir) => dir,
        None => default_manifest_dir()
            .context("Epic import needs Windows or an explicit manifests folder")?,
    };
    scan(&dir)
}

pub fn scan(dir: &Path) -> Result<ImportResult> {
    if !dir.is_dir() {
        bail!(
            "Invalid Epic manifests folder '{}': it does not exist or is not a directory",
            dir.display()
        );
    }
    let entries = fs::read_dir(dir)
        .with_context(|| format!("Cannot list Epic manifests in '{}'", dir.display()))?;
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry =
            entry.with_context(|| format!("Cannot list Epic manifests in '{}'", dir.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("item"))
        {
            paths.push(path);
        }
    }
    paths.sort();

    let mut warnings = Vec::new();
    let mut by_id: HashMap<String, EpicGame> = HashMap::new();
    for path in &paths {
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        match parse_manifest(path) {
            Ok(Some(game)) => {
                let key = game.app_name.to_lowercase();
                by_id.entry(key).or_insert(game);
            }
            Ok(None) => {}
            Err(error) => warnings.push(format!("Skipping '{file_name}': {error:#}")),
        }
    }
    let mut games: Vec<EpicGame> = by_id.into_values().collect();
    games.sort_by(|a, b| {
        a.display_name
            .cmp(&b.display_name)
            .then(a.app_name.cmp(&b.app_name))
    });
    Ok(ImportResult { games, warnings })
}

/// Parse one `.item` manifest. `Ok(None)` means "present but not a
/// launchable game" (DLC, plugin, missing folder); `Err` means warn.
fn parse_manifest(path: &Path) -> Result<Option<EpicGame>> {
    let text = fs::read_to_string(path).map_err(|_| anyhow!("cannot be read"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| anyhow!("cannot be parsed"))?;
    let get = |key: &str| -> String {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let app_name = get("AppName");
    if app_name.is_empty() {
        return Err(anyhow!("missing 'AppName'"));
    }
    let display_name = {
        let display = get("DisplayName");
        if display.is_empty() {
            app_name.clone()
        } else {
            display
        }
    };
    let install_location = get("InstallLocation");
    if install_location.is_empty() {
        return Ok(None);
    }
    let install_path = PathBuf::from(&install_location);
    if !install_path.is_dir() {
        return Ok(None);
    }
    // Upstream exclusions: DLC addons that are not launchable, engine
    // plugins, Unreal engine version stubs, and non-application entries.
    let categories: HashSet<String> = value
        .get("AppCategories")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    if categories.contains("addons") && !categories.contains("addons/launchable") {
        return Ok(None);
    }
    if categories
        .iter()
        .any(|c| c == "plugins" || c == "plugins/engine" || c.starts_with("plugins/"))
    {
        return Ok(None);
    }
    let compatible: Vec<String> = value
        .get("CompatibleApps")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    if compatible.iter().any(|c| c.starts_with("UE_")) {
        return Ok(None);
    }
    let technical = get("TechnicalType").to_lowercase();
    if technical.contains("plugins/engine") {
        return Ok(None);
    }
    if let Some(flag) = value.get("bIsApplication").and_then(|v| v.as_bool())
        && !flag
    {
        return Ok(None);
    }
    Ok(Some(EpicGame {
        app_name,
        display_name: display_name
            .replace("(TM)", "")
            .replace("®", "")
            .trim()
            .to_string(),
        install_location: install_path,
        catalog_namespace: get("CatalogNamespace"),
        catalog_item_id: get("CatalogItemId"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_item(dir: &Path, file: &str, body: &serde_json::Value) {
        fs::write(dir.join(file), serde_json::to_string(body).unwrap()).unwrap();
    }

    fn game_json(
        app: &str,
        display: &str,
        location: &Path,
        extra: serde_json::Value,
    ) -> serde_json::Value {
        let mut base = serde_json::json!({
            "AppName": app,
            "DisplayName": display,
            "InstallLocation": location.display().to_string(),
            "CatalogNamespace": "fn",
            "CatalogItemId": "abc123",
            "bIsApplication": true,
        });
        for (k, v) in extra.as_object().unwrap() {
            base[k] = v.clone();
        }
        base
    }

    #[test]
    fn finds_sorted_games_and_filters_addons_and_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let alpha = dir.join("Alpha");
        let zulu = dir.join("Zulu");
        fs::create_dir_all(&alpha).unwrap();
        fs::create_dir_all(&zulu).unwrap();
        write_item(
            dir,
            "a.item",
            &game_json("ZuluApp", "Zulu", &zulu, serde_json::json!({})),
        );
        write_item(
            dir,
            "b.item",
            &game_json("AlphaApp", "Alpha", &alpha, serde_json::json!({})),
        );
        // DLC addon without launchable flag.
        let dlc = dir.join("Dlc");
        fs::create_dir_all(&dlc).unwrap();
        write_item(
            dir,
            "c.item",
            &game_json(
                "DlcApp",
                "DLC Pack",
                &dlc,
                serde_json::json!({"AppCategories": ["addons"]}),
            ),
        );
        // Engine plugin.
        let plug = dir.join("Plug");
        fs::create_dir_all(&plug).unwrap();
        write_item(
            dir,
            "d.item",
            &game_json(
                "PlugApp",
                "Plugin",
                &plug,
                serde_json::json!({"AppCategories": ["plugins/engine"]}),
            ),
        );
        // Missing folder: silently skipped, not a warning.
        write_item(
            dir,
            "e.item",
            &game_json(
                "GhostApp",
                "Ghost",
                &dir.join("missing-dir"),
                serde_json::json!({}),
            ),
        );
        // Malformed file: warning, not failure.
        fs::write(dir.join("f.item"), "{ broken").unwrap();

        let result = scan(dir).unwrap();
        assert_eq!(
            result
                .games
                .iter()
                .map(|g| g.display_name.clone())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Zulu"]
        );
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("f.item"));
    }

    #[test]
    fn dedupes_by_app_name_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let loc = dir.join("Game");
        fs::create_dir_all(&loc).unwrap();
        write_item(
            dir,
            "a.item",
            &game_json("MyGame", "My Game", &loc, serde_json::json!({})),
        );
        write_item(
            dir,
            "b.item",
            &game_json("mygame", "My Game Copy", &loc, serde_json::json!({})),
        );
        let result = scan(dir).unwrap();
        assert_eq!(result.games.len(), 1);
    }

    #[test]
    fn invalid_dirs_fail_with_context() {
        assert!(scan(Path::new("/definitely/not/an/epic/dir")).is_err());
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(scan(file.path()).is_err());
    }

    #[test]
    fn launch_url_prefers_full_form_and_falls_back() {
        let full = crate::launch::epic_uri("fn", "abc", "Fortnite").unwrap();
        assert_eq!(
            full,
            "com.epicgames.launcher://apps/fn:abc:Fortnite?action=launch&silent=true"
        );
        let fallback = crate::launch::epic_uri("", "", "SideLoaded").unwrap();
        assert!(fallback.contains("SideLoaded"));
        assert!(crate::launch::epic_uri("", "", "  ").is_err());
    }
}
