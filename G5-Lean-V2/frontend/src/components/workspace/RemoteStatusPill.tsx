// =============================================================================
// G5 — RemoteStatusPill
// =============================================================================
//
// Full-text remote-tracking status pill for the Workspace Details repository
// card. State is carried by a colored dot, a full-text label and a hue
// together, never by color alone; the label is the accessible name. Colors
// come from `--g5-*` tokens so the pill is correct in every theme and mode.
//
// Presentational only — callers derive the state (see remoteTrackingStatus).
//
// Ontology: g5.workspace.remote-ui.status, g5.workspace.remote-ui.unknown
// =============================================================================

import { classNames } from "../ui/UI";
import type { RemoteTrackingState } from "./remoteTrackingStatus";

/** Pill states: derived tracking states plus local-only and unknown states. */
export type RemoteStatusPillState =
  | RemoteTrackingState
  | "local"
  | "loading"
  | "unavailable";

// Literal class strings so Tailwind's scanner emits every variant.
const STATE_CLASSES: Record<
  RemoteStatusPillState,
  { pill: string; dot: string }
> = {
  healthy: {
    pill: "text-[var(--g5-ok)] border-[var(--g5-ok)]/40 bg-[var(--g5-ok)]/10",
    dot: "bg-[var(--g5-ok)]",
  },
  ahead: {
    pill: "text-[var(--g5-info)] border-[var(--g5-info)]/40 bg-[var(--g5-info)]/10",
    dot: "bg-[var(--g5-info)]",
  },
  behind: {
    pill: "text-[var(--g5-warn)] border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10",
    dot: "bg-[var(--g5-warn)]",
  },
  review: {
    pill: "text-[var(--g5-warn)] border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10",
    dot: "bg-[var(--g5-warn)]",
  },
  blocked: {
    pill: "text-[var(--g5-blocked)] border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10",
    dot: "bg-[var(--g5-blocked)]",
  },
  unavailable: {
    pill: "text-[var(--g5-blocked)] border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10",
    dot: "bg-[var(--g5-blocked)]",
  },
  local: {
    pill: "text-[var(--g5-text2)] border-[var(--g5-stroke)] bg-[var(--g5-bg2)]",
    dot: "bg-[var(--g5-text2)]",
  },
  loading: {
    pill: "text-[var(--g5-text2)] border-[var(--g5-stroke)] bg-[var(--g5-bg2)]",
    dot: "bg-[var(--g5-text2)] animate-pulse",
  },
};

export interface RemoteStatusPillProps {
  state: RemoteStatusPillState;
  label: string;
  /** Raw `sync_paused_reason`, exposed as `data-reason` for paused states. */
  pauseReason?: string;
  className?: string;
}

export function RemoteStatusPill({
  state,
  label,
  pauseReason,
  className,
}: RemoteStatusPillProps) {
  const classes = STATE_CLASSES[state];
  return (
    <span
      data-testid="repo-status-pill"
      data-state={state}
      data-reason={pauseReason}
      className={classNames(
        "inline-flex w-fit items-center gap-1.5",
        "rounded-[var(--g5-radius-sm)] border px-2 py-0.5",
        "text-xs font-medium",
        classes.pill,
        className,
      )}
    >
      <span
        aria-hidden="true"
        className={classNames("h-2 w-2 shrink-0 rounded-full", classes.dot)}
      />
      {label}
    </span>
  );
}
