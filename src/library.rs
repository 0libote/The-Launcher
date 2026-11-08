use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Executable {
        path: PathBuf,
        arguments: Vec<String>,
        working_directory: Option<PathBuf>,
    },
    Steam {
        app_id: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Game {
    pub name: String,
    pub target: Target,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub last_launched: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed: Option<bool>,
    #[serde(default)]
    pub hidden: bool,
}

impl Game {
    pub fn source(&self) -> &'static str {
        match self.target {
            Target::Executable { .. } => "PC",
            Target::Steam { .. } => "Steam",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Library {
    pub schema_version: u32,
    pub games: Vec<Game>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            schema_version: 1,
            games: vec![],
        }
    }
}
impl Library {
    pub fn load(path: &Path) -> Result<Self> {
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).context("Cannot read library"),
        };
        let library: Self = serde_json::from_slice(&data).context(
            "Library is damaged; restore a backup or repair the JSON. It has not been overwritten",
        )?;
        if library.schema_version != 1 {
            bail!(
                "Unsupported library version {}. Update the launcher; your file has not been changed",
                library.schema_version
            );
        }
        for game in &library.games {
            validate_game(game)?;
        }
        Ok(library)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .context("Library path needs a parent directory")?;
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.as_file().sync_all()?;
        temp.persist(path)
            .context("Cannot replace library; previous file is unchanged")?;
        Ok(())
    }

    // Save first, then publish the new state to the UI.
    pub fn change(
        &mut self,
        path: &Path,
        edit: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        let mut next = self.clone();
        edit(&mut next)?;
        for game in &next.games {
            validate_game(game)?;
        }
        next.save(path)?;
        *self = next;
        Ok(())
    }

    pub fn add(&mut self, game: Game) -> Result<()> {
        validate_game(&game)?;
        if self
            .games
            .iter()
            .any(|g| same_target(&g.target, &game.target))
        {
            bail!("This game is already in your library");
        }
        self.games.push(game);
        Ok(())
    }
}
fn same_target(a: &Target, b: &Target) -> bool {
    match (a, b) {
        (Target::Steam { app_id: a }, Target::Steam { app_id: b }) => a == b,
        (Target::Executable { path: a, .. }, Target::Executable { path: b, .. }) => {
            if cfg!(windows) {
                a.to_string_lossy()
                    .eq_ignore_ascii_case(&b.to_string_lossy())
            } else {
                a == b
            }
        }
        _ => false,
    }
}
pub fn validate_game(game: &Game) -> Result<()> {
    if game.name.trim().is_empty() {
        bail!("Give the game a name");
    }
    match &game.target {
        Target::Steam { app_id: 0 } => bail!("Steam app ID must be positive"),
        Target::Executable { path, .. } if !path.is_absolute() => {
            bail!("Executable path must be absolute")
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game(id: u32) -> Game {
        Game {
            name: "Test".into(),
            target: Target::Steam { app_id: id },
            favorite: false,
            last_launched: None,
            cover: None,
            installed: None,
            hidden: false,
        }
    }
    #[test]
    fn round_trip_and_duplicate_protection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let mut library = Library::load(&path).unwrap();
        library.change(&path, |l| l.add(game(10))).unwrap();
        assert_eq!(Library::load(&path).unwrap(), library);
        assert!(library.change(&path, |l| l.add(game(10))).is_err());
        assert_eq!(library.games.len(), 1);
    }
    #[test]
    fn failed_write_keeps_memory_and_existing_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("occupied");
        fs::create_dir(&path).unwrap();
        let mut library = Library::default();
        assert!(library.change(&path, |l| l.add(game(10))).is_err());
        assert!(library.games.is_empty());
        assert!(path.is_dir());
    }
    #[test]
    fn damaged_and_future_data_are_never_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        for data in ["broken", r#"{"schema_version":2,"games":[]}"#] {
            fs::write(&path, data).unwrap();
            assert!(Library::load(&path).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), data);
        }
    }
    #[test]
    fn hidden_defaults_to_visible_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(
            &path,
            r#"{"schema_version":1,"games":[{"name":"Game","target":{"kind":"steam","app_id":42}}]}"#,
        )
        .unwrap();
        let mut library = Library::load(&path).unwrap();
        assert!(!library.games[0].hidden);
        library
            .change(&path, |l| {
                l.games[0].hidden = true;
                Ok(())
            })
            .unwrap();
        assert!(Library::load(&path).unwrap().games[0].hidden);
    }
}
