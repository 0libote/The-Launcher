//! Self-hosted companion server: a read-only catalogue and sync hub.
//!
//! The desktop app remains the only place that launches games. This server
//! exposes the library over HTTP with a bearer API key so phones and other
//! devices can browse stats and games, and desktops can publish (`PUT`) and
//! collect (`GET`) the same `library.json` as a source of truth.
//!
//! Deliberately dependency-free (only `std` plus the existing `anyhow`,
//! `serde` and `serde_json`): it must build wherever the core library builds,
//! including hosts without GUI packages.
//!
//! ```text
//! GET  /api/health            no auth; liveness for container orchestration
//! GET  /api/stats             auth; totals by source, favorites, recency
//! GET  /api/library           auth; the whole versioned library document
//! PUT  /api/library           auth; validate + atomically replace the library
//! GET  /api/games             auth; the game list with a total count
//! GET  /api/games/<handle>    auth; one entry by stable handle
//! POST /api/launch*           403; launching stays on the desktop agent
//! ```

use std::io::{BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::library::Library;

/// Crate version advertised by `/api/health`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Upper bounds keep one hostile connection from exhausting the host.
const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub data_dir: PathBuf,
    pub bind: String,
    pub api_key: String,
    pub public_dir: Option<PathBuf>,
}

impl ServerConfig {
    /// Read configuration from the environment. The key must be supplied;
    /// everything else has container-friendly defaults.
    ///
    /// - `LAUNCHER_DATA_DIR` (default `/data`): persistent volume holding `library.json`
    /// - `LAUNCHER_BIND` (default `0.0.0.0:8080`)
    /// - `LAUNCHER_API_KEY` (required): bearer token the app sends
    /// - `LAUNCHER_PUBLIC_DIR` (optional): built web UI served as static files
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("LAUNCHER_API_KEY").unwrap_or_default();
        let config = Self {
            data_dir: std::env::var("LAUNCHER_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/data")),
            bind: std::env::var("LAUNCHER_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            public_dir: std::env::var("LAUNCHER_PUBLIC_DIR").ok().map(PathBuf::from),
            api_key,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.api_key.trim().is_empty() {
            anyhow::bail!(
                "Set LAUNCHER_API_KEY to a long random value; the server refuses to start without one"
            );
        }
        Ok(())
    }

    pub fn library_path(&self) -> PathBuf {
        self.data_dir.join("library.json")
    }

