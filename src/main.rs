#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod shell;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use fs2::FileExt;
use std::fs::OpenOptions;
use the_launcher::library::Library;

fn main() {
    if let Err(error) = run() {
        eprintln!("The Launcher could not start: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Headless tooling (snapshots, trees, traces) renders fixtures without
    // touching the window. It never needs the instance lock: scenes replace
    // the on-disk library in memory, and snapshots never persist.
    let headless = rune::tooling::is_headless();
    let dirs = ProjectDirs::from("", "", "TheLauncher")
        .context("Cannot locate your local app data folder")?;
    let data = dirs.data_local_dir();
    std::fs::create_dir_all(data)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data.join("library.lock"))?;
    let _guard = match FileExt::try_lock_exclusive(&lock) {
        Ok(()) => Some(lock),
        Err(error) if headless => {
            eprintln!("Headless run without the instance lock: {error:#}");
            None
        }
        Err(error) => {
            return Err(error)
                .context("The Launcher is already running. Close the other instance first");
        }
    };
    let path = data.join("library.json");
    let library =
        Library::load(&path).with_context(|| format!("Unable to load {}", path.display()))?;
    let state = shell::Shell::from_disk(library, path);
    rune::App::new(shell::spec()).init(move || state).run();
    drop(_guard);
    Ok(())
}
