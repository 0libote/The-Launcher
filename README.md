# The Launcher

A native Rust game launcher with a Rune-rendered shelf. Inspired by Playnite’s unified library and [BoxShelf’s](https://github.com/sassiestrhihno/boxshelf-beta) tactile shelves. This is an independent implementation; no source or artwork is copied from either project.

**Status: Rune-native foundation, mid-migration, not a packaged release.** Application logic and the shell are Rust on Rune. Bun provides convenient development commands; the built application does not require Bun, Node, a browser, or a webview. Application logic and UI are Rust. Bun provides convenient development commands; the built application does not require Bun, Node, a browser, or a webview.

The card art is original gradient art with initials.

## What works

- Browse the shelf in grid, list, or shelf views, search by name, filter by collection (All/Favorites/PC/Steam/Epic/Xbox/Hidden), favorite games, and inspect details.
- Add PC games through a dialog with inline validation, edit launch options, and remove entries or hide store games (Steam/Epic/Xbox, restorable from Hidden).
- Import Steam, Epic, and Xbox libraries through background scans with warnings surfaced; entries merge without touching custom names or favorites.
- Fetch Steam descriptions, credits, genres, features, age ratings, critic scores, store links, and artwork; match PC/Epic/Xbox entries to Steam titles for metadata and pick alternate artwork. Automatic fills run after imports and saves unless disabled.
- Choose local PNG/JPEG cover art per game, with original generated art as a fallback.
- Arrange navigation/details panels, pick views and tile sizes, and restyle with named layouts and color presets, all saved to a workspace file.
- Launch games through Play, with errors surfaced in the status bar. On Windows, Steam entries play/install through the Steam client, Epic through the Epic launcher protocol, and Xbox through `explorer.exe shell:AppsFolder`.
- Persist a local JSON library with atomic replacement and a single-instance lock.
- Browse with the mouse and search by typing. Keyboard navigation and controller input are future work.

No demo games are added to the real library. The gradient card art is original, not downloaded box art. Artwork downloads come from Steam or SteamGridDB only when you fetch them. Controller input and television-distance tuning are future work.

## Develop on Windows

Install Rust **1.99.1** (pinned by `rust-toolchain.toml` for the Rune dependency) with the MSVC toolchain and Visual Studio Build Tools (Desktop development with C++, including a Windows SDK). Install [Bun](https://bun.sh/docs/installation). A GPU/driver supporting modern DirectX 12 or Vulkan is needed.

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
4. For installed Steam games, select **Import Steam → Import library**. Windows detects Steam through its registry path; use the optional override if needed. To include owned, uninstalled games, provide SteamID64 and a personal API key for that import. Read [Steam integration](docs/steam-integration.md) for details and parity limits. For Epic, use **Import Epic** (local `.item` manifests, see [Epic integration](docs/epic-integration.md)); for Xbox, use **Import Xbox** (installed UWP packages on Windows, see [Xbox integration](docs/xbox-integration.md)).

Import is additive. It does not remove uninstalled games or overwrite your existing names/favorites. Steam manifests can include tools or DLC; this importer excludes the redistributable but does not query Steam’s catalogue for every edge case. Epic manifests exclude DLC/plugins; Xbox imports include UWP packages that are not games until you hide them. Disconnecting a drive skips that additional library. Malformed manifests produce warnings while valid entries still import. An unusable primary root or failed account request leaves the library unchanged. Refresh through the matching Import after the store client finishes an install/uninstall.

Removing an entry only removes it from this launcher, never from disk or Steam. Executables run with the launcher’s normal user permissions; add programs you trust.

## Data and recovery

On Windows, the library is `%LOCALAPPDATA%\TheLauncher\data\library.json`. Use **Open library folder** for the actual platform-specific location. The adjacent `library.lock` prevents two instances from overwriting each other; the OS releases it when the app exits, and the file may remain.

Back up `library.json` before manual edits. Close the app first. If JSON is damaged or its schema is newer than supported, startup stops and displays its path instead of resetting your collection. Restore your backup or repair the file. Atomic writes protect an existing library if saving fails; they are not a versioned backup system or a guarantee against every power-loss scenario.

## Project notes

- [Visual layout editor and skinnable frame](docs/customization.md)
- [Steam integration and Playnite reference](docs/steam-integration.md)
- [Epic integration and Playnite reference](docs/epic-integration.md)
- [Xbox integration and Playnite reference](docs/xbox-integration.md)
- [Metadata providers, automatic fetching, and artwork choices](docs/metadata.md)
- [Bundled visual system and cover art](docs/design.md)
- [Architecture and decisions](docs/architecture.md)
- [Development and validation](docs/development.md)
- [Roadmap](docs/roadmap.md)
- [Data format](docs/library-format.md)

Keep behavioral changes and their docs in the same change. Built-in metadata providers now complement local launching; external plugins and synchronization remain future work. The project’s distribution license has not been chosen yet; choose one before publishing releases.
