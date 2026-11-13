// =============================================================================
// IntentLogView — workspace center-panel tab container for the Intent Log.
// Spec: g5/client/workspace-page/intent-log-viewer.
// Owns data fetching/pagination, URL-reflected filter state, the audit-incomplete
// banner, and trace/open navigation wiring. Presentation lives in the table view.
// =============================================================================

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { AlertTriangle } from "lucide-react";

import {
  useIntentLog,
  type IntentLogEntry,
  type IntentLogFilters,
} from "../../hooks/useIntentLog";
import { Button } from "../ui/UI";
import {
  IntentTraceabilityPanel,
  type NavigateTarget,
} from "../editor/IntentTraceabilityPanel";
import { IntentLogFilters as FiltersBar } from "./IntentLogFilters";
import type { IntentLogLineageTarget } from "./IntentLogLineageGraph";
import { IntentLogTableView } from "./IntentLogTableView";

export interface IntentLogViewProps {
  workspaceId: string;
  repoId: string;
  onNavigate?: (target: NavigateTarget) => void;
}

const CSV_KEYS: Array<keyof IntentLogFilters> = [
  "authorType",
  "intentKind",
  "traceStatus",
  "actor",
];
const STRING_KEYS: Array<keyof IntentLogFilters> = [
  "timeRange",
  "from",
  "to",
  "repo",
];
const INTENT_LOG_QUERY_KEYS = new Set<string>([
  "q",
  "sort",
  "group",
  "cursor",
  ...CSV_KEYS,
  ...STRING_KEYS,
]);

function filtersFromParams(params: URLSearchParams): IntentLogFilters {
  const filters: IntentLogFilters = { authorType: ["human"] };
  const q = params.get("q");
  if (q) filters.q = q;
  const sort = params.get("sort");
  if (sort) filters.sort = sort;
  const group = params.get("group");
  if (group) filters.group = group;
  for (const key of STRING_KEYS) {
    const raw = params.get(key);
    if (raw) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (filters as any)[key] = raw;
    }
  }
  for (const key of CSV_KEYS) {
    const raw = params.get(key);
    if (raw) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (filters as any)[key] = raw.split(",").filter(Boolean);
    }
  }
  return filters;
}

function cursorFromParams(params: URLSearchParams): string | null {
  const raw = params.get("cursor");
  return raw && raw.trim() ? raw : null;
}

function paramsFromFilters(
  filters: IntentLogFilters,
  current: URLSearchParams,
  cursor: string | null,
): URLSearchParams {
  const out = new URLSearchParams(current);
  for (const key of INTENT_LOG_QUERY_KEYS) {
    out.delete(key);
  }
  if (filters.q) out.set("q", filters.q);
  if (filters.sort) out.set("sort", filters.sort);
  if (filters.group) out.set("group", filters.group);
  for (const key of STRING_KEYS) {
    const value = filters[key] as string | undefined;
    if (value) out.set(key, value);
  }
  for (const key of CSV_KEYS) {
    const value = filters[key] as string[] | undefined;
    if (value && value.length > 0) out.set(key, value.join(","));
  }
  if (cursor) out.set("cursor", cursor);
  return out;
}

function isProjectDocFile(fileName: string | null): boolean {
  return !!fileName && fileName.startsWith("docs/");
}

function navigateTypeForFile(
  fileType: string,
  fileName: string | null,
): NavigateTarget["type"] | null {
  if (!fileName) return null;
  if (fileType === "ontology") return "spec";
  if (fileType === "code") return "file";
  if (fileType === "view") return "doc";
  if (fileType === "external_doc" && isProjectDocFile(fileName)) return "doc";
  return null;
}

function navigateTargetFor(
  entry: IntentLogEntry,
  workspaceId: string,
): NavigateTarget | null {
  if (!entry.traceKey) return null;
  const { fileType, fileName, id } = entry.traceKey;
  const type = navigateTypeForFile(fileType, fileName);
  if (!type) return null;
  return {
    type,
    id: fileName,
    intentRecordNumber: id,
    workspaceId,
    repoId: entry.rawSource.repoId,
  };
}

