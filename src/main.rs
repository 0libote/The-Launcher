#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use anyhow::{Context, Result};
use directories::ProjectDirs;
use fs2::FileExt;
use std::fs::OpenOptions;
use the_launcher::desktop::load_app_state;

fn main() {
    if let Err(error) = run() {
        eprintln!("The Launcher could not start: {error:#}");
        rfd::MessageDialog::new()
            .set_title("The Launcher could not start")
            .set_description(format!("{error:#}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Same data home as every previous release. Read or schema errors stop
    // startup; they never silently create an empty collection over data.
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
        Ok(()) => lock,
        Err(error) => {
            return Err(error)
                .context("The Launcher is already running. Close the other instance first");
        }
    };
    let state = load_app_state(data)?;
    for warning in state.startup_warnings() {
        eprintln!("The Launcher warning: {warning}");
    }
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            the_launcher::desktop::library_load,
            the_launcher::desktop::library_save_game,
            the_launcher::desktop::startup_warnings,
            the_launcher::desktop::library_set_favorite,
            the_launcher::desktop::library_set_hidden,
            the_launcher::desktop::library_remove,
            the_launcher::desktop::cover_set_local,
            the_launcher::desktop::cover_remove,
            the_launcher::desktop::artwork_thumbnail,
            the_launcher::desktop::artwork_preview,
            the_launcher::desktop::game_launch,
            the_launcher::desktop::import_steam,
            the_launcher::desktop::import_epic,
            the_launcher::desktop::import_xbox,
            the_launcher::desktop::metadata_search,
            the_launcher::desktop::metadata_image_choices,
            the_launcher::desktop::metadata_fetch,
            the_launcher::desktop::metadata_apply_image,
            the_launcher::desktop::workspace_load,
            the_launcher::desktop::workspace_save,
            the_launcher::desktop::preferences_save,
            the_launcher::desktop::settings_load,
            the_launcher::desktop::settings_save,
            the_launcher::desktop::data_folder_open,
            the_launcher::desktop::game_link_open,
            the_launcher::desktop::pick_executable,
            the_launcher::desktop::pick_image,
            the_launcher::desktop::pick_folder,
            the_launcher::desktop::pick_file,
            the_launcher::desktop::pick_save_file,
            the_launcher::desktop::backup_export,
            the_launcher::desktop::backup_import,
        ])
        .run(tauri::generate_context!())
        .context("The Launcher window could not start")?;
    drop(_guard);
    Ok(())
}
