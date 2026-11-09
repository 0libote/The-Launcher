//! Built-in metadata providers. Network work belongs on a worker; only local paths reach rendering.
use crate::library::{Game, Target};
use anyhow::{Context, Result, bail};
use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameMetadata {
    pub steam_app_id: Option<u32>,
    pub description: String,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    pub features: Vec<String>,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub age_rating: String,
    pub series: String,
    pub critic_score: Option<u8>,
    pub release_date: String,
    pub background: Option<PathBuf>,
    pub logo: Option<PathBuf>,
    pub sources: BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtworkProvider {
    #[default]
    Steam,
    SteamGridDb,
}
impl ArtworkProvider {
    pub fn label(self) -> &'static str {
        match self {
            Self::Steam => "Steam official",
            Self::SteamGridDb => "SteamGridDB",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub automatic: bool,
    pub download_text: bool,
    pub download_cover: bool,
    pub download_background: bool,
    pub download_logo: bool,
    pub artwork_provider: ArtworkProvider,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            automatic: true,
            download_text: true,
            download_cover: true,
            download_background: true,
            download_logo: false,
            artwork_provider: ArtworkProvider::Steam,
        }
    }
}
impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .context("Metadata settings are damaged; file has not been changed"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).context("Cannot read metadata settings"),
        }
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().context("Settings need a parent directory")?;
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.as_file().sync_all()?;
        temp.persist(path)
            .context("Could not save metadata settings")?;
        Ok(())
    }
    pub fn for_game(&self, game: &Game, replace: bool) -> Self {
        let mut next = self.clone();
        if !replace {
            next.download_text &= game.metadata.description.is_empty()
                || game.metadata.developers.is_empty()
                || game.metadata.publishers.is_empty()
                || game.metadata.genres.is_empty()
                || game.metadata.features.is_empty()
                || game.metadata.links.is_empty()
                || game.metadata.age_rating.is_empty()
                || game.metadata.critic_score.is_none()
                || game.metadata.release_date.is_empty();
            next.download_cover &= game.cover.is_none();
            next.download_background &= game.metadata.background.is_none();
            next.download_logo &= game.metadata.logo.is_none();
        }
        next
    }
    pub fn any_fields(&self) -> bool {
        self.download_text || self.download_cover || self.download_background || self.download_logo
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageKind {
    Cover,
    Background,
    Logo,
}
impl ImageKind {
    pub fn field(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Background => "background",
            Self::Logo => "logo",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Match {
    pub app_id: u32,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct ImageChoice {
    pub label: String,
    pub url: String,
    pub source: String,
}
#[derive(Clone, Debug, Default)]
pub struct Fetched {
    pub metadata: GameMetadata,
    pub cover: Option<PathBuf>,
    pub warnings: Vec<String>,
}

pub fn app_id(game: &Game) -> Option<u32> {
    match game.target {
        Target::Steam { app_id } => Some(app_id),
        _ => game.metadata.steam_app_id.filter(|id| *id > 0),
    }
}
// Never replace a field edited while the request was in flight, even for explicit replacement.
pub fn apply(game: &mut Game, baseline: &Game, fetched: Fetched, replace: bool) {
    if game.target != baseline.target || app_id(game) != app_id(baseline) {
        return;
    }
    let from = fetched.metadata;
    macro_rules! field {
        ($field:ident) => {
            if !from.$field.is_empty()
                && game.metadata.$field == baseline.metadata.$field
                && (replace || game.metadata.$field.is_empty())
            {
                game.metadata.$field = from.$field;
                if let Some(source) = from.sources.get(stringify!($field)) {
                    game.metadata
                        .sources
                        .insert(stringify!($field).into(), source.clone());
                }
            }
        };
    }
    field!(description);
    field!(developers);
    field!(publishers);
    field!(genres);
    field!(features);
    field!(links);
    field!(age_rating);
    field!(series);
    field!(release_date);
    // Playnite-style scores: only fill when the current value matches the
    // request baseline, so manual corrections and in-flight edits survive.
    if from.critic_score.is_some()
        && game.metadata.critic_score == baseline.metadata.critic_score
        && (replace || game.metadata.critic_score.is_none())
    {
        game.metadata.critic_score = from.critic_score;
        if let Some(source) = from.sources.get("critic_score") {
            game.metadata
                .sources
                .insert("critic_score".into(), source.clone());
        }
    }
    // Tags stay manual-only (Playnite user labels). Provider results never
    // overwrite them; they are preserved here by not merging.
    for kind in [ImageKind::Cover, ImageKind::Background, ImageKind::Logo] {
        let (current, old, incoming) = match kind {
            ImageKind::Cover => (&mut game.cover, &baseline.cover, fetched.cover.clone()),
            ImageKind::Background => (
                &mut game.metadata.background,
                &baseline.metadata.background,
                from.background.clone(),
            ),
            ImageKind::Logo => (
                &mut game.metadata.logo,
                &baseline.metadata.logo,
                from.logo.clone(),
            ),
        };
        if incoming.is_some() && current == old && (replace || current.is_none()) {
            *current = incoming;
            if let Some(source) = from.sources.get(kind.field()) {
                game.metadata
                    .sources
                    .insert(kind.field().into(), source.clone());
            }
        }
    }
}

pub struct Provider {
    client: Client,
}
impl Provider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .connect_timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("TheLauncher/0.1 metadata")
                .build()
                .map_err(|_| anyhow::anyhow!("Cannot initialize metadata connection"))?,
        })
    }
    fn bytes(&self, request: RequestBuilder, limit: u64) -> Result<Vec<u8>> {
        let response = request.send().map_err(|_| {
            anyhow::anyhow!("Provider could not be reached (connection or timeout)")
        })?;
        match response.status().as_u16() {
            200..=299 => {}
            401 | 403 => {
                bail!("Provider denied access; check your API key or service availability")
            }
            429 => bail!("Provider rate limit reached; try again later"),
            404 => bail!("Provider has no data for this game"),
            _ => bail!("Provider returned an unsuccessful response"),
        }
        if response.content_length().is_some_and(|size| size > limit) {
            bail!("Provider response is too large");
        }
        let mut bytes = Vec::new();
        response
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| anyhow::anyhow!("Provider download was interrupted"))?;
        if bytes.len() as u64 > limit {
            bail!("Provider response is too large");
        }
        Ok(bytes)
    }
    fn json(&self, request: RequestBuilder) -> Result<Value> {
        serde_json::from_slice(&self.bytes(request, 2 * 1024 * 1024)?)
            .context("Provider returned invalid metadata")
    }
    pub fn search(&self, query: &str) -> Result<Vec<Match>> {
        if query.trim().is_empty() || query.len() > 256 {
            bail!("Enter a title up to 256 bytes");
        }
        let data = self.json(
            self.client
                .get("https://store.steampowered.com/api/storesearch/")
                .query(&[("term", query.trim()), ("l", "english"), ("cc", "US")]),
        )?;
        parse_matches(&data)
    }
    pub fn images(
        &self,
        id: u32,
        kind: ImageKind,
        provider: ArtworkProvider,
        key: &str,
    ) -> Result<Vec<ImageChoice>> {
        if id == 0 {
            bail!("Steam app ID must be positive");
        }
        if provider == ArtworkProvider::Steam {
            return Ok(steam_images(id, kind));
        }
        if key.trim().is_empty() {
            bail!("Enter a SteamGridDB API key for this session, or choose Steam official");
        }
        let resource = match kind {
            ImageKind::Cover => "grids",
            ImageKind::Background => "heroes",
            ImageKind::Logo => "logos",
        };
        let mut request = self
            .client
            .get(format!(
                "https://www.steamgriddb.com/api/v2/{resource}/steam/{id}"
            ))
            .bearer_auth(key.trim())
            .query(&[
                ("types", "static"),
                ("nsfw", "false"),
                ("humor", "false"),
                ("mimes", "image/png,image/jpeg"),
            ]);
        if kind == ImageKind::Cover {
            request = request.query(&[("dimensions", "600x900")]);
        }
        parse_images(&self.json(request)?)
    }
    pub fn download(&self, choice: &ImageChoice, cache: &Path) -> Result<PathBuf> {
        trusted_image_url(&choice.url)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        choice.url.hash(&mut hasher);
        let path = cache.join(format!("{:016x}.png", hasher.finish()));
        if path.is_file()
            && std::fs::metadata(&path)?.len() <= 16 * 1024 * 1024
            && validate_image(&std::fs::read(&path)?).is_ok()
        {
            return Ok(path);
        }
        let bytes = self.bytes(self.client.get(&choice.url), 16 * 1024 * 1024)?;
        let image = validate_image(&bytes)?.thumbnail(1920, 1920);
        std::fs::create_dir_all(cache)?;
        let mut temp = tempfile::NamedTempFile::new_in(cache)?;
        image.write_to(temp.as_file_mut(), image::ImageFormat::Png)?;
        temp.as_file().sync_all()?;
        temp.persist(&path).context("Could not save artwork")?;
        Ok(path)
    }
    pub fn fetch(&self, id: u32, settings: &Settings, key: &str, cache: &Path) -> Result<Fetched> {
        if id == 0 {
            bail!("Steam app ID must be positive");
        }
        let mut out = Fetched::default();
        if settings.download_text {
            let response = self
                .json(
                    self.client
                        .get("https://store.steampowered.com/api/appdetails")
                        .query(&[
                            ("appids", id.to_string()),
                            ("l", "english".into()),
                            ("cc", "US".into()),
                        ]),
                )
                .and_then(|v| parse_details(id, &v));
            match response {
                Ok(m) => out.metadata = m,
                Err(e) => out.warnings.push(format!("Steam text: {e}")),
            }
        }
        for (kind, enabled) in [
            (ImageKind::Cover, settings.download_cover),
            (ImageKind::Background, settings.download_background),
            (ImageKind::Logo, settings.download_logo),
        ] {
            if out
                .warnings
                .iter()
                .any(|warning| warning.contains("rate limit"))
            {
                break;
            }
            if !enabled {
                continue;
            }
            let mut choices = Vec::new();
            if settings.artwork_provider == ArtworkProvider::SteamGridDb {
                match self.images(id, kind, ArtworkProvider::SteamGridDb, key) {
                    Ok(c) => choices = c,
                    Err(e) => out
                        .warnings
                        .push(format!("SteamGridDB {}: {e}. Trying Steam.", kind.field())),
                }
            }
            choices.extend(steam_images(id, kind));
            let mut downloaded = false;
            for choice in choices {
                let path = match self.download(&choice, cache) {
                    Ok(path) => path,
                    Err(error) if error.to_string().contains("rate limit") => {
                        out.warnings.push(error.to_string());
                        break;
                    }
                    Err(_) => continue,
                };
                match kind {
                    ImageKind::Cover => out.cover = Some(path),
                    ImageKind::Background => out.metadata.background = Some(path),
                    ImageKind::Logo => out.metadata.logo = Some(path),
                }
                out.metadata
                    .sources
                    .insert(kind.field().into(), choice.source);
                downloaded = true;
                break;
            }
            if !downloaded {
                out.warnings.push(format!(
                    "No downloadable {} image; existing artwork kept",
                    kind.field()
                ));
            }
        }
        if out.metadata.sources.is_empty() {
            bail!("No metadata downloaded. {}", out.warnings.join(" "));
        }
        Ok(out)
    }
}
fn steam_images(id: u32, kind: ImageKind) -> Vec<ImageChoice> {
    let names: &[(&str, &str)] = match kind {
        ImageKind::Cover => &[
            ("Official portrait", "library_600x900.jpg"),
            ("Official portrait (large)", "library_600x900_2x.jpg"),
        ],
        ImageKind::Background => &[
            ("Library hero", "library_hero.jpg"),
            ("Store background", "page_bg_generated_v6b.jpg"),
            ("Store header", "header.jpg"),
        ],
        ImageKind::Logo => &[("Official logo", "logo.png")],
    };
    names
        .iter()
        .map(|(label, file)| ImageChoice {
            label: (*label).into(),
            url: format!("https://cdn.akamai.steamstatic.com/steam/apps/{id}/{file}"),
            source: "Steam".into(),
        })
        .collect()
}
fn parse_matches(value: &Value) -> Result<Vec<Match>> {
    let items = value["items"]
        .as_array()
        .context("Steam returned invalid search results")?;
    Ok(items
        .iter()
        .filter_map(|v| {
            Some(Match {
                app_id: positive_id(&v["id"])?,
                name: v["name"].as_str()?.trim().to_owned(),
            })
        })
        .filter(|m| !m.name.is_empty())
        .take(20)
        .collect())
}
fn positive_id(v: &Value) -> Option<u32> {
    u32::try_from(v.as_u64()?).ok().filter(|v| *v > 0)
}
fn parse_details(id: u32, value: &Value) -> Result<GameMetadata> {
    let result = &value[id.to_string()];
    if result["success"].as_bool() != Some(true) {
        bail!("Steam has no public store metadata for this app");
    }
    let data = &result["data"];
    if positive_id(&data["steam_appid"]) != Some(id) {
        bail!("Steam returned a different app ID");
    }
    let strings = |field: &str| -> Vec<String> {
        data[field]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .take(64)
            .map(|s| s.chars().take(256).collect())
            .collect()
    };
    let mut out = GameMetadata {
        description: plain_text(data["short_description"].as_str().unwrap_or_default()),
        developers: strings("developers"),
        publishers: strings("publishers"),
        genres: data["genres"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["description"].as_str())
            .take(64)
            .map(plain_text)
            .collect(),
        features: data["categories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["description"].as_str())
            .take(64)
            .map(plain_text)
            .collect(),
        links: vec![format!("https://store.steampowered.com/app/{id}")],
        age_rating: match data["required_age"].as_str().unwrap_or_default().trim() {
            "" | "0" => String::new(),
            raw => match raw.parse::<u8>() {
                Ok(0) => String::new(),
                Ok(age) => format!("{age}+"),
                Err(_) => String::new(),
            },
        },
        critic_score: data["metacritic"]["score"]
            .as_u64()
            .and_then(|score| u8::try_from(score).ok().filter(|score| *score > 0)),
        release_date: plain_text(data["release_date"]["date"].as_str().unwrap_or_default()),
        ..Default::default()
    };
    for (field, present) in [
        ("description", !out.description.is_empty()),
        ("developers", !out.developers.is_empty()),
        ("publishers", !out.publishers.is_empty()),
        ("genres", !out.genres.is_empty()),
        ("features", !out.features.is_empty()),
        ("links", !out.links.is_empty()),
        ("age_rating", !out.age_rating.is_empty()),
        ("critic_score", out.critic_score.is_some()),
        ("release_date", !out.release_date.is_empty()),
    ] {
        if present {
            out.sources.insert(field.into(), "Steam".into());
        }
    }
    Ok(out)
}
fn plain_text(value: &str) -> String {
    let mut text = String::new();
    let mut tag = false;
    for c in value.chars().take(16000) {
        match c {
            '<' => {
                tag = true;
                text.push(' ');
            }
            '>' => tag = false,
            _ if !tag => text.push(c),
            _ => {}
        }
    }
    let text = text
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn parse_images(value: &Value) -> Result<Vec<ImageChoice>> {
    if value["success"].as_bool() != Some(true) {
        bail!("SteamGridDB could not supply artwork");
    }
    let items = value["data"]
        .as_array()
        .context("SteamGridDB returned invalid artwork")?;
    let out: Vec<_> = items
        .iter()
        .filter(|v| v["nsfw"].as_bool() != Some(true) && v["humor"].as_bool() != Some(true))
        .filter_map(|v| {
            let url = v["url"].as_str()?;
            if trusted_image_url(url).is_err()
                || !(url.ends_with(".png") || url.ends_with(".jpg") || url.ends_with(".jpeg"))
            {
                return None;
            }
            Some(ImageChoice {
                label: format!(
                    "{} · {} × {} · {}",
                    v["author"]["name"].as_str().unwrap_or("Community"),
                    v["width"].as_u64().unwrap_or(0),
                    v["height"].as_u64().unwrap_or(0),
                    v["style"].as_str().unwrap_or("Artwork")
                ),
                url: url.into(),
                source: format!(
                    "SteamGridDB · {}",
                    v["author"]["name"].as_str().unwrap_or("Community")
                ),
            })
        })
        .take(6)
        .collect();
    if out.is_empty() {
        bail!("No supported static PNG/JPEG artwork found");
    }
    Ok(out)
}
fn trusted_image_url(raw: &str) -> Result<()> {
    let url = reqwest::Url::parse(raw).context("Invalid artwork URL")?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(host.ends_with(".steamstatic.com")
            || host == "steamcdn-a.akamaihd.net"
            || host == "cdn2.steamgriddb.com")
    {
        bail!("Artwork URL is outside supported provider media hosts");
    }
    Ok(())
}
fn validate_image(bytes: &[u8]) -> Result<image::DynamicImage> {
    if bytes.len() > 16 * 1024 * 1024 {
        bail!("Artwork is too large");
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        bail!("Artwork must be PNG or JPEG");
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
        .decode()
        .context("Artwork is invalid or exceeds 4096 × 4096")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_data_is_bounded_and_matches_exact_app() {
        let value = serde_json::json!({"42":{"success":true,"data":{"steam_appid":42,"short_description":"<b>Hello</b> &amp; welcome","developers":["Studio"],"genres":[{"description":"Action"}],"categories":[{"description":"Single-player"}],"required_age":"18","metacritic":{"score":88},"release_date":{"date":"Coming soon"}}}});
        let out = parse_details(42, &value).unwrap();
        assert_eq!(out.description, "Hello & welcome");
        assert_eq!(out.sources["genres"], "Steam");
        assert_eq!(out.features, vec!["Single-player"]);
        assert_eq!(out.links, vec!["https://store.steampowered.com/app/42"]);
        assert_eq!(out.age_rating, "18+");
        assert_eq!(out.critic_score, Some(88));
        assert_eq!(out.sources["features"], "Steam");
        assert_eq!(out.sources["critic_score"], "Steam");
        assert!(parse_details(43, &value).is_err());
        let wrong = serde_json::json!({"42":{"success":true,"data":{"steam_appid":43}}});
        assert!(parse_details(42, &wrong).is_err());
        assert!(parse_details(42, &serde_json::json!({"42":{"success":false}})).is_err());
        let matches = parse_matches(
            &serde_json::json!({"items":[{"id":0,"name":"bad"},{"id":42,"name":"OK"}]}),
        )
        .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].app_id, 42);
    }
    #[test]
    fn untrusted_artwork_and_unsupported_formats_are_rejected() {
        for url in [
            "http://cdn2.steamgriddb.com/a.png",
            "https://cdn2.steamgriddb.com.evil.test/a.png",
            "https://localhost/a.png",
            "https://u:p@cdn2.steamgriddb.com/a.png",
            "https://cdn2.steamgriddb.com/a.png?key=secret",
        ] {
            assert!(trusted_image_url(url).is_err(), "{url}");
        }
        let items = serde_json::json!({"success":true,"data":[{"url":"https://cdn2.steamgriddb.com/a.png","author":{"name":"Artist"},"width":600,"height":900},{"url":"https://cdn2.steamgriddb.com/b.gif"},{"url":"https://localhost/a.png"},{"url":"https://cdn2.steamgriddb.com/c.png","nsfw":true}]});
        let out = parse_images(&items).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].label.contains("Artist"));
        assert!(validate_image(b"<html>error</html>").is_err());
        let mut image = Cursor::new(Vec::new());
        image::RgbaImage::new(4097, 1)
            .write_to(&mut image, image::ImageFormat::Png)
            .unwrap();
        assert!(validate_image(image.get_ref()).is_err());
    }
    #[test]
    fn settings_round_trip_without_credentials_and_corruption_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.json");
        let settings = Settings {
            artwork_provider: ArtworkProvider::SteamGridDb,
            ..Default::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        assert!(!std::fs::read_to_string(&path).unwrap().contains("key"));
        std::fs::write(&path, b"bad").unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"bad");
    }
    fn sample_game() -> Game {
        serde_json::from_value(serde_json::json!({"name":"My custom name", "target":{"kind":"steam","app_id":42},"favorite":true,"hidden":true,"installed":false,"last_launched":123})).unwrap()
    }
    #[test]
    fn merges_only_missing_fields_and_respects_edits_during_download() {
        let mut game = sample_game();
        game.cover = Some(PathBuf::from("custom.png"));
        game.metadata.publishers = vec!["My publisher".into()];
        game.metadata.tags = vec!["my-tag".into()];
        let baseline = game.clone();
        game.metadata.description = "My edit while downloading".into();
        let fetched = Fetched {
            metadata: GameMetadata {
                description: "Downloaded description".into(),
                developers: vec!["Studio".into()],
                publishers: vec!["Provider publisher".into()],
                features: vec!["Single-player".into()],
                links: vec!["https://store.steampowered.com/app/42".into()],
                age_rating: "18+".into(),
                critic_score: Some(88),
                background: Some(PathBuf::from("background.png")),
                sources: BTreeMap::from([
                    ("description".into(), "Steam".into()),
                    ("developers".into(), "Steam".into()),
                    ("features".into(), "Steam".into()),
                    ("links".into(), "Steam".into()),
                    ("age_rating".into(), "Steam".into()),
                    ("critic_score".into(), "Steam".into()),
                    ("cover".into(), "Steam".into()),
                ]),
                ..Default::default()
            },
            cover: Some(PathBuf::from("download.png")),
            ..Default::default()
        };
        apply(&mut game, &baseline, fetched.clone(), false);
        assert_eq!(game.name, baseline.name);
        assert_eq!(game.target, baseline.target);
        assert_eq!(game.favorite, baseline.favorite);
        assert_eq!(game.hidden, baseline.hidden);
        assert_eq!(game.installed, baseline.installed);
        assert_eq!(game.last_launched, baseline.last_launched);
        assert_eq!(game.metadata.description, "My edit while downloading");
        assert_eq!(game.metadata.publishers, vec!["My publisher"]);
        assert_eq!(game.cover, baseline.cover);
        assert_eq!(game.metadata.developers, vec!["Studio"]);
        assert_eq!(game.metadata.features, vec!["Single-player"]);
        assert_eq!(game.metadata.critic_score, Some(88));
        assert_eq!(game.metadata.tags, vec!["my-tag"], "tags stay manual-only");
        assert!(!game.metadata.sources.contains_key("cover"));
        let next_baseline = game.clone();
        apply(&mut game, &next_baseline, fetched, true);
        assert_eq!(game.metadata.description, "Downloaded description");
        assert_eq!(game.cover, Some(PathBuf::from("download.png")));
        assert_eq!(game.metadata.sources["cover"], "Steam");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let library = crate::library::Library {
            schema_version: 1,
            games: vec![game],
        };
        library.save(&path).unwrap();
        assert_eq!(crate::library::Library::load(&path).unwrap(), library);
    }
    #[test]
    fn matched_executables_keep_their_launch_target_and_skip_existing_images() {
        let game:Game=serde_json::from_value(serde_json::json!({"name":"Local", "target":{"kind":"executable","path":"/game.exe","arguments":[],"working_directory":null},"cover":"/custom.png","metadata":{"steam_app_id":620}})).unwrap();
        assert_eq!(app_id(&game), Some(620));
        let settings = Settings::default().for_game(&game, false);
        assert!(!settings.download_cover);
        assert!(settings.download_text);
        assert!(settings.download_background);
        assert!(Settings::default().for_game(&game, true).download_cover);
        let old: Game = serde_json::from_value(
            serde_json::json!({"name":"Legacy", "target":{"kind":"steam","app_id":42}}),
        )
        .unwrap();
        assert_eq!(old.metadata, GameMetadata::default());
    }
    #[test]
    fn http_failures_are_bounded_and_never_expose_request_credentials() {
        use std::net::TcpListener;
        for response in [
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 2048];
                let _ = stream.read(&mut request);
                stream.write_all(response.as_bytes()).unwrap();
            });
            let provider = Provider::new().unwrap();
            let error = provider
                .bytes(
                    provider
                        .client
                        .get(format!("http://{address}/?key=TOP_SECRET")),
                    8,
                )
                .unwrap_err()
                .to_string();
            assert!(!error.contains("TOP_SECRET"));
            assert!(!error.contains("127.0.0.1"));
            assert!(error.contains("rate limit") || error.contains("too large"));
            server.join().unwrap();
        }
    }
}
