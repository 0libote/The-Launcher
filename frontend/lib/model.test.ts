import { describe, expect, test } from 'bun:test';
import {
  filterGames,
  identity,
  isLegacyDefaultSkin,
  legacyDefaultSkin,
  t3DarkSkin,
  t3LightSkin,
} from './model';
import { fixtureGames } from './fixtures';
describe('library browsing', () => {
  test('combines collection, search, and installation without changing source data', () => {
    const games = fixtureGames();
    games[0].hidden = true;
    const original = games.map(identity);
    expect(filterGames(games, 'All games', '', 'name')).toHaveLength(11);
    expect(filterGames(games, 'Hidden', 'terra', 'name')).toEqual([games[0]]);
    expect(filterGames(games, 'Favorites', 'RPG', 'name', true)).toHaveLength(
      0,
    );
    expect(
      filterGames(games, 'Steam', '', 'name').every(
        (game) => game.target.kind === 'steam',
      ),
    ).toBe(true);
    expect(games.map(identity)).toEqual(original);
  });
  test('sorts recent and retains selection identity across order changes', () => {
    const games = fixtureGames();
    const selected = identity(games[2]);
    expect(filterGames(games, 'Recently played', '', 'recent')[0]).toBe(
      games[0],
    );
    expect(
      filterGames(games, 'All games', '', 'name').find(
        (game) => identity(game) === selected,
      ),
    ).toBe(games[2]);
  });
  test('search matches tags and is case insensitive', () => {
    expect(
      filterGames(fixtureGames(), 'All games', 'DEMO COLLECTION', 'name'),
    ).toHaveLength(12);
  });
  test('only the untouched pre-T3 skin counts as legacy', () => {
    expect(isLegacyDefaultSkin(legacyDefaultSkin())).toBe(true);
    expect(isLegacyDefaultSkin(t3LightSkin())).toBe(false);
    expect(isLegacyDefaultSkin(t3DarkSkin())).toBe(false);
    expect(
      isLegacyDefaultSkin({ ...legacyDefaultSkin(), accent: [1, 2, 3] }),
    ).toBe(false);
  });
});
