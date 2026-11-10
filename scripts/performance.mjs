import { cpus, platform, arch } from 'node:os';
import { chromium } from '@playwright/test';
import { mkdir, writeFile, readdir, stat } from 'node:fs/promises';
const baseURL = process.env.BENCHMARK_URL || 'http://localhost:1421';
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH || '/usr/bin/chromium',
  args: ['--no-sandbox'],
});
const results = [];
for (const count of [100, 1000, 5000]) {
  const samples = [];
  for (let i = 0; i < 3; i++) {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 800 },
    });
    const page = await context.newPage();
    await page.goto(`${baseURL}/?demo&count=${count}`);
    await page
      .getByRole('button', { name: 'Terraria', exact: true })
      .waitFor();
    await page.evaluate(() => document.fonts.ready);
    const readyMs = await page.evaluate(() => performance.now());
    const started = performance.now();
    await page
      .getByRole('textbox', { name: 'Search games' })
      .fill('Witcher');
    await page.waitForFunction(
      () =>
        document.querySelectorAll('.game-card').length > 0 &&
        [...document.querySelectorAll('.game-name')].every((el) =>
          el.textContent.includes('Witcher'),
        ),
    );
    const searchMs = performance.now() - started;
    const cdp = await context.newCDPSession(page);
    await cdp.send('Performance.enable');
    await page.waitForTimeout(1000);
    const before = (await cdp.send('Performance.getMetrics')).metrics.find(
      (m) => m.name === 'TaskDuration',
    ).value;
    await page.waitForTimeout(5000);
    const metrics = (await cdp.send('Performance.getMetrics')).metrics;
    const cpu =
      ((metrics.find((m) => m.name === 'TaskDuration').value - before) / 5) *
      100;
    samples.push({
      readyMs: Math.round(readyMs),
      searchMs: Math.round(searchMs),
      jsHeapMiB: +(
        metrics.find((m) => m.name === 'JSHeapUsedSize').value / 1048576
      ).toFixed(1),
      idleRendererTaskPercent: +cpu.toFixed(2),
    });
    await context.close();
  }
  results.push({ games: count, samples });
  console.log(JSON.stringify(results.at(-1)));
}
const assets = [];
for (const name of await readdir('dist/assets'))
  assets.push({ name, bytes: (await stat(`dist/assets/${name}`)).size });
const report = {
  measuredAt: new Date().toISOString(),
  browser: await browser.version(),
  hardware: {
    cpu: cpus()[0]?.model,
    logicalCPUs: cpus().length,
    platform: platform(),
    arch: arch(),
    host: 'Shared host; background load and caches vary',
  },
  environment:
    'Headless Chromium on Linux; production Vite assets over localhost. Demo catalogue games with remote covers. Not native Tauri or Windows. Ready includes navigation, UI and fonts; search includes automation overhead. Heap is JS only; idle is renderer TaskDuration over five seconds after a one-second settling period.',
  assets,
  results,
};
await mkdir('docs/performance', { recursive: true });
await writeFile(
  'docs/performance/rewrite.json',
  JSON.stringify(report, null, 2) + '\n',
);
await browser.close();
