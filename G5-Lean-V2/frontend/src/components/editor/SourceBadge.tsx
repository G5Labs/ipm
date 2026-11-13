// =============================================================================
// G5 Editor — SourceBadge
// =============================================================================
// Which source a file view shows (`g5.documents.read.labels`): the live
// workspace, its saved snapshot or its published branch, with the
// repository OID beside it and the source, OID and capture time on hover.
// A saved snapshot is never presented as the live tree.
// =============================================================================

import React from "react";

export interface SourceBadgeProps {
  source: "live" | "flush" | "committed";
  repoOid?: string | null;
  snapshotAt?: string | null;
  testId?: string;
}

export const SourceBadge: React.FC<SourceBadgeProps> = ({
  source,
  repoOid = null,
  snapshotAt = null,
  testId,
}) =>
  source === "flush" ? (
    <span
      data-testid={testId}
      className="rounded bg-[var(--g5-bg2)] px-1.5 py-0.5 text-[10px] uppercase tracking-wide"
      title={[
        `Source: ${source}`,
        repoOid ? `Repository OID: ${repoOid}` : null,
        snapshotAt ? `Observed: ${snapshotAt}` : null,
      ]
        .filter(Boolean)
        .join("\n")}
    >
      Saved workspace
      {repoOid ? ` · ${repoOid.slice(0, 8)}` : ""}
    </span>
  ) : null;
