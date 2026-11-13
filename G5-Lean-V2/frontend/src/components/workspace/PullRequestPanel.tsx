import { PullRequestList } from "./PullRequestList";

export interface PullRequestPanelProps {
  workspaceId: string;
  dirtyCount?: number;
}

/**
 * Base `Show All PRs` center-tab surface. It owns the tab-level layout
 * (full-height scroll, centered max width) and delegates the actual PR list +
 * merge/refresh/close controls to the shared {@link PullRequestList}, which the
 * non-Base `Create PR` dialog reuses so both surfaces render identically
 * (software_architecture/client/workspace-page/pull-request-panel I2/D2).
 */
export function PullRequestPanel({
  workspaceId,
  dirtyCount,
}: PullRequestPanelProps) {
  return (
    <div className="h-full overflow-auto bg-[var(--g5-bg0)]">
      <div className="mx-auto max-w-6xl px-5 py-4">
        <PullRequestList workspaceId={workspaceId} dirtyCount={dirtyCount} />
      </div>
    </div>
  );
}
