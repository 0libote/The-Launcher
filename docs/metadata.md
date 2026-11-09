# Metadata and artwork

Metadata downloading is built into the launcher. No extension installation is required. The first providers are Steam Store for information and official artwork, and optional SteamGridDB for community artwork. Provider preferences belong to the library, separately from workspace layouts and skins.

## Everyday use

Open **Metadata** in the library toolbar. Select which fields to fetch and the preferred artwork provider. **Save defaults** persists these choices in `metadata.json`; changes also apply for the current session before saving. Automatic downloading is enabled by default after a successful Steam/Epic/Xbox import, saving a local entry, or confirming a title match. It fills missing fields for visible Steam games and matched PC/Epic/Xbox entries. It does not run on every startup, fetch during browsing, or guess matches by title.

Use **Fill missing metadata for library** to enrich an existing collection. Downloads run in one background queue; the library remains usable and the shelf stays responsive. Image decoding and thumbnails happen on worker threads; the UI thread only merges text and inserts pre-decoded pixels (a few per frame). The toolbar shows **Metadata…** while a job is active. Open it for progress, warnings, or **Stop downloads**. Stopping waits for the in-flight game or image request to finish, discards its unsaved result, and flushes already merged games to disk; completed batches are kept. Closing the dialog leaves the job running; closing the app cancels remaining work but keeps flushed batches.

Select a game and open **Options → Metadata & artwork…** for individual downloads and image selection. This remains available through Options above the collection when the details panel is hidden.

For a local executable, Epic, or Xbox entry, search for its title and select the correct Steam result, or enter a positive Steam app ID. Confirming a match records only the metadata identity; launch targets, arguments, install locations, and package monikers stay unchanged. When automatic downloads are enabled, confirming the match also fills missing metadata. Games outside Steam's catalogue need local artwork for now. Changing an existing match does not erase saved fields; use the explicit replacement option to refresh them from the corrected identity.

**Replace existing enabled fields** is an explicit per-game operation. By default, names, custom images, existing metadata, favorites, hidden state, installation state, and launch history are preserved. A field edited while a request is running is preserved even during replacement. A result for a removed game or changed target/match is ignored. Merges apply to memory immediately; disk writes are batched (every few games plus a final flush on Done/stop) through one atomic save instead of one save per game, so bulk fetches do not hitch the UI. Partial provider failures do not reset the collection.

## Providers and fields

| Provider | Available now | Authentication |
| --- | --- | --- |
| Steam Store | Short description, developer, publisher, genres, features (Steam categories like Single-player), displayed release date, age rating (`required_age`), Metacritic critic score, and a store-page link; official portrait covers, hero/store backgrounds and logo | None; independent of Steam account import |
| SteamGridDB | Up to six static portrait cover, hero, or logo choices for a Steam app ID | Personal API key in a password field, retained only for the current app session |

Playnite parity fields in this slice: description, developers, publishers, genres, features, links, age rating, critic score, series (manual), release date, community tags (manual-only user labels, never overwritten by providers), cover, background, and logo with per-field source attribution. Tags stay manual so provider refreshes cannot erase user labels.

Artwork defaults to **Steam official**. Choose **SteamGridDB first, Steam fallback** to prefer community artwork during automatic/individual downloads. When SteamGridDB is unavailable or has no supported asset, the downloader tries Steam. The manual artwork browser queries the source you select and reports its errors directly; it does not silently mix providers. It previews locally cached candidates before **Use image** replaces just that artwork field. The manual picker works independently of the automatic field checkboxes.

SteamGridDB's key is separate from a Steam Web API account key. Obtain one from [SteamGridDB preferences](https://www.steamgriddb.com/profile/preferences/api). It is sent only as a Bearer header to the fixed SteamGridDB API origin, never to artwork CDNs. It is never serialized, printed in errors, or stored in the library/workspace. **Forget** clears the UI's session copy; an already dispatched worker owns its copy until that job ends. No bundled developer key is used.

