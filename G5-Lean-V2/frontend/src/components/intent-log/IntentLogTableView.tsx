// =============================================================================
// IntentLogTableView — dense, read-only Intent Log audit table.
// Spec: g5/client/workspace-page/intent-log-table-view.
// Presentation plus expanded-row read-only lineage preview.
// =============================================================================

import { useState, type ReactNode } from "react";
import {
  ChevronDown,
  ChevronRight,
  Copy,
  ExternalLink,
  Share2,
} from "lucide-react";

import type { IntentLogEntry, TraceStatus } from "../../hooks/useIntentLog";
import type { State } from "../../lib/types";
import { Chip, IconButton, classNames } from "../ui/UI";
import { IntentLogRowDetail } from "./IntentLogRowDetail";
import type { IntentLogLineageNavigate } from "./IntentLogLineageGraph";

const TRACE_STATE: Record<TraceStatus, State> = {
  traceable: "ok",
  unlinked: "muted",
  unregistered: "warn",
  not_applicable: "muted",
  unknown: "info",
};

const TRACE_LABEL: Record<TraceStatus, string> = {
  traceable: "traceable",
  unlinked: "unlinked",
  unregistered: "unregistered",
  not_applicable: "n/a",
  unknown: "unknown",
};

export interface IntentLogTableViewProps {
  rows: IntentLogEntry[];
  group: string;
  workspaceId: string;
  onOpen: (entry: IntentLogEntry) => void;
  onTrace: (entry: IntentLogEntry) => void;
  onNavigate?: IntentLogLineageNavigate;
}

function groupKey(entry: IntentLogEntry, group: string): string {
  switch (group) {
    case "kind":
      return entry.intentKind;
    case "actor":
      return entry.actor ?? "unknown";
    case "trace":
      return entry.traceStatus;
    case "artifact":
      return entry.artifactType ?? "unknown";
    case "file":
      return entry.artifactPath ?? "unknown";
    case "day":
      return entry.occurredAt ? entry.occurredAt.slice(0, 10) : "unknown";
    default:
      return "";
  }
}

function whenLabel(entry: IntentLogEntry): string {
  if (!entry.occurredAt) return "—";
  return entry.occurredAt.replace("T", " ").replace("Z", " UTC");
}

function canOpenEntry(entry: IntentLogEntry): boolean {
  const key = entry.traceKey;
  if (!key) return false;
  if (
    key.fileType === "ontology" ||
    key.fileType === "code" ||
    key.fileType === "view"
  ) {
    return true;
  }
  return key.fileType === "external_doc" && key.fileName.startsWith("docs/");
}

function HeaderCell({ children }: { children: ReactNode }) {
  return (
    <th className="px-2 py-1.5 text-left font-medium text-[var(--g5-text2)]">
      {children}
    </th>
  );
}

function Row({
  entry,
  expanded,
  onToggle,
  workspaceId,
  onOpen,
  onTrace,
  onNavigate,
}: {
  entry: IntentLogEntry;
  expanded: boolean;
  onToggle: () => void;
  workspaceId: string;
  onOpen: (e: IntentLogEntry) => void;
  onTrace: (e: IntentLogEntry) => void;
  onNavigate?: IntentLogLineageNavigate;
}) {
  const canTrace = entry.traceStatus === "traceable";
  const canOpen = canOpenEntry(entry);
  return (
    <>
      <tr className="border-b border-[var(--g5-stroke)] align-top hover:bg-[var(--g5-bg2)]">
        <td className="px-2 py-1.5">
          <button
            type="button"
            data-testid="intent-log-row-toggle"
            aria-label={expanded ? "Collapse row" : "Expand row"}
            onClick={onToggle}
            className="text-[var(--g5-text2)] hover:text-[var(--g5-text0)]"
          >
            {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        </td>
        <td className="whitespace-nowrap px-2 py-1.5 text-[var(--g5-text1)]">
          <span>{whenLabel(entry)}</span>
          <span className="ml-1 text-[10px] text-[var(--g5-text2)]">
            {entry.occurredAtSource === "git_blame"
              ? "git blame"
              : entry.occurredAtSource === "record"
                ? "record"
                : ""}
          </span>
        </td>
        <td className="w-[60%] min-w-[34rem] max-w-[84rem] px-2 py-1.5 text-[var(--g5-text0)]">
          <span className="line-clamp-3">{entry.contentPreview || "—"}</span>
        </td>
        <td className="whitespace-nowrap px-2 py-1.5 text-[var(--g5-text1)]">
          {entry.actor ?? "unknown"}
        </td>
        <td className="whitespace-nowrap px-2 py-1.5">
          <Chip state="info" text={entry.intentKind.replace("_", " ")} />
          {entry.artifactType &&
            (entry.artifactType === "design_doc" ||
              entry.artifactType === "project_doc") && (
              <span className="ml-1 text-[10px] text-[var(--g5-text2)]">
                {entry.artifactType.replace("_", " ")}
              </span>
            )}
        </td>
        <td className="max-w-xs truncate px-2 py-1.5 text-[var(--g5-text1)]">
          {entry.artifactPath ?? entry.rawSource.sessionId ?? "—"}
          {entry.blockId && (
            <span className="text-[var(--g5-text2)]"> #{entry.blockId}</span>
          )}
        </td>
        <td className="whitespace-nowrap px-2 py-1.5">
          <Chip
            state={TRACE_STATE[entry.traceStatus]}
            text={TRACE_LABEL[entry.traceStatus]}
          />
        </td>
        <td className="whitespace-nowrap px-2 py-1.5">
          <div className="flex items-center gap-1">
            <IconButton
              title={canTrace ? "Trace lineage" : "No grounding to trace"}
              onClick={() => canTrace && onTrace(entry)}
            >
              <Share2
                size={14}
                className={classNames(!canTrace && "opacity-30")}
              />
            </IconButton>
            <IconButton
              title={canOpen ? "Open artifact" : "No file to open"}
              onClick={() => canOpen && onOpen(entry)}
            >
              <ExternalLink
                size={14}
                className={classNames(!canOpen && "opacity-30")}
              />
            </IconButton>
            <IconButton
              title="Copy stable id"
              onClick={() => {
                void navigator.clipboard?.writeText(entry.id);
              }}
            >
              <Copy size={14} />
            </IconButton>
          </div>
        </td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={8} className="p-0">
            <IntentLogRowDetail
              entry={entry}
              workspaceId={workspaceId}
              onNavigate={onNavigate}
            />
          </td>
        </tr>
      )}
    </>
  );
}

