/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { resolve } from 'path';

/**
 * Vitest config for the API smoke tests.
 *
 * Key differences from the main vitest config:
 *   - No jsdom (runs in Node — plain HTTP fetch, no DOM)
 *   - No MSW setup file (tests hit the real API)
 *   - Only includes e2e/api-smoke.test.ts
 *   - Sequential execution (tests share state: JWT, orgId, workspaceId)
 *
 * Usage:
 *   just api-smoke                # with E2E stack running
 *   E2E_API_URL=http://localhost:7500 npx vitest run -c vitest.config.smoke.ts
 */
export default defineConfig({
  resolve: {
    alias: {
      katex: resolve(__dirname, 'src/test/__mocks__/katex.ts'),
    },
  },
  test: {
    globals: true,
    environment: 'node',
    include: ['e2e/api-smoke.test.ts'],
    testTimeout: 30_000,
    sequence: {
      concurrent: false,
    },
    alias: {
      katex: resolve(__dirname, 'src/test/__mocks__/katex.ts'),
    },
  },
});
