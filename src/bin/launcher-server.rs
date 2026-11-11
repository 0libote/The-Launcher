//! launcher-server: self-hosted catalogue and sync hub.
//!
//! Run it on a home server (see `Dockerfile.server` and `docs/server.md`),
//! publish the desktop library with `PUT /api/library`, then browse stats
//! and games from anywhere with the same API key. Launching stays on the
//! desktop app; `POST /api/launch*` answers 403 on purpose.

use std::path::PathBuf;

use anyhow::Result;
use the_launcher::server::{ServerConfig, serve};

fn arg_value(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
        if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
            return Some(value.to_string());
        }
    }
    None
}

fn main() -> Result<()> {
    let mut config = ServerConfig::from_env().unwrap_or_else(|error| {
        eprintln!("launcher-server: {error:#}");
        std::process::exit(1);
    });
    if let Some(dir) = arg_value("--data-dir") {
        config.data_dir = PathBuf::from(dir);
    }
    if let Some(bind) = arg_value("--bind") {
        config.bind = bind;
    }
    if let Some(key) = arg_value("--api-key") {
        config.api_key = key;
    }
    if let Some(public) = arg_value("--public-dir") {
        config.public_dir = Some(PathBuf::from(public));
    }
    if let Err(error) = config.validate() {
        eprintln!("launcher-server: {error:#}");
        std::process::exit(1);
    }
    if config.key_looks_weak() {
        eprintln!("launcher-server warning: the API key is short; use a long random value");
    }
    if let Err(error) = serve(config) {
        eprintln!("launcher-server: {error:#}");
        std::process::exit(1);
    }
    Ok(())
}
