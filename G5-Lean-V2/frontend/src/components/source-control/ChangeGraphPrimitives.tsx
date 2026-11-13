import type { LucideIcon } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";

import { classNames } from "../ui/UI";

export type ChangeGraphCardFocusState =
  "default" | "selected" | "neighbor" | "dimmed";

export function ChangeGraphCard({
  ariaLabel,
  testId,
  column,
  focusState,
  layer,
  side,
  ontologyKind,
  changeState,
  position,
  width,
  height,
  accentColor,
  ArtifactIcon,
  artifactLabel,
  title,
  path,
  additions,
  deletions,
  notice,
  compact = false,
  onSelect,
}: {
  ariaLabel: string;
  testId: string;
  column?: string;
  focusState: ChangeGraphCardFocusState;
  layer?: string;
  side?: string;
  ontologyKind?: string;
  changeState: string;
  position: { x: number; y: number };
  width: number;
  height: number;
  accentColor: string;
  ArtifactIcon: LucideIcon;
  artifactLabel: string;
  title: string;
  path?: string;
  additions?: number;
  deletions?: number;
  /** A visible warning such as "Cannot compare" or missing link targets. */
  notice?: string;
  compact?: boolean;
  onSelect: () => void;
}) {
  const selected = focusState === "selected";
  return (
    <button
      type="button"
      aria-pressed={selected}
      aria-label={ariaLabel}
      data-testid={testId}
      data-ui="change-graph-card"
      data-column={column}
      data-focus-state={focusState}
      data-layer={layer}
      data-side={side}
      data-ontology-kind={ontologyKind}
      data-change-state={changeState}
      onClick={onSelect}
      className={classNames(
        "absolute z-20 flex flex-col rounded-md border bg-[var(--g5-bg1)] text-left shadow-sm",
        compact ? "p-2.5" : "p-3",
        "transition-[opacity,background-color,border-color,box-shadow] duration-150 motion-reduce:transition-none",
        "focus-visible:z-50 focus-visible:!opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--g5-bg0)]",
        focusState === "default" &&
          "border-[var(--g5-stroke)] hover:border-[var(--g5-text2)] hover:bg-[var(--g5-bg2)]",
        focusState === "selected" &&
          "z-50 border-[var(--g5-info)] bg-[var(--g5-bg2)] opacity-100 shadow-lg ring-2 ring-[var(--g5-info)]",
        focusState === "neighbor" &&
          "z-40 border-[var(--g5-text1)] opacity-100 hover:bg-[var(--g5-bg2)]",
        focusState === "dimmed" &&
          "border-[var(--g5-stroke)] opacity-[0.55] hover:z-50 hover:border-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:opacity-100",
      )}
      style={{
        left: position.x,
        top: position.y,
        width,
        height,
      }}
    >
      <div className="flex w-full items-center justify-between gap-2">
        <span
          data-testid="change-graph-card-eyebrow"
          className={classNames(
            "flex min-w-0 items-center gap-1.5 font-semibold uppercase tracking-[0.08em]",
            compact ? "text-[8px]" : "text-[9px]",
          )}
          style={{ color: accentColor }}
        >
          <ArtifactIcon
            data-testid="change-graph-card-icon"
            aria-hidden="true"
            size={compact ? 11 : 12}
            strokeWidth={2}
            className="shrink-0"
          />
          <span className="truncate">{artifactLabel}</span>
        </span>
        <ChangeGraphChangeBadge state={changeState} compact={compact} />
      </div>
      <span
        data-testid="change-graph-card-title"
        className={classNames(
          "mt-1 line-clamp-2 font-semibold leading-4 text-[var(--g5-text0)]",
          compact ? "text-[11px]" : "text-xs",
        )}
      >
        {title}
      </span>
      {notice && (
        <span
          data-testid="change-graph-card-notice"
          className="mt-0.5 truncate text-[10px] font-medium leading-3 text-[var(--g5-warn)]"
          title={notice}
        >
          ⚠ {notice}
        </span>
      )}
      {compact ? (
        <div className="mt-auto flex w-full items-center justify-between gap-2 text-[8px]">
          {path && (
            <span className="min-w-0 flex-1 truncate font-mono text-[var(--g5-text1)]">
              {path}
            </span>
          )}
          {(additions !== undefined || deletions !== undefined) && (
            <ChangeGraphChangeCount
              additions={additions}
              deletions={deletions}
            />
          )}
        </div>
      ) : (
        <>
          {path && (
            <span className="mt-auto truncate font-mono text-[10px] leading-3 text-[var(--g5-text1)]">
              {path}
            </span>
          )}
          {(additions !== undefined || deletions !== undefined) && (
            <ChangeGraphChangeCount
              additions={additions}
              deletions={deletions}
              className={classNames(
                "text-[10px] leading-3",
                path && "mt-1",
                !path && "mt-auto",
              )}
            />
          )}
        </>
      )}
    </button>
  );
}

