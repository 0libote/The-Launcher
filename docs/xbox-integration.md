# Xbox / Xbox Game Pass integration and Playnite reference

The implementation follows the **installed UWP packages plus client launch**
approach examined in Playnite's Xbox library extension. This is original Rust
code. It does not run or bundle Playnite's C# extension and does not claim
complete feature parity.

## Reference inspected

The reference files are in
[JosefNemec/PlayniteExtensions](https://github.com/JosefNemec/PlayniteExtensions/tree/master/source/Libraries/XboxLibrary):

- `XboxLibrary.cs`: matching installed `GetUWPApps` packages against account
  titles, `shell:AppsFolder\<PackageFamilyName>!<AppId>` play monikers,
  PC-vs-console device filtering, playtime/last-played wiring, and the
  installed/uninstalled import switches.
- `XboxLibraryClient` / `XboxAccountClient`: authenticated title-history and
  stats lookups that require an Xbox account connection.

These are behavioral references inspected for this change, not a promise that
all current/future Playnite extensions behave identically.

## Installed library

Select **Import Xbox → Scan**. Windows lists installed UWP packages through
PowerShell (`Get-AppxPackage | ConvertTo-Json`); the optional JSON override
exists for development fixtures on other hosts. Linux development has no UWP
stack, so an explicit JSON file is required there.

Import parses the package list (array, single object, or empty), requires a
non-empty package family name, and builds an AppsFolder moniker
`<FamilyName>!<AppId>`. The Application Id prefers the first
`<Application Id="…">` in the package's `AppxManifest.xml` and falls back to
`App`, the common default. Missing family names and unparsable lists become
warnings instead of failed imports. Entries deduplicate case-insensitively
and sort by name.

Without the authenticated title API this version cannot tell games from other
UWP apps, so non-game packages are imported and the user hides them (Hidden
collection), the same pattern Steam entries use. Install location is recorded
when PowerShell reports it. No Xbox login is needed for this path.

No cached artwork is read from the packages. The separate built-in
[metadata downloader](metadata.md) can match an Xbox entry to a Steam title
for descriptions, genres, features, scores, and artwork without changing its
Xbox launch moniker.

## Launch

Xbox entries launch through `explorer.exe shell:AppsFolder\<FamilyName>!<AppId>`
on Windows, without a shell. Off Windows the platform handler is tried so
launch plans stay testable, but Xbox games need Windows to run. Refresh
through **Import Xbox** after an installation or removal. Xbox entries hide
instead of deleting, like Steam entries, and re-imports preserve favorites,
custom names, covers, and hidden state.

## Current parity limits

| Capability | This launcher |
| --- | --- |
| Installed UWP scan, manifest AppId preference, dedupe | Implemented |
| Direct AppsFolder launch delegation | Implemented |
| Existing-user-data preservation, hidden restore | Implemented |
| Account-owned/uninstalled catalog via Xbox API | Not implemented |
| Console-only games, Game Pass catalog, playtime/last-played sync | Not implemented |
| Automatic install/uninstall/run-state watchers | Not implemented |

Do not describe this version as full Playnite Xbox parity, as Xbox Game Pass
catalog sync, or as Xbox account login. A future account connection needs an
explicit native authentication/credential-storage design; it should not
silently reuse Xbox tokens from another client.
