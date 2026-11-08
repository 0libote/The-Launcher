#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod ui;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use fs2::FileExt;
use std::fs::OpenOptions;
use the_launcher::library::Library;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        rfd::MessageDialog::new()
            .set_title("The Launcher could not start")
            .set_description(format!("{error:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
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
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_icon(ui::window_icon())
            .with_min_inner_size([840.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "The Launcher",
        options,
        Box::new(move |cc| Ok(Box::new(ui::Launcher::new(cc, library, path)))),
    )
    .map_err(|e| anyhow::anyhow!("Window failed: {e}"))?;
    drop(lock);
    Ok(())
}
