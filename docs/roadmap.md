# Roadmap

This is a direction, not a release promise. Finish and validate each slice before adding the next integration.

## Foundation — implemented, Windows validation remains

Native Rust library UI; manual executable entries and launch options; safe direct process launching; local persistence; favorites and search; source filters; installed Steam import and URI launching; setup, architecture, and data documentation.

## User-owned presentation — implemented foundation

Visual layout editor with panel docking, grid/list/shelf views, sizes, colors, named layouts, atomic Save/Cancel, and a launcher-owned frame. See `customization.md` for the supported regions and current limits.

Steam now follows the registry/manifests plus optional Web API account-import approach examined in Playnite. Browser login, family sharing, and session/install watchers remain future work.

## Next: make it pleasant to use daily

- Validate on Windows hardware and add Windows CI, including an actual release build.
- Implemented: custom local cover art, title-based fallback art, bounded texture cache, bundled typography, and the redesigned native library. Validate these on Windows hardware.
- Implemented: up/down grid navigation and scroll-to-selection. Continue Windows keyboard/accessibility and DPI validation.
- Controller navigation and an intentional big-screen mode with controller disconnect recovery.
- Remember window state, shelf selection, sorting, and user-selected Steam roots.
- Import previews and per-file warnings; refresh/uninstalled handling that never silently removes custom entries.
- Timestamp display, recent sorting, and backup/restore UI.

## Then: deepen the shelf

Explore 3D cases or a spatial shelf, optional sound and motion with off switches, metadata and artwork providers with opt-in downloads, and collections. Prototype the interaction and performance before committing the entire UI to a 3D renderer. Preserve a usable low-motion path.

## Later: broaden the library

Consider other storefronts, emulators and ROM launch profiles, portable mode, robust play-session tracking, and an extension model only after multiple real integrations expose a repeated boundary. Online accounts, cloud sync, remote play, achievements, and a plugin marketplace are not foundation requirements.

## Release gates

Choose a project license. Test Windows graphics drivers, DPI, accessibility, native dialogs, Unicode paths, Steam across multiple drives, and update/recovery behavior. Decide packaging and signing. Publish known limitations and backup instructions alongside a reproducible build process before inviting users to entrust a large library to the launcher.
