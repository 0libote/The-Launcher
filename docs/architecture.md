# Architecture

## Decision: native Rust, Bun for commands

The user’s constraint is a fully Rust launcher that uses Bun. A webview stack would put UI behavior in JavaScript or require a Rust-to-Wasm build pipeline. Instead, Rune renders a native Rust interface on its own GPU renderer with headless snapshots for agent-checked UI work. Bun runs the development commands and the small cross-platform check script. It is not an application runtime or dependency manager for Rust; Cargo owns Rust dependencies and `Cargo.lock` pins them.

This keeps a single language for the application and leaves room for a more deliberate console UI. Tradeoffs: Rune is pre-release and pinned to a `dev` rev, accessibility depends on Rune/AccessKit and platform support, and gamepad navigation requires explicit work. Hardware and screen-reader checks on Windows remain necessary.

## Boundaries

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Local data path, instance lock, fail-safe startup, window creation |
| `src/shell.rs` | Rune shell: toolbar, sidebar, grid/list/shelf views, details, search, add/edit/remove dialogs, Steam import, metadata & artwork, cover art, workspace editor; headless scenes and shell tests |
| `src/library.rs` | Versioned data model, duplicate checks, transactional edits and atomic persistence |
| `src/launch.rs` | Validate launch targets, construct launch plans, start processes or Steam URIs |
| `src/steam.rs` | Client discovery, local installed manifests, optional owned-game API merge |
| `src/metadata.rs` | Metadata model/settings, built-in providers, bounded network/image cache, preservation-aware merge |
| `src/workspace.rs` | User-owned layout/skin model, validation, atomic persistence |
| `scripts/check.ts` | Run formatting, Clippy, and tests through Bun on Windows or Unix |

There is no plugin API, service container, database, or event bus. Add boundaries when a concrete integration needs them. Keep launch target variants explicit, so data cannot supply arbitrary shell command strings.

## Library edits

UI actions clone the small in-memory library, apply and validate an edit, serialize to a temporary file in the data directory, sync the temporary file, and atomically persist it over the existing file. The UI adopts the new library only after persistence succeeds. An exclusive file lock lives for the application lifetime. Read or schema errors stop startup; they never silently create an empty collection over existing data.

Executable paths are canonicalized at entry, and duplicate executable checks ignore ASCII case on Windows. Windows path aliases and non-ASCII case differences are not a complete filesystem identity check. Launch-time existence checks handle moved or removed games. Missing executable paths remain in the library so users can edit or remove them. Steam entries hide instead of deleting: `Game.hidden` defaults to visible, stays out of the normal collections while set, and reappears through the Hidden collection. Steam re-imports preserve the flag.

## Launching

Executable launches use `std::process::Command` with a vector of arguments and an explicit working directory. There is no shell expansion, command-line interpolation, or elevation. Child processes are reaped while the launcher runs. Closing the launcher leaves games running. Games that spawn another process may exit their original process immediately; this version does not track playtime or claim running-state detection.

Steam targets contain a numeric app ID. Windows starts detected `steam.exe -silent` with a launch or install URI as a separate argument; non-Windows development uses the registered handler. Known uninstalled entries request installation and do not record a played timestamp. Success means dispatch was accepted, not proof that Steam started the game. Launch timestamps are recorded after dispatch. If timestamp persistence fails, the UI reports that error even though the game may already have started.

## Steam integration

See [the Playnite reference and parity table](steam-integration.md). Local discovery and optional account-owned games merge by numeric app ID. Steam client launch/install stays in `launch.rs`; the UI owns neither protocol parsing nor network credentials after dispatch. Import runs on a worker thread. Invalid individual manifests produce warnings; fatal root/account errors leave existing library state intact.

## User-owned workspace

The rendering backend is Rune (native GPU, headless-capable). A separate `workspace.json` stores named panel arrangements, views, sizes, and skin colors; it contains no games or secrets. The workspace editor overlay edits layouts live, validates on save, and restores the snapshot on cancel. Presentation changes do not migrate or rewrite game launch targets.

The launcher uses the native window frame. File pickers are native OS dialogs through Rune. Windows Snap/high-DPI behavior still requires native validation.

## Visual direction

The midnight-blue shell, coral primary actions, mint selection, gradient card art with initials, local cover images, and grid/list/shelf views are described in [the visual system](design.md). The shell lives in `src/shell.rs`: one Rune view with a toolbar, sidebar, game views, and details pane, plus `--scene` fixtures that render headlessly to PNG and element-tree JSON. No game data or launch execution lives in the view helpers.

An optional, defaulted `Game.cover` path extends schema 1 compatibly. Covers remain local references with a generated fallback. Built-in metadata providers can now populate them through the local artwork cache. PNG/JPEG are the only enabled image formats. Retained GPU textures and decode dimensions are bounded; decoding is currently synchronous on first display.

The shelf remains two-dimensional. Controller input, a 3D renderer, and television-distance tuning are separate later iterations. The launcher uses the native window frame; native window operations go through the backend.

## Metadata pipeline

See [metadata behavior and provider references](metadata.md). Steam account ownership import and public metadata fetching are separate operations; metadata needs no Steam Web API account key. The metadata overlay queues snapshots of games on a worker thread and applies individual results on the UI thread through `Library::change`, with a stop button and per-game progress. Matching a local executable records a separate Steam metadata ID without changing its launch target. Results resolve by target plus metadata ID instead of vector index, so removals/reordering cannot attach data to another entry. Title matches and artwork choices that arrive late are dropped unless the panel still shows their game and query.

The merge fills empty fields unless explicit per-game replacement was selected, compares every field against its request-time baseline to preserve in-flight edits, and retains source attribution. Artwork requests use fixed provider origins, restricted HTTPS CDN hosts, disabled redirects, time/size/decode limits, and atomic PNG cache writes. No key is serialized. `metadata.json` owns provider/download preferences independently of `workspace.json`; cached images live under `artwork/`. Decoded pixels live in a 64-entry covers cache plus a panel-scoped choice cache that clears when the panel reopens or a new browse starts, so browsing artwork cannot grow retained images without bound. The current concrete provider choices are Steam text/art and optional SteamGridDB art with Steam fallback; there is no dynamic plugin loader.
