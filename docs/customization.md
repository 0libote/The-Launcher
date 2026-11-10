# Build your workspace

Open **Preferences → Appearance** to change the shell. Changes preview across the library and dialogs; **Save preferences** persists the workspace, and **Cancel** restores the previous appearance.

- Choose grid, list, or shelf view. Shelf uses a wrapped grid of case-like covers with a shelf edge, not a 3D renderer.
- Drag the cover-size slider in the toolbar for quick zoom (Playnite-style); the full range stays in Preferences.
- Collapse the navigation rail with the top-left toggle; on phones the same button opens the drawer.
- Place navigation on the left/right or hide it; the top-left navigation toggle can reopen it. On phones navigation becomes a drawer.
- Open game details in a dialog or side panel. **Open on request** retains the dialog without a persistent side panel; selecting a game always makes its actions available.
- Put the search/view toolbar above or below the collection.
- Adjust cover and panel widths. Narrow viewports cap sizes to keep the library usable.
- Change all ten skin colors. Covers keep their own artwork colors. Your choices can affect contrast; the bundled palette is tested, arbitrary user colors are not guaranteed accessible.
- Duplicate/name up to 32 layouts and switch between them. Existing workspace files retain their model and saved settings.

The rewrite uses native window controls and native file pickers. It replaces the earlier drag-and-drop docking overlay with explicit placement controls. Arbitrary widgets, scripting, custom fonts, and 3D shelves are deferred.

`workspace.json` lives beside `library.json`; it contains no games, launch targets, or API keys. Rust validates names, placements, colors, and size ranges before atomic save. Damaged/newer workspaces produce a temporary default and warning; the existing file remains until an explicit save.

`src/workspace.rs` owns the model. `frontend/components/Preferences.svelte` edits a draft; the shell renders that draft while the dialog is open. Library operations do not depend on panel placement.

Current browser captures use explicit synthetic fixtures. They show the actual Svelte screens, not a Windows-native runtime validation. See [the rewrite report](rewrite-report.md).
