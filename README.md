# The Launcher

A personal game shelf with a Svelte 5 / TypeScript interface and a Tauri 2 / Rust desktop core. Browse your PC collection, bring Steam/Epic/Xbox libraries together, and launch games from one place. Inspired by Playnite’s unified library and the tactile appeal of physical game shelves; this is an independent implementation.

**Status: desktop rewrite implemented; Windows runtime and installer validation remain.** The previous Rune renderer has been replaced. Existing schema-1 library data and its location are retained. This is not yet a signed, release-ready distribution.

## What you can do

- Browse grid, list, and shelf views; search titles, genres, and tags; filter favorites, recently played, installed games, and stores.
- Add and edit executable entries, arguments, working directories, tags, and series. Favorite, hide, restore, or remove entries without deleting game files.
- Import installed Steam, Epic, and Xbox entries. Steam optionally imports account-owned games using a session-only API key. Scans merge without replacing your custom titles, favorites, or covers.
- Launch through the executable or the correct store client. Known uninstalled Steam entries request installation, without recording a played timestamp.
- Match games to Steam metadata; download descriptions and artwork; preview/select alternate artwork; use local PNG/JPEG covers or original generated fallback art.
- Change named layouts, colors, cover sizes, navigation/details placement, and default view in **Preferences**. Changes preview live; Cancel restores the previous appearance.
- Export and restore a validated backup of library entries, workspace, and metadata preferences. Game files and artwork files are not included.
- Browse with keyboard controls, a controller, or the mouse. The responsive browser layout prepares the interface for a later mobile catalogue; it is not a server implementation.

Real libraries start empty. The explicit browser demo shows a catalogue of real store games with hotlinked artwork; demo edits stay in that tab. The app uses bundled fonts; no remote font service or art is required to browse offline.

## Develop

Install Rust (the version is pinned in `rust-toolchain.toml`) and Bun (pinned in `package.json`). On Windows, install Visual Studio Build Tools with Desktop development with C++ and a Windows SDK. Tauri requires WebView2 on Windows; the NSIS installer uses Tauri’s default WebView2 installation handling.

```sh
bun install --frozen-lockfile
bun run dev
bun run check
bun run build
```

`bun run dev` starts Vite and the native Tauri app. `bun run build` produces a release executable and an unsigned Windows NSIS installer under `target/release/`. Bun/Node and a development server are not shipped with the installed app. Native Windows controls provide window dragging, minimizing, maximizing, and closing.

For frontend development without native prerequisites:

```sh
bun run dev:web
```

Open `http://localhost:1420/?demo`. Demo edits stay in that browser tab’s session storage and never touch the real library. Without `?demo`, a plain browser displays a desktop-app explanation and an empty library. There is no Docker API or synchronization yet.

Linux is a development host. Native Linux builds require Tauri’s GTK/WebKit development packages; see [development](docs/development.md). `cargo test --lib` and `bun run check` test the Rust core without those GUI packages.

## Data and safety

The existing data location is preserved: `%LOCALAPPDATA%\TheLauncher\data\library.json` on Windows. **Preferences → Library & backups → Open library folder** opens the actual location. The adjacent `workspace.json`, `metadata.json`, and `artwork/` retain presentation, provider preferences, and downloaded art.

The lifetime OS lock prevents competing instances. Edits save an atomic replacement before updating memory; overlapping mutations are serialized. Damaged or newer library files stop startup with a recovery error instead of being overwritten. Close the app before manual edits and keep backups.

Arguments go one per line; values containing spaces stay unquoted. Launching uses argument vectors rather than shell strings and does not elevate permissions. Success means the OS accepted dispatch, not that a game finished loading. The launcher does not track playtime or running-state detection. Closing it leaves games running.

## Validation and direction

- [Rewrite report, screenshots, performance and limitations](docs/rewrite-report.md)
- [Architecture](docs/architecture.md)
- [Development and Windows smoke tests](docs/development.md)
- [Roadmap](docs/roadmap.md)
- [Appearance and workspace](docs/customization.md)
- [Visual design](docs/design.md)
- [Library format](docs/library-format.md)
- [Metadata](docs/metadata.md)
- [Steam](docs/steam-integration.md), [Epic](docs/epic-integration.md), [Xbox](docs/xbox-integration.md)

The next product steps are retro/physical ownership records, external emulator profiles, then an optional Docker-hosted mobile catalogue. Browser emulation and remote streaming are later integrations. A distribution license has not been chosen; choose one before publishing releases.
