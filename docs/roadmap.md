# Roadmap

This is a direction, not a release promise. Finish and validate each slice before adding the next integration.

## Foundation — implemented, Windows validation remains

Native Rust library UI; manual executable entries and launch options; safe direct process launching; local persistence; favorites and search; source filters; installed Steam/Epic/Xbox imports and protocol-handler launching; setup, architecture, and data documentation.

## User-owned presentation — implemented foundation

Visual layout editor with panel docking, grid/list/shelf views, sizes, colors, named layouts, atomic Save/Cancel, and a launcher-owned frame. See `customization.md` for the supported regions and current limits.

Steam now follows the registry/manifests plus optional Web API account-import approach examined in Playnite. Epic follows the local `.item` manifests approach; Xbox follows the installed UWP packages approach. Browser/account logins, family sharing, console catalogs, playtime sync, and session/install watchers remain future work.

## Next: make it pleasant to use daily

- Validate on Windows hardware and add Windows CI, including an actual release build.
- Implemented: custom local cover art, title-based fallback art, bounded texture cache, bundled typography, and the redesigned native library. Validate these on Windows hardware.
- Implemented: up/down grid navigation and scroll-to-selection. Continue Windows keyboard/accessibility and DPI validation.
- Controller navigation and an intentional big-screen mode with controller disconnect recovery.
- Remember window state, shelf selection, sorting, and user-selected Steam roots.
- Import previews and per-file warnings; refresh/uninstalled handling that never silently removes custom entries.
- Timestamp display, recent sorting, and backup/restore UI.

## Then: deepen the shelf

Explore 3D cases or a spatial shelf, optional sound and motion with off switches, richer metadata-driven collections (using the new features/tags/scores fields), and expanded built-in providers. Prototype the interaction and performance before committing the entire UI to a 3D renderer. Preserve a usable low-motion path.

## Later: broaden the library

Consider emulators and ROM launch profiles, GOG/itch.io scans, portable mode, robust play-session tracking, and an extension model only after multiple real integrations expose a repeated boundary. Online accounts, cloud sync, remote play, achievements, and a plugin marketplace are not foundation requirements.

## Release gates

Choose a project license. Test Windows graphics drivers, DPI, accessibility, native dialogs, Unicode paths, Steam across multiple drives, and update/recovery behavior. Decide packaging and signing. Publish known limitations and backup instructions alongside a reproducible build process before inviting users to entrust a large library to the launcher.

## Built-in metadata foundation (implemented)

Steam descriptions/credits/genres/features/links/age ratings/critic scores/release dates, automatic fill after import or confirmed matching (Steam, Epic, Xbox, and executables via Steam matching), official cover/background/logo downloads, optional SteamGridDB artwork preference with Steam fallback, individual image previews/selections, source attribution, offline PNG cache, manual-only tags/series, and preservation-aware background results. See [metadata](metadata.md) for current limits.

Next metadata work: another real catalogue for games outside Steam (evaluate IGDB authentication/licensing), per-field provider priorities, manual text editing and metadata-driven filters, non-Steam artwork identities, credential storage through the OS, cache quotas/cleanup, and icons/screenshots. Dynamic extensions should follow working provider contracts rather than become a prerequisite for everyday metadata.
