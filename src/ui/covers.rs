use anyhow::{Context, Result, bail};
use eframe::egui::{self, ColorImage, TextureHandle};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Covers {
    entries: HashMap<PathBuf, Result<TextureHandle, String>>,
}
impl Covers {
    pub fn texture(&mut self, ctx: &egui::Context, path: &Path) -> Option<TextureHandle> {
        if !self.entries.contains_key(path) {
            // Bound GPU memory even when browsing a large library. Only visible tiles request covers.
            if self.entries.len() >= 64 {
                self.entries.clear();
            }
            let texture = decode(path)
                .map(|image| {
                    ctx.load_texture(
                        path.display().to_string(),
                        image,
                        egui::TextureOptions::LINEAR,
                    )
                })
                .map_err(|e| format!("{e:#}"));
            self.entries.insert(path.into(), texture);
        }
        self.entries[path].as_ref().ok().cloned()
    }
    pub fn error(&self, path: &Path) -> Option<&str> {
        self.entries.get(path)?.as_ref().err().map(String::as_str)
    }
    pub fn forget(&mut self, path: &Path) {
        self.entries.remove(path);
    }
}

pub fn decode(path: &Path) -> Result<ColorImage> {
    let file = std::fs::File::open(path)
        .context("Cover image is unavailable; choose it again from Options")?;
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
        .context("Could not read cover; use PNG or JPEG, up to 4096 × 4096")?
        .thumbnail(480, 720)
        .to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

/// Center-crop without stretching a landscape or square source into a portrait.
pub fn crop_uv(source: egui::Vec2, target: egui::Vec2) -> egui::Rect {
    let ratio = (target.x / target.y) / (source.x / source.y);
    let size = if ratio < 1.0 {
        egui::vec2(ratio, 1.0)
    } else {
        egui::vec2(1.0, 1.0 / ratio)
    };
    egui::Rect::from_center_size(egui::pos2(0.5, 0.5), size)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn center_crops_without_distorting_the_image() {
        let uv = crop_uv(egui::vec2(1000.0, 500.0), egui::vec2(200.0, 300.0));
        assert!((uv.width() - 1.0 / 3.0).abs() < 0.0001);
        assert_eq!(uv.center(), egui::pos2(0.5, 0.5));
        assert_eq!(uv.height(), 1.0);
        assert_eq!(
            crop_uv(egui::vec2(200.0, 300.0), egui::vec2(200.0, 300.0)),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0))
        );
    }
    #[test]
    fn downsizes_covers_and_rejects_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.png");
        image::RgbaImage::from_pixel(800, 1200, image::Rgba([22, 33, 44, 255]))
            .save(&path)
            .unwrap();
        let cover = decode(&path).unwrap();
        assert_eq!(cover.size, [480, 720]);
        std::fs::write(&path, b"not an image").unwrap();
        assert!(decode(&path).is_err());
        assert!(decode(&dir.path().join("missing.jpg")).is_err());
    }
    #[test]
    fn old_libraries_load_without_cover_and_custom_covers_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        std::fs::write(&path,r#"{"schema_version":1,"games":[{"name":"Game","target":{"kind":"steam","app_id":42}}]}"#).unwrap();
        let mut library = the_launcher::library::Library::load(&path).unwrap();
        assert!(library.games[0].cover.is_none());
        library.games[0].cover = Some(dir.path().join("art.png"));
        library.save(&path).unwrap();
        assert_eq!(
            the_launcher::library::Library::load(&path).unwrap(),
            library
        );
    }
}
