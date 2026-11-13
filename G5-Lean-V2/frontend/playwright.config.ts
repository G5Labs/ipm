// Thin wrapper so `npx playwright test` works from client/ without
// needing the explicit `-c e2e/playwright.config.ts` flag.
// All paths are made absolute so they resolve identically regardless
// of which config file Playwright loads.

import { defineConfig, devices } from "@playwright/test";
import path from "path";
import { fileURLToPath } from "url";

const __esmDirname = path.dirname(fileURLToPath(import.meta.url));
const E2E_DIR = path.join(__esmDirname, "e2e");
const AUTH_FILE = path.join(E2E_DIR, ".auth", "user.json");
const DEFAULT_BASE_URL = process.env.CI
  ? "http://localhost:3000"
  : "http://localhost:5173";
const PLAYWRIGHT_WORKERS = Number.parseInt(
  process.env.PLAYWRIGHT_WORKERS ?? "1",
  10,
);
// The CI edges serve HTTPS from Caddy's internal CA so previews run on their
// isolated `https://<task>.<suffix>/` hosts as in production. The runner
// ignores the untrusted certificate but still speaks HTTPS, and Chromium
// resolves the wildcard preview hosts through E2E_PREVIEW_HOST_RESOLVER
// ("MAP *.preview.example <edge-ip>"), which Docker DNS cannot express.
const BASE_IS_HTTPS = (process.env.E2E_BASE_URL ?? "").startsWith("https://");
const PREVIEW_HOST_RESOLVER = process.env.E2E_PREVIEW_HOST_RESOLVER;
const CHROMIUM_LAUNCH_ARGS = PREVIEW_HOST_RESOLVER
  ? [`--host-resolver-rules=${PREVIEW_HOST_RESOLVER}`]
  : [];


const VITE_DEV_SERVER_ENV = {
  ...Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => typeof entry[1] === "string",
    ),
  ),
  VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS:
    process.env.VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS ?? "1",
  VITE_G5_ENABLE_INTENT_LOG_VIEWER:
    process.env.VITE_G5_ENABLE_INTENT_LOG_VIEWER ?? "1",
};

const config = defineConfig({
  testDir: path.join(E2E_DIR, "flows"),
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers:
    Number.isFinite(PLAYWRIGHT_WORKERS) && PLAYWRIGHT_WORKERS > 0
      ? PLAYWRIGHT_WORKERS
      : 1,
  reporter: process.env.CI
    ? [
        [path.join(E2E_DIR, "reporters", "ci-progress-reporter.mjs")],
        ["html", { open: "never" }],
        ["github"],
        // Shard-balance calibration (cicd I6): per-file durations for the CI
        // shard split are read from this JSON report; it rides inside the
        // test-results tarball the CI steps already export.
        ["json", { outputFile: "test-results/report.json" }],
      ]
    : [["list"]],
  // Declare a longer budget at load time with test.describe.configure({
  // timeout }) (file, describe or an anonymous test.describe group). The CI
  // progress reporter interrupts the shard only when a test reports no step
  // for max(budget, this timeout) + this timeout + 30 s, and fails the run
  // when a test raises its timeout at runtime.
  timeout: 120_000,
  expect: {
    timeout: process.env.CI ? 15_000 : 10_000,
  },

  use: {
    baseURL: process.env.E2E_BASE_URL || DEFAULT_BASE_URL,
    ignoreHTTPSErrors: BASE_IS_HTTPS,
    trace: process.env.CI ? "on-first-retry" : "retain-on-failure",
    screenshot: "only-on-failure",
    video: process.env.CI ? "on-first-retry" : "off",
    actionTimeout: process.env.CI ? 60_000 : 15_000,
  },

  projects: [
    {
      name: "setup",
      testDir: E2E_DIR,
      testMatch: /auth\.setup\.ts/,
    },
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        storageState: AUTH_FILE,
        launchOptions: { args: CHROMIUM_LAUNCH_ARGS },
      },
      dependencies: ["setup"],
    },
    {
      name: "firefox",
      use: { ...devices["Desktop Firefox"], storageState: AUTH_FILE },
      dependencies: ["setup"],
    },
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], storageState: AUTH_FILE },
      dependencies: ["setup"],
    },
  ],

  webServer:
    process.env.CI || process.env.E2E_BASE_URL
      ? undefined
      : {
          command: "npm run dev",
          env: VITE_DEV_SERVER_ENV,
          url: DEFAULT_BASE_URL,
          reuseExistingServer: true,
          timeout: 30_000,
        },
});

export default config;
