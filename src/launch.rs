use crate::library::{Game, Target, validate_game};
use anyhow::{Context, Result, bail};
use std::{
    path::PathBuf,
    process::{Child, Command},
};

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchPlan {
    Executable {
        path: PathBuf,
        arguments: Vec<String>,
        directory: PathBuf,
    },
    Steam(String),
}

/// Steam URI for an app: the install flow when the library records the
/// entry as not installed, otherwise the play flow.
pub fn steam_uri(app_id: u32, installed: Option<bool>) -> Result<String> {
    if app_id == 0 {
        bail!("Steam app ID must be positive");
    }
    if installed == Some(false) {
        Ok(format!("steam://install/{app_id}"))
    } else {
        Ok(format!("steam://rungameid/{app_id}"))
    }
}

pub fn plan(game: &Game) -> Result<LaunchPlan> {
    validate_game(game)?;
    match &game.target {
        Target::Steam { app_id } => Ok(LaunchPlan::Steam(steam_uri(*app_id, game.installed)?)),
        Target::Executable {
            path,
            arguments,
            working_directory,
        } => {
            if !path.is_file() {
                bail!("Executable is missing: {}", path.display());
            }
            if cfg!(windows)
                && !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                bail!("Choose a Windows .exe file");
            }
            let directory = working_directory
                .clone()
                .unwrap_or_else(|| path.parent().unwrap().to_owned());
            if !directory.is_absolute() || !directory.is_dir() {
                bail!("Working directory must be an existing absolute folder");
            }
            Ok(LaunchPlan::Executable {
                path: path.clone(),
                arguments: arguments.clone(),
                directory,
            })
        }
    }
}
pub fn launch(game: &Game) -> Result<Option<Child>> {
    match plan(game)? {
        LaunchPlan::Steam(uri) => {
            open_steam_uri(&uri)?;
            Ok(None)
        }
        LaunchPlan::Executable {
            path,
            arguments,
            directory,
        } => {
            // Pass each argument directly. Game names and paths never become shell commands.
            Ok(Some(
                Command::new(path)
                    .args(arguments)
                    .current_dir(directory)
                    .spawn()
                    .context("Could not start the game")?,
            ))
        }
    }
}

/// Open a Steam URI through the detected client on Windows (`steam.exe`
/// `-silent` plus the URI as separate arguments, no shell), otherwise
/// through the platform protocol handler.
#[cfg(windows)]
fn open_steam_uri(uri: &str) -> Result<()> {
    let executable = crate::steam::installation_path()
        .context("Cannot launch: Steam installation not found")?
        .join("steam.exe");
    if !executable.is_file() {
        bail!("Cannot launch: steam.exe not found; reinstall Steam");
    }
    Command::new(executable)
        .arg("-silent")
        .arg(uri)
        .spawn()
        .context("Cannot start Steam")?;
    Ok(())
}

#[cfg(not(windows))]
fn open_steam_uri(uri: &str) -> Result<()> {
    open::that_detached(uri)
        .context("Cannot open Steam; install Steam and its protocol handler")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Built through JSON so these fixtures keep working when the model
    // gains defaulted fields.
    fn steam_game(app_id: u32) -> Game {
        serde_json::from_value(json!({
            "name": "Steam game",
            "target": { "kind": "steam", "app_id": app_id },
        }))
        .expect("fixture Steam game")
    }

    #[test]
    fn steam_uri_uses_install_flow_only_when_known_uninstalled() {
        assert_eq!(steam_uri(42, None).unwrap(), "steam://rungameid/42");
        assert_eq!(steam_uri(42, Some(true)).unwrap(), "steam://rungameid/42");
        assert_eq!(steam_uri(42, Some(false)).unwrap(), "steam://install/42");
        assert!(steam_uri(0, None).is_err());
        assert!(steam_uri(0, Some(false)).is_err());
    }

    #[test]
    fn steam_plan_defaults_to_play_flow() {
        // These tests never execute Steam: only `plan` runs.
        assert_eq!(
            plan(&steam_game(42)).unwrap(),
            LaunchPlan::Steam("steam://rungameid/42".into())
        );
        assert!(plan(&steam_game(0)).is_err());
    }

    #[test]
    fn steam_plan_uses_install_flow_for_uninstalled_entries() {
        let game: Game = serde_json::from_value(json!({
            "name": "Steam game",
            "target": { "kind": "steam", "app_id": 42 },
            "installed": false,
        }))
        .expect("fixture uninstalled Steam game");
        assert_eq!(
            plan(&game).unwrap(),
            LaunchPlan::Steam("steam://install/42".into())
        );
    }

    #[test]
    fn arguments_are_preserved_without_shell_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a game.exe");
        std::fs::write(&path, "fixture").unwrap();
        let args = vec![
            "a b".to_string(),
            "$(touch nope)".to_string(),
            "\"quoted\"".to_string(),
        ];
        let game: Game = serde_json::from_value(json!({
            "name": "Game",
            "target": {
                "kind": "executable",
                "path": path.to_string_lossy(),
                "arguments": args,
                "working_directory": null,
            },
        }))
        .expect("fixture executable game");
        assert_eq!(
            plan(&game).unwrap(),
            LaunchPlan::Executable {
                path,
                arguments: args,
                directory: dir.path().into()
            }
        );
    }
}
