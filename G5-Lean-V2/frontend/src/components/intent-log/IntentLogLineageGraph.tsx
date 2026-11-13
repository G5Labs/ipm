// =============================================================================
// IntentLogLineageGraph — expanded-row adapter for the circular lineage preview.
// Spec: g5/client/workspace-page/intent-log-table-view [D3].
// Uses only existing grounding trace read endpoints; it does not infer edges.
// =============================================================================

import { useMemo } from "react";
import { AlertTriangle } from "lucide-react";

import type { IntentLogEntry } from "../../hooks/useIntentLog";
import {
  useTraceChildren,
  useTraceParents,
  type TraceRequest,
  type TraceResult,
} from "../../hooks/useGrounding";
import { LineageMiniGraph, type LineageRenderNode } from "./LineageMiniGraph";

export interface IntentLogLineageTarget {
  fileType: string;
  fileName: string | null;
  recordId: string;
  workspaceId?: string;
  repoId?: string;
}

export type IntentLogLineageNavigate = (target: IntentLogLineageTarget) => void;

interface IntentLogLineageGraphProps {
  entry: IntentLogEntry;
  workspaceId: string;
  onNavigate?: IntentLogLineageNavigate;
}

function colorFor(fileType: string): string {
  switch (fileType) {
    case "ontology":
      return "var(--g5-info)";
    case "view":
      return "var(--g5-ok)";
    case "code":
      return "var(--g5-active)";
    case "chat":
      return "var(--g5-warn)";
    case "external_doc":
      return "var(--g5-info)";
    default:
      return "var(--g5-text2)";
  }
}

