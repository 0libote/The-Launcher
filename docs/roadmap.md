# Roadmap

The direction is a polished, lightweight personal game shelf for PC, retro, and physical collections. Finish and validate each slice before broadening integrations.

## Desktop rewrite

Tauri 2 shell; Svelte/TypeScript grid, list and shelf views; retained Rust launching/import/metadata/persistence services; responsive game details; keyboard/controller browsing; user-owned appearance; backup/restore; browser fixture mode. Validate native Windows behavior before calling the rewrite release-ready. See `rewrite-report.md` for evidence and outstanding checks.

## Next: make the desktop dependable

- Complete Windows MSVC build, native dialogs, real store dispatch, controller, DPI, multi-monitor and screen-reader checks.
- Choose a distribution license, installer/signing/update strategy and release process.
- Remember window state and library browsing preferences; improve import previews and refresh/install state without deleting custom entries.
- Add catalogue coverage outside Steam, provider credential storage through the OS, artwork cache quotas and cleanup.

## Then: retro and ownership

- Separate stable catalogue identity, owned copies and device-specific launch options, with explicit migrations and backups.
- Allow non-launchable entries for physical games and owned but uninstalled copies.
- Add platforms, ROM scanning with import previews, and profiles for installed external emulators.
- Expand collections around platform, edition, format and collection status.

## Later: self-hosted companion (started)

Reuse the responsive UI with a Docker-hosted API and persistent data volume. The first slice exists: `launcher-server` publishes a read-only browsable catalogue (`GET /api/library|games|stats`), the desktop app publishes its library (`PUT /api/library`, Server & sync screen) and any device views it read-only. Authentication is one shared bearer key; launching is refused server-side. Still to come: editing and conflict-aware synchronization, playtime history, artwork serving, and a portable asset model before exposing private libraries beyond your own network. Standalone desktop use remains offline.

## Last: remote play

Evaluate browser emulation (server supplies files, client runs the emulator) separately from server-side game streaming. Integrate existing emulation/streaming systems rather than build a streaming engine. Device compatibility, storage access and saves need their own validation.

## Deferred

3D/spatial shelves, an unrestricted drag-and-drop layout editor, dynamic plugin marketplace, achievements, complex account integrations and custom streaming infrastructure. Retain practical appearance settings and a usable low-motion path.
