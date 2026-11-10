import { emptyMetadata, type Game, type Target } from './model';

// Demo-only catalogue: real store games with hotlinked Steam CDN artwork so
// screenshots and UI iteration look like a lived-in library. Nothing here is
// bundled or shipped; offline the covers fall back to generated art. The two
// entries without CDN art (Fortnite, Halo Infinite) intentionally exercise
// that fallback alongside real covers.
const cdn = (appId: number, file: string) =>
  `https://cdn.cloudflare.steamstatic.com/steam/apps/${appId}/${file}`;
const art = (appId: number) => ({
  cover: cdn(appId, 'library_600x900.jpg'),
  background: cdn(appId, 'library_hero.jpg'),
  logo: cdn(appId, 'logo.png'),
});

interface Template {
  name: string;
  target: Target;
  art: number | null;
  favorite: boolean;
  installed: boolean;
  recent: boolean;
  developers: string[];
  release: string;
  genres: string[];
  tags: string[];
  description: string;
}

const RECENT = 1794000000;
const DAY = 86400;

const templates: Template[] = [
  {
    name: 'Terraria',
    target: {
      kind: 'executable',
      path: '/demo/terraria',
      arguments: [],
      working_directory: null,
    },
    art: 105600,
    favorite: true,
    installed: true,
    recent: true,
    developers: ['Re-Logic'],
    release: '2011',
    genres: ['Sandbox'],
    tags: ['Demo collection', 'Survival', 'Co-op'],
    description:
      'Dig, fight, explore, and build in a procedurally generated world where every night brings something new to survive.',
  },
  {
    name: 'Portal 2',
    target: { kind: 'steam', app_id: 620 },
    art: 620,
    favorite: true,
    installed: true,
    recent: true,
    developers: ['Valve'],
    release: '2011',
    genres: ['Puzzle'],
    tags: ['Demo collection', 'Co-op', 'Classic'],
    description:
      'GLaDOS is back with a new testing initiative. Bend space with portals across a devious single-player campaign and two-player co-op.',
  },
  {
    name: 'Cyberpunk 2077',
    target: {
      kind: 'epic',
      app_name: 'cyberpunk2077',
      catalog_namespace: 'cdprojektred',
      catalog_item_id: '',
    },
    art: 1091500,
    favorite: false,
    installed: true,
    recent: true,
    developers: ['CD PROJEKT RED'],
    release: '2020',
    genres: ['RPG'],
    tags: ['Demo collection', 'Open world', 'Story rich'],
    description:
      'An open-world action RPG on the neon streets of Night City, where a cyberware heist gone wrong leaves a legend living in your head.',
  },
  {
    name: 'Hollow Knight',
    target: { kind: 'xbox', package_id: 'teamcherry-hollowknight!App' },
    art: 367520,
    favorite: false,
    installed: true,
    recent: true,
    developers: ['Team Cherry'],
    release: '2017',
    genres: ['Metroidvania'],
    tags: ['Demo collection', 'Hand-drawn', 'Boss fights'],
    description:
      'Descend into Hallownest, a fallen kingdom of bugs and heroes. Master nail combat and ancient magic to seal the spreading infection.',
  },
  {
    name: 'Stardew Valley',
    target: {
      kind: 'executable',
      path: '/demo/stardew',
      arguments: [],
      working_directory: null,
    },
    art: 413150,
    favorite: true,
    installed: false,
    recent: false,
    developers: ['ConcernedApe'],
    release: '2016',
    genres: ['Simulation'],
    tags: ['Demo collection', 'Cozy', 'Farming'],
    description:
      'Inherit a run-down farm and make it your own. Grow crops, raise animals, befriend the valley, and settle in for the seasons.',
  },
  {
    name: 'Hades',
    target: { kind: 'steam', app_id: 1145360 },
    art: 1145360,
    favorite: false,
    installed: true,
    recent: false,
    developers: ['Supergiant Games'],
    release: '2020',
    genres: ['Roguelike'],
    tags: ['Demo collection', 'Fast-paced', 'Mythology'],
    description:
      'Defy the god of the dead as Zagreus, Prince of the Underworld. Every escape attempt makes you stronger for the next.',
  },
  {
    name: 'The Witcher 3',
    target: {
      kind: 'epic',
      app_name: 'thewitcher3',
      catalog_namespace: 'cdprojektred',
      catalog_item_id: '',
    },
    art: 292030,
    favorite: false,
    installed: true,
    recent: false,
    developers: ['CD PROJEKT RED'],
    release: '2015',
    genres: ['RPG'],
    tags: ['Demo collection', 'Open world', 'Story rich'],
    description:
      'Ride with Geralt of Rivia across war-torn kingdoms to find the child of prophecy in a saga where every choice has a price.',
  },
  {
    name: 'Celeste',
    target: { kind: 'xbox', package_id: 'maddymakesgames-celeste!App' },
    art: 504230,
    favorite: true,
    installed: true,
    recent: false,
    developers: ['Maddy Makes Games'],
    release: '2018',
    genres: ['Platformer'],
    tags: ['Demo collection', 'Precise', 'Emotional'],
    description:
      'Climb Celeste Mountain one screen at a time. A tight, heartfelt platformer about perseverance, with assist options for every player.',
  },
  {
    name: 'Dead Cells',
    target: {
      kind: 'executable',
      path: '/demo/deadcells',
      arguments: [],
      working_directory: null,
    },
    art: 588650,
    favorite: false,
    installed: true,
    recent: false,
    developers: ['Motion Twin'],
    release: '2018',
    genres: ['Roguelike'],
    tags: ['Demo collection', 'Fast-paced', 'Pixel art'],
    description:
      'A rogue-lite without checkpoints. Carve through an ever-changing castle, stack brutal weapon synergies, and die trying.',
  },
  {
    name: 'Slay the Spire',
    target: { kind: 'steam', app_id: 646570 },
    art: 646570,
    favorite: false,
    installed: false,
    recent: false,
    developers: ['Mega Crit'],
    release: '2019',
    genres: ['Card game'],
    tags: ['Demo collection', 'Deck-building', 'Strategy'],
    description:
      'Climb a living spire with a deck you build as you go. Hundreds of cards and relics make every run a new puzzle.',
  },
  {
    name: 'Fortnite',
    target: {
      kind: 'epic',
      app_name: 'fortnite',
      catalog_namespace: 'epicgames',
      catalog_item_id: '',
    },
    art: null,
    favorite: false,
    installed: true,
    recent: false,
    developers: ['Epic Games'],
    release: '2017',
    genres: ['Battle royale'],
    tags: ['Demo collection', 'Free to play', 'Multiplayer'],
    description:
      'Drop onto the island with 99 other players. Build, shoot, and outlast the storm in the ever-changing battle royale.',
  },
  {
    name: 'Halo Infinite',
    target: { kind: 'xbox', package_id: 'microsoft-haloinfinite!App' },
    art: null,
    favorite: false,
    installed: true,
    recent: false,
    developers: ['343 Industries'],
    release: '2021',
    genres: ['Shooter'],
    tags: ['Demo collection', 'Multiplayer', 'Campaign'],
    description:
      'The Master Chief returns to Zeta Halo. Take the fight to the Banished across the widest Halo campaign yet.',
  },
];

