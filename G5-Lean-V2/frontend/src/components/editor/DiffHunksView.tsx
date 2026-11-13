import React, { useMemo } from "react";

import type { DiffHunk } from "../../lib/types";
import { DiffLineRow } from "./DiffLineRow";
import { DIFF_SCROLL_CONTENT_STYLE } from "./diffRowStyles";

export const MAX_RENDERED_DIFF_LINES = 3000;

export interface DiffHunksViewProps {
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
  partialFinalHunk?: boolean;
  testId?: string;
}

export const DiffHunksView: React.FC<DiffHunksViewProps> = ({
  hunks,
  additions,
  deletions,
  partialFinalHunk = false,
  testId,
}) => {
  const lineCount = useMemo(
    () => hunks.reduce((sum, hunk) => sum + hunk.lines.length, 0),
    [hunks],
  );

  if (lineCount > MAX_RENDERED_DIFF_LINES) {
    return (
      <div className="px-4 py-6 text-sm text-[var(--g5-text2)]">
        This diff is too large to render here ({lineCount.toLocaleString()}
        lines, +{additions} −{deletions}). Open the file directly to review it.
      </div>
    );
  }

  return (
    <div data-testid={testId} className="min-w-full">
      <div
        data-testid="diff-scroll-content"
        className="font-mono text-xs"
        style={DIFF_SCROLL_CONTENT_STYLE}
      >
        {hunks.map((hunk, hunkIndex) => (
          <div key={`${hunk.header}-${hunkIndex}`} className="pb-2">
            <div className="px-2 py-1 whitespace-pre text-[var(--g5-info)] bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)] border-y border-[var(--g5-stroke)]/50">
              {hunk.header}
            </div>
            {hunk.lines.map((line, lineIndex) => (
              <DiffLineRow key={`${hunkIndex}-${lineIndex}`} line={line} />
            ))}
          </div>
        ))}
      </div>
      {partialFinalHunk && (
        <p className="border-t border-[var(--g5-warn)]/35 bg-[var(--g5-warn)]/10 px-3 py-2 text-[10px] text-[var(--g5-text1)]">
          The final hunk is partial because this evidence reached its output
          limit.
        </p>
      )}
    </div>
  );
};