    /// Short keys are almost always a forgotten placeholder. Start anyway so
    /// a home lab is not blocked, but say so loudly in the logs.
    pub fn key_looks_weak(&self) -> bool {
        self.api_key.trim().len() < 16
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceCounts {
    pub pc: usize,
    pub steam: usize,
    pub epic: usize,
    pub xbox: usize,
}

/// Small numbers for the server catalogue header and for clients that only
/// need an overview before pulling the whole library.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stats {
    pub total: usize,
    pub favorites: usize,
    pub hidden: usize,
    pub installed: usize,
    pub recently_played: usize,
    pub by_source: SourceCounts,
    pub last_launched_max: Option<u64>,
}

pub fn stats_for(library: &Library) -> Stats {
    let mut stats = Stats {
        total: library.games.len(),
        favorites: 0,
        hidden: 0,
        installed: 0,
        recently_played: 0,
        by_source: SourceCounts::default(),
        last_launched_max: None,
    };
    for game in &library.games {
        if game.favorite {
            stats.favorites += 1;
        }
        if game.hidden {
            stats.hidden += 1;
        }
        if game.installed == Some(true) {
            stats.installed += 1;
        }
        if game.last_launched.is_some() {
            stats.recently_played += 1;
        }
        match game.source() {
            "PC" => stats.by_source.pc += 1,
            "Steam" => stats.by_source.steam += 1,
            "Epic" => stats.by_source.epic += 1,
            "Xbox" => stats.by_source.xbox += 1,
            _ => {}
        }
        if game.last_launched > stats.last_launched_max {
            stats.last_launched_max = game.last_launched;
        }
    }
    stats
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Constant-time bearer check so key length is not leaked through timing.
pub fn check_auth(header: Option<&str>, api_key: &str) -> bool {
    let Some(header) = header else {
        return false;
    };
    let token = header.strip_prefix("Bearer ").unwrap_or("");
    constant_time_eq(token.as_bytes(), api_key.trim().as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Shared server state. The mutex only guards the in-memory clone; file
/// writes use the same atomic-replace helper as the desktop app.
pub struct ServerState {
    pub config: ServerConfig,
    library: Mutex<Library>,
}

impl ServerState {
    pub fn load(config: ServerConfig) -> Result<Self> {
        config.validate()?;
        let library = Library::load(&config.library_path())
            .with_context(|| format!("Unable to load {}", config.library_path().display()))?;
        Ok(Self {
            config,
            library: Mutex::new(library),
        })
    }

    pub fn snapshot(&self) -> Library {
        self.library.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn stats(&self) -> Stats {
        self.library
            .lock()
            .map(|l| stats_for(&l))
            .unwrap_or_else(|_| stats_for(&Library::default()))
    }

    /// Validate a published library, persist it atomically, then publish it
    /// to readers. Last writer wins; conflict-aware merging is future work.
    pub fn replace(&self, next: Library) -> Result<usize> {
        next.validate()?;
        next.save(&self.config.library_path())?;
        let count = next.games.len();
        if let Ok(mut library) = self.library.lock() {
            *library = next;
        }
        Ok(count)
    }
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn route_path(&self) -> &str {
        self.path.split(['?', '#']).next().unwrap_or("/")
    }
}

fn read_request(stream: &TcpStream) -> Result<Request> {
    let mut reader = BufReader::new(stream);
    let mut head = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        head.push(byte[0]);
        if head.len() > MAX_HEADER_BYTES {
            anyhow::bail!("Request headers are too large");
        }
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8(head).context("Request is not valid HTTP")?;
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_uppercase();
    let path = parts.next().unwrap_or("/").to_string();
    let mut headers = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    let content_length: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        anyhow::bail!("Request body is too large");
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Content Too Large",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

fn send(
    stream: &mut TcpStream,
    code: u16,
    content_type: &str,
    extra: &[(&str, &str)],
    body: &[u8],
) {
    let mut head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        reason(code),
        body.len()
    );
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn cors() -> [(&'static str, &'static str); 3] {
    [
        ("Access-Control-Allow-Origin", "*"),
        (
            "Access-Control-Allow-Headers",
            "Authorization, Content-Type",
        ),
        ("Access-Control-Allow-Methods", "GET, PUT, POST, OPTIONS"),
    ]
}

fn json(stream: &mut TcpStream, code: u16, value: &impl Serialize) {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    send(stream, code, "application/json", &cors(), &body);
}

fn error(stream: &mut TcpStream, code: u16, message: impl Into<String>) {
    json(
        stream,
        code,
        &serde_json::json!({ "error": message.into() }),
    );
}

fn percent_decode(raw: &str) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

fn serve_static(state: &ServerState, stream: &mut TcpStream, route: &str) {
    let Some(root) = state.config.public_dir.clone() else {
        error(stream, 404, "No web UI is bundled with this server build");
        return;
    };
    let relative = route.trim_start_matches('/');
    let candidate = if relative.is_empty() {
        root.join("index.html")
    } else {
        root.join(relative)
    };
    // Never escape the public directory.
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        error(stream, 404, "Unknown page");
        return;
    }
    if candidate.is_file() {
        serve_file(stream, &candidate);
        return;
    }
    // Single-page fallback so deep links into the catalogue still load.
    let index = root.join("index.html");
    if !relative.contains('.') && index.is_file() {
        serve_file(stream, &index);
        return;
    }
    error(stream, 404, "Unknown page");
}

fn serve_file(stream: &mut TcpStream, path: &Path) {
    match std::fs::read(path) {
        Ok(bytes) => send(stream, 200, content_type_for(path), &[], &bytes),
        Err(_) => error(stream, 404, "Unknown page"),
    }
}

fn handle_api(state: &Arc<ServerState>, stream: &mut TcpStream, request: &Request) {
    if request.method == "OPTIONS" {
        send(stream, 204, "text/plain; charset=utf-8", &cors(), b"");
        return;
    }

    let route = request.route_path().to_string();

    // Liveness probe stays unauthenticated for container orchestration.
    if route == "/api/health" && request.method == "GET" {
        let library = state.snapshot();
        json(
            stream,
            200,
            &serde_json::json!({
                "status": "ok",
                "version": VERSION,
                "games": library.games.len(),
            }),
        );
        return;
    }

    if !check_auth(request.header("authorization"), &state.config.api_key) {
        send(
            stream,
            401,
            "application/json",
            &[
                ("Access-Control-Allow-Origin", "*"),
                ("WWW-Authenticate", "Bearer"),
            ],
            br#"{"error":"Provide the server API key as a Bearer token"}"#,
        );
        return;
    }

    match (request.method.as_str(), route.as_str()) {
        ("GET", "/api/stats") => {
            let stats = state.stats();
            json(
                stream,
                200,
                &serde_json::json!({
                    "server_time": unix_now(),
                    "stats": stats,
                }),
            );
        }
        ("GET", "/api/library") => {
            json(stream, 200, &state.snapshot());
        }
        ("PUT", "/api/library") => {
            let parsed: Result<Library, _> = serde_json::from_slice(&request.body);
            match parsed {
                Ok(next) => match state.replace(next) {
                    Ok(count) => json(
                        stream,
                        200,
                        &serde_json::json!({ "games": count, "total": count }),
                    ),
                    Err(e) => error(stream, 400, format!("{e:#}")),
                },
                Err(e) => error(stream, 400, format!("Library is not valid: {e}")),
            }
        }
        ("GET", "/api/games") => {
            let library = state.snapshot();
            json(
                stream,
                200,
                &serde_json::json!({ "games": library.games, "total": library.games.len() }),
            );
        }
        ("GET", path) if path.starts_with("/api/games/") => {
            let handle = percent_decode(path.trim_start_matches("/api/games/"));
            let library = state.snapshot();
            match library.games.iter().find(|g| g.handle() == handle) {
                Some(game) => json(stream, 200, game),
                None => error(stream, 404, "No game with that handle on this server"),
            }
        }
        ("POST", path)
            if path == "/api/launch"
                || path.starts_with("/api/launch/")
                || path.contains("/launch") =>
        {
            error(
                stream,
                403,
                "Launching is disabled on the server; play from the desktop app",
            );
        }
        _ => error(stream, 404, "Unknown API route"),
    }
}

/// Serve one connection: parse, route, answer, close.
pub fn handle_connection(state: &Arc<ServerState>, stream: TcpStream) {
    let mut stream = stream;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(15)));
    let request = match read_request(&stream) {
        Ok(request) => request,
        Err(e) => {
            let message = e.to_string();
            if message.contains("too large") {
                error(&mut stream, 413, message);
            } else {
                error(
                    &mut stream,
                    400,
                    format!("Could not read request: {message}"),
                );
            }
            return;
        }
    };
    if request.route_path().starts_with("/api/") {
        handle_api(state, &mut stream, &request);
    } else if request.method == "GET" || request.method == "HEAD" {
        serve_static(state, &mut stream, request.route_path());
    } else if request.method == "OPTIONS" {
        send(&mut stream, 204, "text/plain; charset=utf-8", &cors(), b"");
    } else {
        error(
            &mut stream,
            405,
            "Only GET, PUT, POST and OPTIONS are supported",
        );
    }
}

/// Bind and serve forever, one thread per connection.
pub fn serve(config: ServerConfig) -> Result<()> {
    let state = Arc::new(ServerState::load(config.clone())?);
    let listener =
        TcpListener::bind(&config.bind).with_context(|| format!("Cannot bind {}", config.bind))?;
    println!("launcher-server {} listening on {}", VERSION, config.bind);
    println!("data: {}", config.library_path().display());
    if let Some(public) = &config.public_dir {
        println!("web UI: {}", public.display());
    }
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                std::thread::spawn(move || handle_connection(&state, stream));
            }
            Err(e) => eprintln!("launcher-server: connection failed: {e:#}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Game, Target};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn game(name: &str, target: Target) -> Game {
        Game {
            metadata: Default::default(),
            name: name.into(),
            target,
            favorite: false,
            last_launched: None,
            cover: None,
            installed: None,
            hidden: false,
        }
    }

    fn sample_library() -> Library {
        // `Library::add` requires an absolute executable path, and what
        // counts as absolute differs per OS, so anchor the fixture in the
        // platform temp dir instead of a Unix literal.
        let exe = std::env::temp_dir().join("launcher-server-test-game.exe");
        let mut library = Library::default();
        library
            .add(game(
                "Local",
                Target::Executable {
                    path: exe,
                    arguments: vec![],
                    working_directory: None,
                },
            ))
            .unwrap();
        let mut steam = game("Steam hit", Target::Steam { app_id: 10 });
        steam.favorite = true;
        steam.installed = Some(true);
        steam.last_launched = Some(1_700_000_000);
        library.add(steam).unwrap();
        let mut epic = game(
            "Epic hit",
            Target::Epic {
                app_name: "Fortnite".into(),
                catalog_namespace: String::new(),
                catalog_item_id: String::new(),
            },
        );
        epic.hidden = true;
        library.add(epic).unwrap();
        library
    }

    #[test]
    fn stats_count_sources_and_recency() {
        let stats = stats_for(&sample_library());
        assert_eq!(stats.total, 3);
        assert_eq!(stats.favorites, 1);
        assert_eq!(stats.hidden, 1);
        assert_eq!(stats.installed, 1);
        assert_eq!(stats.recently_played, 1);
        assert_eq!(
            stats.by_source,
            SourceCounts {
                pc: 1,
                steam: 1,
                epic: 1,
                xbox: 0
            }
        );
        assert_eq!(stats.last_launched_max, Some(1_700_000_000));
    }

    #[test]
    fn bearer_tokens_compare_exactly() {
        assert!(check_auth(Some("Bearer correct-horse"), "correct-horse"));
        assert!(!check_auth(Some("Bearer correct-horse"), "correct-horsf"));
        assert!(!check_auth(Some("Bearer "), "correct-horse"));
        assert!(!check_auth(None, "correct-horse"));
        assert!(!check_auth(Some("correct-horse"), "correct-horse"));
        assert!(!check_auth(Some("Bearer "), ""));
        assert!(!check_auth(Some("Bearer anything"), ""));
    }

    #[test]
    fn config_requires_an_api_key() {
        let empty = ServerConfig {
            data_dir: PathBuf::from("/tmp"),
            bind: "127.0.0.1:0".into(),
            api_key: "   ".into(),
            public_dir: None,
        };
        assert!(empty.validate().is_err());
        let weak = ServerConfig {
            api_key: "short".into(),
            ..empty.clone()
        };
        assert!(weak.validate().is_ok());
        assert!(weak.key_looks_weak());
    }

    fn test_state() -> (tempfile::TempDir, Arc<ServerState>) {
        let dir = tempfile::tempdir().unwrap();
        let config = ServerConfig {
            data_dir: dir.path().to_path_buf(),
            bind: "127.0.0.1:0".into(),
            api_key: "test-key-1234567890".into(),
            public_dir: None,
        };
        let state = Arc::new(ServerState::load(config).unwrap());
        (dir, state)
    }

    fn round_trip(state: &Arc<ServerState>, raw: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = {
            let state = Arc::clone(state);
            std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                handle_connection(&state, stream);
            })
        };
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut out = Vec::new();
        stream.read_to_end(&mut out).unwrap();
        worker.join().unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    fn get(path: &str, auth: bool) -> String {
        format!(
            "GET {path} HTTP/1.1\r\nHost: x\r\n{}Connection: close\r\n\r\n",
            if auth {
                "Authorization: Bearer test-key-1234567890\r\n"
            } else {
                ""
            }
        )
    }

    #[test]
    fn health_is_public_but_library_needs_the_key() {
        let (_dir, state) = test_state();
        let health = round_trip(&state, &get("/api/health", false));
        assert!(health.contains("200 OK"), "{health}");
        assert!(health.contains("\"status\":\"ok\""), "{health}");
        let denied = round_trip(&state, &get("/api/library", false));
        assert!(denied.contains("401 Unauthorized"), "{denied}");
        let wrong = round_trip(
            &state,
            &get("/api/library", true).replace("test-key", "wrong-key"),
        );
        assert!(wrong.contains("401 Unauthorized"), "{wrong}");
    }

    #[test]
    fn library_round_trips_through_put_and_get() {
        let (_dir, state) = test_state();
        let before = round_trip(&state, &get("/api/library", true));
        assert!(before.contains("\"games\":[]"), "{before}");
        let body = serde_json::to_vec(&sample_library()).unwrap();
        let put = format!(
            "PUT /api/library HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer test-key-1234567890\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            String::from_utf8(body).unwrap()
        );
        let saved = round_trip(&state, &put);
        assert!(saved.contains("200 OK"), "{saved}");
        assert!(saved.contains("\"games\":3"), "{saved}");
        let stats = round_trip(&state, &get("/api/stats", true));
        assert!(stats.contains("\"total\":3"), "{stats}");
        assert!(stats.contains("\"steam\":1"), "{stats}");
        let games = round_trip(&state, &get("/api/games", true));
        assert!(games.contains("Steam hit"), "{games}");
        let one = round_trip(&state, &get("/api/games/steam:10", true));
        assert!(one.contains("Steam hit"), "{one}");
        let missing = round_trip(&state, &get("/api/games/steam:999", true));
        assert!(missing.contains("404 Not Found"), "{missing}");
        // The persisted file matches what readers see.
        let on_disk = Library::load(&state.config.library_path()).unwrap();
        assert_eq!(on_disk.games.len(), 3);
    }

    #[test]
    fn invalid_libraries_are_rejected_without_touching_disk() {
        let (_dir, state) = test_state();
        let bad_body = r#"{"schema_version":1,"games":"nope"}"#;
        let bad = format!(
            "PUT /api/library HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer test-key-1234567890\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{bad_body}",
            bad_body.len()
        );
        let response = round_trip(&state, &bad);
        assert!(response.contains("400 Bad Request"), "{response}");
        assert!(!state.config.library_path().exists());
        let dupe = {
            let mut library = sample_library();
            library.games.push(library.games[0].clone());
            serde_json::to_vec(&library).unwrap()
        };
        let dupe_request = format!(
            "PUT /api/library HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer test-key-1234567890\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            dupe.len(),
            String::from_utf8(dupe).unwrap()
        );
        let response = round_trip(&state, &dupe_request);
        assert!(response.contains("400 Bad Request"), "{response}");
        assert!(!state.config.library_path().exists());
    }

    #[test]
    fn launch_is_explicitly_forbidden_on_the_server() {
        let (_dir, state) = test_state();
        let response = round_trip(
            &state,
            "POST /api/launch HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer test-key-1234567890\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
        assert!(response.contains("403 Forbidden"), "{response}");
    }

    #[test]
    fn static_files_serve_and_never_escape_the_public_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<h1>catalogue</h1>").unwrap();
        let config = ServerConfig {
            data_dir: dir.path().join("data"),
            bind: "127.0.0.1:0".into(),
            api_key: "test-key-1234567890".into(),
            public_dir: Some(dir.path().to_path_buf()),
        };
        let state = Arc::new(ServerState::load(config).unwrap());
        let home = round_trip(&state, &get("/", false));
        assert!(home.contains("200 OK"), "{home}");
        assert!(home.contains("catalogue"), "{home}");
        let deep = round_trip(&state, &get("/games/steam:10", false));
        assert!(deep.contains("catalogue"), "{deep}");
        let escape = round_trip(&state, &get("/../secret", false));
        assert!(escape.contains("404"), "{escape}");
    }
}
