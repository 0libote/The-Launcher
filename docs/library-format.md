# Library format, version 1

The app owns `library.json`. Close it and back up the file before editing. Paths below are examples; they must point to programs on the machine using the library.

```json
{
  "schema_version": 1,
  "games": [
    {
      "name": "A local game",
      "target": {
        "kind": "executable",
        "path": "C:\\Games\\Example\\game.exe",
        "arguments": ["--profile", "My Profile"],
        "working_directory": null
      },
      "favorite": false,
      "last_launched": null
    },
    {
      "name": "A Steam game",
      "target": { "kind": "steam", "app_id": 10 },
      "favorite": true,
      "last_launched": 1700000000
    },
    {
      "name": "An Epic game",
      "target": {
        "kind": "epic",
        "app_name": "Fortnite",
        "catalog_namespace": "fn",
        "catalog_item_id": "abc"
      },
      "installed": true
    },
    {
      "name": "An Xbox game",
      "target": { "kind": "xbox", "package_id": "Publisher.Game_abc!App" },
      "installed": true
    }
  ]
}
```

| Field | Contract |
| --- | --- |
| `schema_version` | Required integer; only 1 is supported. Newer versions cause a startup error. |
| `games` | Required list, stored in addition/import order. |
| `name` | Nonempty display name. |
| `target.kind` | `executable`, `steam`, `epic`, or `xbox`. Older files with only `executable`/`steam` still load. |
| `target.path` | Absolute executable path, canonicalized when saved through the UI. Existence is checked on launch, not load. |
| `target.arguments` | Required vector of separate literal argument strings for executables. No shell parsing. |
| `target.working_directory` | Optional folder for executable targets; null/omitted uses the executable’s parent. |
| `target.app_id` | Positive 32-bit Steam application ID. |
| `target.app_name` | Nonempty Epic AppName (for example `Fortnite`). `catalog_namespace`/`catalog_item_id` default empty for sideloaded manifests and fall back to the app-only launch URL. |
| `target.package_id` | Xbox AppsFolder moniker `<FamilyName>!<AppId>` (for example `Publisher.Game_abc!App`). Shape is checked at launch. |
| `favorite` | Boolean, defaults to false when absent. |
| `installed` | Optional boolean. Steam/Epic/Xbox imports set it; false requests installation (Steam) or marks a disconnected entry. Null/absent preserves legacy launch behavior. |
| `cover` | Optional local PNG/JPEG path. Manual selections are external references; downloaded covers point into the local `artwork/` cache. Missing files use generated artwork. |
| `metadata` | Defaulted object containing `steam_app_id` (optional metadata-only match for executables and store games outside Steam), `description`, `developers`, `publishers`, `genres`, `features` (Steam categories), `tags` (manual-only user labels), `links` (store page), `age_rating`, `series` (manual), `critic_score` (Steam Metacritic), `release_date`, optional local `background`/`logo` paths, and per-field `sources` map. Missing fields default empty. See [metadata](metadata.md). |
| `hidden` | Boolean, defaults false. Store games (Steam/Epic/Xbox) hide instead of deleting; hidden games remain in the library and are excluded from automatic metadata work. |
| `last_launched` | Optional Unix timestamp in seconds, defaults to null when absent. Records dispatch, not play duration. |

Do not share your library publicly without reviewing paths and launch arguments; these may contain personal folder names or other private values. Metadata can use public Steam data without credentials; optional Steam account import and SteamGridDB require separate session-only keys. No keys are stored here. There is no automatic migration in version 1. Unknown JSON fields are currently ignored by Serde and will not survive a save; do not use them to store custom metadata.