function basename(path: string | null | undefined, fallback: string): string {
  if (!path) return fallback;
  const normalized = path
    .replace(/^\.?semlift\/specs\//, "")
    .replace(/^\.semlift\/specs\//, "")
    .replace(/^views\//, "")
    .replace(/^docs\//, "")
    .replace(/\.semlift\.md$/, "")
    .replace(/\.pm_view\.md$/, "")
    .replace(/\.test_view\.md$/, "");
  return normalized.split("/").pop() ?? fallback;
}

function recordLabel(result: TraceResult): string {
  if (result.codeBlockName) return "fn";
  return result.intentRecordNumber ?? "record";
}

/** Strip chat scaffolding so titles read cleanly. */
function cleanContent(content: string | null | undefined): string {
  if (!content) return "";
  return content
    .replace(/^Chat context:[\s\S]*?\n-+\nUser input:\s*/i, "")
    .replace(/^Chat context:[\s\S]*?User input:\s*/i, "")
    .replace(/^(?:User|Human) input:\s*/i, "")
    .replace(/^context:[\s\S]*?,\s*user instruction:\s*["“]?/i, "")
    .replace(/["”]\s*$/, "")
    .trim();
}

/** Short title for a node: server canonical-extraction metadata first
 * (D10 — the client never parses `content` for labels), then bounded
 * scaffolding-stripped content, then the fallback. */
function titleFrom(
  title: string | null | undefined,
  blockLabel: string | null | undefined,
  content: string | null | undefined,
  fallback: string,
): string {
  const clamp = (s: string) => (s.length > 44 ? `${s.slice(0, 42)}…` : s);
  if (title && title.trim()) return clamp(title.trim());
  if (blockLabel && blockLabel.trim()) return clamp(blockLabel.trim());
  const cleaned = cleanContent(content);
  if (!cleaned) return fallback;
  const first = cleaned.split(/[.:;—\n]/)[0]?.trim() || cleaned;
  return clamp(first);
}

/** Human-readable kind for the inspector header. */
function kindName(label: string, fileType: string): string {
  const prefix = label.trim().charAt(0).toUpperCase();
  if (prefix === "I") return "Intent";
  if (prefix === "D") return "Design";
  if (prefix === "Q") return "Question";
  if (fileType === "code") return "Code block";
  if (fileType === "chat") return "Chat";
  return "Block";
}

function centerLabel(entry: IntentLogEntry): string {
  if (entry.traceKey?.fileType === "code") return "fn";
  return entry.blockId ?? entry.traceKey?.id ?? "record";
}

function resultRecordId(result: TraceResult): string {
  return (
    result.intentRecordNumber ?? result.codeBlockName ?? result.intentRecordId
  );
}

function traceResultNode(
  result: TraceResult,
  depth: number,
  idPrefix: string,
): LineageRenderNode {
  const file = basename(
    result.fileName,
    result.fileType === "chat" ? "chat" : "source",
  );
  return {
    id: `${idPrefix}:${result.intentRecordId}`,
    depth,
    label: recordLabel(result),
    sublabel: titleFrom(result.title, result.blockLabel, result.content, file),
    colorVar: colorFor(result.fileType),
    content: result.content,
    fileName: result.fileName,
    kindName: kindName(recordLabel(result), result.fileType),
    // Chat / supervisor-decision records have no openable I/D markdown block.
    navigable: result.fileType !== "chat",
  };
}

export function IntentLogLineageGraph({
  entry,
  workspaceId,
  onNavigate,
}: IntentLogLineageGraphProps) {
  const request = useMemo<TraceRequest | null>(() => {
    if (!entry.traceKey || entry.traceStatus !== "traceable") return null;
    return {
      workspaceId,
      repoId: entry.rawSource.repoId,
      fileType: entry.traceKey.fileType,
      fileName: entry.traceKey.fileName,
      intentRecordNumber: entry.blockId ?? entry.traceKey.id,
    };
  }, [
    entry.blockId,
    entry.rawSource.repoId,
    entry.traceKey,
    entry.traceStatus,
    workspaceId,
  ]);

  const parents = useTraceParents(request);
  const children = useTraceChildren(request);

  const centerId = `center:${entry.id}`;
  const centerTarget = useMemo<IntentLogLineageTarget | null>(() => {
    if (!entry.traceKey) return null;
    return {
      fileType: entry.traceKey.fileType,
      fileName: entry.traceKey.fileName,
      recordId: entry.blockId ?? entry.traceKey.id,
      workspaceId,
      repoId: entry.rawSource.repoId,
    };
  }, [entry.blockId, entry.rawSource.repoId, entry.traceKey, workspaceId]);

  const graph = useMemo(() => {
    const parentResults = parents.status === "ok" ? parents.data.results : [];
    const childResults = children.status === "ok" ? children.data.results : [];

    const centerFile = basename(
      entry.traceKey?.fileName ?? entry.rawSource.sessionId,
      entry.intentKind === "chat" ? "chat" : "intent",
    );
    const centerNode: LineageRenderNode = {
      id: centerId,
      depth: 0,
      label: centerLabel(entry),
      sublabel: titleFrom(
        entry.title,
        entry.blockLabel,
        entry.content,
        centerFile,
      ),
      colorVar: colorFor(entry.traceKey?.fileType ?? entry.intentKind),
      content: entry.content,
      fileName: entry.traceKey?.fileName ?? entry.artifactPath,
      kindName: kindName(
        centerLabel(entry),
        entry.traceKey?.fileType ?? entry.intentKind,
      ),
      navigable: (entry.traceKey?.fileType ?? entry.intentKind) !== "chat",
    };

    const parentNodes = parentResults.map((result) =>
      traceResultNode(result, -1, "parent"),
    );
    const childNodes = childResults.map((result) =>
      traceResultNode(result, 1, "child"),
    );

    const lookup: Record<string, IntentLogLineageTarget> = {};
    if (centerTarget) lookup[centerId] = centerTarget;
    for (const result of parentResults) {
      lookup[`parent:${result.intentRecordId}`] = {
        fileType: result.fileType,
        fileName: result.fileName,
        recordId: resultRecordId(result),
        workspaceId: result.workspaceId,
        repoId: result.repoId,
      };
    }
    for (const result of childResults) {
      lookup[`child:${result.intentRecordId}`] = {
        fileType: result.fileType,
        fileName: result.fileName,
        recordId: resultRecordId(result),
        workspaceId: result.workspaceId,
        repoId: result.repoId,
      };
    }

    return {
      nodes: [...parentNodes, centerNode, ...childNodes],
      edges: [
        ...parentNodes.map((node) => ({ from: node.id, to: centerId })),
        ...childNodes.map((node) => ({ from: centerId, to: node.id })),
      ],
      ancestorCount: parentResults.length,
      descendantCount: childResults.length,
      lookup,
    };
  }, [centerId, centerTarget, children, entry, parents]);

  if (!request) return null;

  if (parents.status === "error" || children.status === "error") {
    return (
      <div className="flex items-center gap-2 rounded-[var(--g5-radius-md)] border border-[var(--g5-warn)] bg-[color-mix(in_srgb,var(--g5-warn)_10%,transparent)] px-3.5 py-3 text-[11.5px] text-[var(--g5-text1)]">
        <AlertTriangle className="h-3.5 w-3.5 text-[var(--g5-warn)]" />
        <span>Intent graph unavailable from the trace service.</span>
      </div>
    );
  }

  return (
    <LineageMiniGraph
      key={centerId}
      centerId={centerId}
      nodes={graph.nodes}
      edges={graph.edges}
      ancestorCount={graph.ancestorCount}
      descendantCount={graph.descendantCount}
      loading={parents.status === "loading" || children.status === "loading"}
      maxHeight={560}
      focus
      onJump={
        onNavigate
          ? (id) => {
              const target = graph.lookup[id];
              if (target) onNavigate(target);
            }
          : undefined
      }
    />
  );
}
