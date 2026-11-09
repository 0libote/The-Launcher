# Development and validation

## Prerequisites

Use the Rust toolchain in `rust-toolchain.toml` and Bun version in `package.json`. Commit both `Cargo.lock` and `bun.lock` when dependencies change. Windows native development needs MSVC Build Tools (Desktop development with C++ and a Windows SDK) and WebView2. The Tauri CLI is a local development dependency.

For native Linux development, install the distribution equivalents of `build-essential`, `pkg-config`, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, and `patchelf`. Linux remains a development host rather than a supported release target. The current validation environment lacks these development libraries and has no sudo.

## Commands

```sh
bun install --frozen-lockfile
bun run dev                  # Vite plus native Tauri app
bun run dev:web              # UI only; open /?demo for synthetic fixtures
bun run check                # Svelte, frontend unit tests/build, Rust fmt/Clippy/tests
bun run test:ui              # Browser workflows, accessibility and mocked desktop contract
bun run build                # Native release + Windows NSIS installer on Windows
```

`cargo test --lib` and default Cargo checks need no GUI packages. The desktop binary requires `--features desktop`; release binaries also enable `custom-protocol` so assets are embedded rather than depending on Vite. `tauri.conf.json` enables it for Tauri CLI commands. Run `cargo clippy --features desktop --all-targets -- -D warnings` and `cargo test --features desktop --all-targets` on a machine with native prerequisites. Use `cargo build --features desktop,custom-protocol --release` for an executable without installer bundling.

Browser tests use `CHROMIUM_PATH` or the system Chromium at `/usr/bin/chromium`; on other hosts set the environment variable to an installed Chromium executable. They launch an isolated test browser and reuse/start Vite. Install a supported Chromium browser before running them. They never use your real library. Mocked IPC tests verify frontend wiring, not native IPC execution.

## UI system

The interface uses Tailwind CSS v4 (`@tailwindcss/vite` in `vite.config.ts`),
`bits-ui` headless primitives, `lucide-svelte` icons, and
`class-variance-authority` variants, mirroring the T3 Code stack. `style.css`
defines `@theme inline` tokens bound to the skin CSS variables, so utilities
like `bg-surface` or `text-muted` follow light/dark skins and custom themes.
Shared pieces live in `frontend/components/ui/` (`Button` with
`primary`/`secondary`/`ghost`/`danger`/`icon` variants) with `cn()` in
`frontend/lib/utils.ts` for class merging. Dialogs use `bits-ui` Dialog with
the same modal contract; keep the `Modal` props stable and put new generic
controls in `ui/` rather than hand-rolled CSS. The overlay sits above the
sticky topbar. The desktop window is frameless (`decorations: false` in
`tauri.conf.json`) with its own themed `Titlebar.svelte` (Tauri-only drag
region plus minimize/maximize/close through `lib/api.ts`); the browser demo
hides it.

## Screenshots and benchmarks

With the development server on port 1420, `bun run screenshots` writes fixture captures to `docs/screenshots/rewrite/`. Set `CHROMIUM_PATH` when needed.

For production measurements, first run `bun run build:web`, then `bunx vite preview --host 127.0.0.1 --port 1421 --strictPort` in another terminal, then `node scripts/performance.mjs`. `BENCHMARK_URL` overrides the default URL. Results go to `docs/performance/rewrite.json`. The script samples 100/1,000/5,000 synthetic entries three times, includes fonts/navigation in ready time, measures search with automation overhead, and reports JS heap and renderer idle task time over five seconds after a one-second settling period. These are browser measurements, not total-process memory, GPU usage, or Windows startup benchmarks. The demo caps fixture generation at 10,000 entries.

## Windows CI

`.github/workflows/windows.yml` runs core/frontend checks, native desktop Clippy/tests, and a full MSVC/NSIS build. It uploads an unsigned installer artifact with seven-day retention; it does not publish a release. Actions are pinned to verified release commits, with Dependabot checking the pins monthly. The workflow still needs its first hosted run; local GNU cross-checks do not prove MSVC packaging.

## Windows smoke test

1. Back up existing data. Start the rewrite with a schema-1 library and workspace; confirm names, favorites, hidden flags, artwork and layouts survive. Confirm corrupt/future libraries show an error without resetting files. Try a second instance.
2. Check empty state, search, all views, installed/source/favorite filters, tags, long titles, and 100%/150%/200% DPI. Resize/move/maximize/Snap across monitors. Check dialog clipping at the minimum window size.
3. Add a trusted `.exe` in a Unicode path containing spaces. Validate separate arguments and working directory, edit/save/restart, and reject duplicates. Move the executable and confirm launch fails without removing it.
4. Import real Steam (multiple drives, offline drive, account-owned games), Epic manifests, and Xbox packages. Check warnings, case variants, preservation, and real client dispatch. Known-uninstalled Steam entries install without getting a played timestamp.
5. Match a non-Steam title, fetch information, choose local/provider artwork, inspect sources/background/logo, replace explicitly, stop a queue, and simulate offline/rate limits. Check that edits during requests survive.
6. Preview/save/cancel skins and named layouts, including hidden navigation and right details. Check keyboard focus, Escape, slash/Ctrl+K, Tab, arrow rows, and controller reconnect/confirm/back. Test with a real screen reader.
7. Export/restore a backup. Try damaged and future versions, write failures, and an export destination matching the live library. Keep a separate copy: backups contain references, not game/artwork binaries. Restore stages files and attempts rollback on IO failure; it is not a power-loss-proof multi-file transaction.
8. Install/uninstall the unsigned NSIS build on a clean Windows machine. Verify WebView2 handling, app-data preservation, startup and recovery messages, and executable dispatch. Installer signing/updating and distribution license remain release work.

## Commit identity

Always commit as `0libote <79215282+0libote@users.noreply.github.com>`. Check both Git config fields before the first commit in a fresh environment and stop if either differs. Never commit as T3 Code or `t3@t3code.local`.
