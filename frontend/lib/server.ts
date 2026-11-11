import type { Game, Library } from './model';

// Transport for the self-hosted companion server (see docs/server.md).
// The desktop IPC in api.ts stays untouched; server mode uses plain fetch so
// the same screens can browse a remote catalogue without Tauri. Launching,
// imports and metadata work stay desktop-only.

export interface ServerConfig {
  url: string;
  apiKey: string;
}

export interface ServerStats {
  total: number;
  favorites: number;
  hidden: number;
  installed: number;
  recently_played: number;
  by_source: { pc: number; steam: number; epic: number; xbox: number };
  last_launched_max: number | null;
}

const URL_KEY = 'launcher-server-url';
const KEY_KEY = 'launcher-server-key';

/** Normalize user input to a bare `http(s)://host[:port]` origin. */
export function normalizeServerUrl(raw: string): string {
  const trimmed = raw.trim().replace(/\/+$/, '');
  if (!trimmed) throw new Error('Enter the server address, for example http://192.168.1.10:8080.');
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error('That server address is not a valid URL. Include http:// or https://.');
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:')
    throw new Error('The server address must start with http:// or https://.');
  if (parsed.username || parsed.password)
    throw new Error('Put credentials in the API key field, not the address.');
  return `${parsed.protocol}//${parsed.host}${parsed.pathname.replace(/\/+$/, '')}`;
}

export function authHeaders(apiKey: string): Record<string, string> {
  if (!apiKey.trim()) throw new Error('Enter the server API key first.');
  return { Authorization: `Bearer ${apiKey.trim()}` };
}

export function loadServerConfig(): ServerConfig {
  try {
    return {
      url: localStorage.getItem(URL_KEY) ?? '',
      apiKey: localStorage.getItem(KEY_KEY) ?? '',
    };
  } catch {
    return { url: '', apiKey: '' };
  }
}

export function saveServerConfig(config: ServerConfig): void {
  try {
    localStorage.setItem(URL_KEY, config.url);
    localStorage.setItem(KEY_KEY, config.apiKey);
  } catch {
    /* Private browsing can block storage; the session still works. */
  }
}

export function clearServerConfig(): void {
  try {
    localStorage.removeItem(URL_KEY);
    localStorage.removeItem(KEY_KEY);
  } catch {
    /* Nothing to clean up. */
  }
}

/** `?server=http://host:port` deep-links a device straight at a server. */
export function serverFromQuery(): string | null {
  try {
    const value = new URLSearchParams(location.search).get('server');
    return value ? normalizeServerUrl(value) : null;
  } catch {
    return null;
  }
}

async function request<T>(url: string, apiKey: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(url, init);
  } catch {
    throw new Error('Could not reach the server. Check the address and that the container is running.');
  }
  if (response.status === 401)
    throw new Error('The server rejected the API key. Check it and try again.');
  if (response.status === 403)
    throw new Error('The server refused that action. Launching stays on the desktop app.');
  if (!response.ok) {
    let detail = '';
    try {
      detail = (await response.json()).error ?? '';
    } catch {
      /* Fall through to the status text. */
    }
    throw new Error(detail || `The server answered ${response.status}.`);
  }
  return response.json() as Promise<T>;
}

export async function fetchServerLibrary(url: string, apiKey: string): Promise<Library> {
  const origin = normalizeServerUrl(url);
  const library = await request<Library>(`${origin}/api/library`, apiKey, {
    headers: authHeaders(apiKey),
  });
  if (library.schema_version !== 1 || !Array.isArray(library.games))
    throw new Error('The server sent a library this app does not understand. Update the app and server together.');
  return library;
}

export async function fetchServerStats(
  url: string,
  apiKey: string,
): Promise<{ server_time: number; stats: ServerStats }> {
  const origin = normalizeServerUrl(url);
  return request(`${origin}/api/stats`, apiKey, { headers: authHeaders(apiKey) });
}

export async function pushLibraryToServer(
  url: string,
  apiKey: string,
  library: Library,
): Promise<{ games: number; total: number }> {
  const origin = normalizeServerUrl(url);
  return request(`${origin}/api/library`, apiKey, {
    method: 'PUT',
    headers: { ...authHeaders(apiKey), 'Content-Type': 'application/json' },
    body: JSON.stringify(library),
  });
}

/** Single-entry lookup for a future details deep-link; unused by the shelf today. */
export async function fetchServerGame(
  url: string,
  apiKey: string,
  handle: string,
): Promise<Game> {
  const origin = normalizeServerUrl(url);
  return request(`${origin}/api/games/${encodeURIComponent(handle)}`, apiKey, {
    headers: authHeaders(apiKey),
  });
}