export function IntentLogTableView({
  rows,
  group,
  workspaceId,
  onOpen,
  onTrace,
  onNavigate,
}: IntentLogTableViewProps) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(
    new Set(),
  );

  const toggle = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  if (rows.length === 0) {
    return (
      <div
        data-testid="intent-log-empty"
        className="p-6 text-center text-sm italic text-[var(--g5-text2)]"
      >
        No intent records match the current filters.
      </div>
    );
  }

  const grouped = group && group !== "none";
  const groups: Array<[string, IntentLogEntry[]]> = grouped
    ? Object.entries(
        rows.reduce<Record<string, IntentLogEntry[]>>((acc, entry) => {
          const key = groupKey(entry, group);
          (acc[key] ??= []).push(entry);
          return acc;
        }, {}),
      )
    : [["", rows]];

  return (
    <div className="overflow-x-auto" data-testid="intent-log-table">
      <table className="w-full border-collapse text-xs">
        <thead>
          <tr className="border-b border-[var(--g5-stroke)]">
            <HeaderCell> </HeaderCell>
            <HeaderCell>When</HeaderCell>
            <HeaderCell>What</HeaderCell>
            <HeaderCell>Who</HeaderCell>
            <HeaderCell>Kind</HeaderCell>
            <HeaderCell>File</HeaderCell>
            <HeaderCell>Trace</HeaderCell>
            <HeaderCell>Actions</HeaderCell>
          </tr>
        </thead>
        <tbody>
          {groups.map(([key, groupRows]) => {
            const isCollapsed = collapsedGroups.has(key);
            return (
              <GroupBody
                key={key || "__all__"}
                groupName={key}
                grouped={!!grouped}
                isCollapsed={isCollapsed}
                rows={groupRows}
                expanded={expanded}
                onToggleGroup={() =>
                  setCollapsedGroups((prev) => {
                    const next = new Set(prev);
                    if (next.has(key)) next.delete(key);
                    else next.add(key);
                    return next;
                  })
                }
                onToggleRow={toggle}
                workspaceId={workspaceId}
                onOpen={onOpen}
                onTrace={onTrace}
                onNavigate={onNavigate}
              />
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function GroupBody({
  groupName,
  grouped,
  isCollapsed,
  rows,
  expanded,
  onToggleGroup,
  onToggleRow,
  workspaceId,
  onOpen,
  onTrace,
  onNavigate,
}: {
  groupName: string;
  grouped: boolean;
  isCollapsed: boolean;
  rows: IntentLogEntry[];
  expanded: Set<string>;
  onToggleGroup: () => void;
  onToggleRow: (id: string) => void;
  workspaceId: string;
  onOpen: (e: IntentLogEntry) => void;
  onTrace: (e: IntentLogEntry) => void;
  onNavigate?: IntentLogLineageNavigate;
}) {
  return (
    <>
      {grouped && (
        <tr className="bg-[var(--g5-bg1)]">
          <td colSpan={8} className="px-2 py-1">
            <button
              type="button"
              data-testid="intent-log-group-toggle"
              onClick={onToggleGroup}
              className="flex items-center gap-1 font-medium text-[var(--g5-text1)]"
            >
              {isCollapsed ? (
                <ChevronRight size={12} />
              ) : (
                <ChevronDown size={12} />
              )}
              {groupName || "(unknown)"}
              <span className="text-[var(--g5-text2)]">({rows.length})</span>
            </button>
          </td>
        </tr>
      )}
      {!isCollapsed &&
        rows.map((entry) => (
          <Row
            key={entry.id}
            entry={entry}
            expanded={expanded.has(entry.id)}
            onToggle={() => onToggleRow(entry.id)}
            workspaceId={workspaceId}
            onOpen={onOpen}
            onTrace={onTrace}
            onNavigate={onNavigate}
          />
        ))}
    </>
  );
}
