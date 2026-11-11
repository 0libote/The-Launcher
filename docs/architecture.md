# Architecture

## Decision: web UI, native Rust services

The Launcher uses Tauri 2 with a Svelte 5 / TypeScript interface, built by Vite and Bun. Rust retains library persistence, process dispatch, store discovery, metadata providers, bounded image decoding, and workspace validation. Rune is replaced, rather than wrapped or shipped alongside the new interface.

The earlier fully-Rust/no-webview constraint has been superseded by the product goals: easier visual iteration, a lightweight desktop game shelf, and a future responsive browser catalogue. Tauri uses the system webview; Bun is a development tool, not a shipped runtime. A smaller frontend bundle does not prove lower desktop memory use: Windows runtime measurement remains necessary.

## Boundaries

- `frontend/App.svelte`: library navigation and orchestration, search/sort/filter, keyboard and controller input, progress and errors.
- `frontend/components/`: covers, game details, forms, dialogs, and appearance/provider preferences. `frontend/components/ui/` holds shared primitives (buttons); icons come from `lucide-svelte` through `Icon.svelte`.
- `frontend/lib/model.ts`: typed frontend data and pure browsing operations.
- `frontend/lib/utils.ts`: `cn()` class merging (Tailwind-aware) for component variants.
- `frontend/lib/api.ts`: the single desktop transport boundary. Components do not import Tauri APIs. A later server transport can reuse the screens without giving browsers local process access.
- `frontend/lib/artwork.ts`: lazy thumbnail requests, four in-flight requests, 64-entry LRU cache.
- `src/main.rs`: existing data location, single-instance lock, fail-safe startup, Tauri command registration.
- `src/desktop.rs`: validated application services and Tauri commands. Pure services compile/test without GUI libraries; native commands are behind the `desktop` feature.
- `src/library.rs`, `launch.rs`, `steam.rs`, `epic.rs`, `xbox.rs`, `metadata.rs`, `workspace.rs`: retained native domains and their fixture tests.

The default Cargo feature set tests the Rust core without GTK/WebKit. `bun run dev` and `bun run build` enable the desktop feature through the Tauri configuration. No local HTTP server is required in the installed app. The companion catalogue/sync server is a separate `launcher-server` binary (`src/server.rs`, `Dockerfile.server`, `docs/server.md`); the desktop app embeds no server. Standalone desktop use remains offline.

## Data and launching

The data directory remains `ProjectDirs::from("", "", "TheLauncher")`. Existing schema-1 JSON libraries and workspace/provider files remain readable. The frontend identifies entries by launch-target identity, never sorted vector indices. This is a migration bridge; ownership/catalogue IDs should replace launch-derived identity when non-launchable physical collections are introduced.

Edits validate and save an atomic replacement before publishing state. A lifetime OS file lock prevents competing app instances. Invalid/newer libraries stop startup instead of resetting the collection. Launching uses typed targets and argument vectors, without shell interpolation or elevation. Success means dispatch accepted, not proof a game started; closing the app leaves games running.

Imports merge by store identity and preserve custom titles, favorites, hidden flags and artwork. Network/scanning/image work runs off the UI thread. Metadata uses request-time baselines to preserve edits made during fetches. Account and artwork API keys are session inputs, never persisted.

Artwork is read through bounded Rust decoding and returned as PNG thumbnails, not unrestricted filesystem URLs. Missing artwork has deterministic generated covers. Only nearby covers are requested; list/grid/shelf screens progressively render entries. Browsing, filtering, and selection stay local and usable offline.

## Desktop versus browser

A browser without Tauri displays an empty explanatory screen. The explicit `?demo` mode uses synthetic fixture entries, generated art and tab-scoped session storage; it cannot read or overwrite a real collection. It exists for UI development, screenshots, and performance testing. It is not the Docker server or synchronization implementation.

Future server ownership records must separate catalogue information, owned copies, and device-specific availability. A phone must be able to list a physical cartridge or remote game without pretending it has a Windows launch path. Start the eventual server with read-only published catalogue access; leave local launching in the desktop agent.

## Verification boundaries

Svelte diagnostics, browser workflow/accessibility tests, Rust service tests, production bundle measurements, and cross-compilation are separate evidence. Browser tests do not prove native IPC/window/dialog behavior. Cross-compilation does not validate Windows runtime behavior. See `development.md` and `rewrite-report.md` for measured results and remaining release gates.
