// =============================================================================
// G5 Files — FileNoticeCard
// =============================================================================
// The card a file tab shows instead of content: a binary file with no
// viewer, an image or PDF that cannot be decoded, a cancelled open. It names
// the file, says why, and keeps Download whenever the bytes are at hand
// (`g5.documents.viewers`).
// =============================================================================

import React from "react";
import { Download, FileWarning } from "lucide-react";

export interface FileNoticeCardProps {
  testId: string;
  /** Why the content is not shown; exposed as `data-reason` for tests. */
  reason: string;
  filePath: string;
  detail: React.ReactNode;
  /** Downloads the file's bytes; absent when there are none to offer. */
  onDownload?: () => void;
  /** The Download button's test id; `<testId>-download` by default. */
  downloadTestId?: string;
  /** Further actions, after Download. */
  children?: React.ReactNode;
}

export const cardButton =
  "flex items-center gap-1.5 rounded border border-[var(--g5-stroke)] px-3 py-1.5 text-xs text-[var(--g5-text0)] hover:bg-[var(--g5-overlay-hover)]";

export const FileNoticeCard: React.FC<FileNoticeCardProps> = ({
  testId,
  reason,
  filePath,
  detail,
  onDownload,
  downloadTestId,
  children,
}) => {
  const name = filePath.split("/").pop() || filePath;
  return (
    <div className="flex h-full items-center justify-center p-6">
      <div
        data-testid={testId}
        data-reason={reason}
        data-file-path={filePath}
        role="status"
        className="flex max-w-sm flex-col items-center gap-2 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-6 text-center"
      >
        <FileWarning className="h-8 w-8 text-[var(--g5-text2)]" />
        <p className="text-sm font-medium text-[var(--g5-text0)] break-all">
          {name}
        </p>
        <p className="text-xs text-[var(--g5-text1)]">{detail}</p>
        <div className="mt-2 flex flex-wrap items-center justify-center gap-2">
          {onDownload && (
            <button
              type="button"
              data-testid={downloadTestId ?? `${testId}-download`}
              onClick={onDownload}
              className={cardButton}
            >
              <Download className="h-3.5 w-3.5" />
              Download
            </button>
          )}
          {children}
        </div>
      </div>
    </div>
  );
};
