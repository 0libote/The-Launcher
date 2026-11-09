import { emptyMetadata, type Game } from './model';
export function fixtureGames(count = 12): Game[] {
  const titles = [
    'Astral Voyager',
    'Mosslight',
    'Circuit Breaker',
    'The Last Orchard',
    'Ocean of Stars',
    'Paper Kingdom',
    'Night Drive',
    'Wildwood',
    'Echoes of Tomorrow',
    'Little Odyssey',
    'Sunset Rally',
    'The Quiet Mountain',
  ];
  const genres = [
    'Adventure',
    'Platformer',
    'Arcade',
    'Exploration',
    'RPG',
    'Strategy',
  ];
  return Array.from({ length: count }, (_, i) => ({
    name: titles[i % titles.length] + (i >= titles.length ? ` ${i + 1}` : ''),
    target:
      i % 4 === 0
        ? {
            kind: 'executable' as const,
            path: `/demo/game-${i}`,
            arguments: [],
            working_directory: null,
          }
        : i % 4 === 1
          ? { kind: 'steam' as const, app_id: 100000 + i }
          : i % 4 === 2
            ? {
                kind: 'epic' as const,
                app_name: `demo-${i}`,
                catalog_namespace: '',
                catalog_item_id: '',
              }
            : { kind: 'xbox' as const, package_id: `demo-${i}!App` },
    favorite: [0, 1, 4, 7].includes(i),
    hidden: false,
    installed: i % 5 !== 4,
    last_launched: i < 4 ? 1794000000 - i * 86400 : null,
    metadata: {
      ...emptyMetadata(),
      description:
        'A place to get lost, a story to make your own. Explore a world full of small discoveries and find a new favorite along the way.',
      genres: [genres[i % genres.length]],
      developers: ['Demo studio'],
      release_date: '2025',
      tags: ['Demo collection'],
    },
  }));
}