Information is currently English, with US storefront search. Release dates preserve the provider's display text, including "Coming soon". HTML is reduced to native text; the launcher does not execute or display remote HTML. Provider names are stored per field; SteamGridDB image selections also retain the returned artist name. Sources can be inspected under **Saved information → Sources**. The game details modal and right-hand pane display downloaded information, backgrounds, and logos, with launch controls and the full information available through Options.

## Local storage and network limits

`Game.metadata` is a backward-compatible, defaulted schema-1 object. Downloaded covers use the existing `Game.cover` field. Artwork is decoded, resized to at most 1920×1920, encoded as PNG, and atomically cached under `artwork/` beside `library.json`. The UI receives local paths and uses its existing bounded texture cache. Back up the library **and** artwork directory to retain downloaded images. Existing locally selected covers remain external references.

Requests have 8-second connection and 15-second total timeouts, no redirects, a 2 MiB JSON limit, and a 16 MiB artwork limit. Image decoding permits PNG/JPEG only, dimensions up to 4096×4096, and a 64 MiB allocation limit. HTTPS media hosts are restricted to Steam CDN hosts and `cdn2.steamgriddb.com`; SVG, WebP, GIF, animation, and arbitrary remote URLs are not supported. SteamGridDB requests exclude NSFW/humor artwork. This filter depends on provider labels.

One worker processes games sequentially with a short 500 ms politeness gap (down from 2 s) and stops the queue on HTTP 429. Per-field failures are reported; there is no aggressive automatic retry loop. Successful fields survive unavailable images/text. Existing valid cache files are reused; this version does not expire them, remove unreferenced files, or manage a disk quota. Removing a game does not delete downloaded cache files. A refresh reuses an asset at the same URL, so changed remote content at that URL may need its cached file removed while the app is closed. Startup and bulk-fetch cover thumbnails decode on workers and stream to the UI a few per frame; cards show gradient art until their decode arrives.

## Playnite reference and extension direction

[Playnite's metadata guide](https://api.playnite.link/docs/manual/library/games/metadata.html) lists name, description, genres, release date, developers/publishers, platform, community rating, tags, cover, background, links, and more, with automatic download after import, bulk/single download, and manual editing. This slice covers the Steam-fillable subset (description, developers, publishers, genres, features, links, age rating, critic score, release date, cover, background, logo) plus manual-only tags and series, with the same preservation rules: in-flight edits win, removals cannot misattach data, and tags are never overwritten.

Its [provider configuration guide](https://api.playnite.link/docs/manual/features/extensionsSupport/installingExtensions.html) describes ordered providers stopping at the first result. The [Steam metadata implementation](https://github.com/JosefNemec/PlayniteExtensions/blob/master/source/Libraries/SteamLibrary/SteamShared/MetadataProvider.cs) separates store data from portrait covers, background variants, and logos. Those behaviors informed this original Rust implementation.

SteamGridDB endpoint/filter/auth behavior follows its [official API](https://www.steamgriddb.com/api/v2) and [official wrapper source](https://github.com/SteamGridDB/node-steamgriddb/blob/master/src/index.ts). Steam's Store endpoints and CDN paths are public but do not have the same stability guarantee as the documented Steamworks Web API. Provider changes can require launcher updates.

This is the first built-in provider slice, not Playnite's full extension system. It has no IGDB/LaunchBox/ScreenScraper providers, arbitrary per-field provider ordering, external plugin loading, credentials vault, non-Steam catalogue identities, screenshot/video/icon downloading, or metadata-driven collection filters. Epic and Xbox entries reuse Steam metadata through manual matching; native Epic Store or Xbox catalogue providers are future work. New providers should return the existing typed `Fetched`/`ImageChoice` values; they must not modify launch targets or workspace settings. Add another provider when its actual authentication, matching, field, and licensing requirements are implemented, rather than shipping inactive provider choices.
