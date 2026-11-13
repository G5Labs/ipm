// =============================================================================
// UsageRing — per-window subscription utilization ring
// =============================================================================
// Ontology: software_architecture/client/settings-page D18 (ring anatomy,
// thresholds, a11y); system_invariants/external_agent_provider_contract D13
// (the ring renders the display-only provider quota snapshot and never gates
// anything); user_experience_model/agent_interaction_ux D8 (stale data keeps
// rendering with its age — the caller threads that age through `note`).
// Scenario owner: test_contract/provider_accounts I11.
//
// An 18px SVG: a track circle (radius 6.5, stroke 3) plus a fill arc whose
// dash length is the utilization fraction of the circumference, rotated −90°
// so the arc starts at twelve o'clock. Color by utilization: ok below 50%,
// warn from 50% to 79%, danger from 80% (the composer chip's danger color).
// =============================================================================

import React from "react";

import { classNames } from "../ui/UI";

export type UsageRingLevel = "ok" | "warn" | "danger" | "none";

/** Utilization at or above which the ring turns amber (settings-page D18). */
export const USAGE_WARN_AT_PERCENT = 50;
/** Utilization at or above which the ring turns red (settings-page D18). */
export const USAGE_DANGER_AT_PERCENT = 80;

const RING_SIZE = 18;
const RING_CENTER = RING_SIZE / 2;
const RING_RADIUS = 6.5;
const RING_STROKE = 3;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

/** Whole-percent utilization (0..100) from a 0..1 fraction; `null` = no data. */
export function usagePercent(
  utilization: number | null | undefined,
): number | null {
  if (typeof utilization !== "number" || !Number.isFinite(utilization)) {
    return null;
  }
  return Math.round(Math.min(1, Math.max(0, utilization)) * 100);
}

/** Threshold level for a whole-percent utilization (D18: amber 50, red 80). */
export function usageLevel(percent: number | null): UsageRingLevel {
  if (percent == null) return "none";
  if (percent >= USAGE_DANGER_AT_PERCENT) return "danger";
  if (percent >= USAGE_WARN_AT_PERCENT) return "warn";
  return "ok";
}

function levelColorVar(level: UsageRingLevel): string {
  switch (level) {
    case "danger":
      return "var(--g5-blocked)";
    case "warn":
      return "var(--g5-warn)";
    case "ok":
      return "var(--g5-ok)";
    case "none":
      return "var(--g5-stroke)";
  }
}

export interface UsageRingProps {
  /** Utilization fraction 0..1; `null` renders the empty track. */
  utilization: number | null | undefined;
  /** Window tag shown to assistive tech: "5h" or "7d". */
  label: string;
  /** Hover title (reset time, capture age, …). */
  note?: string;
  testId?: string;
  className?: string;
}

export const UsageRing: React.FC<UsageRingProps> = ({
  utilization,
  label,
  note,
  testId,
  className,
}) => {
  const percent = usagePercent(utilization);
  const level = usageLevel(percent);
  const filled = ((percent ?? 0) / 100) * RING_CIRCUMFERENCE;
  const ariaLabel =
    percent == null ? `${label} no data` : `${label} ${percent}% used`;

  return (
    <svg
      className={classNames("shrink-0", className)}
      width={RING_SIZE}
      height={RING_SIZE}
      viewBox={`0 0 ${RING_SIZE} ${RING_SIZE}`}
      role="img"
      aria-label={ariaLabel}
      data-testid={testId}
      data-level={level}
      data-percent={percent ?? ""}
    >
      {note ? <title>{note}</title> : null}
      <circle
        cx={RING_CENTER}
        cy={RING_CENTER}
        r={RING_RADIUS}
        fill="none"
        stroke="var(--g5-stroke)"
        strokeWidth={RING_STROKE}
      />
      {percent != null && percent > 0 ? (
        <circle
          cx={RING_CENTER}
          cy={RING_CENTER}
          r={RING_RADIUS}
          fill="none"
          stroke={levelColorVar(level)}
          strokeWidth={RING_STROKE}
          strokeLinecap="butt"
          strokeDasharray={`${filled.toFixed(2)} ${RING_CIRCUMFERENCE.toFixed(2)}`}
          transform={`rotate(-90 ${RING_CENTER} ${RING_CENTER})`}
        />
      ) : null}
    </svg>
  );
};

export default UsageRing;
