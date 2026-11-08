use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Steamworks redistributable entry: a support package, never a game.
const REDIST_APP_ID: u32 = 228_980;
/// `StateFlags` bit marking a fully installed app.
const FULLY_INSTALLED: u32 = 4;
/// Official owned-games route, the same one upstream account import uses.
const OWNED_GAMES_URL: &str = "https://api.steampowered.com/IPlayerService/GetOwnedGames/v0001/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
    pub installed: bool,
    pub cover: Option<std::path::PathBuf>,
}

/// How to find Steam games. Everything is optional: `root` overrides
/// auto-detection, while `account_id` plus `api_key` add the optional
/// owned-games route for the current session only. The key is never stored
/// and never appears in errors or warnings.
#[derive(Default)]
pub struct ImportOptions {
    pub root: Option<PathBuf>,
    pub account_id: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ImportResult {
    pub games: Vec<SteamGame>,
    pub warnings: Vec<String>,
}

/// Locate the Steam installation: the Windows registry on Windows, the
/// usual home-directory locations elsewhere. An explicit
/// [`ImportOptions::root`] bypasses this entirely.
pub fn installation_path() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        windows_installation_path()
    }
    #[cfg(not(windows))]
    {
        unix_installation_path()
    }
}

#[cfg(windows)]
fn windows_installation_path() -> Result<PathBuf> {
    let key = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(r"Software\Valve\Steam")
        .map_err(|_| anyhow!("Steam installation not found in the registry"))?;
    let raw: String = key
        .get_value("SteamPath")
        .map_err(|_| anyhow!("Steam installation path is not set in the registry"))?;
    let path = PathBuf::from(raw.replace('/', "\\"));
    if path.is_dir() {
        Ok(path)
    } else {
        Err(anyhow!("Steam installation folder is missing"))
    }
}

#[cfg(not(windows))]
fn unix_installation_path() -> Result<PathBuf> {
    let home = directories::UserDirs::new()
        .map(|user| user.home_dir().to_path_buf())
        .filter(|home| !home.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .ok_or_else(|| anyhow!("Cannot locate a home directory to find Steam"))?;
    default_roots(&home)
        .into_iter()
        .find(|candidate| candidate.is_dir())
        .ok_or_else(|| anyhow!("Steam installation not found in the usual Linux locations"))
}

/// Usual Steam roots below a home directory, in preference order.
#[cfg(any(not(windows), test))]
fn default_roots(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".steam").join("steam"),
        home.join(".local").join("share").join("Steam"),
    ]
}

/// Local cached cover Steam keeps per app, if present. Never downloads:
/// missing art simply stays `None`.
fn cover_for(root: &Path, app_id: u32) -> Option<PathBuf> {
    let cache = root.join("appcache").join("librarycache");
    ["jpg", "png"]
        .into_iter()
        .map(|ext| cache.join(format!("{app_id}_library_600x900.{ext}")))
        .find(|path| path.is_file())
}

/// Import Steam games: always the local installed scan, plus the optional
/// owned-games route when both `account_id` (numeric Steam64 ID) and
/// `api_key` are supplied. Malformed manifests become warnings, never a
/// failed import; invalid account details fail instead of importing nothing.
pub fn import(options: ImportOptions) -> Result<ImportResult> {
    let root = match options.root {
        Some(root) => root,
        None => installation_path().context("Cannot find Steam automatically")?,
    };
    let (local, mut warnings) = collect_installed(&root)?;
    let account = match (options.account_id, options.api_key) {
        (None, None) => None,
        (Some(id), Some(key)) => Some((id, key)),
        (Some(_), None) | (None, Some(_)) => {
            bail!("Steam account import needs both the Steam64 ID and the API key")
        }
    };
    let Some((account_id, api_key)) = account else {
        return Ok(ImportResult {
            games: sorted(local.into_values().collect()),
            warnings,
        });
    };
    let steam_id = validate_account_id(&account_id)?;
    let key = api_key.trim().to_string();
    if key.is_empty() {
        bail!("Steam API key is empty");
    }
    let (owned, skipped_blank) = fetch_owned_games(steam_id, &key)?;
    if skipped_blank > 0 {
        warnings.push(format!(
            "Skipped {skipped_blank} owned entries without names"
        ));
    }
    Ok(ImportResult {
        games: merge_owned(&root, local, owned),
        warnings,
    })
}

