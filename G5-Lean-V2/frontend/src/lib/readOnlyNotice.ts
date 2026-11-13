// =============================================================================
// G5 Client — Read-Only Workspace Notice Bus
// =============================================================================
// A tiny module-level event bus that bridges the app-wide React Query
// MutationCache (constructed in `main.tsx`, outside the React tree) to the
// in-tree toast surface. Any mutation that rejects with a `WORKSPACE_READ_ONLY`
// (HTTP 403) `ApiError` emits a notice here; `WorkspacePage` subscribes and
// surfaces a single "Read-only workspace" toast.
//
// This is the SHARED mutation-error surface for read-only rejections: it covers
// every blocked action, including races and stale tabs where a control was not
// pre-disabled. It never swallows other errors — the MutationCache `onError`
// only inspects and re-emits, and non-read-only errors flow through untouched.
// =============================================================================

type ReadOnlyNoticeListener = () => void;

const listeners = new Set<ReadOnlyNoticeListener>();

/** Notify all subscribers that a read-only mutation rejection occurred. */
export function emitReadOnlyNotice(): void {
  for (const listener of listeners) {
    try {
      listener();
    } catch {
      // A misbehaving listener must not break the emit loop for others.
    }
  }
}

/**
 * Subscribe to read-only mutation rejections. Returns an unsubscribe function
 * suitable for direct use as a React effect cleanup.
 */
export function subscribeReadOnlyNotice(
  listener: ReadOnlyNoticeListener,
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
