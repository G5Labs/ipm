// =============================================================================
// G5 Client — File-change notification coalescing
// =============================================================================
//
// The owner's VM reports one `workspace.file_changed` event per written
// path, build output included, so a build or an agent turn produces a burst
// of hundreds. Refreshing the live Files tree and the Changes report on every
// one cancelled each fetch before it finished and starved both views
// (`g5.documents.files-view.refresh`). The coalescer refreshes at most once
// per window per workspace and never throws a tree or Changes fetch away:
//
// - An event with no such fetch in flight refreshes at once.
// - An event while one is in flight stays pending: a fetch that started
//   before the write cannot carry it, and invalidating now would only reuse
//   that fetch and clear the mark. The refresh runs once the fetch has
//   returned, so a read started after the notification always follows.
// - A fetch that stays in flight past `maxDeferMs` is hung; the refresh then
//   cancels it and starts over rather than wait forever.

import type { QueryClient } from "@tanstack/react-query";

/** The window one refresh covers while events keep arriving. */
export const FILE_CHANGE_COALESCE_MS = 1500;

/**
 * How long a refresh waits behind a fetch in flight before cancelling it, so
 * a hung request cannot freeze the view.
 */
export const FILE_CHANGE_MAX_DEFER_MS = 20_000;

/**
 * The refresh a window ends with: the workspace, the repositories it saw
 * change, and whether a fetch still in flight must be cancelled (only after
 * `maxDeferMs`) or may finish first.
 */
export type FileChangeRefresh = (
  workspaceId: string,
  repoIds: ReadonlySet<string>,
  options: { cancelRefetch: boolean },
) => void;

interface Window {
  timer: ReturnType<typeof setTimeout>;
  /** Repositories with changes not yet covered by a refresh. */
  repoIds: Set<string>;
  /** When a refresh was first deferred behind a fetch in flight; null when none was. */
  deferredSince: number | null;
}

export class FileChangeCoalescer {
  private readonly windows = new Map<string, Window>();

  /**
   * `gateOnFetch` makes a refresh wait behind a tree or Changes read in
   * flight (the expensive reads a refresh keeps). The cheap families, read
   * again at once, use an ungated coalescer so a slow tree read never
   * delays an open file or document.
   */
  constructor(
    private readonly queryClient: Pick<QueryClient, "isFetching">,
    private readonly refresh: FileChangeRefresh,
    private readonly windowMs: number = FILE_CHANGE_COALESCE_MS,
    private readonly maxDeferMs: number = FILE_CHANGE_MAX_DEFER_MS,
    private readonly now: () => number = () => Date.now(),
    private readonly gateOnFetch: boolean = true,
  ) {}

  /** A path changed in `repoId` of `workspaceId`. */
  changed(workspaceId: string, repoId: string): void {
    const window = this.windows.get(workspaceId);
    if (window) {
      window.repoIds.add(repoId);
      return;
    }
    if (this.fetchInFlight(workspaceId)) {
      // The running fetch predates this change; refresh once it has landed.
      this.open(workspaceId, new Set([repoId]), this.now());
      return;
    }
    this.refresh(workspaceId, new Set([repoId]), { cancelRefetch: false });
    this.open(workspaceId, new Set(), null);
  }

  /** Drop every pending refresh; used when the connection goes away. */
  clear(): void {
    for (const window of this.windows.values()) {
      clearTimeout(window.timer);
    }
    this.windows.clear();
  }

  private open(
    workspaceId: string,
    repoIds: Set<string>,
    deferredSince: number | null,
  ): void {
    this.windows.set(workspaceId, {
      timer: setTimeout(() => this.close(workspaceId), this.windowMs),
      repoIds,
      deferredSince,
    });
  }

  private close(workspaceId: string): void {
    const window = this.windows.get(workspaceId);
    if (!window) return;
    if (window.repoIds.size === 0) {
      this.windows.delete(workspaceId);
      return;
    }
    if (this.fetchInFlight(workspaceId)) {
      const since = window.deferredSince ?? this.now();
      if (this.now() - since < this.maxDeferMs) {
        // Let the running fetch land; refresh once it has.
        window.deferredSince = since;
        window.timer = setTimeout(() => this.close(workspaceId), this.windowMs);
        return;
      }
      // Hung: cancel it and read again rather than wait forever.
      this.refresh(workspaceId, window.repoIds, { cancelRefetch: true });
    } else {
      this.refresh(workspaceId, window.repoIds, { cancelRefetch: false });
    }
    this.open(workspaceId, new Set(), null);
  }

  /**
   * Whether a Files tree or Changes fetch of `workspaceId` is running. These
   * are the fetches a refresh keeps rather than cancels, so they are the
   * ones a pending change must wait behind.
   */
  private fetchInFlight(workspaceId: string): boolean {
    if (!this.gateOnFetch) return false;
    return (
      this.queryClient.isFetching({ queryKey: ["file-tree", workspaceId] }) >
        0 ||
      this.queryClient.isFetching({
        queryKey: ["workspace-changes", workspaceId],
      }) > 0
    );
  }
}
