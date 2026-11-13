// Display-only native subscription usage for Claude and Codex.
// Updated during work, held in browser memory; missing and expired windows
// are hidden. The headline shows remaining percentage, detail shows used.

import { useEffect, useId, useRef, useState } from "react";

import type { AgentAuthModeV1, SubscriptionUsage } from "../../lib/types";
import { classNames } from "../ui/UI";

/** Rate-limit window the chip headline reflects. 7-day is the D8 default. */
export type SubscriptionUsageWindow = "5h" | "7d";

/** Snapshots older than this (ms) render as stale with an "as of" age (D8). */
const STALE_AFTER_MS = 15 * 60 * 1000;

/** Remaining-budget styling thresholds (percent left). */
const AMBER_BELOW_PERCENT = 20;
const RED_BELOW_PERCENT = 10;

type UsageTone = "ok" | "amber" | "red";

export interface SubscriptionUsageChipProps {
  usage: SubscriptionUsage;
  /** Preferred window; falls back to the available window. Defaults to 7-day. */
  window?: SubscriptionUsageWindow;
  /**
   * Current time (ms since epoch), threaded from the caller's ticking clock so
   * capture-age/staleness stays live and render stays pure.
   */
  nowMs: number;
}

/** Only the active subscription route may display its live observation. */
export function shouldRenderSubscriptionUsageChip(session: {
  authMode?: AgentAuthModeV1 | null;
  subscriptionUsage?: SubscriptionUsage | null;
}): boolean {
  return (
    (session.authMode === "anthropic_oauth" ||
      session.authMode === "chat_gpt_subscription") &&
    session.subscriptionUsage != null
  );
}

function windowUtilization(
  usage: SubscriptionUsage,
  window: SubscriptionUsageWindow,
  nowMs?: number,
): number | null {
  const reset = windowReset(usage, window);
  if (nowMs != null && reset && Date.parse(reset) <= nowMs) return null;
  const raw =
    window === "5h" ? usage.fiveHourUtilization : usage.sevenDayUtilization;
  return typeof raw === "number" && Number.isFinite(raw) && raw >= 0 && raw <= 1
    ? raw
    : null;
}

function windowReset(
  usage: SubscriptionUsage,
  window: SubscriptionUsageWindow,
): string | null | undefined {
  return window === "5h" ? usage.fiveHourResetAt : usage.sevenDayResetAt;
}

function percentLeftFromUtilization(utilization: number | null): number | null {
  if (utilization == null) return null;
  const remaining = Math.min(1, Math.max(0, 1 - utilization));
  return Math.round(remaining * 100);
}

function toneForPercentLeft(percentLeft: number | null): UsageTone {
  if (percentLeft == null) return "ok";
  if (percentLeft < RED_BELOW_PERCENT) return "red";
  if (percentLeft < AMBER_BELOW_PERCENT) return "amber";
  return "ok";
}

function toneColorVar(tone: UsageTone): string {
  switch (tone) {
    case "red":
      return "var(--g5-blocked)";
    case "amber":
      return "var(--g5-warn)";
    case "ok":
      return "var(--g5-ok)";
  }
}