/// Scan a Steam installation folder for fully installed games.
///
/// Looks in `root/steamapps` plus any extra libraries listed in
/// `root/steamapps/libraryfolders.vdf`, parses `appmanifest_*.acf` files,
/// and returns de-duplicated installed games sorted by name. One malformed
/// manifest is skipped; use [`import`] for the per-file warnings.
pub fn scan(root: &Path) -> Result<Vec<SteamGame>> {
    let (by_id, _) = collect_installed(root)?;
    Ok(sorted(by_id.into_values().collect()))
}

fn sorted(mut games: Vec<SteamGame>) -> Vec<SteamGame> {
    games.sort_by(|a, b| a.name.cmp(&b.name).then(a.app_id.cmp(&b.app_id)));
    games
}

/// Local scan shared by [`scan`] and [`import`].
///
/// Fatal errors (an unusable root) abort; per-file and per-library problems
/// become warnings so one bad manifest cannot hide the whole collection.
fn collect_installed(root: &Path) -> Result<(HashMap<u32, SteamGame>, Vec<String>)> {
    let mut warnings = Vec::new();
    if !root.is_dir() {
        bail!(
            "Invalid Steam folder '{}': it does not exist or is not a directory",
            root.display()
        );
    }
    let primary_steamapps = root.join("steamapps");
    if !primary_steamapps.is_dir() {
        bail!(
            "Invalid Steam folder '{}': 'steamapps' subdirectory not found",
            root.display()
        );
    }

    let mut libraries = vec![primary_steamapps.clone()];
    let mut seen = vec![canonical_key(&primary_steamapps)];
    match library_roots(&primary_steamapps.join("libraryfolders.vdf")) {
        Ok(extra) => {
            for lib_root in extra {
                let steamapps = lib_root.join("steamapps");
                if !steamapps.is_dir() {
                    warnings.push(format!(
                        "Skipping missing Steam library '{}'",
                        lib_root.display()
                    ));
                    continue;
                }
                let key = canonical_key(&steamapps);
                if !seen.contains(&key) {
                    seen.push(key);
                    libraries.push(steamapps);
                }
            }
        }
        Err(error) => warnings.push(format!("Ignoring Steam libraries file: {error:#}")),
    }

    let mut by_id: HashMap<u32, SteamGame> = HashMap::new();
    for (index, steamapps) in libraries.iter().enumerate() {
        let paths = match manifest_paths(steamapps) {
            Ok(paths) => paths,
            Err(error) => {
                if index == 0 {
                    return Err(error);
                }
                warnings.push(format!("Skipping unreadable Steam library: {error:#}"));
                continue;
            }
        };
        for manifest_path in &paths {
            let file_name = manifest_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            match parse_manifest(manifest_path, steamapps) {
                Ok(Some((app_id, name))) => {
                    by_id.entry(app_id).or_insert_with(|| SteamGame {
                        app_id,
                        name,
                        installed: true,
                        cover: cover_for(root, app_id),
                    });
                }
                Ok(None) => {}
                Err(error) => warnings.push(format!("Skipping '{file_name}': {error:#}")),
            }
        }
    }
    Ok((by_id, warnings))
}

fn canonical_key(p: &Path) -> PathBuf {
    // Cheap duplicate suppression without touching the filesystem: the
    // libraryfolders.vdf "0" entry usually repeats the chosen root.
    p.components().collect()
}

/// Extra library roots from `libraryfolders.vdf`.
///
/// Supports both the legacy format (`"1" "D:\\Lib"`) and the current format
/// (`"1" { "path" "D:\\Lib" ... }`). Missing file means "primary only".
fn library_roots(vdf_path: &Path) -> Result<Vec<PathBuf>> {
    if !vdf_path.exists() {
        return Ok(Vec::new());
    }
    if !vdf_path.is_file() {
        bail!(
            "Cannot read Steam libraries file '{}': not a file",
            vdf_path.display()
        );
    }
    let text = fs::read_to_string(vdf_path)
        .with_context(|| format!("Cannot read Steam libraries file '{}'", vdf_path.display()))?;
    let kv = parse_kv(&text)
        .with_context(|| format!("Cannot parse Steam libraries file '{}'", vdf_path.display()))?;

    let Some(folders) = find_key_ci(&kv, "libraryfolders").and_then(|v| v.as_obj()) else {
        bail!(
            "Malformed Steam libraries file '{}': missing libraryfolders section",
            vdf_path.display()
        );
    };
    let mut roots = Vec::new();
    for (key, value) in folders {
        let path_str = match value {
            KvValue::Str(s) if key.trim().chars().all(|c| c.is_ascii_digit()) => s.clone(),
            KvValue::Obj(map) => match find_key_ci(map, "path").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            },
            _ => continue,
        };
        if path_str.trim().is_empty() {
            continue;
        }
        roots.push(PathBuf::from(path_str));
    }
    Ok(roots)
}

