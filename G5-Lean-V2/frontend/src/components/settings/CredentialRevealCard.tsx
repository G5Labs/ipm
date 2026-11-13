// =============================================================================
// G5 Settings — CredentialRevealCard
// =============================================================================
// Shared "shown once" reveal card for newly-created secret credentials (API
// keys and worker registration tokens). The plaintext secret is returned by the
// backend exactly once at creation time; this card surfaces it with a copy
// affordance and a dismiss control. Used by OrgApiKeyManager.
//
// Behavior parity with the previous inline reveal block:
//   - warn-tinted card with an AlertTriangle and a "shown once" caption
//   - a <code> element showing the plaintext value verbatim
//   - a Copy button that writes to the clipboard and flips its label to "Copied"
//   - a "Dismiss" button that clears the reveal
// =============================================================================

import React, { useEffect, useState } from "react";
import { AlertTriangle, Copy } from "lucide-react";

import { Button, classNames } from "../ui/UI";

export interface CredentialRevealCardProps {
  /** Heading text, e.g. "New API key" or "New worker registration token". */
  title: string;
  /** The plaintext secret to reveal (shown once). */
  value: string;
  /** Called when the user dismisses the card. */
  onDismiss: () => void;
}

export const CredentialRevealCard: React.FC<CredentialRevealCardProps> = ({
  title,
  value,
  onDismiss,
}) => {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) {
      return undefined;
    }
    const timeoutId = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(timeoutId);
  }, [copied]);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(value);
    setCopied(true);
  };

  return (
    <div
      className={classNames(
        "rounded-[var(--g5-radius-md)] border border-[var(--g5-warn)]/30",
        "bg-[var(--g5-warn)]/10 p-3 flex flex-col gap-2",
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-2">
          <AlertTriangle className="mt-0.5 h-4 w-4 text-[var(--g5-warn)]" />
          <div>
            <p className="text-sm font-medium text-[var(--g5-text0)]">
              {title}
            </p>
            <p className="text-xs text-[var(--g5-text2)]">
              This value is only shown once. Store it in your external secret
              manager.
            </p>
          </div>
        </div>
        <button
          type="button"
          data-testid="credential-reveal-dismiss"
          className="text-xs text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
          onClick={onDismiss}
        >
          Dismiss
        </button>
      </div>

      <div className="flex items-center gap-2">
        <code
          data-testid="credential-reveal-value"
          className={classNames(
            "flex-1 overflow-x-auto rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
            "bg-[var(--g5-bg2)] px-3 py-2 text-xs text-[var(--g5-text0)]",
          )}
        >
          {value}
        </code>
        <Button
          data-testid="credential-reveal-copy"
          kind="secondary"
          onClick={() => {
            void handleCopy();
          }}
        >
          <Copy className="h-4 w-4" />
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
    </div>
  );
};
