import { test, expect } from '@playwright/test';
import { fixtureGames } from '../frontend/lib/fixtures';
import { defaultSettings, defaultWorkspace } from '../frontend/lib/model';
import { readFileSync } from 'node:fs';

test('desktop screens send supported commands and apply returned library state', async ({
  page,
}) => {
  const games = fixtureGames(2);
  await page.addInitScript(
    ({ games, workspace, settings }) => {
      const state = { games, workspace, settings };
      const calls: { name: string; args: Record<string, any> }[] = [];
      (window as any).__calls = calls;
      (window as any).__TAURI_INTERNALS__ = {
        invoke: async (name: string, args: Record<string, any> = {}) => {
          calls.push({ name, args });
          switch (name) {
            case 'library_load':
              return structuredClone(state.games);
            case 'workspace_load':
              return structuredClone(state.workspace);
            case 'settings_load':
              return structuredClone(state.settings);
            case 'startup_warnings':
              return [];
            case 'library_set_favorite':
              state.games[0].favorite = args.favorite;
              return structuredClone(state.games[0]);
            case 'library_save_game':
              state.games[0].name = args.game.name;
              state.games[0].metadata.tags = args.game.metadata.tags;
              return structuredClone(state.games[0]);
            case 'metadata_search':
              return [{ app_id: 620, name: 'Matching game' }];
            case 'metadata_fetch':
              if ((window as any).__holdFetch)
                return new Promise((resolve) => {
                  (window as any).__releaseFetch = () =>
                    resolve({
                      game: structuredClone(state.games[0]),
                      warnings: [],
                    });
                });
              state.games[0].metadata.description = 'Fetched description';
              return { game: structuredClone(state.games[0]), warnings: [] };
            case 'game_launch':
              state.games[0].last_launched = 1794000000;
              return { launched: true, dispatched: 'fixture' };
            case 'preferences_save':
              state.workspace = args.workspace;
              state.settings = args.settings;
              return null;
            case 'workspace_save':
              state.workspace = args.workspace;
              return structuredClone(state.workspace);
            case 'settings_save':
              state.settings = args.settings;
              return structuredClone(state.settings);
            default:
              throw new Error(`Unexpected native command: ${name}`);
          }
        },
      };
    },
    {
      games,
      workspace: defaultWorkspace(),
      settings: { ...defaultSettings(), automatic: false },
    },
  );
  await page.goto('/');
  await page
    .getByRole('button', { name: 'Terraria', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Remove from favorites', exact: true })
    .click();
  await page.getByRole('button', { name: 'Edit game', exact: true }).click();
  await page
    .getByLabel('Game name', { exact: true })
    .fill('A renamed native game');
  await page.getByRole('button', { name: 'Save game', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'A renamed native game', exact: true }),
  ).toBeVisible();
  await page
    .getByRole('button', { name: 'Find metadata', exact: true })
    .click();
  await page.getByRole('button', { name: 'Search', exact: true }).click();
  await page.getByRole('button', { name: /Matching game/ }).click();
  await expect(
    page.getByText('Fetched description', { exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Play game', exact: true }).click();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('status')).toContainText('Launch request sent');
  await page.getByRole('button', { name: 'Preferences', exact: true }).click();
  await page.getByLabel('Navigation', { exact: true }).selectOption('right');
  await page
    .getByRole('button', { name: 'Save preferences', exact: true })
    .click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.locator('.app-shell')).toHaveClass(/nav-right/);
  // A completed old request must leave a newly opened editor intact.
  await page
    .getByRole('button', { name: 'A renamed native game', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Find metadata', exact: true })
    .click();
  await page.getByRole('button', { name: 'Search', exact: true }).click();
  await page.evaluate(() => {
    (window as any).__holdFetch = true;
  });
  await page.getByRole('button', { name: /Matching game/ }).click();
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Add game', exact: true }).click();
  await page
    .getByLabel('Game name', { exact: true })
    .fill('Unfinished new game');
  await page.evaluate(() => {
    (window as any).__releaseFetch();
  });
  await expect(
    page.getByRole('dialog', { name: 'Add a game', exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel('Game name', { exact: true })).toHaveValue(
    'Unfinished new game',
  );
  const calls = (await page.evaluate(() => (window as any).__calls)) as {
    name: string;
    args: Record<string, any>;
  }[];
  const registered = readFileSync('src/main.rs', 'utf8');
  for (const call of calls)
    expect(registered).toContain(`desktop::${call.name},`);
  expect(
    calls.find((call) => call.name === 'library_set_favorite')?.args,
  ).toEqual({ handle: 'exe:/demo/terraria', favorite: false });
  expect(
    calls.find((call) => call.name === 'library_save_game')?.args.original,
  ).toBe('exe:/demo/terraria');
  expect(
    calls.find((call) => call.name === 'metadata_fetch')?.args.input,
  ).toMatchObject({ handle: 'exe:/demo/terraria', app_id: 620, replace: false });
  expect(calls.find((call) => call.name === 'game_launch')?.args.handle).toBe(
    'exe:/demo/terraria',
  );
});
