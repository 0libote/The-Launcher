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
    if (active >= 4)
      await new Promise<void>((resolve) => waiting.push(resolve));
    active++;
    try {
      const value = await command<string | null>('read_cover', {
        target: game.target,
        kind,
      });
      cache.set(key, value);
      while (cache.size > 64) cache.delete(cache.keys().next().value!);
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
