# Development

## Routine

Run `bun run dev` to start the native application, `bun run check` before a change is ready, and `bun run build` for a release executable. Cargo equivalents are `cargo run`, `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, and `cargo build --release`.

Use Rust 1.92+ and a current stable toolchain. Keep `Cargo.lock` committed. There are no Bun packages and no Bun lockfile. The Bun check script stops on the first failing Cargo command and preserves its exit code. Do not add a frontend framework just to get a command runner.

## Tests and their limits

Unit tests cover persistence round trips, duplicate prevention, failed-save rollback, corrupt/future schema handling, shell-free launch plans, Steam URI construction, and Steam manifest/library parsing. Tests use temporary directories; they do not execute games or need a real Steam account.

A compile or passing unit test does not prove a native window, graphics driver, file dialog, or Steam protocol handler works. Validate those on Windows before calling a build release-ready. There is no Windows CI or installer in the initial foundation.

## Windows smoke test

1. Start with a new library. Check the empty shelf, keyboard focus, minimum window size, DPI scaling at 100% and 150%, and F11 entry/exit.
2. Add a trusted executable in a path containing spaces. Confirm its working directory and separate arguments. Cancel an add and verify no entry is created.
3. Restart and check persistence. Edit arguments, favorite, search, filter by source, and verify Play launches the selected visible game. Check Tab, arrow keys, Enter, and double click.
4. Add the same executable again. Check duplicate feedback. Move the executable and check that Play reports its missing path without deleting the entry.
5. Import a Steam installation with multiple libraries. Check count, names, already-imported entries, a disconnected drive, and launching through Steam. Repeat import to confirm no duplicates.
6. Remove an entry and confirm its game files remain untouched. Run a second launcher instance and verify the instance warning.
7. Close the launcher, back up its JSON, damage a copy at the real path, and verify startup refuses to overwrite it. Restore the backup. Test a read-only data folder and recovery after a save error.
8. Check Windows screen-reader behavior, contrast, and text clipping with long names. Do not claim full accessibility or controller support based on keyboard-only testing.

## Extending the app

- New source: add a typed launch target if required, discovery code outside the UI, focused fixture tests, and a documented data migration if the serialized model changes.
- New UI feature: preserve keyboard focus and text-input shortcuts; show errors where the user can act on them.
- Data changes: prefer backward-compatible defaulted fields. Bump schema version for incompatible formats and implement explicit migrations before loading them.
- Documentation: update README for current behavior, architecture for consequential decisions, roadmap for remaining work, and the data reference for serialized fields.

## Commit identity

Always commit as `0libote <79215282+0libote@users.noreply.github.com>`. Check both Git config fields before the first commit in a fresh environment and stop if either differs. Never commit as T3 Code or `t3@t3code.local`.

## Foundation validation record

The initial implementation passed formatting, Clippy with warnings denied, and 13 unit tests on Linux with Rust 1.98.1 / Bun 1.4.4. `cargo build` succeeded, as did `cargo check --target x86_64-pc-windows-gnu --all-targets`. A native window rendered under Xvfb; an isolated fixture library loaded, Play dispatched `/usr/bin/true`, and Favorite plus the launch timestamp were persisted. The empty state, populated shelf, and add dialog were visually inspected. No real user library was modified.

This does not validate the MSVC release build, Windows file picker, `.exe` execution on Windows, real Steam protocol dispatch, or screen-reader behavior. Complete the Windows checklist before distributing a release.

## Visual polish validation record

The visual redesign passed `bun run check` (formatting, Clippy with warnings denied, 20 tests), `cargo build`, and Windows GNU target compilation for all targets. New tests cover small-text contrast, local cover decoding/resizing, center-cropping, old-library compatibility and cover persistence, combined filters, arrow-row navigation and modal isolation, and layout bounds at 840×600, 1200×800, and 1600×1000.

Native Xvfb captures covered the empty and populated library, a local image alongside generated covers, and the add dialog at minimum size. A smoke test dispatched `/usr/bin/true` through Play, toggled Favorite, and checked that launch time/favorite changes preserved the cover reference in JSON. Escape closed the add dialog. Test libraries and images lived under `/tmp`, separate from user data. See `docs/design.md` for captures and fixture labeling. Windows runtime, MSVC packaging, high DPI, screen readers, real Steam launches, and native cover-file picking remain Windows smoke-test work.

## User workspace and Steam-reference validation

The user-owned shell/Steam change passed formatting, Clippy with warnings denied, 34 tests, `cargo build`, and `cargo check --target x86_64-pc-windows-gnu --all-targets`. Tests cover workspace persistence/validation, skin application to widgets and custom painting, all three views with alternate panel placements at 840×600 and 1200×800, and Cancel restoring the pre-edit workspace. Steam fixtures cover the Playnite-reference behaviors: multi-library scans, malformed-file warnings, fallback names, redistributable exclusion, owned-result parsing/deduplication, installed-state merging, and install-vs-launch URI selection. UI merge tests preserve custom titles, favorites, covers, and known installed state on partial scans.

Native Xvfb mouse testing dragged Navigation onto the right edge, saved it, and checked the resulting JSON. Native restarts rendered persisted list and shelf layouts with another skin and moved/hidden panels. The compact right-details pane keeps Play and Options outside the scrolling metadata. Captures are linked from `customization.md`. Test data and files stayed under `/tmp`; no real account credential was used.

Still required on Windows: registry discovery against a real Steam client, `.exe` and `steam.exe -silent` dispatch, account API calls with a real consenting account, drag-to-Snap behavior, resize/minimize/maximize/Alt+F4, multiple monitors/DPI, and accessibility. The Win11 hover Snap-layout flyout is not implemented by the custom maximize button. Steam embedded-browser login, family sharing, and install/run-state watchers are not implemented; see the explicit parity table in `steam-integration.md`.
