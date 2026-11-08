# Architecture

## Decision: native Rust, Bun for commands

The user’s constraint is a fully Rust launcher that uses Bun. A webview stack would put UI behavior in JavaScript or require a Rust-to-Wasm build pipeline. Instead, eframe/egui renders a native Rust interface through OpenGL. Bun runs the development commands and the small cross-platform check script. It is not an application runtime or dependency manager for Rust; Cargo owns Rust dependencies and `Cargo.lock` pins them.

This keeps a single language for the application and leaves room for a more deliberate console UI. Tradeoffs: native egui is not HTML/CSS, accessibility depends on egui/AccessKit and platform support, and gamepad navigation requires explicit work. Hardware and screen-reader checks on Windows remain necessary.

## Boundaries

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Local data path, instance lock, fail-safe startup, window creation |
| `src/ui.rs` | Theme, library browsing, keyboard controls, dialogs, background Steam import |
| `src/library.rs` | Versioned data model, duplicate checks, transactional edits and atomic persistence |
| `src/launch.rs` | Validate launch targets, construct launch plans, start processes or Steam URIs |
| `src/steam.rs` | Read-only discovery of Steam libraries and installed app manifests |
| `scripts/check.ts` | Run formatting, Clippy, and tests through Bun on Windows or Unix |

There is no plugin API, service container, database, or event bus. Add boundaries when a concrete integration needs them. Keep launch target variants explicit, so data cannot supply arbitrary shell command strings.

## Library edits

UI actions clone the small in-memory library, apply and validate an edit, serialize to a temporary file in the data directory, sync the temporary file, and atomically persist it over the existing file. The UI adopts the new library only after persistence succeeds. An exclusive file lock lives for the application lifetime. Read or schema errors stop startup; they never silently create an empty collection over existing data.

Executable paths are canonicalized at entry, and duplicate executable checks ignore ASCII case on Windows. Windows path aliases and non-ASCII case differences are not a complete filesystem identity check. Launch-time existence checks handle moved or removed games. Missing executable paths remain in the library so users can edit or remove them.

## Launching

Executable launches use `std::process::Command` with a vector of arguments and an explicit working directory. There is no shell expansion, command-line interpolation, or elevation. Child processes are reaped while the launcher runs. Closing the launcher leaves games running. Games that spawn another process may exit their original process immediately; this version does not track playtime or claim running-state detection.

Steam targets contain a numeric app ID; the launcher opens `steam://rungameid/<id>` through the platform handler. Success means dispatch was accepted, not proof that Steam started the game. Launch timestamps are recorded after dispatch. If timestamp persistence fails, the UI reports that error even though the game may already have started.

## Steam discovery

The user selects a Steam root. A background thread reads its primary `steamapps` directory and additional roots from `libraryfolders.vdf`, then parses `appmanifest_*.acf` files. A small KeyValues parser handles quoted strings, escapes, braces, comments, and legacy/current library listings. Installed candidates require the fully installed flag and an existing safe relative install directory under `common`. Results are deduplicated by app ID. An unreadable or malformed file fails the scan; nothing is imported until the whole scan succeeds.

This avoids network credentials and unstable remote scraping. Local Steam files are an integration detail, not a guaranteed public API; keep fixture tests alongside parsing changes. Missing additional library directories are skipped, covering disconnected drives. The first version asks for the installation folder rather than probing the Windows registry.

## Visual direction

The midnight-blue shell, Manrope/Oxanium typography, coral primary actions, mint selection, original cartridge illustration, and responsive cover grid are described in [the visual system](design.md). Presentation helpers live under `src/ui/`: `theme.rs` owns tokens/fonts/icons, `art.rs` owns original vector compositions, and `covers.rs` handles bounded local image decoding and caching. No game data or launch execution moved into those helpers.

An optional, defaulted `Game.cover` path extends schema 1 compatibly. Covers are local references with a generated fallback, not network downloads. PNG/JPEG are the only enabled image formats. Retained GPU textures and decode dimensions are bounded; decoding is currently synchronous on first display.

The shelf remains two-dimensional. Controller input, a 3D renderer, metadata downloads, and television-distance tuning are separate later iterations. Window chrome stays native; Windows accent preferences still apply to the title bar.
