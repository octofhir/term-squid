import { defineConfig } from '@playwright/test'
export default defineConfig({
  testDir: './tests', outputDir: '../.benchmarks/ui-admin/browser-results',
  use: { baseURL: process.env.UI_BASE_URL ?? 'http://127.0.0.1:8082', headless: true, launchOptions: process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {} },
  reporter: [['list']], workers: 1,
})
