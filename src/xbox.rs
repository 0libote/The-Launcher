//! Xbox / Xbox Game Pass local import. First slice only: installed UWP
//! packages on Windows, mirroring how Playnite matches installed
//! `GetUWPApps` entries against its account titles. Account-owned titles,
//! console-only games, playtime sync, and install/uninstall watchers need an
//! authenticated Xbox API and remain future work.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XboxGame {
    /// AppsFolder moniker `<PackageFamilyName>!<AppId>`.
    pub package_id: String,
    pub name: String,
    pub install_location: Option<PathBuf>,
}

#[derive(Default)]
pub struct ImportOptions {
    /// Override for tests/dev: a JSON file holding `Get-AppxPackage` output.
    pub appx_json: Option<PathBuf>,
    /// Override for the PowerShell lookup itself (tests).
    pub raw_appx_json: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ImportResult {
    pub games: Vec<XboxGame>,
    pub warnings: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AppxPackage {
    #[serde(default, alias = "Name")]
    name: String,
    #[serde(default, alias = "PackageFamilyName")]
    package_family_name: String,
    #[serde(default, alias = "InstallLocation")]
    install_location: String,
}

/// Import installed Xbox (UWP) games. Windows-only; elsewhere this fails
/// with a clear message instead of pretending to scan.
pub fn import(options: ImportOptions) -> Result<ImportResult> {
    let text = if let Some(raw) = options.raw_appx_json {
        raw
    } else if let Some(path) = options.appx_json {
        std::fs::read_to_string(&path)
            .with_context(|| format!("Cannot read Xbox package list '{}'", path.display()))?
    } else {
        read_appx_packages()?
    };
    Ok(parse_appx(&text))
}

/// Run PowerShell to list installed UWP packages as JSON.
#[cfg(windows)]
fn read_appx_packages() -> Result<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-AppxPackage | Select-Object Name, PackageFamilyName, InstallLocation | ConvertTo-Json -Depth 3 -Compress",
        ])
        .output()
        .context("Cannot list Xbox packages; PowerShell is unavailable")?;
    if !output.status.success() {
        bail!("Xbox package lookup failed; PowerShell reported an error");
    }
    String::from_utf8(output.stdout).context("Xbox package list is not valid text")
}

#[cfg(not(windows))]
fn read_appx_packages() -> Result<String> {
    bail!("Xbox import needs Windows; UWP packages cannot be listed here")
}

/// Parse `Get-AppxPackage | ConvertTo-Json` output. Handles an array, a
/// single object, or empty output. Non-game packages are kept: without the
/// authenticated title API we cannot tell games from apps, so the user hides
/// non-games (like Steam hidden entries). Malformed entries become warnings.
pub fn parse_appx(text: &str) -> ImportResult {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return ImportResult::default();
    }
    let values: Vec<serde_json::Value> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).unwrap_or_default()
    } else {
        serde_json::from_str::<serde_json::Value>(trimmed)
            .map(|v| vec![v])
            .unwrap_or_default()
    };
    // Fall back to typed parsing when the untyped route yields nothing.
    let packages: Vec<AppxPackage> = if values.is_empty() {
        serde_json::from_str::<Vec<AppxPackage>>(trimmed).unwrap_or_else(|_| {
            serde_json::from_str::<AppxPackage>(trimmed)
                .map(|p| vec![p])
                .unwrap_or_default()
        })
    } else {
        values
            .iter()
            .filter_map(|v| serde_json::from_value::<AppxPackage>(v.clone()).ok())
            .collect()
    };

    let mut warnings = Vec::new();
    let mut by_id: HashMap<String, XboxGame> = HashMap::new();
    if packages.is_empty() && !trimmed.is_empty() {
        warnings.push("Xbox package list could not be understood; nothing was imported".into());
        return ImportResult {
            games: Vec::new(),
            warnings,
        };
    }
    for package in packages {
        let family = package.package_family_name.trim().to_string();
        if family.is_empty() {
            warnings.push("Skipping a package without a family name".into());
            continue;
        }
        let name = if package.name.trim().is_empty() {
            family.clone()
        } else {
            package.name.trim().to_string()
        };
        let install = if package.install_location.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(package.install_location.trim()))
        };
        let app_id = app_id_for_install(install.as_deref());
        let package_id = format!("{family}!{app_id}");
        if by_id.contains_key(&package_id.to_lowercase()) {
            continue;
        }
        by_id.insert(
            package_id.to_lowercase(),
            XboxGame {
                package_id,
                name,
                install_location: install,
            },
        );
    }
    let mut games: Vec<XboxGame> = by_id.into_values().collect();
    games.sort_by(|a, b| a.name.cmp(&b.name).then(a.package_id.cmp(&b.package_id)));
    ImportResult { games, warnings }
}