/// Sorted `appmanifest_*.acf` files inside one `steamapps` directory.
/// Temporary `*.tmp` copies Steam is still writing are skipped.
fn manifest_paths(steamapps: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(steamapps)
        .with_context(|| format!("Cannot list Steam apps in '{}'", steamapps.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry
            .with_context(|| format!("Cannot list Steam apps in '{}'", steamapps.display()))?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if entry.path().is_file()
            && name.starts_with("appmanifest_")
            && name.ends_with(".acf")
            && !name.to_lowercase().ends_with(".tmp")
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Parse one manifest; `Ok(None)` means "present but not installed".
///
/// Errors describe skipped files for warnings; filtering (StateFlags,
/// missing install dir, appid 0, the redistributable) returns `Ok(None)`.
fn parse_manifest(manifest_path: &Path, steamapps: &Path) -> Result<Option<(u32, String)>> {
    let text = fs::read_to_string(manifest_path).map_err(|_| anyhow!("cannot be read"))?;
    let kv = parse_kv(&text).map_err(|_| anyhow!("cannot be parsed"))?;

    let state = match find_key_ci(&kv, "AppState") {
        Some(KvValue::Obj(map)) => map,
        _ => return Err(anyhow!("missing 'AppState' section")),
    };

    let app_id: u32 = match find_key_ci(state, "appid").and_then(|v| v.as_str()) {
        Some(raw) => raw.trim().parse().map_err(|_| anyhow!("invalid 'appid'"))?,
        None => return Err(anyhow!("missing 'appid'")),
    };
    if app_id == 0 || app_id == REDIST_APP_ID {
        return Ok(None);
    }

    // Upstream falls back to `UserConfig/name` when `name` is blank.
    let name = match find_key_ci(state, "name").and_then(|v| v.as_str()) {
        Some(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
        _ => match find_key_ci(state, "UserConfig")
            .and_then(|v| v.as_obj())
            .and_then(|config| find_key_ci(config, "name"))
            .and_then(|v| v.as_str())
        {
            Some(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
            _ => return Err(anyhow!("missing 'name'")),
        },
    };

    // Only fully installed entries: StateFlags bit 4. Missing/garbled flags
    // mean "not ready", not "broken manifest".
    let installed = match find_key_ci(state, "StateFlags").and_then(|v| v.as_str()) {
        Some(s) => s
            .trim()
            .parse::<u32>()
            .is_ok_and(|flags| flags & FULLY_INSTALLED != 0),
        None => false,
    };
    if !installed {
        return Ok(None);
    }

    let installdir = find_key_ci(state, "installdir")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if !installdir_is_safe(installdir) {
        return Ok(None);
    }
    let game_dir = steamapps
        .join("common")
        .join(installdir_relative(installdir));
    if !game_dir.is_dir() {
        return Ok(None);
    }

    Ok(Some((app_id, name)))
}

/// Reject anything that could escape `steamapps/common`: absolute paths
/// (Unix, Windows drive, UNC) and any `.`, `..`, or empty segment, checked
/// on both `/` and `\` so Windows fixtures are caught on any host.
fn installdir_is_safe(installdir: &str) -> bool {
    if installdir.is_empty() {
        return false;
    }
    if installdir.starts_with('/') || installdir.starts_with('\\') {
        return false;
    }
    let bytes = installdir.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return false;
    }
    for part in installdir.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." {
            return false;
        }
    }
    let path = Path::new(installdir);
    if path.is_absolute() {
        return false;
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => return false,
        }
    }
    true
}

/// Join an install dir without letting a Windows separator escape handling:
/// split on both separators and re-join as relative components.
fn installdir_relative(installdir: &str) -> PathBuf {
    installdir.split(['/', '\\']).collect()
}

fn validate_account_id(raw: &str) -> Result<u64> {
    match raw.trim().parse::<u64>() {
        Ok(id) if id > 0 => Ok(id),
        _ => Err(anyhow!(
            "Invalid Steam account ID; expected the numeric Steam64 ID"
        )),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedEntry {
    app_id: u32,
    name: String,
}

fn fetch_owned_games(steam_id: u64, api_key: &str) -> Result<(Vec<OwnedEntry>, usize)> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| anyhow!("Cannot prepare the Steam account request"))?;
    // The key travels only inside this request. Nothing below logs it or
    // echoes it back inside an error.
    let response = client
        .get(OWNED_GAMES_URL)
        .query(&[
            ("key".to_owned(), api_key.to_owned()),
            ("steamid".to_owned(), steam_id.to_string()),
            ("include_appinfo".to_owned(), "1".to_owned()),
            ("include_played_free_games".to_owned(), "1".to_owned()),
            ("format".to_owned(), "json".to_owned()),
        ])
        .send()
        .map_err(|_| {
            anyhow!("Cannot reach the Steam account service; check the network and try again")
        })?;
    let status = response.status();
    if !status.is_success() {
        match status.as_u16() {
            400 => bail!("Steam rejected the account ID; check the Steam64 ID and try again"),
            401 | 403 => bail!("Steam rejected the API key; check the key and try again"),
            429 => bail!("Steam is rate-limiting requests; wait a bit and try again"),
            code => bail!("Steam account request failed (HTTP {code}); try again later"),
        }
    }
    let mut text = String::new();
    use std::io::Read;
    response
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| anyhow!("Cannot read the Steam account response"))?;
    if text.len() > 16 * 1024 * 1024 {
        bail!("Steam account response is too large");
    }
    parse_owned_json(&text)
}

