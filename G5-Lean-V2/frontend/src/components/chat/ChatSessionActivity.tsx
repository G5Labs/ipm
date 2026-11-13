// =============================================================================
// Chat activity presentation
// =============================================================================
// The chat picker's activity indicator (chat g5.chat.activity) and the
// header's rename editor (g5.chat.ui.header-title).
// =============================================================================

import { Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  chatActivityState,
  type ChatActivityState,
} from "../../lib/chatSessionActivity";
import type { ChatSessionActivityDTO } from "../../lib/types";
import { classNames } from "../ui/UI";

const ACTIVITY_LABELS: Record<Exclude<ChatActivityState, "idle">, string> = {
  active: "Turn running",
  unseen: "New result you have not viewed",
};

/** Spinning circle, solid circle, or nothing; always the same width so rows
 * stay aligned. */
export function ChatActivityIndicator({
  activity,
  className,
}: {
  activity: ChatSessionActivityDTO | null | undefined;
  className?: string;
}) {
  const state = chatActivityState(activity);
  return (
    <span
      className={classNames(
        "inline-flex h-3 w-3 shrink-0 items-center justify-center",
        className,
      )}
      data-testid="chat-activity-indicator"
      data-state={state}
    >
      {state === "active" ? (
        <span
          role="img"
          aria-label={ACTIVITY_LABELS.active}
          title={ACTIVITY_LABELS.active}
        >
          <Loader2
            aria-hidden="true"
            className="h-3 w-3 animate-spin text-[var(--g5-info)]"
          />
        </span>
      ) : state === "unseen" ? (
        <span
          role="img"
          aria-label={ACTIVITY_LABELS.unseen}
          title={ACTIVITY_LABELS.unseen}
          className="h-2 w-2 rounded-full bg-[var(--g5-info)]"
        />
      ) : null}
    </span>
  );
}

/** The header's inline editor for the current chat's title. */
export function ChatTitleRenameEditor({
  initialTitle,
  onSave,
  onClose,
}: {
  initialTitle: string;
  /** Saves a trimmed title, or null to clear it; rejects on failure. */
  onSave: (title: string | null) => void | Promise<unknown>;
  onClose: () => void;
}) {
  const [value, setValue] = useState(initialTitle);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  // The pencil opened the editor to type a name: take focus, title selected.
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  async function save() {
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await onSave(value.trim() || null);
      onClose();
    } catch (failure) {
      setError(
        failure instanceof Error && failure.message
          ? `Could not rename: ${failure.message}`
          : "Could not rename the chat.",
      );
      setSaving(false);
    }
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <div className="flex min-w-0 items-center gap-1">
        <input
          ref={inputRef}
          type="text"
          data-testid="chat-header-title-input"
          aria-label="Chat title"
          value={value}
          // Read-only, not disabled, so keyboard focus stays for a retry.
          readOnly={saving}
          aria-busy={saving}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void save();
            } else if (event.key === "Escape") {
              event.preventDefault();
              // Like Cancel, closing waits for the save to settle.
              if (!saving) onClose();
            }
          }}
          placeholder="New Session"
          className="min-w-0 flex-1 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-1.5 py-0.5 text-[11px] text-[var(--g5-text0)] focus-visible:outline-none focus:border-[var(--g5-info)] read-only:opacity-60"
        />
        <button
          type="button"
          data-testid="chat-header-title-save"
          disabled={saving}
          onClick={() => void save()}
          className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-[var(--g5-info)] hover:bg-white/5 disabled:opacity-60"
        >
          {saving ? "Saving…" : "Save"}
        </button>
        <button
          type="button"
          data-testid="chat-header-title-cancel"
          disabled={saving}
          onClick={onClose}
          className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-[var(--g5-text2)] hover:bg-white/5 hover:text-[var(--g5-text0)]"
        >
          Cancel
        </button>
      </div>
      {error && (
        <p
          role="alert"
          data-testid="chat-header-title-error"
          className="mt-0.5 truncate text-[10px] text-[var(--g5-blocked)]"
          title={error}
        >
          {error}
        </p>
      )}
    </div>
  );
}
