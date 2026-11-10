import { test, expect } from '@playwright/test';
test('browse, search, favorite, hide, and restore entries', async ({
  page,
}) => {
  await page.goto('/?demo');
  await expect(page.locator('.game-card')).toHaveCount(12);
  await page.getByRole('textbox', { name: 'Search games' }).fill('portal');
  await expect(page.locator('.game-card')).toHaveCount(1);
  await page
    .getByRole('button', { name: 'Portal 2', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Remove from favorites', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: 'Add to favorites', exact: true }),
  ).toBeVisible();
  await page
    .getByRole('button', { name: 'Hide from library', exact: true })
    .click();
  await expect(
    page.getByRole('heading', { name: 'No games on this shelf' }),
  ).toBeVisible();
  await page.getByRole('button', { name: /^Hidden games/ }).click();
  await page
    .getByRole('button', { name: 'Portal 2', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Restore to library', exact: true })
    .click();
  await page.getByRole('button', { name: /^All games/ }).click();
  await expect(
    page.getByRole('button', { name: 'Portal 2', exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(
    page.getByRole('button', {
      name: 'Unfavorite Portal 2',
      exact: true,
    }),
  ).toHaveCount(0);
});
test('add, edit, duplicate validation, and remove an executable entry', async ({
  page,
}) => {
  await page.goto('/?demo&count=0');
  await page.getByRole('button', { name: 'Add your first game' }).click();
  await page.getByLabel('Game name', { exact: true }).fill('My local game');
  await page.getByLabel('Executable', { exact: true }).fill('/demo/my-game');
  await page.getByRole('button', { name: 'Save game' }).click();
  await page
    .getByRole('button', { name: 'My local game', exact: true })
    .click();
  await page.getByRole('button', { name: 'Edit game', exact: true }).click();
  await page.getByLabel('Game name', { exact: true }).fill('My renamed game');
  await page.getByRole('button', { name: 'Save game' }).click();
  await page
    .getByRole('button', { name: 'Remove from library', exact: true })
    .click();
  await page.getByRole('button', { name: 'Remove game', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: 'Every collection starts somewhere.' }),
  ).toBeVisible();
});
test('keyboard selection, dialog escape, view changes and preferences cancel', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/?demo');
  await expect(page.locator('.game-card')).toHaveCount(12);
  await page.locator('main').focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('.game-card[data-selected="true"]')).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.getByRole('button', { name: 'List view', exact: true }).click();
  await expect(page.locator('.games')).toHaveClass(/list-view/);
  await page.getByRole('button', { name: 'Preferences', exact: true }).click();
  await page.getByLabel('Navigation', { exact: true }).selectOption('right');
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(page.locator('.app-shell')).not.toHaveClass(/nav-right/);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.keyboard.press('/');
  await expect(
    page.getByRole('textbox', { name: 'Search games' }),
  ).toBeFocused();
  expect(errors).toEqual([]);
});
test('mobile navigation and dialogs fit the viewport', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/?demo');
  await page
    .getByRole('button', { name: 'Toggle navigation', exact: true })
    .click();
  await page.getByRole('button', { name: /^Favorites/ }).click();
  await expect(
    page.getByRole('heading', { name: 'Favorites', exact: true }),
  ).toBeVisible();
  await page
    .getByRole('button', { name: 'Portal 2', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: 'Play game', exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Add game', exact: true }).click();
  const box = await page.getByRole('dialog').boundingBox();
  expect(box!.width).toBeLessThanOrEqual(390);
});
test('demo launch errors are actionable and do not update played timestamps', async ({
  page,
}) => {
  await page.goto('/?demo');
  await page.getByRole('button', { name: 'Hades', exact: true }).click();
  await page.getByRole('button', { name: 'Play game', exact: true }).click();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('alert')).toContainText(
    'Demo games cannot be launched',
  );
});

test('large collections reveal batches and search beyond the first batch', async ({
  page,
}) => {
  await page.goto('/?demo&count=5000');
  await expect(page.locator('.game-card')).toHaveCount(80);
  await page
    .getByRole('textbox', { name: 'Search games' })
    .fill('Halo Infinite 4992');
  await expect(page.locator('.game-card')).toHaveCount(1);
  await page
    .getByRole('button', { name: 'Halo Infinite 4992', exact: true })
    .click();
  await expect(
    page.getByRole('heading', { name: 'Halo Infinite 4992', exact: true }),
  ).toBeVisible();
});

test('controller resumes browsing after closing a preferences dialog', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const pad = {
      connected: true,
      axes: [0, 0],
      buttons: Array.from({ length: 16 }, () => ({ pressed: false })),
    };
    (window as any).__pad = pad;
    Object.defineProperty(navigator, 'getGamepads', { value: () => [pad] });
  });
  await page.goto('/?demo');
  await expect(page.locator('.game-card')).toHaveCount(12);
  await page.evaluate(() => {
    (window as any).__pad.axes[0] = 1;
  });
  await expect(page.locator('.game-card[data-selected="true"]')).toHaveCount(1);
  await page.evaluate(() => {
    (window as any).__pad.axes[0] = 0;
  });
  await page.getByRole('button', { name: 'Preferences', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.evaluate(() => {
    (window as any).__pad.buttons[1].pressed = true;
  });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const selected = await page
    .locator('.game-card[data-selected="true"]')
    .innerText();
  await page.evaluate(() => {
    (window as any).__pad.buttons[1].pressed = false;
    (window as any).__pad.axes[0] = 1;
  });
  await expect
    .poll(() => page.locator('.game-card[data-selected="true"]').innerText())
    .not.toBe(selected);
  await page.evaluate(() => {
    (window as any).__pad.connected = false;
  });
});