/** "just now" | "Nm ago" | "Nh ago" | "Nd ago" from an ISO timestamp. */
export function formatAgo(
  iso: string | null | undefined,
  nowMs: number,
): string | null {
  if (!iso) return null;
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms)) return null;
  const seconds = Math.max(0, Math.floor((nowMs - ms) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

/**
 * Reset display for one window. A `reset_at` in the past renders as
 * reset-passed rather than still-consumed (D8); a future reset renders as a
 * relative "resets in …"; a missing reset renders no reset line.
 */
export function formatReset(
  iso: string | null | undefined,
  nowMs: number,
): { passed: boolean; text: string } | null {
  if (!iso) return null;
  const ms = Date.parse(iso);
  if (!Number.isFinite(ms)) return null;
  if (ms <= nowMs) return { passed: true, text: "window reset" };
  const seconds = Math.floor((ms - nowMs) / 1000);
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return { passed: false, text: `resets in ${Math.max(1, minutes)}m` };
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return { passed: false, text: `resets in ${hours}h` };
  const days = Math.floor(hours / 24);
  return { passed: false, text: `resets in ${days}d` };
}

function windowLabel(window: SubscriptionUsageWindow): string {
  return window === "5h" ? "5-hour" : "7-day";
}

function windowTag(window: SubscriptionUsageWindow): string {
  return window === "5h" ? "5h" : "7d";
}

interface TooltipWindowRowProps {
  label: string;
  tag: SubscriptionUsageWindow;
  selected: boolean;
  utilization: number;
  reset: { passed: boolean; text: string } | null;
}

function TooltipWindowRow({
  label,
  tag,
  selected,
  utilization,
  reset,
}: TooltipWindowRowProps) {
  const usedPercent = Math.round(utilization * 100);
  const barTone = toneColorVar(
    toneForPercentLeft(percentLeftFromUtilization(utilization)),
  );
  return (
    <div
      data-testid={`usage-tooltip-window-${tag}`}
      data-selected={selected ? "true" : "false"}
      data-reset-passed={reset?.passed ? "true" : "false"}
      className="mb-2 last:mb-0"
    >
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-[var(--g5-text0)]">
          {label}
          {selected ? (
            <span className="ml-1.5 rounded-[var(--g5-radius-sm)] border border-[var(--g5-info)]/50 px-1 py-px text-[9px] uppercase tracking-wide text-[var(--g5-info)]">
              selected
            </span>
          ) : null}
        </span>
        <span className="tabular-nums text-[var(--g5-text2)]">
          {usedPercent}% used
        </span>
      </div>
      <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-[var(--g5-stroke)]">
        <span
          className="block h-full rounded-full"
          style={{
            width: `${usedPercent}%`,
            backgroundColor: barTone,
          }}
          aria-hidden="true"
        />
      </div>
      {reset ? (
        <div className="mt-1 text-[10px] text-[var(--g5-text2)]">
          {reset.passed ? "Window reset" : reset.text}
        </div>
      ) : null}
    </div>
  );
}

export function SubscriptionUsageChip({
  usage,
  window = "7d",
  nowMs,
}: SubscriptionUsageChipProps) {
  const availableWindows = (["5h", "7d"] as const).flatMap((tag) => {
    const utilization = windowUtilization(usage, tag, nowMs);
    return utilization == null ? [] : [{ tag, utilization }];
  });
  const selectedWindow =
    availableWindows.find(({ tag }) => tag === window) ?? availableWindows[0];
  const percentLeft = percentLeftFromUtilization(
    selectedWindow?.utilization ?? null,
  );
  const tone = toneForPercentLeft(percentLeft);
  const toneColor = toneColorVar(tone);

  const remainingTurns =
    percentLeft != null ? Math.min(1, Math.max(0, percentLeft / 100)) : 0;
  // Remaining-budget arc: filled portion is the remaining fraction in the tone
  // color, the rest is the track. Inner disc masks the centre into a ring.
  const ringGradient = `conic-gradient(${toneColor} 0turn ${remainingTurns}turn, var(--g5-stroke) ${remainingTurns}turn 1turn)`;

  const captureAgo = formatAgo(usage.capturedAt, nowMs) ?? "unknown";
  const capturedMs = Date.parse(usage.capturedAt);
  const stale =
    Number.isFinite(capturedMs) && nowMs - capturedMs > STALE_AFTER_MS;

  const headline = percentLeft != null ? `${percentLeft}% left` : "usage";

  const detailsId = useId();
  const [expanded, setExpanded] = useState(false);
  const chipRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!expanded) return;
    const dismissOutside = (event: Event) => {
      if (
        event.target instanceof Node &&
        !chipRef.current?.contains(event.target)
      ) {
        setExpanded(false);
      }
    };
    const dismissOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setExpanded(false);
    };
    document.addEventListener("pointerdown", dismissOutside);
    document.addEventListener("focusin", dismissOutside);
    document.addEventListener("keydown", dismissOnEscape);
    return () => {
      document.removeEventListener("pointerdown", dismissOutside);
      document.removeEventListener("focusin", dismissOutside);
      document.removeEventListener("keydown", dismissOnEscape);
    };
  }, [expanded]);

  if (!selectedWindow) return null;

  const ariaLabel = `Subscription usage: ${headline} in the ${windowLabel(
    selectedWindow.tag,
  )} window${stale ? `, as of ${captureAgo}` : ""}`;

  return (
    <div
      ref={chipRef}
      className="relative inline-flex"
      data-testid="subscription-usage-chip"
      data-tone={tone}
      data-window={selectedWindow.tag}
      data-stale={stale ? "true" : "false"}
    >
      <button
        type="button"
        data-testid="subscription-usage-button"
        onClick={() => setExpanded((open) => !open)}
        className={classNames(
          "inline-flex items-center gap-1.5 whitespace-nowrap",
          "rounded-full bg-[var(--g5-bg2)] px-2.5 py-1",
          "cursor-pointer text-xs font-medium transition-colors hover:bg-[var(--g5-stroke)]",
          "focus-visible:outline-none focus-visible:ring-2",
          "focus-visible:ring-[var(--g5-info)]",
        )}
        aria-label={ariaLabel}
        aria-expanded={expanded}
        aria-controls={detailsId}
      >
        <span
          className="relative inline-block h-3.5 w-3.5 shrink-0 rounded-full"
          style={{ background: ringGradient }}
          aria-hidden="true"
        >
          <span className="absolute inset-[3px] rounded-full bg-[var(--g5-bg2)]" />
        </span>
        <span
          data-testid="subscription-usage-headline"
          style={{ color: tone === "ok" ? "var(--g5-text1)" : toneColor }}
        >
          {headline}
        </span>
        <span className="text-[10px] text-[var(--g5-text2)]">
          {windowTag(selectedWindow.tag)}
        </span>
        {stale ? (
          <span
            data-testid="subscription-usage-stale-dot"
            className="h-1.5 w-1.5 shrink-0 rounded-full bg-[var(--g5-text2)]"
            aria-hidden="true"
          />
        ) : null}
      </button>

      <div
        role="region"
        aria-label="Subscription usage details"
        id={detailsId}
        hidden={!expanded}
        data-testid="subscription-usage-tooltip"
        className={classNames(
          "absolute bottom-[calc(100%+8px)] left-0 z-20 w-72 max-w-[calc(100vw-2rem)]",
          "rounded-[var(--g5-radius-lg)] border border-[var(--g5-stroke)]",
          "bg-[var(--g5-bg1)] p-3 text-left shadow-[var(--shadow-pane)]",
        )}
      >
        <div className="mb-2 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
          Subscription usage
        </div>
        {availableWindows.map(({ tag, utilization }) => (
          <TooltipWindowRow
            key={tag}
            label={windowLabel(tag)}
            tag={tag}
            selected={selectedWindow.tag === tag}
            utilization={utilization}
            reset={formatReset(windowReset(usage, tag), nowMs)}
          />
        ))}
        <div className="mt-2 flex items-center justify-between border-t border-[var(--g5-stroke)] pt-2 text-[10px] text-[var(--g5-text2)]">
          <span data-testid="subscription-usage-capture-age">
            {stale ? `As of ${captureAgo}` : `Captured ${captureAgo}`}
          </span>
          {usage.unifiedStatus ? (
            <span className="tabular-nums">{usage.unifiedStatus}</span>
          ) : null}
        </div>
      </div>
    </div>
  );
}

export default SubscriptionUsageChip;
