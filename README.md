# The Launcher

A native Rust game launcher whose workspace is arranged by its user. Move panels, choose grid/list/shelf views, and save your own layouts and colors. Inspired by Playnite’s unified library and [BoxShelf’s](https://github.com/sassiestrhihno/boxshelf-beta) tactile shelves. This is an independent implementation; no source or artwork is copied from either project.

**Status: early foundation, not a packaged release.** Application logic and UI are Rust. Bun provides convenient development commands; the built application does not require Bun, Node, a browser, or a webview.

The bundled skin is a starting layout. Use **Customize** to build yours.

## What works

- Add a local game, choose its executable, name it, and set arguments and a working directory.
- Launch games, edit launch options, favorite entries, search, and filter by source.
- Detect Steam automatically, import installed games across libraries, and optionally merge owned/uninstalled games through the official account API. On Windows, launch/install through the Steam client.
- Arrange navigation/details/tools panels, choose grid/list/shelf views, edit colors, and save named layouts. The launcher owns its skinnable window frame.
- Persist a local JSON library with atomic replacement and a single-instance lock.
- Automatically fetch Steam descriptions, credits, genres, release dates, and official artwork. Match local executables to Steam titles, or choose optional SteamGridDB artwork with previews.
- Choose local PNG/JPEG cover art for any game, with original generated art as a fallback.
- Browse with mouse or arrow keys, press Enter to play, and toggle full screen with F11. Tab navigates controls.

No demo games are added to the real library. The generated cartridge labels are original artwork, not downloaded box art. Select a game and use **Options → Choose cover art…** to add a local image. Open **Metadata** for download defaults, or **Options → Metadata & artwork…** to match a title, fill missing fields, or choose images. Controller input and a 3D shelf are future work.

## Develop on Windows

Install [Rust stable](https://rustup.rs/) **1.92 or newer** with the MSVC toolchain and Visual Studio Build Tools (Desktop development with C++, including a Windows SDK). Install [Bun](https://bun.sh/docs/installation). A GPU/driver supporting OpenGL 3.3 or later is needed.

```powershell
bun run dev
bun run check
bun run build
```

There are no JavaScript packages to install. `cargo run` works directly too. First compilation downloads Rust dependencies and takes longer. Release output: `target/release/the-launcher.exe`. No installer, signing, or automatic update system exists yet.

Linux is a development host, not a supported release target. Install C/C++ build tools, pkg-config, and the X11/Wayland and OpenGL development packages for your distribution. A graphical session and a working native file-dialog backend are needed to exercise the UI. Linux development can launch native executables; Windows requires `.exe` files.

## Add and play

1. Select **Add game**, then **Browse…**, and choose a game’s `.exe`.
2. Give it a name. Arguments go one per line: `--profile` and `My Profile` are two lines. Do not wrap arguments containing spaces in quotes. Leave the working directory empty to use the executable’s folder.
3. Save, select the tile, and press **Play**. A launch request means the OS accepted the process/URI request, not that the game finished loading.
4. For installed Steam games, select **Import Steam → Import library**. Windows detects Steam through its registry path; use the optional override if needed. To include owned, uninstalled games, provide SteamID64 and a personal API key for that import. Read [Steam integration](docs/steam-integration.md) for details and parity limits.

Import is additive. It does not remove uninstalled games or overwrite your existing names/favorites. Steam manifests can include tools or DLC; this first importer does not query Steam’s catalogue to distinguish them. Disconnecting a drive skips that additional library. Malformed manifests produce warnings while valid entries still import. An unusable primary root or failed account request leaves the library unchanged. Refresh through Import Steam after Steam finishes an install/uninstall.

Removing an entry only removes it from this launcher, never from disk or Steam. Executables run with the launcher’s normal user permissions; add programs you trust.

## Data and recovery

On Windows, the library is `%LOCALAPPDATA%\TheLauncher\data\library.json`. Use **Open library folder** for the actual platform-specific location. The adjacent `library.lock` prevents two instances from overwriting each other; the OS releases it when the app exits, and the file may remain.

Back up `library.json` before manual edits. Close the app first. If JSON is damaged or its schema is newer than supported, startup stops and displays its path instead of resetting your collection. Restore your backup or repair the file. Atomic writes protect an existing library if saving fails; they are not a versioned backup system or a guarantee against every power-loss scenario.

## Project notes

- [Visual layout editor and skinnable frame](docs/customization.md)
- [Steam integration and Playnite reference](docs/steam-integration.md)
- [Metadata providers, automatic fetching, and artwork choices](docs/metadata.md)
- [Bundled visual system and cover art](docs/design.md)
- [Architecture and decisions](docs/architecture.md)
- [Development and validation](docs/development.md)
- [Roadmap](docs/roadmap.md)
- [Data format](docs/library-format.md)

Keep behavioral changes and their docs in the same change. Built-in metadata providers now complement local launching; external plugins and synchronization remain future work. The project’s distribution license has not been chosen yet; choose one before publishing releases.
