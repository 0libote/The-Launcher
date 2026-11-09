import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
test('library and game details have no serious accessibility violations', async ({
  page,
}) => {
  await page.goto('/?demo');
  await page
    .getByRole('button', { name: 'Astral Voyager', exact: true })
    .waitFor();
  const library = await new AxeBuilder({ page }).analyze();
  expect(
    library.violations.filter((v) =>
      ['serious', 'critical'].includes(v.impact ?? ''),
    ),
    JSON.stringify(library.violations),
  ).toEqual([]);
  await page
    .getByRole('button', { name: 'Astral Voyager', exact: true })
    .click();
  const details = await new AxeBuilder({ page }).analyze();
  expect(
    details.violations.filter((v) =>
      ['serious', 'critical'].includes(v.impact ?? ''),
    ),
    JSON.stringify(details.violations),
  ).toEqual([]);
});
