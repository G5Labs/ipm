// =============================================================================
// G5 Workbench — PhaseChevrons
// =============================================================================
// The three engineering phases of a workspace under the `alignment` layout
// (workbench g5.workbench.phases): 01 Business · 02 System · 03 Integration
// Engineering, the active one in its phase hue, earlier ones marked done.
//
// Three styles (g5.workbench.header-styles, the person's "Phase bar"
// setting): `extended` — a row of chevrons with index, icon, name and the
// deliverable subline; `minimal` — the same row without the subline;
// `vertical` — a column of three short right-pointing chevrons, each with the
// phase icon and full name, that the page places at the left of the
// action-bar row. Every style keeps the states, the owner-only move, the
// refusal line and the approval meters.
//
// The phase is explicit workspace state (g5.workbench.phases.state): clicking
// a chevron asks the owner's PATCH to move it; non-owners and Base see the
// same chevrons read-only. Approval meters (g5.workbench.gates) arrive
// through the `approvalMeters` slot — nothing renders for a missing meter.
// =============================================================================

import React from "react";
import { Check } from "lucide-react";
import type { PhaseBarStyle, WorkspacePhase } from "../../lib/types";
import {
  WORKBENCH_PHASES,
  WORKSPACE_PHASES,
  phaseIndex,
} from "../../lib/workbenchDefaults";
import { classNames } from "../ui/UI";
import { PHASE_ICONS } from "./phaseIcons";

// Chevron silhouettes (mock `.phase` clip-path): the first chevron of a row
// has a flat left edge, the others a notch that nests into the previous
// arrow. The vertical column keeps the notch on every row.
const CHEVRON_CLIP =
  "polygon(0 0, calc(100% - 14px) 0, 100% 50%, calc(100% - 14px) 100%, 0 100%, 14px 50%)";
const FIRST_CHEVRON_CLIP =
  "polygon(0 0, calc(100% - 14px) 0, 100% 50%, calc(100% - 14px) 100%, 0 100%)";
const VERTICAL_CHEVRON_CLIP =
  "polygon(0 0, calc(100% - 10px) 0, 100% 50%, calc(100% - 10px) 100%, 0 100%, 10px 50%)";

export interface PhaseChevronsProps {
  /** The workspace's current phase (DTO `phase`, default `be`). */
  phase: WorkspacePhase;
  /** Rendering style (the person's "Phase bar" setting); default `minimal`. */
  variant?: PhaseBarStyle;
  /**
   * Owner move (PATCH). Omitted or `readOnly` → chevrons are presentational:
   * they still show the phase but refuse clicks (`aria-disabled`).
   */
  onSelectPhase?: (phase: WorkspacePhase) => void;
  /** Non-owner viewer or Base workspace (g5.workbench.phases.state). */
  readOnly?: boolean;
  /** A move is in flight; clicks are ignored until it settles. */
  pending?: boolean;
  /**
   * Why the last move was refused (the chevron has already rolled back);
   * the caller clears it on the next move. Rendered as an alert line so a
   * refusal is never silent.
   */
  error?: string | null;
  /**
   * Phase 3 slot (g5.workbench.gates): an approval meter per phase, rendered
   * beside the name. Phase 0 passes nothing and renders nothing.
   */
  approvalMeters?: Partial<Record<WorkspacePhase, React.ReactNode>>;
}

