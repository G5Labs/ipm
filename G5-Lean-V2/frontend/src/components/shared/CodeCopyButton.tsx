// =============================================================================
// G5 Shared Components — CodeCopyButton
// =============================================================================
// The Copy control on a rendered code block (`g5.documents.markdown.code-copy`).
// It copies exactly the text it is given: the fence body, never highlighted
// HTML, prompts or line numbers. Success shows briefly; a missing or refused
// clipboard shows a visible failure. Both are announced through one live
// region that exists before either outcome, so screen readers hear it.
// =============================================================================

import React, { useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { classNames } from "../ui/UI";
import { writeClipboard } from "../../lib/clipboard";

const COPIED_MS = 1500;
const FAILED_MS = 5000;

// A fresh object per press, so a repeated outcome restarts its timer.
type Outcome = { kind: "copied" | "failed" };

export interface CodeCopyButtonProps {
  /** The exact text to place on the clipboard. */
  text: string;
  className?: string;
}

export const CodeCopyButton: React.FC<CodeCopyButtonProps> = ({
  text,
  className,
}) => {
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const attempts = useRef(0);

  useEffect(() => {
    if (!outcome) return undefined;
    const timeoutId = window.setTimeout(
      () => setOutcome(null),
      outcome.kind === "copied" ? COPIED_MS : FAILED_MS,
    );
    return () => window.clearTimeout(timeoutId);
  }, [outcome]);

  const copy = async () => {
    const seq = ++attempts.current;
    const copied = await writeClipboard(text);
    // A later press supersedes an earlier write still settling.
    if (seq !== attempts.current) return;
    setOutcome({ kind: copied ? "copied" : "failed" });
  };

  const failed = outcome?.kind === "failed";
  return (
    <span
      className={classNames(
        "inline-flex shrink-0 items-center gap-2 normal-case tracking-normal",
        className,
      )}
    >
      <span
        role="status"
        data-testid="code-copy-status"
        className={classNames(
          "text-[10px] leading-none",
          failed ? "text-[var(--g5-warn)]" : "text-[var(--g5-ok)]",
        )}
        title={
          failed
            ? "The browser has no clipboard access here or refused it. Select the code and copy it manually."
            : undefined
        }
      >
        {outcome?.kind === "copied" ? "Copied" : failed ? "Couldn't copy" : ""}
      </span>
      <button
        type="button"
        data-testid="code-copy"
        aria-label="Copy code"
        title="Copy code"
        onClick={() => void copy()}
        className={classNames(
          // No vertical padding: the button must not make a code block's
          // header taller than its language label's line.
          "inline-flex items-center gap-1 rounded-[var(--g5-radius-sm)] px-1",
          "text-[10px] font-medium leading-none text-[var(--g5-text1)]",
          "bg-[var(--g5-bg2)] hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
          "cursor-pointer transition-colors duration-150",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
        )}
      >
        {outcome?.kind === "copied" ? (
          <Check size={11} aria-hidden="true" />
        ) : (
          <Copy size={11} aria-hidden="true" />
        )}
        Copy
      </button>
    </span>
  );
};
