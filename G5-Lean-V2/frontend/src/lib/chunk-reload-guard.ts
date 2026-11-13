// =============================================================================
// chunk-reload-guard.ts — lazy-chunk load-failure recovery
// (shared-components D8a, issue G5Labs/G5#1540)
// =============================================================================
// Deploys replace the client's content-hashed lazy chunks (mermaid.core-*.js,
// monaco and katex) because each edge image bakes exactly
// one build into /srv. A tab opened before the deploy then rejects dynamic
// imports ("Failed to fetch dynamically imported module: …"), and the failed
// module stays failed in the document's module map until the page reloads.
//
// Recovery: Vite dispatches `vite:preloadError` on window when a dynamically
// imported chunk (or its CSS deps) fails to load in a production build. The
// guard reloads the page ONCE per guard window so the fresh app shell picks up
// the current chunk hashes — but only when the sessionStorage suppression
// timestamp persists durably (write + read-back verified). Only the persisted
// mark survives `location.reload()`; an in-memory flag resets with every
// reload, so a non-durable guard would reload-loop during a real outage.
// Storage-denied contexts therefore never auto-reload. Whenever the guard
// suppresses or skips a reload, lazy consumers show a reload affordance
// instead of the raw import error (D8a item 4).
// =============================================================================

/** sessionStorage key holding the epoch-ms timestamp of the last auto-reload. */
export const CHUNK_RELOAD_GUARD_KEY = "g5:chunk-reload-at";

const DEFAULT_WINDOW_MS = 60_000;

// Cross-browser dynamic-import failure signatures:
//   Chrome:  "Failed to fetch dynamically imported module: <url>"
//   Firefox: "error loading dynamically imported module"
//   Safari:  "Importing a module script failed."
const CHUNK_LOAD_ERROR_PATTERN =
  /Failed to fetch dynamically imported module|error loading dynamically imported module|Importing a module script failed/i;

type ChunkReloadSafetyCheck = () => boolean;
const chunkReloadSafetyChecks = new Set<ChunkReloadSafetyCheck>();

export interface ChunkReloadGuardOptions {
  /** Reload trigger; defaults to {@link requestPageReload}. Injectable for tests. */
  reload?: () => void;
  /** Clock; defaults to `Date.now`. Injectable for tests. */
  now?: () => number;
  /** Guard persistence; defaults to `window.sessionStorage` when accessible. */
  storage?: Pick<Storage, "getItem" | "setItem">;
  /** Suppression window between automatic reloads; defaults to 60s. */
  windowMs?: number;
  /** Event source; defaults to `window`. Injectable for tests. */
  target?: EventTarget;
  /** Live-document safety gate; defaults to all registered live workspace file checks. */
  isReloadSafe?: () => boolean;
}

/**
 * Registers one lifecycle-scoped live-document reload safety check.
 *
 * Checks run synchronously when Vite reports a missing chunk. Any false result
 * or exception blocks automatic navigation, preserving recovery through the
 * consumer affordance. The returned cleanup must run when the document scope
 * unmounts so stale workspaces cannot suppress later clean reloads.
 */
export function registerChunkReloadSafetyCheck(
  check: ChunkReloadSafetyCheck,
): () => void {
  chunkReloadSafetyChecks.add(check);
  return () => {
    chunkReloadSafetyChecks.delete(check);
  };
}

function isRegisteredReloadStateSafe(): boolean {
  for (const check of chunkReloadSafetyChecks) {
    try {
      if (!check()) return false;
    } catch {
      return false;
    }
  }
  return true;
}

/**
 * True when the caught error is a browser dynamic-import (chunk fetch/parse
 * transport) failure — i.e. the lazy chunk is gone or unreachable, typically
 * because a deploy replaced the hashed assets. Mermaid/diagram authoring
 * errors never match.
 */
export function isChunkLoadError(err: unknown): boolean {
  return err instanceof Error && CHUNK_LOAD_ERROR_PATTERN.test(err.message);
}

/** Spyable wrapper around `window.location.reload()` for consumers (D8a item 3). */
export function requestPageReload(): void {
  window.location.reload();
}

function defaultStorage(): Pick<Storage, "getItem" | "setItem"> | undefined {
  try {
    return window.sessionStorage;
  } catch {
    // Sandboxed/storage-denied documents — no durable suppression is
    // possible, so the guard skips auto-reload (D8a item 1) and consumers
    // recover via the reload affordance.
    return undefined;
  }
}

/**
 * Installs the `vite:preloadError` auto-reload guard. Call once from the app
 * entry point before React renders. Returns an uninstall function.
 *
 * Behavior (shared-components D8a item 1):
 * - first chunk-load failure inside a fresh guard window → durably mark the
 *   window (write + read-back verified), then reload the page once;
 * - further failures inside the window → suppressed (consumer affordances take
 *   over);
 * - storage unavailable, mark write throws, or the write silently drops → no
 *   automatic reload at all: without a mark that survives `location.reload()`
 *   the guard cannot bound reloads across documents, and a real outage would
 *   loop. Consumer affordances own recovery in those contexts.
 */
export function installChunkReloadGuard(
  options: ChunkReloadGuardOptions = {},
): () => void {
  const {
    reload = requestPageReload,
    now = Date.now,
    windowMs = DEFAULT_WINDOW_MS,
    target = window,
    isReloadSafe = isRegisteredReloadStateSafe,
  } = options;
  const storage = options.storage ?? defaultStorage();

  const handlePreloadError = () => {
    const current = now();

    // A live dirty live workspace file document has no durable acknowledgement for its local
    // edit. Fail closed before consuming the timestamp window: provider/UI
    // "synced" is only a connection handshake and cannot authorize data loss.
    try {
      if (!isReloadSafe()) return;
    } catch {
      return;
    }

    // Durable suppression is a hard precondition for auto-reload: the
    // persisted mark is the only state that survives the reload itself.
    if (!storage) {
      return;
    }
    try {
      const raw = storage.getItem(CHUNK_RELOAD_GUARD_KEY);
      const lastReloadAt = raw === null ? Number.NaN : Number(raw);
      if (Number.isFinite(lastReloadAt) && current - lastReloadAt < windowMs) {
        // Suppressed: a reload already happened recently and did not fix the
        // import — this is an outage, not a stale tab. Consumers render the
        // reload affordance instead (D8a item 4).
        return;
      }
      storage.setItem(CHUNK_RELOAD_GUARD_KEY, String(current));
      if (storage.getItem(CHUNK_RELOAD_GUARD_KEY) !== String(current)) {
        // Write silently dropped — the mark would not survive the reload, so
        // reloading here could loop. Leave recovery to the affordance.
        return;
      }
    } catch {
      // Storage read/write denied — no durable suppression, no auto-reload.
      return;
    }
    reload();
  };

  target.addEventListener("vite:preloadError", handlePreloadError);
  return () => {
    target.removeEventListener("vite:preloadError", handlePreloadError);
  };
}