/// Best-effort Application Id from `AppxManifest.xml`. Most games use `App`;
/// when the manifest is missing or unreadable we keep that default and let
/// the user hide the entry if it does not launch.
fn app_id_for_install(install: Option<&Path>) -> String {
    let Some(dir) = install else {
        return "App".into();
    };
    let manifest = dir.join("AppxManifest.xml");
    let text = match std::fs::read_to_string(&manifest) {
        Ok(text) => text,
        Err(_) => return "App".into(),
    };
    // Minimal scan for `<Application ... Id="...">` without an XML dependency.
    // Skips the plural `<Applications>` container element.
    let mut search = text.as_str();
    while let Some(start) = search.find("<Application") {
        let from_start = &search[start..];
        if from_start.starts_with("<Applications") {
            search = &from_start["<Applications".len().min(from_start.len())..];
            continue;
        }
        let end = from_start.find('>').unwrap_or(from_start.len());
        let tag = &from_start[..end];
        if let Some(id) = application_id(tag) {
            return id;
        }
        search = &from_start["<Application".len().min(from_start.len())..];
    }
    "App".into()
}

fn application_id(tag: &str) -> Option<String> {
    let mut rest = tag;
    while let Some(pos) = rest.find("Id") {
        rest = &rest[pos + 2..];
        let rest_trim = rest.trim_start_matches([' ', '\t', '\n', '\r', '=']);
        let quote = rest_trim.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let after = &rest_trim[1..];
        if let Some(end) = after.find(quote) {
            let id = after[..end].trim().to_string();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_array_single_object_and_empty() {
        let parsed = parse_appx(
            r#"[{"Name":"Halo Infinite","PackageFamilyName":"MS.Halo_abc","InstallLocation":"C:\\Games\\Halo"}]"#,
        );
        assert_eq!(parsed.games.len(), 1);
        assert_eq!(parsed.games[0].package_id, "MS.Halo_abc!App");

        let single = parse_appx(
            r#"{"Name":"Grounded","PackageFamilyName":"MS.Grounded_xyz","InstallLocation":""}"#,
        );
        assert_eq!(single.games.len(), 1);
        assert_eq!(single.games[0].install_location, None);

        assert!(parse_appx("").games.is_empty());
        assert!(parse_appx("null").games.is_empty());
        let bad = parse_appx("{ not json");
        assert!(bad.games.is_empty());
        assert!(!bad.warnings.is_empty());
    }

    #[test]
    fn dedupes_case_insensitively_and_sorts() {
        let parsed = parse_appx(
            r#"[{"Name":"Zulu","PackageFamilyName":"FAM_Z","InstallLocation":""},
                {"Name":"Alpha","PackageFamilyName":"fam_a","InstallLocation":""},
                {"Name":"Alpha Copy","PackageFamilyName":"FAM_A","InstallLocation":""}]"#,
        );
        assert_eq!(parsed.games.len(), 2);
        assert_eq!(parsed.games[0].name, "Alpha");
    }

    #[test]
    fn reads_app_id_from_manifest_or_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("AppxManifest.xml"),
            r#"<Package><Applications><Application Id="Game" /></Applications></Package>"#,
        )
        .unwrap();
        assert_eq!(app_id_for_install(Some(dir.path())), "Game");
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(app_id_for_install(Some(empty.path())), "App");
        assert_eq!(app_id_for_install(None), "App");
        assert!(application_id(r#"Application Id="Main""#).is_some());
    }

    #[test]
    fn monikers_require_family_and_app() {
        assert!(crate::launch::xbox_shell_moniker("Fam!App").is_ok());
        assert!(crate::launch::xbox_shell_moniker("Fam").is_err());
        assert!(crate::launch::xbox_shell_moniker("  ").is_err());
    }

    #[test]
    fn missing_family_name_warns() {
        let parsed =
            parse_appx(r#"[{"Name":"No Fam","PackageFamilyName":"","InstallLocation":""}]"#);
        assert!(parsed.games.is_empty());
        assert!(!parsed.warnings.is_empty());
    }
}
