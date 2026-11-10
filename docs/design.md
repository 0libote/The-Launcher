# Visual design

The shelf takes its cues from Steam first, then Playnite, Epic, and Xbox: a dark-first library with a quiet sidebar, a continue-playing rail of recent games, artwork-led cards with hover Play/Details actions, and a game page with a banner hero, a primary Play action, stat tiles, and grouped management actions. The Svelte interface uses the system font stack for controls and headings. No remote fonts are loaded.

## Palette

The default workspace is dark-first (`#09090b` canvas, `#101013` navigation, `#141417`/`#1f1f24` surfaces, `#fafafa` text) with a pink accent (`#f472b6` dark, `#db2777` light) for Add/Play actions, selection, and keyboard focus. First launch defaults to the dark appearance; light and system themes stay one click away in the top bar. All ten workspace skin fields drive CSS custom properties, including dialogs. Cover-art palettes are deliberately independent of the workspace skin.

## Composition and behavior

A compact navigation rail holds real collection counts, favorites/recent entries, stores, hidden games, and preferences. The top-left toggle collapses the rail on desktop and opens the drawer on phones. The main collection opens with a continue-playing rail of recent games, then a clear heading, search, sorting, installed filter, cover-size zoom, and view switch. Cards prioritize art and keep names readable; hovering a card reveals Play and Details shortcuts. Details open as a Steam-style game page (banner hero, cover plus title, Play action, stat tiles, about and management sections) in a focus-trapped native HTML dialog or a side panel, and the side panel follows keyboard and controller browsing. Screens adapt down to 320 CSS pixels; mobile support is a UI foundation, not a shipped mobile sync service.

Grid wraps portrait covers. Shelf adds case depth and a visible shelf edge without a 3D engine. List uses compact rows. Cards progressively render in batches of 80; scrolling or Show more reveals further entries. Keyboard navigation reveals its next batch automatically.

Fallback covers are deterministic original abstract compositions and initials. They do not imply downloaded commercial artwork. Local/downloaded PNG/JPEG images use bounded native thumbnails. Only nearby cards request them; a four-request queue and 64-entry LRU bound work/cache size, and offscreen cards release their local thumbnail reference.

Empty libraries invite adding or importing games. Filtered-empty states offer a reset. Errors appear in dialogs and the status bar; imported-file warnings remain inspectable. Destructive removal requires confirmation and never deletes game files. Workspace previews can be canceled without saving.

Keyboard users can use Tab, arrows, Enter/Space, Escape, `/`, and Ctrl/Cmd+K. Search and form controls retain normal editing keys. Controllers use the browser Gamepad API for shelf movement, confirm, and back; real WebView2/controller validation is still required. Motion respects reduced-motion preferences; visible focus appears immediately. Automated accessibility scans cover the default library and details dialog, not every possible custom skin or real screen reader.

## Captures

The images in `screenshots/rewrite/` capture the implemented Svelte screens using a demo catalogue of real store games with hotlinked Steam CDN artwork in the explicit browser demo. Fortnite and Halo Infinite intentionally show the generated fallback covers. They are not native Windows captures. The older PNG files outside that directory are historical Rune captures and no longer represent the implementation.
