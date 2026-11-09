# Visual design

The shelf takes its cues from physical game cases and console collection screens. Artwork carries the personality; navigation and controls stay quiet. The Svelte interface uses bundled Manrope for controls and Oxanium for headings and original generated cover typography. No remote fonts are loaded.

## Palette

The retained midnight canvas (`#101420`), navigation (`#151a28`), and slate surfaces (`#1d2333`, `#262e40`) frame ivory text (`#f3f0e8`). Coral (`#ff977c`) marks Add/Play actions; mint (`#9fdecc`) marks selection and keyboard focus. All ten workspace skin fields drive CSS custom properties, including dialogs. Cover-art palettes are deliberately independent of the workspace skin.

## Composition and behavior

A compact navigation rail holds real collection counts, favorites/recent entries, stores, hidden games, and preferences. The main collection has a clear heading, search, sorting, installed filter, and view switch. Cards prioritize art and keep names readable. Details open in a focus-trapped native HTML dialog or a side panel. Screens adapt down to 320 CSS pixels; mobile support is a UI foundation, not a shipped mobile sync service.

Grid wraps portrait covers. Shelf adds case depth and a visible shelf edge without a 3D engine. List uses compact rows. Cards progressively render in batches of 80; scrolling or Show more reveals further entries. Keyboard navigation reveals its next batch automatically.

Fallback covers are deterministic original abstract compositions and initials. They do not imply downloaded commercial artwork. Local/downloaded PNG/JPEG images use bounded native thumbnails. Only nearby cards request them; a four-request queue and 64-entry LRU bound work/cache size, and offscreen cards release their local thumbnail reference.

Empty libraries invite adding or importing games. Filtered-empty states offer a reset. Errors appear in dialogs and the status bar; imported-file warnings remain inspectable. Destructive removal requires confirmation and never deletes game files. Workspace previews can be canceled without saving.

Keyboard users can use Tab, arrows, Enter/Space, Escape, `/`, and Ctrl/Cmd+K. Search and form controls retain normal editing keys. Controllers use the browser Gamepad API for shelf movement, confirm, and back; real WebView2/controller validation is still required. Motion respects reduced-motion preferences; visible focus appears immediately. Automated accessibility scans cover the default library and details dialog, not every possible custom skin or real screen reader.

## Captures

The images in `screenshots/rewrite/` capture the implemented Svelte screens using synthetic games and original generated artwork in the explicit browser demo. They are not native Windows captures. The older PNG files outside that directory are historical Rune captures and no longer represent the implementation.
