# Epic Games integration and Playnite reference

The implementation follows the **local installed scan plus client launch**
approach examined in Playnite's Epic library extension. This is original Rust
code. It does not run or bundle Playnite's C# extension and does not claim
complete feature parity.

## Reference inspected

The reference files are in
[JosefNemec/PlayniteExtensions](https://github.com/JosefNemec/PlayniteExtensions/tree/master/source/Libraries/EpicLibrary):

- [`EpicLauncher.cs`](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/EpicLibrary/EpicLauncher.cs):
  `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests\*.item` discovery,
  `com.epicgames.launcher://apps/<namespace>:<item>:<app>?action=launch&silent=true`
  launch URLs, install/library URLs, and client executable lookup.
- [`EpicLibrary.cs`](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/EpicLibrary/EpicLibrary.cs):
  installed-game filtering (DLC addons without `addons/launchable`, engine
  plugins, `UE_*` compatibles, missing install folders), deduplication by
  `AppName`, and install/uninstall/play controller wiring.
- [`Models/LauncherInstalled.cs`](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/EpicLibrary/Models/LauncherInstalled.cs):
  `.item` manifest shape (`AppName`, `DisplayName`, `InstallLocation`,
  `CatalogNamespace`, `CatalogItemId`, `AppCategories`, `CompatibleApps`).

These are behavioral references inspected for this change, not a promise that
all current/future Playnite extensions behave identically.

## Installed library

Select **Import Epic → Scan**. Windows finds manifests under
`%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests`; use the optional
override for unusual installations or headless fixtures. Linux development
requires the override because there is no default manifests folder.

Import reads installed `*.item` JSON files, requires a non-empty `AppName`
and an existing `InstallLocation` directory, and falls back to `AppName` when
`DisplayName` is blank. DLC addons (`addons` without `addons/launchable`),
engine plugins (`plugins`, `plugins/engine`, `TechnicalType` engine plugins,
`CompatibleApps` starting with `UE_`), non-application entries
(`bIsApplication: false`), and missing folders are skipped. Malformed files
produce warnings while valid entries still import. Entries deduplicate by
case-insensitive `AppName` and sort by display name. An unusable manifests
folder is an error. No Epic login is needed for this path.

No cached artwork is read from the Epic client. The separate built-in
[metadata downloader](metadata.md) can match an Epic entry to a Steam title
for descriptions, genres, features, scores, and artwork without changing its
Epic launch target.

## Launch

Epic entries launch through the registered protocol handler with the full
`com.epicgames.launcher://apps/<namespace>:<item>:<app>?action=launch&silent=true`
URL. Manifests without catalog fields use the app-only form
`com.epicgames.launcher://apps/<app>?action=launch&silent=true` so sideloaded
entries still dispatch. Epic owns download/install and game execution.
Refresh through **Import Epic** after an installation or removal. Epic
entries hide instead of deleting, like Steam entries, and re-imports preserve
favorites, custom names, covers, and hidden state.

## Current parity limits

| Capability | This launcher |
| --- | --- |
| Local `.item` manifest scan, DLC/plugin filtering, dedupe | Implemented |
| Direct client launch delegation (full and app-only URLs) | Implemented |
| Existing-user-data preservation, hidden restore | Implemented |
| Account-owned/uninstalled catalog via Epic login/token | Not implemented |
| Automatic install/uninstall/run-state watchers | Not implemented |
| Playtime sync, achievements, cloud saves | Not implemented |

Do not describe this version as full Playnite Epic parity or as Epic account
login. A future account connection needs an explicit native
authentication/credential-storage design; it should not silently read Epic
tokens from the user's client.
