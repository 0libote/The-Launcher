import { invoke } from '@tauri-apps/api/core';
import {
  defaultSettings,
  defaultWorkspace,
  type Library,
  type Settings,
  type Target,
  type Workspace,
} from './model';
import { fixtureGames } from './fixtures';
export const desktop =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
export const demo =
  !desktop && new URLSearchParams(location.search).has('demo');
export interface Snapshot {
  library: Library;
  workspace: Workspace;
  settings: Settings;
  data_path: string;
  warnings?: string[];
}
const asciiLower = (value: string) =>
  value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
function handle(target: Target): string {
  switch (target.kind) {
    case 'steam':
      return `steam:${target.app_id}`;
    case 'epic':
      return `epic:${asciiLower(target.app_name)}`;
    case 'xbox':
      return `xbox:${asciiLower(target.package_id)}`;
    case 'executable':
      return `exe:${navigator.userAgent.includes('Windows') ? asciiLower(target.path) : target.path}`;
  }
}
async function native(
  name: string,
  args: Record<string, unknown> = {},
): Promise<unknown> {
  return invoke(name, args);
}
// Keep desktop transport out of components so the same screens can use a server API later.
export async function command<T = unknown>(
  name: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop)
    throw new Error(
      'This action needs the desktop app. Open The Launcher to manage your local library.',
    );
  const target = args.target as Target | undefined;
  const id = target ? handle(target) : undefined;
  let result: unknown;
  switch (name) {
    case 'get_snapshot': {
      const [games, workspace, settings, warnings] = await Promise.all([
        native('library_load'),
        native('workspace_load'),
        native('settings_load'),
        native('startup_warnings'),
      ]);
      result = {
        library: { schema_version: 1, games },
        workspace,
        settings,
        warnings,
        data_path: '',
      };
      break;
    }
    case 'save_game':
      result = await native('library_save_game', {
        game: args.game,
        original: args.original ? handle(args.original as Target) : null,
      });
      break;
    case 'update_game': {
      const patch = args.patch as Partial<import('./model').Game>;
      if ('favorite' in patch)
        result = await native('library_set_favorite', {
          handle: id,
          favorite: patch.favorite,
        });
      else if ('hidden' in patch)
        result = await native('library_set_hidden', {
          handle: id,
          hidden: patch.hidden,
        });
      else if (patch.cover === null)
        result = await native('cover_remove', { handle: id });
      else throw new Error('Unsupported game edit.');
      break;
    }
    case 'launch_game':
      result = await native('game_launch', { handle: id });
      break;
    case 'remove_game':
      result = await native('library_remove', { handle: id });
      break;
    case 'open_game_link':
      result = await native('game_link_open', {
        handle: id,
        index: args.index,
      });
      break;
    case 'save_workspace':
      result = await native('workspace_save', args);
      break;
    case 'save_preferences':
      result = await native('preferences_save', args);
      break;
    case 'save_settings':
      result = await native('settings_save', args);
      break;
    case 'open_library_folder':
      result = await native('data_folder_open');
      break;
    case 'pick_path':
      result = await native(
        args.kind === 'folder'
          ? 'pick_folder'
          : args.kind === 'image'
            ? 'pick_image'
            : 'pick_executable',
      );
      break;
    case 'import_games': {
      const source = args.source as string;
      const input =
        source === 'steam'
          ? {
              root: args.root,
              account_id: args.accountId,
              api_key: args.apiKey,
            }
          : source === 'epic'
            ? { manifest_dir: args.root }
            : {};
      result = await native(`import_${source}`, { input });
      break;
    }
    case 'search_metadata':
      result = await native('metadata_search', args);
      break;
    case 'fetch_metadata': {
      const appId =
        args.appId ??
        (target?.kind === 'steam' ? target.app_id : args.linkedId);
      if (!appId)
        throw new Error('Find and select a Steam match for this game first.');
      result = await native('metadata_fetch', {
        input: {
          handle: id,
          app_id: appId,
          replace: args.replace ?? false,
          api_key: args.key || null,
        },
      });
      break;
    }
    case 'choose_cover': {
      const path = await native('pick_image');
      result = path
        ? await native('cover_set_local', { handle: id, path })
        : null;
      break;
    }
    case 'artwork_choices': {
      const appId = target?.kind === 'steam' ? target.app_id : args.linkedId;
      if (!appId)
        throw new Error(
          'Find a Steam match for this game before browsing artwork.',
        );
      result = await native('metadata_image_choices', {
        appId,
        kind: args.kind,
        provider: args.provider ?? 'Steam',
        apiKey: args.key || null,
      });
      break;
    }
    case 'set_artwork':
      result = await native('metadata_apply_image', {
        handle: id,
        kind: args.kind,
        choice: args.choice,
      });
      break;
    case 'read_cover': {
      const thumb = (await native('artwork_thumbnail', {
        handle: id,
        kind: args.kind ?? 'cover',
      })) as { png_base64: string } | null;
      result = thumb ? `data:image/png;base64,${thumb.png_base64}` : null;
      break;
    }
    case 'preview_artwork': {
      const thumb = (await native('artwork_preview', {
        choice: args.choice,
      })) as { png_base64: string };
      result = `data:image/png;base64,${thumb.png_base64}`;
      break;
    }
    case 'backup_library': {
      const path = await native('pick_save_file', {
        title: 'Export library backup',
        fileName: 'the-launcher-backup.json',
      });
      result = path ? await native('backup_export', { path }) : null;
      break;
    }
    case 'restore_library': {
      const path = await native('pick_file', {
        title: 'Choose a library backup',
      });
      result = path ? await native('backup_import', { path }) : null;
      break;
    }
    default:
      throw new Error(`Unknown desktop action: ${name}`);
  }
  return result as T;
}
export function demoSnapshot(): Snapshot {
  let library: Library = {
    schema_version: 1,
    games: fixtureGames(
      Math.min(
        10000,
        Math.max(
          0,
          Number(new URLSearchParams(location.search).get('count') ?? 12),
        ),
      ),
    ),
  };
  try {
    const saved = sessionStorage.getItem('launcher-demo');
    if (saved) library = JSON.parse(saved);
  } catch {
    /* An invalid demo fixture can be discarded. */
  }
  return {
    library,
    workspace: defaultWorkspace(),
    settings: defaultSettings(),
    data_path: 'Browser demo — separate from your library',
  };
}
