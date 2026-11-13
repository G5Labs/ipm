// =============================================================================
// G5 Files — FileCompareDialog
// =============================================================================
// Compare (`g5.documents.files-view.compare`): the newer version of a file
// beside the person's own text, which stays editable, so they can merge by
// hand before saving or pushing against the newer version. Monaco's diff
// editor draws it; if Monaco cannot load, two plain text areas do the same
// job.
// =============================================================================

import React, { useEffect, useRef, useState } from "react";
import { GitCompare } from "lucide-react";
import { defineG5Themes, useThemeMode } from "../editor/monacoTheme";
import { loadMonacoReact } from "../editor/loadMonaco";
import { useDialogFocus } from "../ui/useDialogFocus";

export interface FileCompareDialogProps {
  filePath: string;
  language: string;
  /** What the person's text is compared with (read-only). */
  newerContent: string;
  /** The person's own text, editable in the dialog. */
  ownContent: string;
  /** Short explanation shown under the title. */
  description: string;
  /** Label of the confirming action, e.g. "Use my merged text". */
  applyLabel: string;
  onApply: (merged: string) => void;
  onCancel: () => void;
}

type DiffEditorComponent = React.ComponentType<{
  height: string;
  language: string;
  original: string;
  modified: string;
  theme: string;
  options: Record<string, unknown>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  beforeMount?: (monaco: any) => void;
  onMount?: (editor: {
    getModifiedEditor: () => {
      getValue: () => string;
      onDidChangeModelContent: (listener: () => void) => unknown;
      focus: () => void;
    };
  }) => void;
}>;

export const FileCompareDialog: React.FC<FileCompareDialogProps> = ({
  filePath,
  language,
  newerContent,
  ownContent,
  description,
  applyLabel,
  onApply,
  onCancel,
}) => {
  const themeMode = useThemeMode();
  const [DiffEditor, setDiffEditor] = useState<DiffEditorComponent | null>(
    null,
  );
  const [merged, setMerged] = useState(ownContent);
  const mergedRef = useRef(ownContent);
  const dialogRef = useRef<HTMLDivElement>(null);
  const ownRef = useRef<HTMLTextAreaElement>(null);
  // A modal: the text to merge takes the focus, Tab stays inside, Escape
  // cancels, and closing gives the focus back to whatever opened it.
  useDialogFocus(dialogRef, { onClose: onCancel, initialFocusRef: ownRef });

  useEffect(() => {
    let cancelled = false;
    loadMonacoReact()
      .then((mod) => {
        const diff = (mod as { DiffEditor?: DiffEditorComponent }).DiffEditor;
        if (!cancelled && diff) setDiffEditor(() => diff);
      })
      .catch((err) => {
        console.warn(
          "[FileCompareDialog] Monaco diff editor unavailable — using text areas:",
          err,
        );
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const updateMerged = (next: string) => {
    mergedRef.current = next;
    setMerged(next);
  };

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      role="dialog"
      aria-modal="true"
      aria-label={`Compare ${filePath}`}
      data-testid="file-compare-dialog"
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
    >
      <div className="flex h-full max-h-[80vh] w-full max-w-5xl flex-col rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] shadow-xl">
        <div className="flex items-start gap-3 border-b border-[var(--g5-stroke)] p-3">
          <GitCompare className="mt-0.5 h-5 w-5 shrink-0 text-[var(--g5-info)]" />
          <div className="min-w-0">
            <h3 className="text-sm font-semibold text-[var(--g5-text0)]">
              Compare {filePath}
            </h3>
            <p className="mt-0.5 text-xs text-[var(--g5-text1)]">
              {description}
            </p>
          </div>
        </div>
        <div className="grid shrink-0 grid-cols-2 border-b border-[var(--g5-stroke)] text-[11px] font-medium text-[var(--g5-text2)]">
          <span className="px-3 py-1">Newer version (read-only)</span>
          <span className="px-3 py-1">Your text (editable)</span>
        </div>
        <div className="min-h-0 flex-1">
          {DiffEditor ? (
            <DiffEditor
              height="100%"
              language={language}
              original={newerContent}
              // What was typed while Monaco loaded carries over; later typing
              // equals the model's own value, which Monaco leaves alone.
              modified={merged}
              theme={themeMode === "light" ? "g5-light" : "g5-dark"}
              beforeMount={defineG5Themes}
              options={{
                readOnly: false,
                originalEditable: false,
                renderSideBySide: true,
                automaticLayout: true,
                minimap: { enabled: false },
                fontSize: 13,
              }}
              onMount={(editor) => {
                const modified = editor.getModifiedEditor();
                modified.onDidChangeModelContent(() =>
                  updateMerged(modified.getValue()),
                );
                // It replaces the text area that held the focus.
                modified.focus();
              }}
            />
          ) : (
            <div className="grid h-full grid-cols-2 gap-2 p-2">
              <textarea
                data-testid="file-compare-newer"
                aria-label="Newer version"
                readOnly
                value={newerContent}
                className="h-full min-w-0 resize-none rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 font-mono text-xs text-[var(--g5-text1)]"
              />
              <textarea
                ref={ownRef}
                data-testid="file-compare-own"
                aria-label="Your text"
                value={merged}
                onChange={(event) => updateMerged(event.target.value)}
                className="h-full min-w-0 resize-none rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 font-mono text-xs text-[var(--g5-text0)]"
              />
            </div>
          )}
        </div>
        <div className="flex shrink-0 justify-end gap-2 border-t border-[var(--g5-stroke)] p-3">
          <button
            type="button"
            data-testid="file-compare-cancel"
            onClick={onCancel}
            className="rounded border border-[var(--g5-stroke)] px-3 py-1.5 text-xs text-[var(--g5-text0)] hover:bg-[var(--g5-overlay-hover)]"
          >
            Cancel
          </button>
          <button
            type="button"
            data-testid="file-compare-apply"
            onClick={() => onApply(mergedRef.current)}
            className="rounded bg-[var(--g5-active)] px-3 py-1.5 text-xs font-medium text-white"
          >
            {applyLabel}
          </button>
        </div>
      </div>
    </div>
  );
};
