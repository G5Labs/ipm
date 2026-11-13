import { defineConfig, devices } from "@playwright/test";

// The `frontend-only` preview profile (.g5/preview_profiles/frontend-only.yaml):
// the real client on Vite with the in-browser mock API (VITE_MOCK=true), on the
// profile's own port, with no API stack. These journeys check that the preview
// shows the shipped UI with usable data rather than empty or failed tiles.
export default defineConfig({
  testDir: ".",
  testMatch: "*.spec.ts",
  workers: 1,
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 1440, height: 900 },
    baseURL: "http://127.0.0.1:15174",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 15174 --strictPort",
    url: "http://127.0.0.1:15174",
    env: { VITE_MOCK: "true" },
    reuseExistingServer: true,
  },
});
