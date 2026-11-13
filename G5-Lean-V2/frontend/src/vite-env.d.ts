/// <reference types="vite/client" />

// ---------------------------------------------------------------------------
// Vite environment variable type declarations
// ---------------------------------------------------------------------------
interface ImportMetaEnv {
  readonly MODE: string;
  readonly VITE_MOCK?: string;
  /** Mock mode only: comma-separated instance features replacing the dev defaults. */
  readonly VITE_MOCK_FEATURES?: string;
  readonly VITE_API_URL?: string;
  readonly VITE_DEV_MODE?: string;
  readonly VITE_G5_CI_WORKFLOW?: string;
  readonly VITE_G5_ENABLE_CLOUD_WORKSPACE_WORKFLOWS?: string;
  readonly VITE_G5_ENABLE_INTENT_LOG_VIEWER?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

// ---------------------------------------------------------------------------
// Ambient module declarations for optional/dynamic dependencies.
// These are dynamically imported with .catch() fallbacks at runtime;
// we only need type stubs so tsc doesn't complain.
// ---------------------------------------------------------------------------

declare module "katex" {
  const katex: {
    renderToString(tex: string, options?: Record<string, unknown>): string;
  };
  export default katex;
}

declare module "mermaid" {
  const mermaid: {
    initialize(config: Record<string, unknown>): void;
    render(id: string, definition: string): Promise<{ svg: string }>;
  };
  export default mermaid;
}

// Monaco is installed as a real dependency.
// Its types come from node_modules — no ambient declaration needed.

// The pinned pdfjs-dist version, from the g5-pdfjs-assets plugin in
// vite.config.ts; it names the asset path the PDF viewer loads from.
declare module "virtual:g5-pdfjs" {
  export const version: string;
}
