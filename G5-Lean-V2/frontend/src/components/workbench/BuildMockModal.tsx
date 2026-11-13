// =============================================================================
// BuildMockModal — the ribbon's Build mock dialog (workbench g5.workbench.mock)
// =============================================================================
// One dialog, like Review meeting: it lists this workspace's mocks under
// Project Docs `mocks/`, newest first, so a reviewer opens one in its reader
// tab, and its primary action seeds the chat composer with the mock-building
// prompt (the phase's hidden `generate-mock` action) for the person to send.
// Mocks are ordinary Project Docs: nothing is served and no preview profile
// is involved.
// =============================================================================

import React, { useRef } from "react";
import { Hammer, X } from "lucide-react";
import type { DocNode } from "../../lib/types";
import { classNames } from "../../lib/utils";
import {
  MOCK_FOLDER,
  mockChangedAt,
  mockLabel,
  mockSubfolder,
} from "../../lib/mockDocs";
import { Modal } from "../ui/UI";
import { relativeTime } from "../guided/GuidedJourneySection";

export interface BuildMockModalProps {
  open: boolean;
  /** This workspace's Project Docs folder (`docs/<workspace id>`). */
  docsFolderPath: string | null;
  /** The mocks to list, newest first ({@link recentMockDocs}). */
  mocks: readonly DocNode[];
  /**
   * Why the prompt cannot be seeded now (the composer's own reason, or a
   * missing template); null when Build mock is live.
   */
  buildDisabledReason: string | null;
  /** Open a listed mock in its reader tab; the dialog closes. */
  onOpenMock: (doc: DocNode) => void;
  /** Seed the mock-building prompt; the dialog closes. */
  onBuild: () => void;
  onClose: () => void;
}

export function BuildMockModal({
  open,
  docsFolderPath,
  mocks,
  buildDisabledReason,
  onOpenMock,
  onBuild,
  onClose,
}: BuildMockModalProps) {
  const closeRef = useRef<HTMLButtonElement>(null);
  if (!open) return null;

  return (
    <Modal
      onClose={onClose}
      labelledBy="build-mock-modal-title"
      size="sm"
      initialFocusRef={closeRef}
      className="p-0 flex flex-col"
      testId="build-mock-modal"
    >
      <div className="flex items-start justify-between gap-3 border-b border-[var(--g5-stroke)] px-5 py-4">
        <div>
          <h2
            id="build-mock-modal-title"
            className="text-lg font-semibold text-[var(--g5-text0)]"
          >
            Build mock
          </h2>
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            Clickable screens live in Project Docs under {MOCK_FOLDER}/. Open a
            recent one, or ask the Business Engineering agent to build new ones.
          </p>
        </div>
        <button
          ref={closeRef}
          type="button"
          data-testid="build-mock-close"
          onClick={onClose}
          className="rounded-[var(--g5-radius-sm)] p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
          aria-label="Close"
        >
          <X size={18} />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        <p className="mb-1 text-sm font-medium text-[var(--g5-text1)]">
          Recent mocks
        </p>
        {mocks.length === 0 ? (
          <p
            data-testid="build-mock-empty"
            className="text-xs text-[var(--g5-text2)]"
          >
            No mocks yet under {MOCK_FOLDER}/. Build mock creates the first.
          </p>
        ) : (
          <ul
            data-testid="build-mock-list"
            aria-label="Mocks in Project Docs"
            className="max-h-64 space-y-1 overflow-y-auto"
          >
            {mocks.map((doc) => {
              const subfolder = mockSubfolder(doc, docsFolderPath);
              const changed = mockChangedAt(doc);
              const when = changed ? relativeTime(changed) : "";
              return (
                <li key={doc.id}>
                  <button
                    type="button"
                    data-testid={`build-mock-open-${doc.id}`}
                    onClick={() => {
                      onOpenMock(doc);
                      onClose();
                    }}
                    className="flex w-full items-center gap-2 rounded-[var(--g5-radius-sm)] px-2 py-1 text-left text-sm text-[var(--g5-info)] hover:bg-[var(--g5-bg2)]"
                  >
                    <span className="min-w-0 flex-1 truncate">
                      {mockLabel(doc)}
                      {subfolder ? (
                        <span className="ml-1 text-xs text-[var(--g5-text2)]">
                          {subfolder}/
                        </span>
                      ) : null}
                    </span>
                    {when ? (
                      <span className="shrink-0 text-xs text-[var(--g5-text2)]">
                        {when}
                      </span>
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      <div className="flex items-center justify-end gap-2 border-t border-[var(--g5-stroke)] px-5 py-3">
        <button
          type="button"
          data-testid="build-mock-cancel"
          onClick={onClose}
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-1.5 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]"
        >
          Cancel
        </button>
        <button
          type="button"
          data-testid="build-mock-submit"
          disabled={buildDisabledReason !== null}
          title={buildDisabledReason ?? undefined}
          onClick={() => {
            onBuild();
            onClose();
          }}
          className={classNames(
            "inline-flex items-center gap-1.5 rounded-[var(--g5-radius-sm)] bg-[var(--g5-active)] px-3 py-1.5 text-xs text-white",
            "disabled:cursor-not-allowed disabled:opacity-50",
          )}
        >
          <Hammer size={13} />
          Build mock
        </button>
      </div>
    </Modal>
  );
}