#[derive(Debug, Deserialize)]
struct OwnedPayload {
    #[serde(default)]
    response: OwnedInner,
}

#[derive(Debug, Deserialize, Default)]
struct OwnedInner {
    #[serde(default)]
    games: Vec<OwnedRaw>,
}

#[derive(Debug, Deserialize, Default)]
struct OwnedRaw {
    #[serde(default)]
    appid: u32,
    #[serde(default)]
    name: String,
}

/// Parse an `IPlayerService/GetOwnedGames` body: names with `u32` app IDs,
/// deduplicated, without the redistributable or nameless entries.
///
/// An empty result is an error, never a silent empty import: it means the
/// profile is private, the account owns nothing visible, or the details do
/// not grant access.
fn parse_owned_json(text: &str) -> Result<(Vec<OwnedEntry>, usize)> {
    let payload: OwnedPayload = serde_json::from_str(text)
        .map_err(|_| anyhow!("Cannot understand the Steam account response"))?;
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut skipped_blank = 0;
    for raw in payload.response.games {
        if raw.appid == 0 || raw.appid == REDIST_APP_ID {
            continue;
        }
        let name = raw.name.trim().to_string();
        if name.is_empty() {
            skipped_blank += 1;
            continue;
        }
        if seen.insert(raw.appid) {
            entries.push(OwnedEntry {
                app_id: raw.appid,
                name,
            });
        }
    }
    if entries.is_empty() {
        bail!(
            "Steam account shows no visible owned games; the profile may be private or the account may own nothing. Nothing was imported"
        );
    }
    Ok((entries, skipped_blank))
}

/// Merge owned games over the local scan by app ID: installed flags come
/// from the local scan, online names win when present.
fn merge_owned(
    root: &Path,
    local: HashMap<u32, SteamGame>,
    owned: Vec<OwnedEntry>,
) -> Vec<SteamGame> {
    let mut by_id = local;
    for entry in owned {
        match by_id.get_mut(&entry.app_id) {
            Some(game) => game.name = entry.name,
            None => {
                let cover = cover_for(root, entry.app_id);
                by_id.insert(
                    entry.app_id,
                    SteamGame {
                        app_id: entry.app_id,
                        name: entry.name,
                        installed: false,
                        cover,
                    },
                );
            }
        }
    }
    sorted(by_id.into_values().collect())
}

// --- Minimal quoted Valve KeyValues parser ---

#[derive(Debug, Clone, PartialEq, Eq)]
enum KvValue {
    Str(String),
    Obj(BTreeMap<String, KvValue>),
}

impl KvValue {
    fn as_str(&self) -> Option<&str> {
        match self {
            KvValue::Str(s) => Some(s),
            _ => None,
        }
    }
    fn as_obj(&self) -> Option<&BTreeMap<String, KvValue>> {
        match self {
            KvValue::Obj(m) => Some(m),
            _ => None,
        }
    }
}

fn find_key_ci<'a>(map: &'a BTreeMap<String, KvValue>, key: &str) -> Option<&'a KvValue> {
    map.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Str(String),
    LBrace,
    RBrace,
}

