# Native visual system

The launcher takes its cues from console collection screens and physical game cases. The design stays in Rust, rendered by Rune. It does not use HTML, CSS, a webview, remote fonts, or a 3D renderer.

## Palette and type

| Token | Color | Use |
| --- | --- | --- |
| `BG` | `#101420` | Main midnight canvas |
| `RAIL` | `#151a28` | Navigation and inputs |
| `SURFACE` | `#1d2333` | Welcome panel, game details modal, dialogs |
| `TEXT` | `#f3f0e8` | Warm ivory text |
| `CORAL` | `#ff977c` | Add and Play actions |
| `MINT` | `#9fdecc` | Active collection, keyboard focus, selection |
| `LILAC` | `#b7afe0` | Original fallback artwork |

Rune renders the interface with its embedded Inter face. The launcher draws a custom 60pt titlebar (draggable toolbar with brand and search) and keeps the platform window controls clear via chrome insets. See [workspace customization](customization.md).

## Composition

This describes the bundled starting layout. Navigation and details can be hidden, Tools can move, and the user can choose grid/list/shelf presentation and override colors in the visual editor. The renderer reads the saved workspace.


A 208-point navigation rail holds the brand, source/favorite filters with real counts, and library-folder access. The main header owns Add and Steam import. The collection toolbar contains the current collection and search. Selecting a game opens a modal with the Play action, details, and management options.

The empty collection shows an original, vector-drawn trio of cartridge cases and a direct invitation to add or import games. It creates no fictional entries. Shorter windows omit supporting tips, and the welcome panel and add dialog scroll when necessary. Filtered-empty views offer a way to clear the search and return to all games.

Tiles use original title-based artwork until a local cover or downloaded artwork is selected. The title’s hash picks its gradient colors, so removing or filtering another game does not change its appearance. Long titles are truncated visually; full titles remain in accessible labels. Selected tiles use selection outlines with a soft glow, top-lit rims, drop shadows, and press/spring motion. These are desktop layouts, not a claim of complete controller or screen-reader support.

## Local cover art

Select a game, then **Options → Choose cover art…**. PNG and JPEG are supported. Covers center-crop to the tile’s portrait ratio without stretching. **Remove cover art** returns to generated artwork. Editing executable launch options preserves the chosen cover.

The optional `cover` JSON field stores a local file reference, not a copied image. Keep that file in place. Missing or invalid images fall back to generated artwork, with the error available in the tile’s tooltip. Select the image again or restart to refresh an externally edited file. The launcher does not download, collect, or license commercial game artwork for you.

Images are limited to 16 MiB and 4096 pixels per dimension, decoded with a 64 MiB allocation limit, and thumbnailed to at most 480×720 before GPU upload. A 64-entry cache bounds retained pixels; reaching the limit clears old entries. Local covers decode once when the library loads; downloaded artwork decodes when its fetch or choice completes.

## Interaction and validation

Mouse, search-by-typing, arrow-key navigation (Enter opens the details modal, then launches; Escape backs out), controller navigation (d-pad/stick, confirm, back with auto-repeat and disconnect recovery), add/edit/remove dialogs, Steam import, metadata & artwork, cover art, and the workspace editor are available; fullscreen is future work. Escape closes dialogs.

Headless Rune checks cover the shell: `--scene` fixtures render to PNG and element-tree JSON, and unit tests drive selection, favorites, filters, and launch errors through scripted pointer input. Native captures and manual interaction checks complement these tests. Verify Windows behavior, high DPI, and accessibility on Windows hardware before release.

## Captures

These historical captures show the earlier fixed-layout foundation before the visual editor/custom-frame change. They are native Linux captures, not web mockups; see customization.md for current behavior. The populated screenshot uses an isolated synthetic fixture library and an original local image to exercise cover loading. These names and entries are not shipped to users.

![Empty library at 1200 × 800](screenshots/library-empty.png)

![Populated library with synthetic fixture data at 1200 × 800](screenshots/library-populated.png)

![Add-game dialog at the 840 × 600 minimum window size](screenshots/add-game-compact.png)
