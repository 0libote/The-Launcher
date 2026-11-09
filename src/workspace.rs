//! User-owned presentation. No launch targets, credentials, or game data belong here.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Grid,
    List,
    Shelf,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Navigation {
    #[default]
    Left,
    Right,
    Hidden,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Details {
    #[default]
    #[serde(alias = "bottom")]
    Modal,
    Right,
    Hidden,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tools {
    #[default]
    Top,
    Bottom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Layout {
    pub navigation: Navigation,
    pub details: Details,
    pub tools: Tools,
    pub view: View,
    pub tile_width: f32,
    pub navigation_width: f32,
    pub details_width: f32,
    pub show_shortcuts: bool,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            navigation: Navigation::Left,
            details: Details::Modal,
            tools: Tools::Top,
            view: View::Grid,
            tile_width: 180.0,
            navigation_width: 232.0,
            details_width: 300.0,
            show_shortcuts: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Skin {
    pub background: [u8; 3],
    pub navigation: [u8; 3],
    pub surface: [u8; 3],
    pub raised: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub faint: [u8; 3],
    pub line: [u8; 3],
    pub accent: [u8; 3],
    pub selection: [u8; 3],
}
impl Default for Skin {
    fn default() -> Self {
        Self {
            background: [252, 252, 252],
            navigation: [250, 250, 250],
            surface: [255, 255, 255],
            raised: [244, 244, 245],
            text: [24, 24, 27],
            muted: [82, 82, 91],
            faint: [113, 113, 122],
            line: [228, 228, 231],
            accent: [219, 39, 119],
            selection: [219, 39, 119],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NamedLayout {
    pub name: String,
    pub layout: Layout,
    pub skin: Skin,
}
impl Default for NamedLayout {
    fn default() -> Self {
        Self {
            name: "My library".into(),
            layout: Layout::default(),
            skin: Skin::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Workspace {
    pub version: u32,
    pub active: usize,
    pub layouts: Vec<NamedLayout>,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: 1,
            active: 0,
            layouts: vec![NamedLayout::default()],
        }
    }
}
impl Workspace {
    pub fn current(&self) -> &NamedLayout {
        &self.layouts[self.active]
    }
    pub fn current_mut(&mut self) -> &mut NamedLayout {
        &mut self.layouts[self.active]
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "Unsupported workspace version {}; file has not been changed",
                self.version
            );
        }
        if self.layouts.is_empty() || self.layouts.len() > 32 || self.active >= self.layouts.len() {
            bail!("Workspace needs 1–32 layouts and a valid active layout");
        }
        for (i, preset) in self.layouts.iter().enumerate() {
            if preset.name.trim().is_empty() || preset.name.chars().count() > 64 {
                bail!("Layout names must contain 1–64 characters");
            }
            if self.layouts[..i]
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case(&preset.name))
            {
                bail!("Layout names must be unique");
            }
            let l = &preset.layout;
            for (value, min, max) in [
                (l.tile_width, 130.0, 320.0),
                (l.navigation_width, 160.0, 280.0),
                (l.details_width, 220.0, 340.0),
            ] {
                if !value.is_finite() || value < min || value > max {
                    bail!("Layout panel or tile size is outside its supported range");
                }
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        let data = match std::fs::read(path) {
            Ok(data) => data,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).context("Cannot read workspace"),
        };
        let workspace: Self = serde_json::from_slice(&data)
            .context("Cannot read workspace JSON; file has not been changed")?;
        workspace.validate()?;
        Ok(workspace)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.as_file().sync_all()?;
        file.persist(path)
            .context("Cannot save workspace; previous file is unchanged")?;
        Ok(())
    }
    pub fn duplicate(&mut self) -> Result<()> {
        if self.layouts.len() >= 32 {
            bail!("A maximum of 32 layouts can be saved");
        }
        let mut copy = self.current().clone();
        copy.name = (1..)
            .map(|i| format!("Layout {i}"))
            .find(|name| {
                !self
                    .layouts
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case(name))
            })
            .unwrap();
        self.layouts.push(copy);
        self.active = self.layouts.len() - 1;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrangements_skins_and_views_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        let mut w = Workspace::default();
        w.duplicate().unwrap();
        let p = w.current_mut();
        p.layout.view = View::Shelf;
        p.layout.navigation = Navigation::Right;
        p.layout.details = Details::Hidden;
        p.skin.accent = [12, 34, 56];
        w.save(&path).unwrap();
        assert_eq!(Workspace::load(&path).unwrap(), w);
    }
    #[test]
    fn corrupt_future_and_invalid_layouts_are_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        for data in ["bad", r#"{"version":2,"active":0,"layouts":[]}"#] {
            std::fs::write(&path, data).unwrap();
            assert!(Workspace::load(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), data);
        }
        let mut w = Workspace::default();
        w.current_mut().layout.tile_width = f32::NAN;
        assert!(w.save(&path).is_err());
        w = Workspace::default();
        w.active = 9;
        assert!(w.validate().is_err());
    }
    #[test]
    fn legacy_bottom_details_load_as_modal() {
        assert_eq!(
            serde_json::from_str::<Details>("\"bottom\"").unwrap(),
            Details::Modal
        );
        assert_eq!(
            serde_json::from_str::<Details>("\"modal\"").unwrap(),
            Details::Modal
        );
    }
}
