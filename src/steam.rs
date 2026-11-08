use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
}

/// Scan a user-chosen Steam installation folder for fully installed games.
///
/// Looks in `root/steamapps` plus any extra libraries listed in
/// `root/steamapps/libraryfolders.vdf`, parses `appmanifest_*.acf` files,
/// and returns de-duplicated games sorted by name.
pub fn scan(root: &Path) -> Result<Vec<SteamGame>> {
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

    let mut library_steamapps: Vec<PathBuf> = vec![primary_steamapps.clone()];
    let mut seen_dirs: Vec<PathBuf> = vec![canonical_key(&primary_steamapps)];
    for lib_root in library_roots(&primary_steamapps.join("libraryfolders.vdf"))? {
        let steamapps = lib_root.join("steamapps");
        // A library on a disconnected drive (or a Windows path fixture on
        // Unix) simply contributes nothing; only the chosen root is required.
        if !steamapps.is_dir() {
            continue;
        }
        let key = canonical_key(&steamapps);
        if !seen_dirs.contains(&key) {
            seen_dirs.push(key);
            library_steamapps.push(steamapps);
        }
    }

    let mut by_id: HashMap<u32, SteamGame> = HashMap::new();
    for steamapps in &library_steamapps {
        for manifest_path in manifest_paths(steamapps)? {
            if let Some(game) = parse_manifest(&manifest_path, steamapps)? {
                by_id.entry(game.app_id).or_insert(game);
            }
        }
    }

    let mut games: Vec<SteamGame> = by_id.into_values().collect();
    games.sort_by(|a, b| a.name.cmp(&b.name).then(a.app_id.cmp(&b.app_id)));
    Ok(games)
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
/// A present-but-broken file is an error, not a silent empty scan.
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
fn manifest_paths(steamapps: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(steamapps)
        .with_context(|| format!("Cannot list Steam apps in '{}'", steamapps.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry
            .with_context(|| format!("Cannot list Steam apps in '{}'", steamapps.display()))?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if entry.path().is_file() && name.starts_with("appmanifest_") && name.ends_with(".acf") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

/// Parse one manifest; `Ok(None)` means "present but not fully installed".
///
/// Errors only when the file cannot be read or understood; filtering
/// (StateFlags, missing install dir, appid 0) returns `Ok(None)`.
fn parse_manifest(manifest_path: &Path, steamapps: &Path) -> Result<Option<SteamGame>> {
    let text = fs::read_to_string(manifest_path)
        .with_context(|| format!("Cannot read Steam manifest '{}'", manifest_path.display()))?;
    let kv = parse_kv(&text)
        .with_context(|| format!("Cannot parse Steam manifest '{}'", manifest_path.display()))?;
    let ctx = || format!("Malformed Steam manifest '{}'", manifest_path.display());

    let state = match find_key_ci(&kv, "AppState") {
        Some(KvValue::Obj(map)) => map,
        Some(_) => bail!(
            "Malformed Steam manifest '{}': 'AppState' is not a section",
            manifest_path.display()
        ),
        None => bail!(
            "Malformed Steam manifest '{}': missing 'AppState' section",
            manifest_path.display()
        ),
    };

    let app_id: u32 = match find_key_ci(state, "appid").and_then(|v| v.as_str()) {
        Some(s) => s.trim().parse().with_context(|| {
            format!(
                "Malformed Steam manifest '{}': invalid 'appid'",
                manifest_path.display()
            )
        })?,
        None => return Err(anyhow::anyhow!(ctx())).with_context(|| "missing 'appid'"),
    };
    if app_id == 0 {
        return Ok(None);
    }
    let name = match find_key_ci(state, "name").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Err(anyhow::anyhow!(ctx())).with_context(|| "missing 'name'"),
    };

    // Only fully installed entries: StateFlags bit 4. Missing/garbled flags
    // mean "not ready", not "broken manifest".
    let installed = match find_key_ci(state, "StateFlags").and_then(|v| v.as_str()) {
        Some(s) => s.trim().parse::<u32>().is_ok_and(|flags| flags & 4 != 0),
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

    Ok(Some(SteamGame { app_id, name }))
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

    #[test]
    fn rejects_extreme_nesting_and_missing_library_section() {
        let nested = format!("{}{}", "\"key\" {".repeat(100), "}".repeat(100));
        assert!(parse_kv(&nested).is_err());
        let root = tempfile::tempdir().unwrap();
        let primary = primary_fixture(root.path());
        fs::write(primary.join("libraryfolders.vdf"), "\"other\" {}").unwrap();
        assert!(scan(root.path()).is_err());
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
                    name: "Alpha".into()
                },
                SteamGame {
                    app_id: 20,
                    name: "Zulu".into()
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

        let mut names: Vec<String> = scan(root_tmp.path())
            .unwrap()
            .into_iter()
            .map(|g| g.name)
            .collect();
        names.sort();
        assert_eq!(names, vec!["Extra Game", "Primary Game"]);
    }

    #[test]
    fn legacy_libraryfolders_with_escaped_windows_paths() {
        // Escaped backslashes must unescape to real Windows paths. They do
        // not exist on this host, so they contribute nothing but must not
        // break the primary library scan.
        let root_tmp = tempfile::tempdir().unwrap();
        let primary = primary_fixture(root_tmp.path());
        write_manifest(&primary, 9, "Local", 4, "Local", true);
        fs::write(
            primary.join("libraryfolders.vdf"),
            "\"LibraryFolders\"\n{\n\t\"1\"\t\t\"D:\\\\SteamLibrary\"\n\t\"2\"\t\t\"E:\\\\Games\\\\Lib\"\n}\n",
        )
        .unwrap();

        let games = scan(root_tmp.path()).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "Local");

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
    fn malformed_manifests_and_roots_are_errors_not_empty() {
        let root_tmp = tempfile::tempdir().unwrap();
        let steamapps = primary_fixture(root_tmp.path());
        write_manifest(&steamapps, 1, "Good", 4, "Good", true);
        fs::write(
            steamapps.join("appmanifest_2.acf"),
            "\"AppState\" { \"oops\" ",
        )
        .unwrap();
        assert!(scan(root_tmp.path()).is_err());

        fs::remove_file(steamapps.join("appmanifest_2.acf")).unwrap();
        fs::write(steamapps.join("appmanifest_3.acf"), "not kv at all {{{").unwrap();
        assert!(scan(root_tmp.path()).is_err());

        assert!(scan(Path::new("/definitely/not/a/steam/root")).is_err());
        let file_tmp = tempfile::NamedTempFile::new().unwrap();
        assert!(scan(file_tmp.path()).is_err());
        let no_steamapps = tempfile::tempdir().unwrap();
        assert!(scan(no_steamapps.path()).is_err());
    }

    #[test]
    fn malformed_libraryfolders_is_an_error() {
        let root_tmp = tempfile::tempdir().unwrap();
        let primary = primary_fixture(root_tmp.path());
        write_manifest(&primary, 1, "Good", 4, "Good", true);
        fs::write(primary.join("libraryfolders.vdf"), "\"libraryfolders\" { ").unwrap();
        assert!(scan(root_tmp.path()).is_err());
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