function ChangeGraphChangeCount({
  additions,
  deletions,
  className,
}: {
  additions?: number;
  deletions?: number;
  className?: string;
}) {
  return (
    <span className={classNames("shrink-0 tabular-nums", className)}>
      <span className="text-[var(--g5-ok)]">+{additions ?? 0}</span>{" "}
      <span className="text-[var(--g5-blocked)]">−{deletions ?? 0}</span>
    </span>
  );
}

export function ChangeGraphChangeBadge({
  state,
  label,
  compact = false,
}: {
  state: string;
  label?: string;
  compact?: boolean;
}) {
  const displayLabel = label ?? formatLabel(state);
  return (
    <span
      className={classNames(
        "shrink-0 rounded border font-semibold uppercase tracking-[0.04em]",
        compact ? "px-1 py-0.5 text-[8px]" : "px-1.5 py-0.5 text-[9px]",
        state === "added" &&
          "border-[var(--g5-ok)]/45 bg-[var(--g5-ok)]/25 text-[var(--g5-ok)]",
        state === "deleted" &&
          "border-[var(--g5-blocked)]/45 bg-[var(--g5-blocked)]/25 text-[var(--g5-blocked)]",
        state === "modified" &&
          "border-[var(--g5-warn)]/45 bg-[var(--g5-warn)]/25 text-[var(--g5-warn)]",
        state === "unchanged" &&
          "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text2)]",
      )}
    >
      {displayLabel}
    </span>
  );
}

export function ChangeGraphHumanBadge() {
  return (
    <span
      aria-label="Human-authored I/D block"
      className="shrink-0 rounded border border-[var(--g5-ok)]/45 bg-[var(--g5-ok)]/15 px-1.5 py-0.5 text-[9px] font-semibold tracking-[0.04em] text-[var(--g5-ok)]"
    >
      Human
    </span>
  );
}

export function ChangeGraphControl({
  label,
  onClick,
  children,
  text,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  text?: string;
}) {
  return (
    <button
      type="button"
      data-testid="change-graph-control"
      aria-label={label}
      title={label}
      onClick={onClick}
      className="inline-flex h-7 items-center gap-1 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 text-xs text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
    >
      {children}
      {text}
    </button>
  );
}

export function ChangeGraphLaneHeading({
  as: Heading = "h3",
  label,
  color,
  markerTestId,
  className,
  style,
}: {
  as?: "h3" | "h4";
  label: string;
  color: string;
  markerTestId?: string;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <Heading
      className={classNames(
        "flex items-center gap-2 border-b border-[var(--g5-stroke)]/45 font-semibold uppercase tracking-[0.08em]",
        className,
      )}
      style={{ ...style, color }}
    >
      <span
        data-testid={markerTestId}
        aria-hidden="true"
        className="h-1.5 w-1.5 shrink-0 rounded-full bg-current"
      />
      {label}
    </Heading>
  );
}

function formatLabel(value: string): string {
  return value
    .replaceAll("_", " ")
    .replace(/\b\w/g, (character) => character.toUpperCase());
}
