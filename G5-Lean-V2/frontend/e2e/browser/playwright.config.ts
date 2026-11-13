import { defineConfig, devices } from "@playwright/test";

// Real browser layout and component interactions, without a running API stack.
// Persistence/authority is covered by api_workspace_reviews and the native flow.
export default defineConfig({
  testDir: ".",
  testMatch: "*.spec.ts",
  workers: 1,
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 900 },
    baseURL: "http://127.0.0.1:5174",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1",
    url: "http://127.0.0.1:5174",
    reuseExistingServer: true,
  },
});