/// Tokenize quoted KeyValues: quoted strings with `\"`/`\\` escapes,
/// braces, whitespace, and `//` line comments. Bare words are tolerated so
/// slightly off files still parse instead of failing mysteriously.
fn tokenize(input: &str) -> Result<Vec<Token>> {
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '"' {
            i += 1;
            let mut s = String::new();
            let mut closed = false;
            while i < chars.len() {
                let e = chars[i];
                if e == '\\' {
                    i += 1;
                    if i >= chars.len() {
                        bail!("Unterminated escape in quoted string");
                    }
                    match chars[i] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        other => s.push(other),
                    }
                    i += 1;
                } else if e == '"' {
                    closed = true;
                    i += 1;
                    break;
                } else {
                    s.push(e);
                    i += 1;
                }
            }
            if !closed {
                bail!("Unterminated quoted string");
            }
            tokens.push(Token::Str(s));
            continue;
        }
        if c == '{' {
            tokens.push(Token::LBrace);
            i += 1;
            continue;
        }
        if c == '}' {
            tokens.push(Token::RBrace);
            i += 1;
            continue;
        }
        let mut word = String::new();
        while i < chars.len()
            && !chars[i].is_whitespace()
            && chars[i] != '{'
            && chars[i] != '}'
            && chars[i] != '"'
        {
            if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                break;
            }
            word.push(chars[i]);
            i += 1;
        }
        if !word.is_empty() {
            tokens.push(Token::Str(word));
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn parse_map(&mut self, depth: usize) -> Result<BTreeMap<String, KvValue>> {
        if depth > 64 {
            bail!("Steam KeyValues nesting is too deep");
        }
        let mut map = BTreeMap::new();
        while self.pos < self.tokens.len() {
            if self.tokens[self.pos] == Token::RBrace {
                break;
            }
            let Token::Str(key) = self.tokens[self.pos].clone() else {
                bail!("Expected a key string");
            };
            self.pos += 1;
            if self.pos >= self.tokens.len() {
                bail!("Missing value for key '{key}'");
            }
            match self.tokens[self.pos].clone() {
                Token::Str(value) => {
                    self.pos += 1;
                    map.insert(key, KvValue::Str(value));
                }
                Token::LBrace => {
                    self.pos += 1;
                    let child = self.parse_map(depth + 1)?;
                    if self.pos >= self.tokens.len() || self.tokens[self.pos] != Token::RBrace {
                        bail!("Missing closing brace for section '{key}'");
                    }
                    self.pos += 1;
                    map.insert(key, KvValue::Obj(child));
                }
                Token::RBrace => bail!("Missing value for key '{key}'"),
            }
        }
        Ok(map)
    }
}