export function IntentLogView({
  workspaceId,
  repoId,
  onNavigate,
}: IntentLogViewProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const [filters, setFilters] = useState<IntentLogFilters>(() =>
    filtersFromParams(searchParams),
  );
  const [cursor, setCursor] = useState<string | null>(() =>
    cursorFromParams(searchParams),
  );
  const [rows, setRows] = useState<IntentLogEntry[]>([]);
  const [traceTarget, setTraceTarget] = useState<IntentLogEntry | null>(null);
  const mergedCursorRef = useRef<string | null | undefined>(undefined);
  const lastOwnSearchRef = useRef<string | null>(null);
  const previousBranchRef = useRef(workspaceId);

  const filtersKey = useMemo(() => JSON.stringify(filters), [filters]);
  const searchKey = searchParams.toString();
  const state = useIntentLog(workspaceId, filters, cursor);

  const resetLoadedRows = useCallback(() => {
    mergedCursorRef.current = undefined;
    setRows([]);
    setTraceTarget(null);
  }, []);

  // Browser navigation and pasted URLs hydrate the viewer state.
  useEffect(() => {
    if (lastOwnSearchRef.current === searchKey) {
      lastOwnSearchRef.current = null;
      return;
    }
    const nextFilters = filtersFromParams(searchParams);
    const nextCursor = cursorFromParams(searchParams);
    let changed = false;
    if (JSON.stringify(nextFilters) !== filtersKey) {
      setFilters(nextFilters);
      changed = true;
    }
    if (nextCursor !== cursor) {
      setCursor(nextCursor);
      changed = true;
    }
    if (changed) resetLoadedRows();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchKey]);

  // Reflect local state in the URL (shareable audit view).
  useEffect(() => {
    const nextParams = paramsFromFilters(filters, searchParams, cursor);
    const nextSearch = nextParams.toString();
    if (nextSearch === searchKey) return;
    lastOwnSearchRef.current = nextSearch;
    setSearchParams(nextParams, { replace: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filtersKey, cursor, searchKey]);

  useEffect(() => {
    if (previousBranchRef.current === workspaceId) return;
    previousBranchRef.current = workspaceId;
    resetLoadedRows();
    setCursor(null);
  }, [resetLoadedRows, workspaceId]);

  const handleFilterChange = (nextFilters: IntentLogFilters) => {
    setFilters(nextFilters);
    setCursor(null);
    resetLoadedRows();
  };

  // Accumulate pages as the cursor advances.
  useEffect(() => {
    if (state.status !== "ok") return;
    if (mergedCursorRef.current === cursor) return;
    mergedCursorRef.current = cursor;
    const page = state.data.rows;
    setRows((prev) => {
      if (cursor == null) return page;
      const seen = new Set(prev.map((r) => r.id));
      return [...prev, ...page.filter((r) => !seen.has(r.id))];
    });
  }, [state, cursor]);

  const meta = state.status === "ok" ? state.data.meta : null;

  const handleOpen = (entry: IntentLogEntry) => {
    const target = navigateTargetFor(entry, workspaceId);
    if (target) onNavigate?.(target);
  };

  const handleLineageNavigate = (target: IntentLogLineageTarget) => {
    const type = navigateTypeForFile(target.fileType, target.fileName);
    if (!type || !target.fileName) return;
    onNavigate?.({
      type,
      id: target.fileName,
      intentRecordNumber: target.recordId,
      workspaceId: target.workspaceId ?? workspaceId,
      repoId: target.repoId,
    });
  };

  return (
    <div className="flex h-full flex-col" data-testid="intent-log-view">
      <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] px-3 py-2">
        <h2 className="text-sm font-medium text-[var(--g5-text0)]">
          All intent logs
        </h2>
        <span className="text-xs text-[var(--g5-text2)]">
          {rows.length} row{rows.length === 1 ? "" : "s"}
          {meta?.hasMore ? "+" : ""}
        </span>
      </div>

      <FiltersBar
        filters={filters}
        facets={meta?.facets}
        onChange={handleFilterChange}
      />

      {meta && meta.diagnostics.length > 0 && (
        <div className="flex items-start gap-2 border-b border-[var(--g5-warn)] bg-[color-mix(in_srgb,var(--g5-warn)_12%,transparent)] px-3 py-2 text-xs text-[var(--g5-text1)]">
          <AlertTriangle size={14} className="mt-0.5 text-[var(--g5-warn)]" />
          <div>
            <strong>Audit incomplete:</strong> {meta.diagnostics.length} source
            issue{meta.diagnostics.length === 1 ? "" : "s"} — some rows may be
            missing.
            <ul className="mt-1 list-disc pl-4 text-[var(--g5-text2)]">
              {meta.diagnostics.slice(0, 5).map((d, i) => (
                <li key={i}>
                  {d.sourcePath}
                  {d.line != null ? `:${d.line}` : ""} — {d.reason}
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto">
        {workspaceId && state.status === "loading" && rows.length === 0 ? (
          <div className="p-6 text-center text-sm text-[var(--g5-text2)]">
            Loading intent records…
          </div>
        ) : state.status === "error" ? (
          <div className="p-6 text-center text-sm text-[var(--g5-blocked)]">
            Failed to load intent log: {state.error.message}
          </div>
        ) : (
          <IntentLogTableView
            rows={rows}
            group={filters.group ?? "none"}
            workspaceId={workspaceId}
            onOpen={handleOpen}
            onTrace={setTraceTarget}
            onNavigate={handleLineageNavigate}
          />
        )}

        {meta?.hasMore && (
          <div className="flex justify-center p-3">
            <Button
              kind="secondary"
              onClick={() => setCursor(meta.cursor)}
              disabled={state.status === "loading"}
            >
              Load more
            </Button>
          </div>
        )}
      </div>

      {traceTarget?.traceKey && (
        <div className="max-h-64 overflow-auto border-t border-[var(--g5-stroke)]">
          {(["parents", "children"] as const).map((direction) => (
            <IntentTraceabilityPanel
              key={direction}
              direction={direction}
              workspaceId={workspaceId}
              repoId={traceTarget.rawSource.repoId ?? repoId}
              fileType={traceTarget.traceKey!.fileType}
              fileName={traceTarget.traceKey!.fileName}
              intentRecordNumber={traceTarget.traceKey!.id}
              onDismiss={() => setTraceTarget(null)}
              onNavigate={(target) => onNavigate?.(target as NavigateTarget)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