export const PhaseChevrons: React.FC<PhaseChevronsProps> = ({
  phase,
  variant = "minimal",
  onSelectPhase,
  readOnly = false,
  pending = false,
  error = null,
  approvalMeters,
}) => {
  const interactive = !readOnly && typeof onSelectPhase === "function";
  const activeIndex = phaseIndex(phase);
  const vertical = variant === "vertical";

  const errorLine = error ? (
    <p
      role="alert"
      data-testid="phase-chevrons-error"
      className={classNames(
        "text-[11px] text-[var(--g5-blocked)]",
        vertical ? "mt-1 max-w-[16rem]" : "mb-1.5",
      )}
    >
      Could not move the phase: {error}
    </p>
  ) : null;

  const chevrons = WORKSPACE_PHASES.map((id, index) => {
    const info = WORKBENCH_PHASES[id];
    const Icon = PHASE_ICONS[id];
    const active = id === phase;
    const done = index < activeIndex;
    const hue = `var(${info.token})`;
    const soft = `var(${info.softToken})`;
    const meter = approvalMeters?.[id];
    const shared = {
      type: "button" as const,
      "data-phase": id,
      "data-state": active ? "active" : done ? "done" : "upcoming",
      "aria-pressed": active,
      "aria-disabled": !interactive || pending || undefined,
      "aria-label": `${info.index} ${info.name}`,
      title: interactive
        ? `Move the workspace to ${info.name}`
        : `${info.name} — only the workspace owner moves the phase`,
      onClick: () => {
        if (!interactive || pending || active) return;
        onSelectPhase?.(id);
      },
    };
    const interactiveClass =
      interactive && !active
        ? "cursor-pointer hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]"
        : "cursor-default";
    const focusClass =
      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--g5-info)]";

    if (vertical) {
      return (
        <button
          key={id}
          data-testid={`phase-chevron-${id}`}
          {...shared}
          className={classNames(
            "relative flex h-5 items-center gap-1.5 whitespace-nowrap pl-[18px] pr-5 text-left text-[11px] font-semibold leading-none",
            "bg-[var(--g5-bg2)] text-[var(--g5-text1)] transition-colors",
            interactiveClass,
            focusClass,
          )}
          style={{
            clipPath: VERTICAL_CHEVRON_CLIP,
            ...(active
              ? {
                  backgroundColor: soft,
                  boxShadow: `inset 0 2px 0 ${hue}`,
                  color: hue,
                }
              : {}),
          }}
        >
          {done ? (
            <Check
              size={12}
              strokeWidth={2.5}
              className="shrink-0"
              style={{ color: hue }}
              aria-hidden="true"
            />
          ) : (
            <Icon size={12} className="shrink-0" aria-hidden="true" />
          )}
          <span>{info.name}</span>
          {meter ? (
            <span data-testid={`phase-meter-${id}`} className="ml-1">
              {meter}
            </span>
          ) : null}
        </button>
      );
    }

    return (
      <button
        key={id}
        data-testid={`phase-chevron-${id}`}
        {...shared}
        className={classNames(
          "relative flex min-w-0 flex-1 items-center gap-3 text-left",
          variant === "extended" ? "py-2" : "py-1.5",
          "pr-6",
          index === 0 ? "pl-4" : "pl-8",
          "bg-[var(--g5-bg2)] text-[var(--g5-text2)] transition-colors",
          interactiveClass,
          focusClass,
        )}
        style={{
          clipPath: index === 0 ? FIRST_CHEVRON_CLIP : CHEVRON_CLIP,
          ...(active
            ? { backgroundColor: soft, boxShadow: `inset 0 3px 0 ${hue}` }
            : {}),
        }}
      >
        <span
          className={classNames(
            "flex shrink-0 items-center justify-center rounded-full border text-[11px] font-semibold tabular-nums",
            variant === "extended" ? "h-7 w-7" : "h-6 w-6",
          )}
          style={{
            color: hue,
            borderColor: active || done ? hue : "var(--g5-stroke)",
          }}
          aria-hidden="true"
        >
          {done ? <Check size={14} strokeWidth={2.5} /> : info.index}
        </span>
        <Icon
          size={16}
          className="shrink-0"
          aria-hidden="true"
          {...(active ? { style: { color: hue } } : {})}
        />
        <span className="flex min-w-0 flex-col leading-tight">
          <span
            className={classNames(
              "truncate text-[13px] font-semibold",
              active ? "" : "text-[var(--g5-text1)]",
            )}
            style={active ? { color: hue } : undefined}
          >
            {info.name}
          </span>
          {variant === "extended" ? (
            <span className="truncate text-[11px] text-[var(--g5-text2)]">
              {info.subline}
            </span>
          ) : null}
          {meter ? (
            <span data-testid={`phase-meter-${id}`}>{meter}</span>
          ) : null}
        </span>
      </button>
    );
  });

  if (vertical) {
    return (
      <nav
        aria-label="Engineering phases"
        data-testid="phase-chevrons"
        data-phase={phase}
        data-variant={variant}
        className="flex shrink-0 flex-col justify-center self-stretch border-b border-r border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-1.5"
      >
        <div className="flex flex-col gap-0.5">{chevrons}</div>
        {errorLine}
      </nav>
    );
  }

  return (
    <nav
      aria-label="Engineering phases"
      data-testid="phase-chevrons"
      data-phase={phase}
      data-variant={variant}
      className={classNames(
        "flex shrink-0 flex-col border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3",
        variant === "extended" ? "py-2" : "py-1.5",
      )}
    >
      {errorLine}
      <div className="flex items-stretch gap-0.5">{chevrons}</div>
    </nav>
  );
};
