# Self-hosted companion server

Run The Launcher as a Docker container on your own server so you can see
your collection — stats, owned games, play history — from anywhere, and keep
one shared copy of the library that every device can publish to and collect
from. Launching, importing, and metadata fetching stay on the desktop app;
the server is a catalogue and a source of truth, never a game runner.

## Quick start

```sh
# One long random value; the app sends it as a bearer token.
export LAUNCHER_API_KEY="$(openssl rand -base64 32)"
docker compose up --build
```

Open `http://your-server:8080` for the catalogue, or point the app at it
(see "Connecting the app" below). The library persists in the
`launcher-data` volume (`/data/library.json` inside the container).

Without compose:

```sh
docker build -f Dockerfile.server -t the-launcher-server .
docker run -p 8080:8080 \
  -e LAUNCHER_API_KEY='...' \
  -v launcher-data:/data \
  the-launcher-server
```

## Configuration

| Variable             | Default          | Meaning                                                   |
| -------------------- | ---------------- | --------------------------------------------------------- |
| `LAUNCHER_API_KEY`   | (required)       | Bearer token. The server refuses to start without one.    |
| `LAUNCHER_DATA_DIR`  | `/data`          | Directory holding `library.json`. Mount a volume here.    |
| `LAUNCHER_BIND`      | `0.0.0.0:8080`   | Listen address.                                           |
| `LAUNCHER_PUBLIC_DIR`| `/app/public`    | Built web UI served as static files. Unset to run API-only.|

Flags override the environment: `--data-dir`, `--bind`, `--api-key`,
`--public-dir`. A short key still starts (so a home lab is never blocked)
but logs a warning — use at least 16 random characters, 32+ recommended.

## Connecting the app

1. In the app (desktop or browser), open **Server & sync** from the library
   navigation.
2. Enter the server address (`http://192.168.1.10:8080`) and the API key.
3. **Test connection** checks the key and reports the game count.
4. **View server catalogue** switches to a read-only view of the server
   library. **Publish this library** (desktop only) uploads the local
   library to the server.

`?server=http://host:port` deep-links a device straight at a server. The
address and key are stored in that device's `localStorage` only — never in
the library file.

## Sync model (current)

- **Publish** (`PUT /api/library`) replaces the server's library with the
  desktop's, after the same schema-1 validation the desktop uses. Invalid
  uploads are rejected and the previous file is untouched.
- **Collect** (`GET /api/library`) downloads the server's library for
  viewing. The in-app server view is read-only.
- Last writer wins. There is no merge yet: publishing from two devices
  alternately discards one side's changes. Publish from the device you just
  edited, then collect everywhere else.

Planned next steps: pull-into-desktop with per-entry merging, playtime
history beyond `last_launched`, and artwork serving. See the roadmap.

## API

All routes except `/api/health` require
`Authorization: Bearer <LAUNCHER_API_KEY>`.

| Method | Route                 | Meaning                                                        |
| ------ | --------------------- | -------------------------------------------------------------- |
| GET    | `/api/health`         | Liveness probe: `{status, version, games}`. No auth.            |
| GET    | `/api/stats`          | `{server_time, stats}`: totals, per-source counts, favorites, hidden, installed, recently-played, newest `last_launched`. |
| GET    | `/api/library`        | The whole versioned library document.                          |
| PUT    | `/api/library`        | Validate and atomically replace the library.                   |
| GET    | `/api/games`          | `{games, total}` game list.                                    |
| GET    | `/api/games/<handle>` | One entry by stable handle (e.g. `steam:620`).                 |
| POST   | `/api/launch*`        | Always `403`. Launching stays on the desktop agent.            |

Non-`/api` paths serve the built web UI (`index.html` fallback for deep
links). CORS allows cross-origin `GET/PUT/POST/OPTIONS` with `Authorization`
so the desktop webview and browsers can talk to a LAN address directly.

## Security boundaries

- The library contains executable paths, argument strings, and folder names
  that may reveal personal details. **Do not expose the server to the
  public internet without a reverse proxy** (HTTPS, e.g. Caddy or nginx)
  in front of it, and use a strong key.
- The server never launches processes, never evaluates request bodies as
  code, and never serves files outside its public directory.
- A portable asset model (local paths stripped from shared copies) and
  multi-user access are future work; today one shared key guards one
  private library.