fn parse_kv(input: &str) -> Result<BTreeMap<String, KvValue>> {
    let tokens = tokenize(input)?;
    let mut parser = Parser { tokens, pos: 0 };
    let map = parser.parse_map(0)?;
    if parser.pos != parser.tokens.len() {
        bail!("Unexpected trailing data");
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_manifest(
        steamapps: &Path,
        app_id: u32,
        name: &str,
        state_flags: u32,
        installdir: &str,
        create_game_dir: bool,
    ) {
        if create_game_dir {
            fs::create_dir_all(steamapps.join("common").join(installdir)).unwrap();
        }
        let content = format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{state_flags}\"\n\t\"installdir\"\t\t\"{installdir}\"\n}}\n"
        );
        fs::write(steamapps.join(format!("appmanifest_{app_id}.acf")), content).unwrap();
    }

    fn primary_fixture(dir: &Path) -> PathBuf {
        let steamapps = dir.join("steamapps");
        fs::create_dir_all(steamapps.join("common")).unwrap();
        steamapps
    }

    fn offline_options(dir: &Path) -> ImportOptions {
        ImportOptions {
            root: Some(dir.to_path_buf()),
            account_id: None,
            api_key: None,
        }
    }

    #[test]
    fn rejects_extreme_nesting() {
        let nested = format!("{}{}", "\"key\" {".repeat(100), "}".repeat(100));
        assert!(parse_kv(&nested).is_err());
    }

    #[test]
    fn finds_sorted_games_and_dedupes_by_app_id() {
        let root_tmp = tempfile::tempdir().unwrap();
        let steamapps = primary_fixture(root_tmp.path());
        write_manifest(&steamapps, 20, "Zulu", 4, "Zulu", true);
        write_manifest(&steamapps, 10, "Alpha", 4, "Alpha", true);
        // Duplicate manifest file for the same app: loser's name must not leak in.
        fs::write(
            steamapps.join("appmanifest_10_dup.acf"),
            "\"AppState\"\n{\n\"appid\" \"10\"\n\"name\" \"Alpha Clone\"\n\"StateFlags\" \"4\"\n\"installdir\" \"Alpha\"\n}\n",
        )
        .unwrap();

        let games = scan(root_tmp.path()).unwrap();
        assert_eq!(
            games,
            vec![
                SteamGame {
                    app_id: 10,
                    name: "Alpha".into(),
                    installed: true,
                    cover: None,
                },
                SteamGame {
                    app_id: 20,
                    name: "Zulu".into(),
                    installed: true,
                    cover: None,
                },
            ]
        );
    }

    #[test]
    fn skips_not_installed_missing_dir_zero_id_and_unsafe_dirs() {
        let root_tmp = tempfile::tempdir().unwrap();
        let steamapps = primary_fixture(root_tmp.path());
        write_manifest(&steamapps, 1, "Good", 4, "Good", true);
        write_manifest(&steamapps, 2, "Downloading", 6 - 4, "Downloading", true); // no bit 4
        write_manifest(&steamapps, 3, "Updating", 6, "Updating", true); // bit 4 set
        write_manifest(&steamapps, 4, "Ghost", 4, "Ghost", false); // common dir missing
        write_manifest(&steamapps, 0, "Zero", 4, "Zero", true); // appid 0 excluded
        write_manifest(&steamapps, 5, "Traversal", 4, "../evil", false);
        fs::create_dir_all(steamapps.join("common").join("evil")).unwrap();
        write_manifest(&steamapps, 6, "Absolute", 4, "/evil", false);
        write_manifest(&steamapps, 7, "Drive", 4, "C:\\evil", false);

        let games = scan(root_tmp.path()).unwrap();
        assert_eq!(
            games.iter().map(|g| g.app_id).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert!(games.iter().all(|g| g.installed));
    }

    #[test]
    fn partial_bad_manifest_is_a_warning_not_a_failure() {
        let root_tmp = tempfile::tempdir().unwrap();
        let steamapps = primary_fixture(root_tmp.path());
        write_manifest(&steamapps, 1, "Good", 4, "Good", true);
        fs::write(
            steamapps.join("appmanifest_2.acf"),
            "\"AppState\" { \"oops\" ",
        )
        .unwrap();
        // A stale download copy Steam is still writing must not even warn.
        fs::write(steamapps.join("appmanifest_3.acf.tmp"), "partial").unwrap();

        let result = import(offline_options(root_tmp.path())).unwrap();
        assert_eq!(result.games.len(), 1);
        assert_eq!(result.games[0].app_id, 1);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("appmanifest_2.acf"));

        // The backward-compatible scan stays tolerant too.
        let games = scan(root_tmp.path()).unwrap();
        assert_eq!(games.len(), 1);
    }

    #[test]
    fn redistributable_is_never_imported() {
        let root_tmp = tempfile::tempdir().unwrap();
        let steamapps = primary_fixture(root_tmp.path());
        write_manifest(
            &steamapps,
            REDIST_APP_ID,
            "Steamworks Common Redistributables",
            4,
            "Steamworks Shared",
            true,
        );

        let result = import(offline_options(root_tmp.path())).unwrap();
        assert!(result.games.is_empty());
        assert!(result.warnings.is_empty());

        let (entries, _) = parse_owned_json(
            r#"{"response":{"game_count":2,"games":[
                {"appid":228980,"name":"Steamworks Common Redistributables"},
                {"appid":10,"name":"Alpha"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            entries,
            vec![OwnedEntry {
                app_id: 10,
                name: "Alpha".into()
            }]
        );
    }

    #[test]
    fn name_falls_back_to_userconfig() {
        let root_tmp = tempfile::tempdir().unwrap();
        let steamapps = primary_fixture(root_tmp.path());
        fs::create_dir_all(steamapps.join("common").join("Fallback")).unwrap();
        fs::write(
            steamapps.join("appmanifest_50.acf"),
            "\"AppState\"\n{\n\"appid\" \"50\"\n\"StateFlags\" \"4\"\n\"installdir\" \"Fallback\"\n\"UserConfig\"\n{\n\"name\" \"Fallback Name\"\n}\n}\n",
        )
        .unwrap();
        // No name anywhere: skipped with a warning, not a failure.
        fs::create_dir_all(steamapps.join("common").join("Nameless")).unwrap();
        fs::write(
            steamapps.join("appmanifest_51.acf"),
            "\"AppState\"\n{\n\"appid\" \"51\"\n\"StateFlags\" \"4\"\n\"installdir\" \"Nameless\"\n}\n",
        )
        .unwrap();

        let result = import(offline_options(root_tmp.path())).unwrap();
        assert_eq!(result.games.len(), 1);
        assert_eq!(result.games[0].app_id, 50);
        assert_eq!(result.games[0].name, "Fallback Name");
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("appmanifest_51.acf"));
    }

    #[test]
    fn online_parse_dedupes_and_marks_installed() {
        let (entries, skipped_blank) = parse_owned_json(
            r#"{"response":{"game_count":7,"games":[
                {"appid":10,"name":"Alpha"},
                {"appid":10,"name":"Alpha Duplicate"},
                {"appid":20,"name":"Beta"},
                {"appid":228980,"name":"Steamworks Common Redistributables"},
                {"appid":0,"name":"Zero"},
                {"appid":30,"name":""},
                {"appid":40}]}}"#,
        )
        .unwrap();
        assert_eq!(
            entries,
            vec![
                OwnedEntry {
                    app_id: 10,
                    name: "Alpha".into()
                },
                OwnedEntry {
                    app_id: 20,
                    name: "Beta".into()
                },
            ]
        );
        // One blank name and one missing name.
        assert_eq!(skipped_blank, 2);

        let root_tmp = tempfile::tempdir().unwrap();
        let local = HashMap::from([(
            10,
            SteamGame {
                app_id: 10,
                name: "Old Alpha".into(),
                installed: true,
                cover: None,
            },
        )]);
        let games = merge_owned(root_tmp.path(), local, entries);
        assert_eq!(games.len(), 2);
        // Installed locally: keeps the flag, online name wins.
        assert_eq!(games[0].app_id, 10);
        assert_eq!(games[0].name, "Alpha");
        assert!(games[0].installed);
        // Owned only: not installed.
        assert_eq!(games[1].app_id, 20);
        assert!(!games[1].installed);
    }

    #[test]
    fn empty_or_garbled_owned_responses_are_errors_not_empty_imports() {
        assert!(parse_owned_json(r#"{"response":{}}"#).is_err());
        assert!(parse_owned_json(r#"{"response":{"game_count":0}}"#).is_err());
        assert!(parse_owned_json("not json at all").is_err());
    }

    #[test]
    fn discovery_helpers_find_roots_and_cached_covers() {
        let home = Path::new("/home/player");
        assert_eq!(
            default_roots(home),
            vec![
                home.join(".steam").join("steam"),
                home.join(".local").join("share").join("Steam"),
            ]
        );

        let root_tmp = tempfile::tempdir().unwrap();
        let cache = root_tmp.path().join("appcache").join("librarycache");
        fs::create_dir_all(&cache).unwrap();
        assert_eq!(cover_for(root_tmp.path(), 70), None);
        fs::write(cache.join("71_library_600x900.png"), "cover").unwrap();
        assert_eq!(
            cover_for(root_tmp.path(), 71),
            Some(cache.join("71_library_600x900.png"))
        );
        // jpg wins when both cached sizes exist.
        fs::write(cache.join("72_library_600x900.png"), "cover").unwrap();
        fs::write(cache.join("72_library_600x900.jpg"), "cover").unwrap();
        assert_eq!(
            cover_for(root_tmp.path(), 72),
            Some(cache.join("72_library_600x900.jpg"))
        );
    }

    #[test]
    fn discovers_multiple_libraries_new_format() {
        let root_tmp = tempfile::tempdir().unwrap();
        let primary = primary_fixture(root_tmp.path());
        write_manifest(&primary, 1, "Primary Game", 4, "Primary Game", true);

        let extra_tmp = tempfile::tempdir().unwrap();
        let extra_steamapps = extra_tmp.path().join("steamapps");
        fs::create_dir_all(extra_steamapps.join("common")).unwrap();
        let extra_manifest = |id: u32, name: &str| {
            write_manifest(&extra_steamapps, id, name, 4, name, true);
        };
        extra_manifest(2, "Extra Game");

        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
            root_tmp.path().display().to_string().replace('\\', "\\\\"),
            extra_tmp.path().display().to_string().replace('\\', "\\\\"),
        );
        fs::write(primary.join("libraryfolders.vdf"), vdf).unwrap();

        let result = import(offline_options(root_tmp.path())).unwrap();
        let mut names: Vec<String> = result.games.into_iter().map(|g| g.name).collect();
        names.sort();
        assert_eq!(names, vec!["Extra Game", "Primary Game"]);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn legacy_libraryfolders_with_escaped_windows_paths() {
        // Escaped backslashes must unescape to real Windows paths. They do
        // not exist on this host, so they warn but must not break the
        // primary library scan.
        let root_tmp = tempfile::tempdir().unwrap();
        let primary = primary_fixture(root_tmp.path());
        write_manifest(&primary, 9, "Local", 4, "Local", true);
        fs::write(
            primary.join("libraryfolders.vdf"),
            "\"LibraryFolders\"\n{\n\t\"1\"\t\t\"D:\\\\SteamLibrary\"\n\t\"2\"\t\t\"E:\\\\Games\\\\Lib\"\n}\n",
        )
        .unwrap();

        let result = import(offline_options(root_tmp.path())).unwrap();
        assert_eq!(result.games.len(), 1);
        assert_eq!(result.games[0].name, "Local");
        assert_eq!(result.warnings.len(), 2);

        // Tokenizer-level proof that the escaping was handled.
        let kv = parse_kv("\"LibraryFolders\"\n{\n\"1\" \"D:\\\\SteamLibrary\"\n}").unwrap();
        let folders = find_key_ci(&kv, "libraryfolders")
            .unwrap()
            .as_obj()
            .unwrap();
        assert_eq!(
            find_key_ci(folders, "1").unwrap().as_str().unwrap(),
            "D:\\SteamLibrary"
        );
    }

    #[test]
    fn malformed_libraryfolders_is_a_warning() {
        let root_tmp = tempfile::tempdir().unwrap();
        let primary = primary_fixture(root_tmp.path());
        write_manifest(&primary, 1, "Good", 4, "Good", true);
        fs::write(primary.join("libraryfolders.vdf"), "\"libraryfolders\" { ").unwrap();

        let result = import(offline_options(root_tmp.path())).unwrap();
        assert_eq!(result.games.len(), 1);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("libraries file"));
    }

    #[test]
    fn invalid_roots_still_fail() {
        assert!(scan(Path::new("/definitely/not/a/steam/root")).is_err());
        assert!(import(offline_options(Path::new("/definitely/not/a/steam/root"))).is_err());
        let file_tmp = tempfile::NamedTempFile::new().unwrap();
        assert!(scan(file_tmp.path()).is_err());
        let no_steamapps = tempfile::tempdir().unwrap();
        assert!(scan(no_steamapps.path()).is_err());
    }

    #[test]
    fn account_import_validates_before_any_network() {
        let root_tmp = tempfile::tempdir().unwrap();
        primary_fixture(root_tmp.path());
        let secret = "SECRET-KEY-VALUE-12345";
        // Half-supplied credentials fail without touching the network.
        for options in [
            ImportOptions {
                root: Some(root_tmp.path().to_path_buf()),
                account_id: Some("76561198000000000".into()),
                api_key: None,
            },
            ImportOptions {
                root: Some(root_tmp.path().to_path_buf()),
                account_id: None,
                api_key: Some(secret.into()),
            },
            ImportOptions {
                root: Some(root_tmp.path().to_path_buf()),
                account_id: Some("not-a-steam-id".into()),
                api_key: Some(secret.into()),
            },
            ImportOptions {
                root: Some(root_tmp.path().to_path_buf()),
                account_id: Some("76561198000000000".into()),
                api_key: Some("   ".into()),
            },
        ] {
            let error = format!("{:?}", import(options).unwrap_err());
            assert!(!error.contains(secret), "error must not echo the key value");
        }
        assert!(validate_account_id("76561198000000000").is_ok());
        assert!(validate_account_id(" 76561198000000000 ").is_ok());
        assert!(validate_account_id("abc").is_err());
        assert!(validate_account_id("0").is_err());
    }

    #[test]
    fn duplicate_ids_across_libraries_dedupe() {
        let root_tmp = tempfile::tempdir().unwrap();
        let primary = primary_fixture(root_tmp.path());
        write_manifest(&primary, 42, "Primary Copy", 4, "Primary Copy", true);

        let extra_tmp = tempfile::tempdir().unwrap();
        let extra_steamapps = extra_tmp.path().join("steamapps");
        fs::create_dir_all(extra_steamapps.join("common")).unwrap();
        write_manifest(&extra_steamapps, 42, "Extra Copy", 4, "Extra Copy", true);

        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
            root_tmp.path().display(),
            extra_tmp.path().display(),
        );
        fs::write(primary.join("libraryfolders.vdf"), vdf).unwrap();

        let games = scan(root_tmp.path()).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].app_id, 42);
    }
}
