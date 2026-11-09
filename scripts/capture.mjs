import { chromium } from '@playwright/test';
import { mkdir } from 'node:fs/promises';
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH || '/usr/bin/chromium',
  headless: true,
  args: ['--no-sandbox'],
});
const page = await browser.newPage({
  viewport: { width: 1440, height: 1000 },
  deviceScaleFactor: 1,
});
page.on('pageerror', (error) => console.error('PAGE ERROR:', error.message));
await page.goto('http://localhost:1420/?demo');
await page
  .getByRole('button', { name: 'Astral Voyager', exact: true })
  .waitFor();
await page.evaluate(() => document.fonts.ready);
await mkdir('docs/screenshots/rewrite', { recursive: true });
await page.screenshot({
  path: 'docs/screenshots/rewrite/library-grid.png',
  fullPage: false,
});
await page.getByRole('button', { name: 'Astral Voyager', exact: true }).click();
await page.screenshot({
  path: 'docs/screenshots/rewrite/game-details.png',
  fullPage: false,
});
await page.keyboard.press('Escape');
await page.getByRole('button', { name: 'Shelf view', exact: true }).click();
await page.screenshot({
  path: 'docs/screenshots/rewrite/library-shelf.png',
  fullPage: false,
});
await page.getByRole('button', { name: 'List view', exact: true }).click();
await page.screenshot({
  path: 'docs/screenshots/rewrite/library-list.png',
  fullPage: false,
});
await page.getByRole('button', { name: 'Preferences', exact: true }).click();
await page.screenshot({
  path: 'docs/screenshots/rewrite/preferences.png',
  fullPage: false,
});
await page.keyboard.press('Escape');
await page.getByRole('button', { name: 'Add game', exact: true }).click();
await page.screenshot({
  path: 'docs/screenshots/rewrite/add-game.png',
  fullPage: false,
});
await page.keyboard.press('Escape');
await page.goto('http://localhost:1420/?demo&count=0');
await page
  .getByRole('heading', { name: 'Every collection starts somewhere.' })
  .waitFor();
await page.screenshot({
  path: 'docs/screenshots/rewrite/library-empty.png',
  fullPage: false,
});
await page.goto('http://localhost:1420/?demo');
await page
  .getByRole('button', { name: 'Astral Voyager', exact: true })
  .waitFor();
await page.setViewportSize({ width: 390, height: 844 });
await page.screenshot({
  path: 'docs/screenshots/rewrite/library-mobile.png',
  fullPage: false,
});
const dark = await browser.newPage({
  viewport: { width: 1440, height: 1000 },
  deviceScaleFactor: 1,
});
await dark.addInitScript(() => {
  localStorage.setItem('launcher-appearance', 'dark');
});
dark.on('pageerror', (error) => console.error('PAGE ERROR:', error.message));
await dark.goto('http://localhost:1420/?demo');
await dark
  .getByRole('button', { name: 'Astral Voyager', exact: true })
  .waitFor();
await dark.evaluate(() => document.fonts.ready);
await dark.screenshot({
  path: 'docs/screenshots/rewrite/library-grid-dark.png',
  fullPage: false,
});
await dark.getByRole('button', { name: 'Astral Voyager', exact: true }).click();
await dark.screenshot({
  path: 'docs/screenshots/rewrite/game-details-dark.png',
  fullPage: false,
});
await dark.keyboard.press('Escape');
await dark.getByRole('button', { name: 'List view', exact: true }).click();
await dark.screenshot({
  path: 'docs/screenshots/rewrite/library-list-dark.png',
  fullPage: false,
});
await dark.getByRole('button', { name: 'Preferences', exact: true }).click();
await dark.screenshot({
  path: 'docs/screenshots/rewrite/preferences-dark.png',
  fullPage: false,
});
await browser.close();
