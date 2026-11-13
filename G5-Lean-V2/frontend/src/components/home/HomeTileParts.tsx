// =============================================================================
// Home — shared tile header and row-action styles
// =============================================================================
// Used by every Home tile (Recent Workspaces, Needs Your Attention and, under
// the `alignment` layout, Issues and tasks / Running compute). The defaults
// keep the classic tiles' markup exactly as before.
// =============================================================================

import { Check } from "lucide-react";
import React, { useState } from "react";

import { classNames } from "../ui/UI";

export const HOME_ROW_ACTION_CLASSES =
  "flex-none rounded border border-[var(--g5-stroke)] px-2.5 py-1 text-xs font-medium text-[var(--g5-text0)] no-underline hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

export interface HomeSectionHeaderProps {
  title: string;
  count?: number;
  /** Accessible noun for the count badge, e.g. "owned workspaces". */
  countLabel?: string;
  countTestId?: string;
  actions?: React.ReactNode;
  testId?: string;
}

export function HomeSectionHeader({
  title,
  count,
  countLabel = "owned workspaces",
  countTestId = "home-owned-count",
  actions,
  testId,
}: HomeSectionHeaderProps) {
  return (
    <div
      data-testid={testId}
      className={
        // The actions wrap below the title as one group when they do not
        // fit beside it, so no lone control breaks onto a line of its own.
        actions
          ? "flex min-h-6 flex-wrap items-center gap-2"
          : "flex min-h-6 items-center gap-2"
      }
    >
      <div className="flex min-h-6 items-center gap-2">
        <h2 className="whitespace-nowrap text-xs font-semibold uppercase tracking-[0.14em] text-[var(--g5-text1)]">
          {title}
        </h2>
        {count !== undefined && (
          <span
            data-testid={countTestId}
            className="inline-flex min-w-5 h-5 items-center justify-center rounded-full bg-[var(--g5-bg2)] px-1.5 text-xs text-[var(--g5-text2)]"
            aria-label={`${count} ${countLabel}`}
          >
            {count}
          </span>
        )}
      </div>
      {actions}
    </div>
  );
}

export interface HomeShowMoreProps {
  /** Rows not yet shown; the button renders nothing at zero. */
  remaining: number;
  /** Rows the next press reveals (the tile's page size). */
  pageSize: number;
  /** Singular noun for the label, e.g. "workspace" → "Show 5 more workspaces". */
  noun: string;
  testId: string;
  onShowMore: () => void;
  disabled?: boolean;
  /** Replaces the label while a server page is loading. */
  loadingLabel?: string | null;
}

/**
 * The paging footer every Home tile shares (workbench
 * g5.workbench.home-workspaces): "Show N more <noun>s" for the next page.
 */
export function HomeShowMore({
  remaining,
  pageSize,
  noun,
  testId,
  onShowMore,
  disabled = false,
  loadingLabel = null,
}: HomeShowMoreProps) {
  if (remaining <= 0) return null;
  const next = Math.min(pageSize, remaining);
  return (
    <button
      type="button"
      data-testid={testId}
      disabled={disabled}
      onClick={onShowMore}
      className="w-full border-t border-[var(--g5-stroke)] px-4 py-2 text-left text-sm text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--g5-info)] disabled:cursor-wait disabled:opacity-60"
    >
      {loadingLabel ?? `Show ${next} more ${next === 1 ? noun : `${noun}s`}`}
    </button>
  );
}

export const HOME_FILTER_CHIP_CLASSES =
  "inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11px] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

export interface HomeFilterChipProps {
  selected: boolean;
  label: string;
  testId: string;
  onClick: () => void;
  /** Classes for the selected state; the neutral chip by default. */
  selectedClassName?: string;
}

/** One pressed/unpressed filter chip, as every Home tile's header shows them. */
export function HomeFilterChip({
  selected,
  label,
  testId,
  onClick,
  selectedClassName = "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] font-semibold text-[var(--g5-text0)]",
}: HomeFilterChipProps) {
  return (
    <button
      type="button"
      data-testid={testId}
      aria-pressed={selected}
      onClick={onClick}
      className={classNames(
        HOME_FILTER_CHIP_CLASSES,
        selected
          ? selectedClassName
          : "border-[var(--g5-stroke)] bg-transparent font-medium text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]",
      )}
    >
      {selected && <Check size={12} aria-hidden="true" />}
      {label}
    </button>
  );
}

export interface HomeSortSelectProps<T extends string> {
  /** Accessible name, e.g. "Sort workspaces". */
  label: string;
  value: T;
  options: ReadonlyArray<{ value: T; label: string }>;
  testId: string;
  onChange: (value: T) => void;
}

/**
 * The Home sort pill: a native select showing only the chosen order's name.
 * Every Home tile's sort uses this one control so they look alike
 * (g5.ui.home-attention-order-control).
 */
export function HomeSortSelect<T extends string>({
  label,
  value,
  options,
  testId,
  onChange,
}: HomeSortSelectProps<T>) {
  return (
    <select
      data-testid={testId}
      aria-label={label}
      value={value}
      onChange={(event) => onChange(event.target.value as T)}
      className="h-6 rounded-full border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 text-[11px] font-medium text-[var(--g5-text1)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
    >
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

/**
 * How many pages of `pageSize` a tile shows (g5.ui.home-paging): one until
 * "Show more", and one again whenever the page size itself changes, so a
 * setting edited in another tab never leaves a half page behind.
 */
export function useHomePages(pageSize: number): {
  pages: number;
  showMore: () => void;
  reset: () => void;
} {
  const [state, setState] = useState({ pages: 1, pageSize });
  // Adjust state when the prop changes (React's own pattern): the size the
  // pages were counted at is remembered, so 5 → 10 → 5 starts over each time.
  if (state.pageSize !== pageSize) setState({ pages: 1, pageSize });
  const pages = state.pageSize === pageSize ? state.pages : 1;
  return {
    pages,
    showMore: () => setState({ pages: pages + 1, pageSize }),
    reset: () => setState({ pages: 1, pageSize }),
  };
}
