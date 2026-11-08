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
    }
  ]
}
```

| Field | Contract |
| --- | --- |
| `schema_version` | Required integer; only 1 is supported. Newer versions cause a startup error. |
| `games` | Required list, stored in addition/import order. |
| `name` | Nonempty display name. |
| `target.kind` | `executable` or `steam`. |
| `target.path` | Absolute executable path, canonicalized when saved through the UI. Existence is checked on launch, not load. |
| `target.arguments` | Required vector of separate literal argument strings for executables. No shell parsing. |
| `target.working_directory` | Optional folder for executable targets; null/omitted uses the executable’s parent. |
| `target.app_id` | Positive 32-bit Steam application ID. |
| `favorite` | Boolean, defaults to false when absent. |
| `installed` | Optional boolean. Steam import sets it; false requests installation instead of launching. Null/absent preserves legacy launch behavior. |
| `cover` | Optional local PNG/JPEG path, defaults to null/absent. Not copied; missing files use generated artwork. Older schema-1 libraries continue to load. |
| `last_launched` | Optional Unix timestamp in seconds, defaults to null when absent. Records dispatch, not play duration. |

Do not share your library publicly without reviewing paths and launch arguments; these may contain personal folder names or other private values. No credentials are required by the launcher. There is no automatic migration in version 1. Unknown JSON fields are currently ignored by Serde and will not survive a save; do not use them to store custom metadata.
