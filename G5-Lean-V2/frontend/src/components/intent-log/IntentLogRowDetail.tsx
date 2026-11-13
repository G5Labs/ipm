// =============================================================================
// IntentLogRowDetail — expanded read-only detail for one Intent Log row.
// Spec: g5/client/workspace-page/intent-log-table-view [D3].
// =============================================================================

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ScanSearch } from "lucide-react";

import type { IntentLogEntry } from "../../hooks/useIntentLog";
import {
  IntentLogLineageGraph,
  type IntentLogLineageNavigate,
} from "./IntentLogLineageGraph";

/**
 * Measure the visible width of the nearest horizontal-scroll ancestor. The
 * intent-log table is wider than the center panel (the wide "What" column), so
 * the expanded detail (which spans the whole table) would otherwise center its
 * lineage/inspector against the over-wide table. Pinning the detail to this
 * width and sticky-left keeps the graph + inspector centered in the visible
 * display (spec [D9], PM [D25]).
 */
function useViewportWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let parent: HTMLElement | null = el.parentElement;
    let scroller: HTMLElement | null = null;
    while (parent) {
      const overflowX = getComputedStyle(parent).overflowX;
      if (overflowX === "auto" || overflowX === "scroll") {
        scroller = parent;
        break;
      }
      parent = parent.parentElement;
    }
    const target: HTMLElement = scroller ?? document.documentElement;
    const update = () => setWidth(target.clientWidth);
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(target);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

function MetaRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex gap-2 text-xs">
      <span className="w-32 shrink-0 text-[var(--g5-text2)]">{label}</span>
      <span className="break-all text-[var(--g5-text1)]">{value}</span>
    </div>
  );
}

/**
 * Table-detail affordance (not a modal editor): lineage preview for traceable
 * rows, bounded full content, raw source metadata, natural key, provenance
 * reasons, duplicate source paths, and a trace summary.
 */
export function IntentLogRowDetail({
  entry,
  workspaceId,
  onNavigate,
}: {
  entry: IntentLogEntry;
  workspaceId: string;
  onNavigate?: IntentLogLineageNavigate;
}) {
  const muted = (s: string) => (s === "git_blame" ? "git blame" : s);
  const showLineage = entry.traceStatus === "traceable" && !!entry.traceKey;
  const { ref: stickyRef, width: viewportWidth } = useViewportWidth();

  return (
    <div
      ref={stickyRef}
      className="flex flex-col gap-3 border-l-2 border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3"
      style={{
        position: "sticky",
        left: 0,
        width: viewportWidth ?? undefined,
        maxWidth: viewportWidth ?? undefined,
      }}
    >
      {showLineage && (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-[var(--g5-text2)]">
            <ScanSearch size={13} />
            Intent graph — {entry.blockId ?? entry.traceKey?.id} · its parents
            and descendants
          </div>
          <IntentLogLineageGraph
            entry={entry}
            workspaceId={workspaceId}
            onNavigate={onNavigate}
          />
        </div>
      )}

      {entry.content && (
        <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg2)] p-2 text-xs text-[var(--g5-text1)]">
          {entry.content}
        </pre>
      )}
      <div className="flex flex-col gap-1">
        <MetaRow label="Kind" value={entry.intentKind} />
        <MetaRow label="Author type" value={entry.authorType} />
        {entry.artifactType && (
          <MetaRow label="Artifact type" value={entry.artifactType} />
        )}
        {entry.artifactPath && (
          <MetaRow label="File" value={entry.artifactPath} />
        )}
        {entry.blockId && <MetaRow label="Block" value={entry.blockId} />}
        {entry.rawSource.sessionId && (
          <MetaRow label="Chat session" value={entry.rawSource.sessionId} />
        )}
        <MetaRow
          label="When"
          value={
            entry.occurredAt
              ? `${entry.occurredAt} (${muted(entry.occurredAtSource)})`
              : "unknown"
          }
        />
        <MetaRow
          label="Who"
          value={
            entry.actor
              ? `${entry.actor} (${muted(entry.actorSource)})`
              : "unknown"
          }
        />
        <MetaRow label="Source kind" value={entry.sourceKind} />
        <MetaRow label="Trace status" value={entry.traceStatus} />
        <MetaRow label="Stable id" value={entry.id} />
        {entry.rawSource.repoId && (
          <MetaRow label="Repo" value={entry.rawSource.repoId} />
        )}
        {entry.rawSource.duplicatePaths &&
          entry.rawSource.duplicatePaths.length > 0 && (
            <MetaRow
              label="Duplicate sources"
              value={entry.rawSource.duplicatePaths.join(", ")}
            />
          )}
      </div>
    </div>
  );
}
