import React, { useMemo } from "react";
import { FileText } from "lucide-react";

import type { DiffData } from "../../lib/types";
import { diffFileKey } from "../../lib/diff";
import { classNames } from "../ui/UI";
import { DiffHunksView } from "./DiffHunksView";

interface DiffRepoGroup {
  repoId: string;
  repoName: string;
  files: DiffData[];
}

export interface DiffFileBrowserProps {
  diffs: DiffData[];
  selectedKey: string | null;
  selectedDiff: DiffData | null;
  onSelectDiff: (diff: DiffData) => void;
  formatFilePath?: (file: string) => string;
  navPrefix?: React.ReactNode;
  selectedContent?: React.ReactNode;
  readOnlyLabel: string;
  emptySelectionMessage: string;
  changedFilesTestId?: string;
  repoSectionTestId?: string;
  repoNameTestId?: string;
  fileButtonTestId?: string;
  diffViewTestId?: string;
}

export const DiffFileBrowser: React.FC<DiffFileBrowserProps> = ({
  diffs,
  selectedKey,
  selectedDiff,
  onSelectDiff,
  formatFilePath = (file) => file,
  navPrefix,
  selectedContent,
  readOnlyLabel,
  emptySelectionMessage,
  changedFilesTestId,
  repoSectionTestId,
  repoNameTestId,
  fileButtonTestId,
  diffViewTestId,
}) => {
  const repoGroups = useMemo<DiffRepoGroup[]>(() => {
    const groups: DiffRepoGroup[] = [];
    for (const diff of diffs) {
      let group = groups.find((candidate) => candidate.repoId === diff.repoId);
      if (!group) {
        group = {
          repoId: diff.repoId,
          repoName: diff.repoName,
          files: [],
        };
        groups.push(group);
      }
      group.files.push(diff);
    }
    return groups;
  }, [diffs]);

  return (
    <div className="flex flex-1 min-h-0">
      <nav
        aria-label="Changed files"
        data-testid={changedFilesTestId}
        className="w-72 shrink-0 border-r border-[var(--g5-stroke)] overflow-auto bg-[var(--g5-bg1)]"
      >
        {navPrefix}
        {repoGroups.map((group, groupIndex) => (
          <div
            key={group.repoId || group.repoName || `group-${groupIndex}`}
            data-testid={repoSectionTestId}
          >
            <div className="flex items-center gap-2 px-3 py-1.5 text-[10px] uppercase tracking-wider text-[var(--g5-text2)] border-b border-[var(--g5-stroke)] bg-[var(--g5-bg2)]/40">
              <span
                data-testid={repoNameTestId}
                className="font-semibold truncate text-[var(--g5-text1)]"
              >
                {group.repoName || "Workspace"}
              </span>
              <span className="ml-auto normal-case text-[var(--g5-text2)]">
                {group.files.length} changed
              </span>
            </div>
            {group.files.map((diff) => {
              const key = diffFileKey(diff);
              const isSelected = selectedKey === key;
              const displayPath = formatFilePath(diff.file);
              return (
                <button
                  key={key}
                  type="button"
                  data-testid={fileButtonTestId}
                  aria-current={isSelected || undefined}
                  aria-label={`${displayPath}, ${diff.additions} additions, ${diff.deletions} deletions`}
                  onClick={() => onSelectDiff(diff)}
                  className={classNames(
                    "w-full px-3 py-2 text-left text-xs",
                    "flex items-start gap-2",
                    "border-b border-[var(--g5-stroke)]/40",
                    "hover:bg-[var(--g5-bg2)] transition-colors cursor-pointer",
                    isSelected && "bg-[var(--g5-bg2)]",
                  )}
                >
                  <FileText className="w-3.5 h-3.5 mt-0.5 text-[var(--g5-text2)] shrink-0" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[var(--g5-text0)]">
                      {displayPath}
                    </span>
                  </span>
                  <span className="text-[var(--g5-ok)]">+{diff.additions}</span>
                  <span className="text-[var(--g5-blocked)]">
                    −{diff.deletions}
                  </span>
                </button>
              );
            })}
          </div>
        ))}
      </nav>

      <section
        data-testid="diff-scroll-pane"
        className="flex-1 min-w-0 bg-[var(--g5-bg0)] overflow-auto"
      >
        {selectedContent ? (
          selectedContent
        ) : selectedDiff ? (
          <>
            <div className="sticky top-0 z-10 flex items-center gap-3 px-3 py-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
              <span className="text-[10px] uppercase tracking-wider text-[var(--g5-text2)] shrink-0">
                {selectedDiff.repoName || "Workspace"}
              </span>
              <span className="font-mono text-xs text-[var(--g5-text0)] truncate">
                {formatFilePath(selectedDiff.file)}
              </span>
              <span className="text-xs text-[var(--g5-ok)]">
                +{selectedDiff.additions}
              </span>
              <span className="text-xs text-[var(--g5-blocked)]">
                −{selectedDiff.deletions}
              </span>
              <span className="ml-auto px-2 py-0.5 rounded-[var(--g5-radius-sm)] text-[10px] font-medium bg-[var(--g5-bg2)] text-[var(--g5-text2)] border border-[var(--g5-stroke)]">
                {readOnlyLabel}
              </span>
            </div>
            <DiffHunksView
              hunks={selectedDiff.hunks}
              additions={selectedDiff.additions}
              deletions={selectedDiff.deletions}
              testId={diffViewTestId}
            />
          </>
        ) : (
          <div className="flex items-center justify-center h-full px-6 text-center text-sm text-[var(--g5-text2)]">
            {emptySelectionMessage}
          </div>
        )}
      </section>
    </div>
  );
};
