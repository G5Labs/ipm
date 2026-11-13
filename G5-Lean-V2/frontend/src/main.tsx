// =============================================================================
// G5 Client — Application Entry Point
// =============================================================================
// Vite entry point. Renders the React 19 app in strict mode into the #root
// DOM element. Imports index.css for Tailwind and theme styles.
// =============================================================================

import {
  MutationCache,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ApiError } from "./lib/api";
import { installChunkReloadGuard } from "./lib/chunk-reload-guard";
import { emitReadOnlyNotice } from "./lib/readOnlyNotice";
import "./index.css";

// ---------------------------------------------------------------------------
// Theme Hydration — runs SYNCHRONOUSLY before React mounts to prevent FOUC.
// Reads the user's saved theme from localStorage and applies it to the <html>
// element immediately so the correct CSS variables are active from the first
// paint. Falls back to 'editor' dark if nothing is stored.
// ---------------------------------------------------------------------------
(function hydrateTheme() {
  try {
    const raw = localStorage.getItem("g5-user-settings");
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed.themeName) {
        document.documentElement.dataset.theme = parsed.themeName;
      }
      if (parsed.themeMode) {
        document.documentElement.dataset.mode = parsed.themeMode;
      }
      return;
    }
  } catch {
    // Ignore parse errors — fall through to defaults
  }
  // Default theme if nothing stored
  document.documentElement.dataset.theme = "editor";
  document.documentElement.dataset.mode = "dark";
})();

// ---------------------------------------------------------------------------
// QueryClient — shared across the entire app
// ---------------------------------------------------------------------------
const queryClient = new QueryClient({
  // Shared mutation-error surface: any mutation rejected with a
  // `WORKSPACE_READ_ONLY` (HTTP 403) ApiError re-emits a read-only notice so the
  // in-tree toast surface can show a single consistent warning — even for stale
  // tabs / races where a control was not pre-disabled. Only inspects; never
  // swallows the error (other error handling and per-mutation onError still run).
  mutationCache: new MutationCache({
    onError: (error) => {
      if (error instanceof ApiError && error.isReadOnly) {
        emitReadOnlyNotice();
      }
    },
  }),
  defaultOptions: {
    queries: {
      staleTime: 30_000, // 30s before refetch
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

// ---------------------------------------------------------------------------
// Root Error Boundary — catches ALL uncaught render errors
// ---------------------------------------------------------------------------
class RootErrorBoundary extends React.Component<
  { children: React.ReactNode },
  { error: Error | null }
> {
  constructor(props: { children: React.ReactNode }) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("[RootErrorBoundary] React crashed:", error);
    console.error("[RootErrorBoundary] Component stack:", info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            padding: 40,
            color: "#f38ba8",
            fontFamily: "monospace",
            background: "#1e1e2e",
            minHeight: "100vh",
          }}
        >
          <h1 style={{ color: "#fab387", fontSize: 24 }}>React Crashed</h1>
          <pre
            style={{
              whiteSpace: "pre-wrap",
              marginTop: 16,
              color: "#cdd6f4",
              fontSize: 13,
              lineHeight: 1.5,
            }}
          >
            {this.state.error.message}
            {"\n\n"}
            {this.state.error.stack}
          </pre>
        </div>
      );
    }
    return this.props.children;
  }
}

// ---------------------------------------------------------------------------
// Mock mode — intercepts all API requests with realistic mock data.
// Start with: npm run dev:mock
// ---------------------------------------------------------------------------
async function startApp() {
  // Deploy-invalidated lazy chunks (mermaid, monaco, katex, …) fire Vite's
  // `vite:preloadError`; reload once so the fresh shell picks up current chunk
  // hashes (shared-components D8a, issue #1540). Installed before React
  // renders so the earliest lazy import is already covered.
  installChunkReloadGuard();

  console.log("[G5] startApp() called, VITE_MOCK =", import.meta.env.VITE_MOCK);

  if (import.meta.env.VITE_MOCK === "true") {
    try {
      // Use fetch interceptor — works everywhere (no Service Worker needed)
      const { installFetchInterceptor } =
        await import("./mocks/fetch-interceptor");
      installFetchInterceptor();
      console.log(
        "[Mock] Mock mode active — all /api/* requests are intercepted",
      );
    } catch (err) {
      console.warn("[Mock] Failed to install fetch interceptor:", err);
    }
  }

  // Remove the boot-msg so we know React took over
  const bootMsg = document.getElementById("boot-msg");
  if (bootMsg) bootMsg.style.display = "none";

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <RootErrorBoundary>
          <App />
        </RootErrorBoundary>
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

startApp();
