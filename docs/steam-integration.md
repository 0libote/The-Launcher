# Steam integration and Playnite reference

The implementation follows the **local detection + optional account library + Steam client actions** approach examined in Playnite’s Steam library extension. This is original Rust code. It does not run or bundle Playnite’s C# extension and does not claim complete feature parity.

## Reference inspected

The reference files are in [JosefNemec/PlayniteExtensions](https://github.com/JosefNemec/PlayniteExtensions/tree/master/source/Libraries/SteamLibrary):

- [`Steam.cs`](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/SteamLibrary/Steam.cs): installation lookup in `HKCU\Software\Valve\Steam`, value `SteamPath`.
- [`SteamLocalService.cs`](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/SteamLibrary/Services/SteamLocalService.cs): library-folder and installed manifest scanning, fully-installed flag, fallback names, per-file failures, and redistributable exclusion.
- [`SteamServiceAggregator.cs`](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/SteamLibrary/Services/SteamServiceAggregator.cs): merging installed games with account-owned games, including an API-key route and an authenticated-web route.
- [`SteamGameController.cs`](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/SteamLibrary/SteamGameController.cs): dispatch through `steam.exe -silent` and Steam launch/install URIs.

These are behavioral references inspected for this change, not a promise that all current/future Playnite extensions behave identically.

## Installed library

Select **Import Steam → Import library**. Windows finds the client through the current user’s Steam registry path, then scans the primary `steamapps` folder and the additional roots listed in `steamapps/libraryfolders.vdf`. The optional location override handles unusual installations. Linux development uses `~/.steam/steam` or `~/.local/share/Steam`.

Import reads installed `appmanifest_*.acf` files and requires the fully-installed flag and a safe, existing directory under `steamapps/common`. It falls back to `UserConfig/name` if the main name is blank. Temporary manifests and Steamworks Common Redistributables (228980) are excluded. Malformed files and unavailable additional libraries produce warnings and let other games import. An unusable primary root is an error. No Steam login is needed for this path.

Cached portrait art in `appcache/librarycache/<appid>_library_600x900.jpg` or `.png` is used when present. This currently supports those flat cache paths, not every newer Steam cache layout or network artwork retrieval. Existing user-selected cover paths take priority.

## Account-owned library

To include owned, uninstalled games, enter a numeric **SteamID64** and a personal **Steam Web API key** in the Steam dialog, then import. This uses the same official [GetOwnedGames API](https://partner.steamgames.com/doc/webapi/IPlayerService#GetOwnedGames) route that Playnite supports for API-key imports. Steam’s API-key page is linked in the dialog. The client installation is still required for local installation-state detection.

Both fields are optional together. The key is used for that import and then cleared from the UI; it is not persisted in the workspace, library, or logs. Enter it again for a later account import. Credentials travel only to the official HTTPS endpoint. Redirect following is disabled; requests have a 20-second timeout and response bodies are capped at 16 MiB. Windows uses the OS TLS backend; Linux development uses rustls. Error messages exclude the request URL and API key.

An empty/private/inaccessible account response fails explicitly instead of being mistaken for an empty library. Account imports merge by app ID with the local installed scan. Existing user names, favorites, and covers are preserved. A complete scan updates installation flags; incomplete local scans keep previous flags conservatively where installation is uncertain. No imported game is automatically deleted from the user’s library.

## Launch and install

On Windows, the launcher starts detected `steam.exe` with separate `-silent` and URI arguments, without a shell. Installed/legacy entries use `steam://rungameid/<id>`; known uninstalled entries use `steam://install/<id>`. Non-Windows development uses the registered protocol handler. An installation request does not record a played timestamp.

Steam owns download/install and game execution. Refresh through **Import Steam** after an installation or removal to update state. Automatic installation watching, running-process tracking, and playtime synchronization are not implemented yet. Dispatch success is not proof that a game started successfully.

## Current parity limits

| Capability | This launcher |
| --- | --- |
| Registry client detection, multi-library installed scan | Implemented |
| Per-manifest warnings and redist exclusion | Implemented |
| Owned/uninstalled games via personal API key | Implemented |
| Direct Steam client launch/install delegation | Implemented |
| Existing-user-data preservation, local cached covers | Implemented, limited cache paths above |
| Playnite’s embedded-browser login/session-token route | Not implemented |
| Family Sharing, Source/GoldSrc mods, Steam shortcuts | Not implemented |
| Account playtime, parental API filtering, achievements | Not implemented |
| Automatic install/uninstall/run-state watchers | Not implemented |

Do not describe this version as full Playnite parity or as one-click Steam account login. A future account connection feature needs an explicit native authentication/credential-storage design; it should not silently read Steam’s cookies or extract credentials from the user’s client.
