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
pub fn plan(game: &Game) -> Result<LaunchPlan> {
    validate_game(game)?;
    match &game.target {
        Target::Steam { app_id } => Ok(LaunchPlan::Steam(format!("steam://rungameid/{app_id}"))),
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
            open::that_detached(uri)
                .context("Cannot open Steam; install Steam and its protocol handler")?;
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arguments_are_preserved_without_shell_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a game.exe");
        std::fs::write(&path, "fixture").unwrap();
        let args = vec!["a b".into(), "$(touch nope)".into(), "\"quoted\"".into()];
        let game = Game {
            name: "Game".into(),
            target: Target::Executable {
                path: path.clone(),
                arguments: args.clone(),
                working_directory: None,
            },
            favorite: false,
            last_launched: None,
        };
        assert_eq!(
            plan(&game).unwrap(),
            LaunchPlan::Executable {
                path,
                arguments: args,
                directory: dir.path().into()
            }
        );
    }
    #[test]
    fn steam_uri_is_only_derived_from_numeric_id() {
        let mut game = Game {
            name: "Steam game".into(),
            target: Target::Steam { app_id: 42 },
            favorite: false,
            last_launched: None,
        };
        assert_eq!(
            plan(&game).unwrap(),
            LaunchPlan::Steam("steam://rungameid/42".into())
        );
        game.target = Target::Steam { app_id: 0 };
        assert!(plan(&game).is_err());
    }
}
