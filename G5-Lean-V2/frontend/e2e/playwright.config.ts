import { defineConfig, devices } from "@playwright/test";
import path from "path";
import { fileURLToPath } from "url";

const __esmDirname = path.dirname(fileURLToPath(import.meta.url));
const AUTH_FILE = path.join(__esmDirname, ".auth", "user.json");


const DEFAULT_BASE_URL = process.env.CI
  ? "http://localhost:3000"
  : "http://localhost:5173";

export default defineConfig({
  testDir: path.join(__esmDirname, "flows"),
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI
    ? [
        [path.join(__esmDirname, "reporters", "ci-progress-reporter.mjs")],
        ["html", { open: "never" }],
        ["github"],
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
    // The CI edges serve HTTPS from Caddy's internal CA (see the root config).
    ignoreHTTPSErrors: (process.env.E2E_BASE_URL ?? "").startsWith("https://"),
    trace: process.env.CI ? "on-first-retry" : "retain-on-failure",
    screenshot: "only-on-failure",
    video: process.env.CI ? "on-first-retry" : "off",
    actionTimeout: process.env.CI ? 60_000 : 15_000,
  },

  projects: [
    // Auth setup project — runs first, saves auth state
    {
      name: "setup",
      testDir: __esmDirname,
      testMatch: /auth\.setup\.ts/,
    },
    // Desktop Chromium — primary test target
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        storageState: AUTH_FILE,
        // Wildcard preview hosts resolve through a Chromium rule
        // ("MAP *.preview.example <edge>"); Docker DNS cannot express them.
        launchOptions: {
          args: process.env.E2E_PREVIEW_HOST_RESOLVER
            ? [`--host-resolver-rules=${process.env.E2E_PREVIEW_HOST_RESOLVER}`]
            : [],
        },
      },
      dependencies: ["setup"],
    },
    // Desktop Firefox — cross-browser validation
    {
      name: "firefox",
      use: {
        ...devices["Desktop Firefox"],
        storageState: AUTH_FILE,
      },
      dependencies: ["setup"],
    },
    // Desktop WebKit — Safari compatibility
    {
      name: "webkit",
      use: {
        ...devices["Desktop Safari"],
        storageState: AUTH_FILE,
      },
      dependencies: ["setup"],
    },
  ],

  // In CI, the Docker Compose test stack provides the frontend at :3000.
  // For local dev without Docker, start the Vite dev server automatically.
  webServer:
    process.env.CI || process.env.E2E_BASE_URL
      ? undefined
      : {
          command: "npm run dev",
          env: {
            ...Object.fromEntries(
              Object.entries(process.env).filter(
                (entry): entry is [string, string] => {
                  return typeof entry[1] === "string";
                },
              ),
            ),
            VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS:
              process.env.VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS ?? "1",
            VITE_G5_ENABLE_INTENT_LOG_VIEWER:
              process.env.VITE_G5_ENABLE_INTENT_LOG_VIEWER ?? "1",
          },
          url: DEFAULT_BASE_URL,
          reuseExistingServer: true,
          timeout: 30_000,
        },
});
