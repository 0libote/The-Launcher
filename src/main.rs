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
    FileExt::try_lock_exclusive(&lock)
        .context("The Launcher is already running. Close the other instance first")?;
    let path = data.join("library.json");
    let library =
        Library::load(&path).with_context(|| format!("Unable to load {}", path.display()))?;
    let state = shell::Shell::from_disk(library, path);
    rune::App::new(shell::spec()).init(move || state).run();
    drop(lock);
    Ok(())
}
