import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/native',
  fullyParallel: false,
  workers: 1,
  timeout: 90_000,
  expect: { timeout: 12_000 },
  use: { actionTimeout: 15_000 },
  reporter: [['list'], ['html', { open: 'never' }]],
  outputDir: 'test-results',
});

