export type Target =
  | {
      kind: 'executable';
      path: string;
      arguments: string[];
      working_directory: string | null;
    }
  | { kind: 'steam'; app_id: number }
  | {
      kind: 'epic';
      app_name: string;
      catalog_namespace: string;
      catalog_item_id: string;
    }
  | { kind: 'xbox'; package_id: string };
export interface Metadata {
  steam_app_id?: number | null;
  description: string;
  developers: string[];
  publishers: string[];
  genres: string[];
  features: string[];
  tags: string[];
  links: string[];
  age_rating: string;
  series: string;
  critic_score: number | null;
  release_date: string;
  background?: string | null;
  logo?: string | null;
  sources: Record<string, string>;
}
export interface Game {
  name: string;
  target: Target;
  favorite: boolean;
  hidden: boolean;
  installed: boolean | null;
  last_launched: number | null;
  cover?: string | null;
  metadata: Metadata;
}
export interface Library {
  schema_version: number;
  games: Game[];
}
export type View = 'grid' | 'list' | 'shelf';
export type Collection =
  | 'All games'
  | 'Favorites'
  | 'Recently played'
  | 'PC'
  | 'Steam'
  | 'Epic'
  | 'Xbox'
  | 'Hidden';
export type Sort = 'name' | 'recent' | 'added';
export interface Layout {
  navigation: 'left' | 'right' | 'hidden';
  details: 'modal' | 'right' | 'hidden';
  tools: 'top' | 'bottom';
  view: View;
  tile_width: number;
  navigation_width: number;
  details_width: number;
  show_shortcuts: boolean;
}
export type Skin = Record<
  | 'background'
  | 'navigation'
  | 'surface'
  | 'raised'
  | 'text'
  | 'muted'
  | 'faint'
  | 'line'
  | 'accent'
  | 'selection',
  [number, number, number]
>;
export interface Workspace {
  version: number;
  active: number;
  layouts: { name: string; layout: Layout; skin: Skin }[];
}
export interface Settings {
  automatic: boolean;
  download_text: boolean;
  download_cover: boolean;
  download_background: boolean;
  download_logo: boolean;
  artwork_provider: 'Steam' | 'SteamGridDb';
}
export const emptyMetadata = (): Metadata => ({
  description: '',
  developers: [],
  publishers: [],
  genres: [],
  features: [],
  tags: [],
  links: [],
  age_rating: '',
  series: '',
  critic_score: null,
  release_date: '',
  sources: {},
});
export const defaultSettings = (): Settings => ({
  automatic: true,
  download_text: true,
  download_cover: true,
  download_background: true,
  download_logo: false,
  artwork_provider: 'Steam',
});
export const t3LightSkin = (): Skin => ({
  background: [252, 252, 252],
  navigation: [250, 250, 250],
  surface: [255, 255, 255],
  raised: [244, 244, 245],
  text: [24, 24, 27],
  muted: [82, 82, 91],
  faint: [113, 113, 122],
  line: [228, 228, 231],
  accent: [219, 39, 119],
  selection: [219, 39, 119],
});
export const t3DarkSkin = (): Skin => ({
  background: [9, 9, 11],
  navigation: [16, 16, 19],
  surface: [20, 20, 23],
  raised: [31, 31, 36],
  text: [250, 250, 250],
  muted: [161, 161, 170],
  faint: [113, 113, 122],
  line: [38, 38, 43],
  accent: [244, 114, 182],
  selection: [244, 114, 182],
});
export const defaultWorkspace = (): Workspace => ({
  version: 1,
  active: 0,
  layouts: [
    {
      name: 'My library',
      layout: {
        navigation: 'left',
        details: 'modal',
        tools: 'top',
        view: 'grid',
        tile_width: 180,
        navigation_width: 232,
        details_width: 300,
        show_shortcuts: true,
      },
      skin: t3LightSkin(),
    },
  ],
});
export function source(game: Game): string {
  return { executable: 'PC', steam: 'Steam', epic: 'Epic', xbox: 'Xbox' }[
    game.target.kind
  ];
}
// A target identifies the entry independently of the currently sorted/filtered view.
export const asciiLower = (value: string) =>
  value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
export function identity(game: Game): string {
  const target = game.target;
  switch (target.kind) {
    case 'steam':
      return `steam:${target.app_id}`;
    case 'epic':
      return `epic:${asciiLower(target.app_name)}`;
    case 'xbox':
      return `xbox:${asciiLower(target.package_id)}`;
    case 'executable':
      return `executable:${target.path}`;
  }
}
export function filterGames(
  games: Game[],
  collection: Collection,
  query: string,
  sort: Sort,
  installedOnly = false,
): Game[] {
  const term = query.trim().toLocaleLowerCase();
  const result = games.filter((game) => {
    if (collection === 'Hidden' ? !game.hidden : game.hidden) return false;
    if (collection === 'Favorites' && !game.favorite) return false;
    if (collection === 'Recently played' && !game.last_launched) return false;
    if (
      ['PC', 'Steam', 'Epic', 'Xbox'].includes(collection) &&
      source(game) !== collection
    )
      return false;
    if (installedOnly && game.installed !== true) return false;
    return (
      !term ||
      [game.name, ...game.metadata.genres, ...game.metadata.tags].some((text) =>
        text.toLocaleLowerCase().includes(term),
      )
    );
  });
  if (sort === 'name') result.sort((a, b) => a.name.localeCompare(b.name));
  if (sort === 'recent')
    result.sort(
      (a, b) =>
        (b.last_launched ?? 0) - (a.last_launched ?? 0) ||
        a.name.localeCompare(b.name),
    );
  if (sort === 'added') result.reverse();
  return result;
}
export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => Array.from(word)[0])
    .join('')
    .toUpperCase();
}
export function artSeed(name: string): number {
  return (
    Array.from(name).reduce(
      (hash, char) => (hash * 31 + char.charCodeAt(0)) >>> 0,
      7,
    ) % 6
  );
}
export function lastPlayed(timestamp: number | null): string {
  if (!timestamp) return 'Not played yet';
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(
    new Date(timestamp * 1000),
  );
}
