// =============================================================================
// IntentLogFilters — search + filter/sort/group controls for the Intent Log.
// Spec: g5/client/workspace-page/intent-log-table-view (controls only).
// =============================================================================

import { Search } from "lucide-react";

import type {
  AuthorType,
  FacetBucket,
  IntentKind,
  IntentLogFilters as Filters,
  TraceStatus,
} from "../../hooks/useIntentLog";
import { classNames } from "../ui/UI";

const AUTHOR_TYPES: AuthorType[] = [
  "human",
  "agent",
  "supervisor_agent",
  "system",
  "unknown",
];

const INTENT_KINDS: IntentKind[] = ["external_doc", "chat", "view", "ontology"];

const TRACE_STATUSES: TraceStatus[] = [
  "traceable",
  "unlinked",
  "unregistered",
  "not_applicable",
  "unknown",
];

const TIME_RANGES = ["all", "day", "week", "month", "custom"] as const;
const SORTS = ["when", "kind", "actor", "file", "trace"];
const GROUPS = ["none", "day", "actor", "kind", "artifact", "file", "trace"];

function ToggleChip({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      data-testid="intent-log-filter-chip"
      onClick={onClick}
      aria-pressed={active}
      className={classNames(
        "rounded-full border px-2 py-0.5 text-xs transition-colors",
        active
          ? "border-[var(--g5-info)] bg-[color-mix(in_srgb,var(--g5-info)_15%,transparent)] text-[var(--g5-text0)]"
          : "border-[var(--g5-stroke)] text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
      )}
    >
      {label.replace(/_/g, " ")}
    </button>
  );
}

export interface IntentLogFiltersProps {
  filters: Filters;
  facets?: Record<string, FacetBucket[]>;
  onChange: (next: Filters) => void;
}

function csvValue(values: string[] | undefined): string {
  return values?.join(", ") ?? "";
}

function parseCsvValue(value: string): string[] | undefined {
  const values = value
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
  return values.length > 0 ? values : undefined;
}

export function IntentLogFilters({
  filters,
  facets,
  onChange,
}: IntentLogFiltersProps) {
  const toggle = <T extends string>(
    key: keyof Filters,
    value: T,
    current: T[] | undefined,
  ) => {
    const set = new Set(current ?? []);
    if (set.has(value)) set.delete(value);
    else set.add(value);
    const next = Array.from(set);
    if (key === "authorType" && next.length === 0) {
      onChange({ ...filters, authorType: ["human"] });
      return;
    }
    onChange({ ...filters, [key]: next.length > 0 ? next : undefined });
  };

  return (
    <div className="flex flex-col gap-2 border-b border-[var(--g5-stroke)] p-2">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex flex-1 items-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-1">
          <Search size={14} className="text-[var(--g5-text2)]" />
          <input
            type="text"
            value={filters.q ?? ""}
            placeholder="Search content, actor, file, block, session…"
            onChange={(e) => onChange({ ...filters, q: e.target.value })}
            className="w-full bg-transparent text-xs text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)]"
          />
        </div>
        <label className="flex items-center gap-1 text-xs text-[var(--g5-text2)]">
          Sort
          <select
            data-testid="intent-log-sort-select"
            value={filters.sort ?? "when"}
            onChange={(e) => onChange({ ...filters, sort: e.target.value })}
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1 py-0.5 text-xs text-[var(--g5-text0)]"
          >
            {SORTS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-1 text-xs text-[var(--g5-text2)]">
          Group
          <select
            data-testid="intent-log-group-select"
            value={filters.group ?? "none"}
            onChange={(e) => onChange({ ...filters, group: e.target.value })}
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1 py-0.5 text-xs text-[var(--g5-text0)]"
          >
            {GROUPS.map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <label className="flex items-center gap-1 text-xs text-[var(--g5-text2)]">
          Time
          <select
            data-testid="intent-log-time-range-select"
            value={filters.timeRange ?? "all"}
            onChange={(e) =>
              onChange({
                ...filters,
                timeRange: e.target.value as Filters["timeRange"],
              })
            }
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1 py-0.5 text-xs text-[var(--g5-text0)]"
          >
            {TIME_RANGES.map((range) => (
              <option key={range} value={range}>
                {range}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-1 text-xs text-[var(--g5-text2)]">
          From
          <input
            type="date"
            value={filters.from ?? ""}
            onChange={(e) =>
              onChange({ ...filters, from: e.target.value || undefined })
            }
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1 py-0.5 text-xs text-[var(--g5-text0)]"
          />
        </label>
        <label className="flex items-center gap-1 text-xs text-[var(--g5-text2)]">
          To
          <input
            type="date"
            value={filters.to ?? ""}
            onChange={(e) =>
              onChange({ ...filters, to: e.target.value || undefined })
            }
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1 py-0.5 text-xs text-[var(--g5-text0)]"
          />
        </label>
        <label className="min-w-40 flex-1 text-xs text-[var(--g5-text2)]">
          Actor
          <input
            type="text"
            value={csvValue(filters.actor)}
            placeholder="Ziniu, Reviewer"
            onChange={(e) =>
              onChange({ ...filters, actor: parseCsvValue(e.target.value) })
            }
            className="mt-0.5 w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 text-xs text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)]"
          />
        </label>
        <label className="min-w-40 flex-1 text-xs text-[var(--g5-text2)]">
          Repo
          <input
            type="text"
            value={filters.repo ?? ""}
            placeholder="repo id, name, or slug"
            onChange={(e) =>
              onChange({ ...filters, repo: e.target.value || undefined })
            }
            list="intent-log-repo-facets"
            className="mt-0.5 w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 text-xs text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)]"
          />
          <datalist id="intent-log-repo-facets">
            {(facets?.repo ?? []).map((bucket) => (
              <option key={bucket.value} value={bucket.value} />
            ))}
          </datalist>
        </label>
      </div>
      <div className="flex flex-wrap items-center gap-1">
        <span className="mr-1 text-[10px] uppercase text-[var(--g5-text2)]">
          Author type
        </span>
        {AUTHOR_TYPES.map((type) => (
          <ToggleChip
            key={type}
            label={type}
            active={filters.authorType?.includes(type) ?? false}
            onClick={() => toggle("authorType", type, filters.authorType)}
          />
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-1">
        <span className="mr-1 text-[10px] uppercase text-[var(--g5-text2)]">
          Kind
        </span>
        {INTENT_KINDS.map((k) => (
          <ToggleChip
            key={k}
            label={k}
            active={filters.intentKind?.includes(k) ?? false}
            onClick={() => toggle("intentKind", k, filters.intentKind)}
          />
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-1">
        <span className="mr-1 text-[10px] uppercase text-[var(--g5-text2)]">
          Trace
        </span>
        {TRACE_STATUSES.map((t) => (
          <ToggleChip
            key={t}
            label={t}
            active={filters.traceStatus?.includes(t) ?? false}
            onClick={() => toggle("traceStatus", t, filters.traceStatus)}
          />
        ))}
      </div>
    </div>
  );
}
