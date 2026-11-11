import { command } from './api';
import { identity, type Game } from './model';
const cache = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();
let active = 0;
const waiting: (() => void)[] = [];
export async function coverThumbnail(
  game: Game,
  kind: 'cover' | 'background' | 'logo' = 'cover',
): Promise<string | null> {
  const key = `${identity(game)}:${kind}:${kind === 'cover' ? game.cover : game.metadata[kind]}`;
  if (cache.has(key)) {
    const value = cache.get(key)!;
    cache.delete(key);
    cache.set(key, value);
    return value;
  }
  const existing = pending.get(key);
  if (existing) return existing;
  const request = (async () => {
    // Tiles mount in batches of 80; allow enough parallel decodes to fill a
    // screen quickly without swamping the desktop decode pool.
    if (active >= 8)
      await new Promise<void>((resolve) => waiting.push(resolve));
    active++;
    try {
      const value = await command<string | null>('read_cover', {
        target: game.target,
        kind,
      });
      cache.set(key, value);
      // Must cover a full render batch plus details views; smaller caps
      // evict visible tiles and loop placeholders back on screen.
      while (cache.size > 256) cache.delete(cache.keys().next().value!);
      return value;
    } catch {
      return null;
    } finally {
      active--;
      waiting.shift()?.();
      pending.delete(key);
    }
  })();
  pending.set(key, request);
  return request;
}
export function invalidateArtwork(): void {
  cache.clear();
}
// Decode-ahead for scroll continuity: warm a bounded slice of upcoming tiles
// while the queue is idle. Visible (observer-driven) loads always win: each
// warm item waits for a fully idle queue first and the warmer aborts the
// moment anything is loading, so fast scrolling never queues behind
// prefetch. Resolved values land in the shared cache for tiles to pick up.
export async function warmThumbnails(games: Game[], limit = 40): Promise<void> {
  const slice = games.slice(0, Math.max(0, limit));
  for (const game of slice) {
    for (let i = 0; i < 100 && (active > 0 || waiting.length > 0); i++) {
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
    if (active > 0 || waiting.length > 0) return;
    await coverThumbnail(game).catch(() => null);
  }
}