export function fixtureGames(count = 12): Game[] {
  return Array.from({ length: count }, (_, i) => {
    const template = templates[i % templates.length];
    const name =
      i < templates.length ? template.name : `${template.name} ${i + 1}`;
    // Later pages reuse the base catalogue look; handles stay unique.
    const target: Target =
      i < templates.length
        ? template.target
        : i % 4 === 0
          ? {
              kind: 'executable' as const,
              path: `/demo/game-${i}`,
              arguments: [],
              working_directory: null,
            }
          : i % 4 === 1
            ? { kind: 'steam' as const, app_id: 200000 + i }
            : i % 4 === 2
              ? {
                  kind: 'epic' as const,
                  app_name: `demo-${i}`,
                  catalog_namespace: '',
                  catalog_item_id: '',
                }
              : { kind: 'xbox' as const, package_id: `demo-${i}!App` };
    const images: { cover?: string; background?: string; logo?: string } =
      template.art ? art(template.art) : {};
    return {
      name,
      target,
      favorite: template.favorite,
      hidden: false,
      installed: template.installed,
      last_launched:
        template.recent && i < templates.length ? RECENT - i * DAY : null,
      cover: images.cover ?? null,
      metadata: {
        ...emptyMetadata(),
        description: template.description,
        genres: template.genres,
        developers: template.developers,
        release_date: template.release,
        tags: template.tags,
        background: images.background ?? null,
        logo: images.logo ?? null,
      },
    };
  });
}
