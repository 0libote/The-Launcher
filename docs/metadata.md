# Metadata and artwork

Metadata downloading is built into the launcher. No extension installation is required. The first providers are Steam Store for information and official artwork, and optional SteamGridDB for community artwork. Provider preferences belong to the library, separately from workspace layouts and skins.

## Everyday use

Open **Preferences → Metadata** to choose fields and artwork provider. **Save preferences** persists defaults in `metadata.json`. Automatic filling runs after imports and saves for visible Steam entries and manually matched PC/Epic/Xbox entries. It does not guess matches by title or fetch at every startup. **Fill missing information for library** starts a queue using saved preferences.

The status bar shows the current title and remaining count. **Stop fetching** stops after the in-flight request; already saved results remain. Network/decode work runs in Rust blocking workers. Results persist atomically per game, and the frontend refreshes each completed game. A short 500 ms gap separates queued requests; rate-limit warnings/errors stop the queue. Closing the app stops the remaining UI-owned queue.

Select a game and choose **Find metadata**. Search Steam, pick the correct result, or fetch an already linked identity. Matching records a separate Steam metadata ID and leaves the launch target unchanged. The explicit replacement checkbox can refresh existing fields; request-time baselines preserve fields edited while downloads run. Removed games or changed matches cannot receive stale results. Titles, favorites, tags, hidden state, launch options and history are not replaced by metadata providers.

Choose **Artwork** in game details to select a local cover, return to generated artwork, or browse cover/background/logo candidates. Previews download through the same trusted-host, bounded native decoder as selected images. The SteamGridDB key field retains a session-only value; clear it to forget that UI copy. A dispatched worker retains its own copy until completion. No key is saved in backups or settings.

## Providers and fields

| Provider | Available now | Authentication |
| --- | --- | --- |
| Steam Store | Short description, developer, publisher, genres, features (Steam categories like Single-player), displayed release date, age rating (`required_age`), Metacritic critic score, and a store-page link; official portrait covers, hero/store backgrounds and logo | None; independent of Steam account import |
| SteamGridDB | Up to six static portrait cover, hero, or logo choices for a Steam app ID | Personal API key in a password field, retained only for the current app session |

Playnite parity fields in this slice: description, developers, publishers, genres, features, links, age rating, critic score, series (manual), release date, community tags (manual-only user labels, never overwritten by providers), cover, background, and logo with per-field source attribution. Tags stay manual so provider refreshes cannot erase user labels.

Artwork defaults to **Steam official**. Choose **SteamGridDB first, Steam fallback** to prefer community artwork during automatic/individual downloads. When SteamGridDB is unavailable or has no supported asset, the downloader tries Steam. The manual artwork browser queries the source you select and reports its errors directly; it does not silently mix providers. It previews locally cached candidates before **Use image** replaces just that artwork field. The manual picker works independently of the automatic field checkboxes.

SteamGridDB's key is separate from a Steam Web API account key. Obtain one from [SteamGridDB preferences](https://www.steamgriddb.com/profile/preferences/api). It is sent only as a Bearer header to the fixed SteamGridDB API origin, never to artwork CDNs. It is never serialized, printed in errors, or stored in the library/workspace. Clearing the key field clears the UI's session copy; an already dispatched worker owns its copy until that job ends. No bundled developer key is used.

Information is currently English, with US storefront search. Release dates preserve the provider's display text, including "Coming soon". HTML is reduced to plain text; the launcher does not execute or display remote HTML. Provider names are stored per field; SteamGridDB image selections also retain the returned artist name. Sources can be inspected under **Metadata sources** in game details. The game details modal and right-hand pane display downloaded information, backgrounds, and logos, alongside launch and edit controls.

## Local storage and network limits

`Game.metadata` is a backward-compatible, defaulted schema-1 object. Downloaded covers use the existing `Game.cover` field. Artwork is decoded, resized to at most 1920×1920, encoded as PNG, and atomically cached under `artwork/` beside `library.json`. The frontend requests bounded PNG thumbnails through Tauri; it cannot read arbitrary local files. A 64-entry LRU cache and nearby-card requests bound retained artwork. Back up the library **and** artwork directory to retain downloaded images. Existing locally selected covers remain external references.

Requests have 8-second connection and 15-second total timeouts, no redirects, a 2 MiB JSON limit, and a 16 MiB artwork limit. Image decoding permits PNG/JPEG only, dimensions up to 4096×4096, and a 64 MiB allocation limit. HTTPS media hosts are restricted to Steam CDN hosts and `cdn2.steamgriddb.com`; SVG, WebP, GIF, animation, and arbitrary remote URLs are not supported. SteamGridDB requests exclude NSFW/humor artwork. This filter depends on provider labels.

The frontend queues games sequentially with a 500 ms politeness gap and stops on rate-limit warnings or request errors. Per-field failures are reported; there is no aggressive automatic retry loop. Successful fields survive unavailable images/text. Existing valid cache files are reused; this version does not expire them, remove unreferenced files, or manage a disk quota. Removing a game does not delete downloaded cache files. A refresh reuses an asset at the same URL, so changed remote content at that URL may need its cached file removed while the app is closed. Nearby cover thumbnails decode on workers with at most four frontend requests in flight; cards show original generated art until a thumbnail arrives.

## Playnite reference and extension direction

[Playnite's metadata guide](https://api.playnite.link/docs/manual/library/games/metadata.html) lists name, description, genres, release date, developers/publishers, platform, community rating, tags, cover, background, links, and more, with automatic download after import, bulk/single download, and manual editing. This slice covers the Steam-fillable subset (description, developers, publishers, genres, features, links, age rating, critic score, release date, cover, background, logo) plus manual-only tags and series, with the same preservation rules: in-flight edits win, removals cannot misattach data, and tags are never overwritten.

Its [provider configuration guide](https://api.playnite.link/docs/manual/features/extensionsSupport/installingExtensions.html) describes ordered providers stopping at the first result. The [Steam metadata implementation](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/SteamLibrary/SteamShared/MetadataProvider.cs) separates store data from portrait covers, background variants, and logos. Those behaviors informed this original Rust implementation.

SteamGridDB endpoint/filter/auth behavior follows its [official API](https://www.steamgriddb.com/api/v2) and [official wrapper source](https://github.com/SteamGridDB/node-steamgriddb/blob/master/src/index.ts). Steam's Store endpoints and CDN paths are public but do not have the same stability guarantee as the documented Steamworks Web API. Provider changes can require launcher updates.

This is the first built-in provider slice, not Playnite's full extension system. It has no IGDB/LaunchBox/ScreenScraper providers, arbitrary per-field provider ordering, external plugin loading, credentials vault, non-Steam catalogue identities, screenshot/video/icon downloading, or metadata-driven collection filters. Epic and Xbox entries reuse Steam metadata through manual matching; native Epic Store or Xbox catalogue providers are future work. New providers should return the existing typed `Fetched`/`ImageChoice` values; they must not modify launch targets or workspace settings. Add another provider when its actual authentication, matching, field, and licensing requirements are implemented, rather than shipping inactive provider choices.
