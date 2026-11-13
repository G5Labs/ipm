/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, extname, join, resolve } from 'path';

const apiUrl = process.env.API_URL || 'http://localhost:7500';

/**
 * PDF.js as G5 ships it (`g5.documents.viewers.pdf`): the pinned
 * `pdfjs-dist` compatibility build and its worker, the character maps,
 * standard fonts, ICC profile and image-decoder WebAssembly, and the license
 * notices, emitted unmodified into `assets/pdfjs/<version>/`, where the edge
 * serves them immutable. The viewer imports the build from there at runtime
 * (`src/components/files/pdfjs.ts`, through the `virtual:g5-pdfjs` version)
 * rather than bundling it: PDF.js is self-contained, and parsing its two
 * megabytes took the build past Node's default heap. The scripting sandbox
 * (`quickjs-eval.*`) is deliberately left out. The dev server serves the
 * same paths from node_modules.
 */
function pdfjsAssets(): Plugin {
  const require = createRequire(import.meta.url);
  const packageJson = require.resolve('pdfjs-dist/package.json');
  const root = dirname(packageJson);
  const { version } = JSON.parse(readFileSync(packageJson, 'utf8')) as {
    version: string;
  };
  const prefix = `assets/pdfjs/${version}/`;
  const directories = ['cmaps', 'iccs', 'standard_fonts', 'wasm'];
  const contentTypes: Record<string, string> = {
    '.wasm': 'application/wasm',
    '.js': 'text/javascript',
    '.mjs': 'text/javascript',
    '.ttf': 'font/ttf',
    '.icc': 'application/vnd.iccprofile',
  };
  const virtualId = 'virtual:g5-pdfjs';
  const resolvedVirtualId = '\0' + virtualId;
  let files: Map<string, string> | null = null;
  const assetFiles = (): Map<string, string> => {
    if (files) return files;
    // The ES module builds keep a `.js` name so every edge serves them as
    // JavaScript, which module imports and workers require.
    files = new Map([
      ['LICENSE', join(root, 'LICENSE')],
      ['pdf.min.js', join(root, 'legacy', 'build', 'pdf.min.mjs')],
      ['pdf.worker.min.js', join(root, 'legacy', 'build', 'pdf.worker.min.mjs')],
    ]);
    for (const directory of directories) {
      for (const name of readdirSync(join(root, directory))) {
        const path = join(root, directory, name);
        if (name.startsWith('quickjs-eval') || !statSync(path).isFile()) continue;
        files.set(`${directory}/${name}`, path);
      }
    }
    return files;
  };
  return {
    name: 'g5-pdfjs-assets',
    resolveId(id) {
      return id === virtualId ? resolvedVirtualId : null;
    },
    load(id) {
      return id === resolvedVirtualId
        ? `export const version = ${JSON.stringify(version)};\n`
        : null;
    },
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const url = (request.url ?? '').split('?')[0];
        if (!url.startsWith(`/${prefix}`)) return next();
        // Only listed files are served, so a path can never escape them.
        const name = decodeURIComponent(url.slice(prefix.length + 1));
        const path = assetFiles().get(name);
        if (!path) {
          response.statusCode = 404;
          response.end();
          return;
        }
        // The served name decides the type: the module builds are `.js`.
        response.setHeader('Content-Type', contentTypes[extname(name)] ?? 'application/octet-stream');
        response.end(readFileSync(path));
      });
    },
    generateBundle() {
      for (const [name, path] of assetFiles()) {
        this.emitFile({ type: 'asset', fileName: prefix + name, source: readFileSync(path) });
      }
    },
  };
}

export default defineConfig({
  plugins: [react(), pdfjsAssets()],
  server: {
    host: '0.0.0.0',
    port: 5174,
    fs: {
      // Shared parser-contract tests import fixtures from backend/.
      allow: [resolve(__dirname, '..')],
    },
    // Isolate HMR to a dedicated path so Vite's internal WebSocket server
    // does not intercept application WebSocket upgrades.
    hmr: {
      path: '/__hmr',
    },
    proxy: {
      '/api/v1/ws': { target: apiUrl, ws: true },
      '/api': apiUrl,
      '/ws': { target: apiUrl.replace(/^http/, 'ws'), ws: true },
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    include: ['src/**/*.test.{ts,tsx}'],
    testTimeout: 15_000,
    // Only the bare `katex` module is stubbed; its stylesheets (imported by
    // the FILES Markdown preview) still resolve to the package.
    alias: [
      {
        find: /^katex$/,
        replacement: resolve(__dirname, 'src/test/__mocks__/katex.ts'),
      },
    ],
  },
});
