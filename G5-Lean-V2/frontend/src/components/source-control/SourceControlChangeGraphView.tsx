import {
  AlertTriangle,
  ChevronRight,
  CircleAlert,
  ExternalLink,
  File,
  FileCode2,
  FileText,
  Maximize2,
  MessageSquareText,
  Minus,
  Network,
  Plus,
  RotateCcw,
  Shapes,
  X,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  useSourceControlChangeGraph,
  useSourceControlChangeGraphNodeDetails,
} from "../../hooks/useSourceControlChangeGraph";
import { useWorkspaceNames } from "../../hooks/useWorkspaceNames";
import { ApiError } from "../../lib/api";
import { formatDiffGraphTooLargeMessage } from "../../lib/changeGraphErrors";
import type {
  AsyncState,
  ChangeGraphColumn,
  ChangeGraphRelationship,
  SourceControlChangeGraphComparison,
  SourceControlChangeGraphEdge,
  SourceControlChangeGraphEvidence,
  SourceControlChangeGraphNode,
  SourceControlChangeGraphNodeDetails,
  SourceControlChangeGraphRange,
  SourceControlChangeGraphResponse,
  SourceControlChangeGraphRoute,
} from "../../lib/types";
import { labelWorkspaceFolderInPath } from "../../lib/utils";
import { AnchoredPopover } from "../editor/AnchoredPopover";
import { classNames } from "../ui/UI";
import {
  ChangeGraphCard,
  ChangeGraphChangeBadge,
  ChangeGraphControl,
  ChangeGraphHumanBadge,
  ChangeGraphLaneHeading,
  type ChangeGraphCardFocusState,
} from "./ChangeGraphPrimitives";
import { SourceControlChangeGraphWhatChanged } from "./SourceControlChangeGraphWhatChanged";
import {
  CHANGE_GRAPH_ZOOM_CONTROL_STEP,
  useChangeGraphViewport,
} from "./useChangeGraphViewport";

const COLUMN_ORDER: ChangeGraphColumn[] = [
  "intents",
  "ontology",
  "source_code",
];
const COLUMN_LABELS: Record<ChangeGraphColumn, string> = {
  intents: "Intents",
  ontology: "Ontology",
  source_code: "Source Code",
};
const COLUMN_ACCENTS: Record<ChangeGraphColumn, string> = {
  intents: "var(--g5-graph-intent)",
  ontology: "var(--g5-graph-concept)",
  source_code: "var(--g5-graph-source)",
};
const ARTIFACT_ICONS: Record<string, LucideIcon> = {
  conversation: MessageSquareText,
  document: FileText,
  view_document: FileText,
  design_plan: Network,
  ontology: Shapes,
  source_file: FileCode2,
};
/** Authored link kinds are citations, not dependencies (g5.diff-graph.relationships). */
const RELATIONSHIP_ORDER: ChangeGraphRelationship[] = [
  "reference",
  "expands",
  "provenance",
  "implementation",
  "project_document",
];
const RELATIONSHIP_LABELS: Record<ChangeGraphRelationship, string> = {
  reference: "Reference",
  expands: "Expands",
  provenance: "Provenance",
  implementation: "Implementation",
  project_document: "Project document",
};
const RELATIONSHIP_DASHES: Record<
  ChangeGraphRelationship | "mixed",
  string | undefined
> = {
  reference: undefined,
  expands: undefined,
  provenance: "6 4",
  implementation: "2 3",
  project_document: "8 3 2 3",
  mixed: undefined,
};
const RELATIONSHIP_WIDTH_BONUS: Record<
  ChangeGraphRelationship | "mixed",
  number
> = {
  reference: 0,
  expands: 1.25,
  provenance: 0,
  implementation: 0,
  project_document: 0,
  mixed: 0,
};
const COMMITTED_REASON_COPY: Record<string, string> = {
  not_owner: "only the workspace owner sees saved edits",
  vm_not_running: "the workspace VM is not running",
  unpublished_commits:
    "local commits are not published yet; ontology files are still current",
  status_unavailable: "the workspace status is unavailable",
  report_unavailable: "saved edits could not be read",
  workspace_synchronizing: "the workspace is synchronizing",
};
const TOP_LEVEL_ONTOLOGY_GROUP = "";
const INTENT_GROUP_ORDER = ["conversations", "documents", "other"] as const;
type IntentGroup = (typeof INTENT_GROUP_ORDER)[number];
const INTENT_GROUP_LABELS: Record<IntentGroup, string> = {
  documents: "Documents",
  conversations: "Chats",
  other: "Other",
};
const CARD_WIDTH = 236;
const COLUMN_WIDTH = 280;
const CARD_HEIGHT = 112;
const ROW_GAP = 16;
const EMPTY_LANE_HEIGHT = 56;
const WORLD_PADDING = 24;
const COLUMN_HEADER_HEIGHT = 40;
const COLUMN_CONTENT_GAP = 16;
const INTENT_FIRST_GROUP_GAP = 16;
const INTENT_GROUP_GAP = 24;
const INTENT_GROUP_HEADER_HEIGHT = 20;
const INTENT_GROUP_CARD_GAP = 12;
const NODE_DETAILS_SECTION_HEADING_CLASS =
  "text-[10px] font-semibold uppercase tracking-[0.08em] text-[var(--g5-text1)]";
const RELATIONSHIP_CONTEXT_LIMIT = 256;
const GRAPH_CONNECTION_LIMIT = 1_024;
const GRAPH_EVIDENCE_LIMIT = 4_096;
const LINK_TARGET_LIMIT = 1_024;

const MATCHING_DIAGNOSTIC_CODES = new Set<string>();
const OWNERSHIP_DIAGNOSTIC_CODES = new Set<string>();
const SUPPORTING_RECORD_DIAGNOSTIC_CODES = new Set<string>();
const BUDGET_DIAGNOSTIC_LIMITS = new Map<string, string>([
  [
    "relationship_context_budget_reached",
    `Showing up to ${RELATIONSHIP_CONTEXT_LIMIT.toLocaleString("en-US")} related context artifacts.`,
  ],
  [
    "edge_budget_reached",
    `Showing up to ${GRAPH_CONNECTION_LIMIT.toLocaleString("en-US")} connections.`,
  ],
  [
    "evidence_budget_reached",
    `Showing up to ${GRAPH_EVIDENCE_LIMIT.toLocaleString("en-US")} evidence rows.`,
  ],
  [
    "link_target_budget_reached",
    `Showing links to up to ${LINK_TARGET_LIMIT.toLocaleString("en-US")} unchanged files.`,
  ],
]);
// Each refused file already carries a visible "Cannot compare" notice.
const INFORMATIONAL_DIAGNOSTIC_CODES = new Set(["ontology_comparison_refused"]);

type ProjectionDiagnosticDetail = {
  id: string;
  text: string;
};
type ProjectionDiagnosticDisclosure = {
  introduction: string;
  details: ProjectionDiagnosticDetail[];
};
type ProjectionDiagnosticDisclosureAnchor = {
  element: HTMLButtonElement;
  graphKey: string;
};

type GraphPosition = { x: number; y: number };
type GraphEdgeFocusState = "default" | "active" | "highlighted" | "dimmed";
type GraphEdgeLayer = "background" | "foreground";
type GraphFocus = {
  selectedNodeId: string;
  neighborNodeIds: ReadonlySet<string>;
  activeEdgeIds: ReadonlySet<string>;
  highlightedEdgeId: string | null;
};
type EvidenceSelection = {
  evidenceId: string;
  edgeId: string;
};
type FocusedEvidence = EvidenceSelection & {
  revision: string;
};
type IncidentEvidence = {
  evidence: SourceControlChangeGraphEvidence;
  edgeId: string;
  sourceNode?: SourceControlChangeGraphNode;
  targetNode?: SourceControlChangeGraphNode;
};

function projectionDiagnosticDisclosure(
  graph: SourceControlChangeGraphResponse,
): ProjectionDiagnosticDisclosure | null {
  const counts = new Map<string, number>();
  for (const diagnostic of graph.diagnostics) {
    if (diagnostic.count <= 0) continue;
    counts.set(
      diagnostic.code,
      (counts.get(diagnostic.code) ?? 0) + diagnostic.count,
    );
  }

  const countCodes = (codes: ReadonlySet<string>) =>
    [...codes].reduce((sum, code) => sum + (counts.get(code) ?? 0), 0);
  const matchingCount = countCodes(MATCHING_DIAGNOSTIC_CODES);
  const ownershipCount = countCodes(OWNERSHIP_DIAGNOSTIC_CODES);
  const supportingRecordCount = countCodes(SUPPORTING_RECORD_DIAGNOSTIC_CODES);
  const budgetDetails = [...BUDGET_DIAGNOSTIC_LIMITS.entries()]
    .filter(([code]) => (counts.get(code) ?? 0) > 0)
    .map(([code, text]) => ({ id: code, text }));

  const handledCodes = new Set([
    ...MATCHING_DIAGNOSTIC_CODES,
    ...OWNERSHIP_DIAGNOSTIC_CODES,
    ...SUPPORTING_RECORD_DIAGNOSTIC_CODES,
    ...BUDGET_DIAGNOSTIC_LIMITS.keys(),
    ...INFORMATIONAL_DIAGNOSTIC_CODES,
  ]);
  const unknownCount = [...counts.entries()].reduce(
    (sum, [code, count]) => sum + (handledCodes.has(code) ? 0 : count),
    0,
  );
  const hasMatchingCause =
    matchingCount > 0 || ownershipCount > 0 || supportingRecordCount > 0;
  const hasLimitCause = graph.truncated || budgetDetails.length > 0;
  if (!hasMatchingCause && !hasLimitCause && unknownCount === 0) return null;

  let introduction: string;
  if (hasMatchingCause && hasLimitCause) {
    introduction =
      "Some relationships or supporting evidence couldn’t be matched, and some weren’t shown because this graph reached its display limits.";
  } else if (hasMatchingCause) {
    introduction =
      "Some relationships or supporting evidence couldn’t be matched.";
  } else if (hasLimitCause) {
    introduction =
      "Some relationships or supporting evidence weren’t shown because this graph reached its display limits.";
  } else {
    introduction =
      "Some relationship or supporting evidence details couldn’t be displayed.";
  }

  const details: ProjectionDiagnosticDetail[] = [];
  if (matchingCount > 0) {
    details.push({
      id: "matching",
      text: `${matchingCount.toLocaleString("en-US")} ${matchingCount === 1 ? "relationship" : "relationships"} couldn’t be matched.`,
    });
  }
  if (ownershipCount > 0) {
    details.push({
      id: "ownership",
      text: `${ownershipCount.toLocaleString("en-US")} ${ownershipCount === 1 ? "relationship" : "relationships"} couldn’t be assigned to a single repository.`,
    });
  }
  if (supportingRecordCount > 0) {
    details.push({
      id: "supporting-records",
      text: `${supportingRecordCount.toLocaleString("en-US")} supporting ${supportingRecordCount === 1 ? "record" : "records"} couldn’t be read or connected to changed artifacts.`,
    });
  }
  details.push(...budgetDetails);
  if (hasLimitCause && budgetDetails.length === 0) {
    details.push({
      id: "truncated",
      text: "Additional graph details were limited.",
    });
  }
  if (unknownCount > 0) {
    details.push({
      id: "unknown",
      text: `${unknownCount.toLocaleString("en-US")} additional relationship or supporting-evidence ${unknownCount === 1 ? "detail" : "details"} couldn’t be displayed.`,
    });
  }

  return { introduction, details };
}

function evidenceIncludesSelection(
  evidence: IncidentEvidence[],
  selection: EvidenceSelection,
): boolean {
  return evidence.some(
    (item) =>
      item.edgeId === selection.edgeId &&
      item.evidence.id === selection.evidenceId,
  );
}

function workspaceComparisonErrorDetail(error: ApiError | null): string | null {
  if (!error) return null;
  if (error.is("DIFF_GRAPH_TOO_LARGE")) {
    const detail = formatDiffGraphTooLargeMessage(error, {
      subject: "This comparison",
      fallback:
        "This complete workspace comparison exceeds the supported size.",
    });
    return detail;
  }
  if (!error.is("DIFF_GRAPH_UNAVAILABLE")) return error.message;
  switch (error.details.reason) {
    case "no_common_ancestor":
      return "The workspace and its parent have no common Git ancestor.";
    case "multiple_merge_bases":
      return "The workspace and its parent have multiple merge bases. This comparison does not yet support that history.";
    case "parent_unavailable":
      return "The parent workspace is unavailable or you do not have access to it.";
    case "repository_set_mismatch":
      return "The workspace and its parent have different repository membership.";
    case "missing_ref":
      return "A selected repository branch is missing.";
    case "not_forked":
      return "The All changes comparison requires a parent workspace.";
    case "missing_accepted_head":
      return "One or more workspace repositories have no accepted commit yet.";
    case "uncommitted_unsupported":
      return "Uncommitted only is not available yet. All changes includes saved uncommitted edits.";
    case "base_workspace":
      return "Diff Graph is not available for Base workspaces.";
    default:
      return error.message;
  }
}

/** Where an ontology file or changed statement opens (g5.diff-graph.navigation). */
export interface ChangeGraphOntologyTarget {
  route: SourceControlChangeGraphRoute;
  /** A changed statement; absent for the whole file. */
  statementId?: string;
  /** The statement (or, without one, the file) was deleted in this comparison. */
  deleted: boolean;
}

export interface SourceControlChangeGraphViewProps {
  workspaceId: string;
  defaultComparison?: SourceControlChangeGraphComparison;
  onOpenDocument?: (route: SourceControlChangeGraphRoute) => void;
  onOpenDiff?: (route: SourceControlChangeGraphRoute) => void;
  onOpenOntology?: (target: ChangeGraphOntologyTarget) => void;
}

export const SourceControlChangeGraphView: React.FC<
  SourceControlChangeGraphViewProps
> = (props) => {
  // Uncommitted only is not offered until it works (g5.diff-graph.modes).
  const defaultComparison = "workspace";
  return (
    <SourceControlChangeGraphViewContent
      key={`${props.workspaceId}:${defaultComparison}`}
      {...props}
      defaultComparison={defaultComparison}
    />
  );
};

const SourceControlChangeGraphViewContent: React.FC<
  SourceControlChangeGraphViewProps
> = ({
  workspaceId,
  defaultComparison = "workspace",
  onOpenDocument,
  onOpenDiff,
  onOpenOntology,
}) => {
  // All changes is the only comparison offered (g5.diff-graph.modes).
  const comparison: SourceControlChangeGraphComparison = defaultComparison;
  const [humanIntent, setHumanIntent] = useState(false);
  const state = useSourceControlChangeGraph(
    workspaceId,
    comparison,
    humanIntent,
  );
  const workspaceNamesById = useWorkspaceNames();
  const queryClient = useQueryClient();
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [focusedEvidence, setFocusedEvidence] =
    useState<FocusedEvidence | null>(null);
  const [drawerHeight, setDrawerHeight] = useState(280);
  const [diagnosticDisclosureAnchor, setDiagnosticDisclosureAnchor] =
    useState<ProjectionDiagnosticDisclosureAnchor | null>(null);
  const graph = state.status === "ok" ? state.data : null;
  const graphError =
    state.status === "error" && state.error instanceof ApiError
      ? state.error
      : null;
  const currentWorkspaceComparisonUnavailable =
    comparison === "workspace" && graphError?.is("DIFF_GRAPH_UNAVAILABLE");
  const workspaceComparisonUnavailable = currentWorkspaceComparisonUnavailable;
  const workspaceComparisonTooLarge =
    comparison === "workspace" && graphError?.is("DIFF_GRAPH_TOO_LARGE");
  const comparisonErrorDetail = workspaceComparisonErrorDetail(graphError);
  const nodes = useMemo(() => graph?.nodes ?? [], [graph]);
  const edges = useMemo(() => graph?.edges ?? [], [graph]);
  const diagnosticDisclosure = useMemo(
    () => (graph ? projectionDiagnosticDisclosure(graph) : null),
    [graph],
  );
  const diagnosticDisclosureGraphKey = graph
    ? `${comparison}:${humanIntent}:${graph.revision}`
    : null;
  const diagnosticDisclosureAnchorElement =
    diagnosticDisclosure &&
    diagnosticDisclosureGraphKey &&
    diagnosticDisclosureAnchor?.graphKey === diagnosticDisclosureGraphKey &&
    diagnosticDisclosureAnchor.element.isConnected
      ? diagnosticDisclosureAnchor.element
      : null;
  const dismissDiagnosticDisclosure = useCallback(
    () => setDiagnosticDisclosureAnchor(null),
    [],
  );
  const { positions, groupTops, emptyLaneTops, worldHeight } = useMemo(
    () => layoutNodes(nodes),
    [nodes],
  );
  const worldWidth = COLUMN_WIDTH * COLUMN_ORDER.length;
  const clearSelection = useCallback(() => {
    setFocusedEvidence(null);
    setSelectedNodeId(null);
  }, []);
  const { viewportRef, scale, changeScale, fitGraph, viewportPointerHandlers } =
    useChangeGraphViewport({
      worldWidth,
      worldHeight,
      onBackgroundClick: clearSelection,
    });
  const selectedNode = nodes.find((node) => node.id === selectedNodeId) ?? null;
  const focusedEdgeId = useMemo(() => {
    if (
      !focusedEvidence ||
      focusedEvidence.revision !== graph?.revision ||
      !selectedNode
    ) {
      return null;
    }
    const edge = edges.find(
      (item) =>
        item.id === focusedEvidence.edgeId &&
        (item.sourceNodeId === selectedNode.id ||
          item.targetNodeId === selectedNode.id) &&
        item.evidence.some(
          (evidence) => evidence.id === focusedEvidence.evidenceId,
        ),
    );
    return edge?.id ?? null;
  }, [edges, focusedEvidence, graph?.revision, selectedNode]);
  const graphFocus = useMemo(
    () => deriveGraphFocus(selectedNode?.id ?? null, edges, focusedEdgeId),
    [edges, focusedEdgeId, selectedNode?.id],
  );

  const retry = useCallback(() => {
    queryClient.invalidateQueries({
      queryKey: [
        "source-control-change-graph",
        workspaceId,
        comparison,
        humanIntent,
      ],
    });
  }, [comparison, humanIntent, queryClient, workspaceId]);

  // All changes turns the "Human intent" lens off; the lens layers over All
  // changes. Either switch resets scoped selection; re-selecting is a no-op.
  const selectAllChanges = useCallback(() => {
    if (!humanIntent) return;
    setHumanIntent(false);
    setFocusedEvidence(null);
    setSelectedNodeId(null);
  }, [humanIntent]);

  const selectHumanIntent = useCallback(() => {
    if (humanIntent) return;
    setHumanIntent(true);
    setFocusedEvidence(null);
    setSelectedNodeId(null);
  }, [humanIntent]);

  const rangeCopy = graph?.range?.parentWorkspaceName
    ? `${graph.range.parentWorkspaceName} → Current workspace`
    : state.status === "loading"
      ? "Loading workspace range…"
      : null;

  return (
    <section
      className="flex h-full min-h-0 flex-col bg-[var(--g5-bg0)] text-[var(--g5-text0)]"
      data-testid="source-control-change-graph"
      aria-label="Source Control Diff Graph"
    >
      <header className="flex min-h-14 shrink-0 flex-wrap items-center justify-between gap-3 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2">
        <div className="flex min-w-0 basis-48 grow flex-col items-start gap-0.5 sm:flex-row sm:items-baseline sm:gap-2">
          <h2 className="shrink-0 text-sm font-semibold">Diff Graph</h2>
          {rangeCopy ? (
            <>
              <span
                aria-hidden="true"
                className="hidden shrink-0 text-[11px] text-[var(--g5-text2)] sm:inline"
              >
                ·
              </span>
              <p className="flex w-full min-w-0 max-w-full items-baseline gap-1.5 overflow-hidden sm:w-auto sm:flex-1">
                <span
                  className="min-w-0 flex-1 truncate text-[11px] text-[var(--g5-text1)]"
                  title={rangeCopy}
                >
                  {rangeCopy}
                </span>
                {graph ? (
                  <>
                    <span
                      aria-hidden="true"
                      className="shrink-0 text-[11px] text-[var(--g5-text2)]"
                    >
                      ·
                    </span>
                    <span
                      aria-label={`Revision ${graph.revision}`}
                      className="shrink-0 rounded font-mono text-[10px] text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
                      tabIndex={0}
                      title={`Revision ${graph.revision}`}
                    >
                      rev {graph.revision.slice(0, 8)}
                    </span>
                  </>
                ) : null}
              </p>
            </>
          ) : null}
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-3">
          <div
            className="flex rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-0.5"
            role="group"
            aria-label="Diff Graph comparison"
          >
            <button
              type="button"
              data-testid="change-graph-comparison-workspace"
              aria-pressed={!humanIntent}
              disabled={workspaceComparisonUnavailable}
              onClick={selectAllChanges}
              className={classNames(
                "rounded px-2 py-1 text-[11px] font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                comparison === "workspace"
                  ? "bg-[var(--g5-bg2)] text-[var(--g5-text0)]"
                  : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
                workspaceComparisonUnavailable &&
                  "cursor-not-allowed opacity-50",
              )}
            >
              All changes
            </button>
            <button
              type="button"
              data-testid="change-graph-comparison-human-intent"
              aria-pressed={humanIntent}
              disabled={workspaceComparisonUnavailable}
              onClick={selectHumanIntent}
              className={classNames(
                "rounded px-2 py-1 text-[11px] font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                humanIntent
                  ? "bg-[var(--g5-bg2)] text-[var(--g5-text0)]"
                  : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
                workspaceComparisonUnavailable &&
                  "cursor-not-allowed opacity-50",
              )}
            >
              Human intent
            </button>
          </div>
          <div className="flex items-center gap-1" aria-label="Graph controls">
            {diagnosticDisclosure ? (
              <button
                type="button"
                aria-label="Explain unavailable Diff Graph relationship details"
                aria-haspopup="dialog"
                aria-expanded={diagnosticDisclosureAnchorElement !== null}
                data-g5-anchored-popover-trigger
                data-testid="change-graph-diagnostic-disclosure"
                title="About unavailable Diff Graph relationship details"
                onClick={(event) => {
                  const anchor = event.currentTarget;
                  if (!diagnosticDisclosureGraphKey) return;
                  setDiagnosticDisclosureAnchor((current) =>
                    current?.element === anchor &&
                    current.graphKey === diagnosticDisclosureGraphKey
                      ? null
                      : {
                          element: anchor,
                          graphKey: diagnosticDisclosureGraphKey,
                        },
                  );
                }}
                className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
              >
                <CircleAlert size={14} aria-hidden="true" />
              </button>
            ) : null}
            <ChangeGraphControl
              label="Zoom out"
              onClick={() => changeScale(-CHANGE_GRAPH_ZOOM_CONTROL_STEP)}
            >
              <Minus size={14} />
            </ChangeGraphControl>
            <span className="w-12 text-center text-[10px] tabular-nums text-[var(--g5-text2)]">
              {Math.round(scale * 100)}%
            </span>
            <ChangeGraphControl
              label="Zoom in"
              onClick={() => changeScale(CHANGE_GRAPH_ZOOM_CONTROL_STEP)}
            >
              <Plus size={14} />
            </ChangeGraphControl>
            <ChangeGraphControl
              label="Fit graph"
              onClick={fitGraph}
              text="Fit graph"
            >
              <Maximize2 size={13} />
            </ChangeGraphControl>
          </div>
        </div>
      </header>
      {graph && (liveContentCopy(graph.range) || edges.length > 0) ? (
        <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-1 text-[10px] text-[var(--g5-text2)]">
          {liveContentCopy(graph.range) ? (
            <p
              data-testid="change-graph-live-content"
              className={classNames(
                "min-w-0",
                graph.range?.liveContent?.some(
                  (repo) =>
                    !repo.includesUncommitted &&
                    repo.reason !== "not_owner" &&
                    repo.reason !== "vm_not_running",
                ) && "text-[var(--g5-warn)]",
              )}
            >
              {liveContentCopy(graph.range)}
            </p>
          ) : null}
          {edges.length > 0 ? <RelationshipLegend /> : null}
        </div>
      ) : null}

      {diagnosticDisclosure && diagnosticDisclosureAnchorElement ? (
        <AnchoredPopover
          anchor={diagnosticDisclosureAnchorElement}
          ariaLabel="Diff Graph relationship details"
          onDismiss={dismissDiagnosticDisclosure}
          testId="change-graph-diagnostic-popover"
          width={360}
          className="space-y-2.5 p-3 text-xs text-[var(--g5-text1)]"
        >
          <p className="font-medium leading-relaxed text-[var(--g5-text0)]">
            {diagnosticDisclosure.introduction}
          </p>
          <ul className="space-y-1.5">
            {diagnosticDisclosure.details.map((detail) => (
              <li
                key={detail.id}
                className="flex items-start gap-2 leading-relaxed"
              >
                <span aria-hidden="true" className="text-[var(--g5-text2)]">
                  •
                </span>
                <span>{detail.text}</span>
              </li>
            ))}
          </ul>
          <p className="border-t border-[var(--g5-stroke)] pt-2 leading-relaxed text-[var(--g5-text2)]">
            Underlying files and saved changes are unaffected.
          </p>
        </AnchoredPopover>
      ) : null}

      <div className="flex min-h-0 flex-1 flex-col">
        <div
          ref={viewportRef}
          data-testid="change-graph-viewport"
          className="relative min-h-44 flex-1 cursor-grab overflow-auto overscroll-contain bg-[var(--g5-bg0)] active:cursor-grabbing"
          {...viewportPointerHandlers}
        >
          {state.status === "loading" ? (
            <GraphSkeleton />
          ) : state.status === "error" ? (
            <GraphMessage
              title={
                workspaceComparisonTooLarge
                  ? "All changes comparison is too large"
                  : workspaceComparisonUnavailable
                    ? "All changes unavailable"
                    : "Diff Graph could not be loaded"
              }
              detail={comparisonErrorDetail ?? state.error.message}
              action={
                workspaceComparisonUnavailable ||
                workspaceComparisonTooLarge ? null : (
                  <button
                    type="button"
                    data-testid="change-graph-retry"
                    onClick={retry}
                    className="inline-flex items-center gap-1 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-xs hover:bg-[var(--g5-bg2)]"
                  >
                    <RotateCcw size={12} /> Retry
                  </button>
                )
              }
            />
          ) : nodes.length === 0 ? (
            <GraphMessage
              title={
                humanIntent
                  ? "No human-authored changes"
                  : "No workspace changes"
              }
              detail={
                humanIntent
                  ? "No human-tagged intent blocks changed in this comparison."
                  : "The current workspace matches its parent baseline."
              }
            />
          ) : (
            <div
              data-testid="change-graph-footprint"
              className="relative select-none"
              style={{
                width: worldWidth * scale,
                height: worldHeight * scale,
                marginInline: "auto",
              }}
            >
              <div
                className="relative origin-top-left"
                style={{
                  width: worldWidth,
                  height: worldHeight,
                  transform: `scale(${scale})`,
                }}
              >
                <ColumnFrames height={worldHeight} />
                <GroupLabels groupTops={groupTops} />
                <EmptyLanePlaceholders emptyLaneTops={emptyLaneTops} />
                <GraphEdges
                  edges={edges}
                  positions={positions}
                  focus={graphFocus}
                  layer="background"
                />
                <MissingTargetStubs nodes={nodes} positions={positions} />
                {nodes.map((node) => {
                  const position = positions.get(node.id);
                  if (!position) return null;
                  return (
                    <SourceControlGraphCard
                      key={node.id}
                      node={node}
                      displayPath={
                        node.path
                          ? labelWorkspaceFolderInPath(
                              node.path,
                              workspaceNamesById,
                            )
                          : undefined
                      }
                      position={position}
                      focusState={cardFocusState(node.id, graphFocus)}
                      onSelect={() => {
                        if (node.id !== selectedNodeId) {
                          setFocusedEvidence(null);
                        }
                        setSelectedNodeId(node.id);
                      }}
                    />
                  );
                })}
                {graphFocus && graphFocus.activeEdgeIds.size > 0 && (
                  <GraphEdges
                    edges={edges}
                    positions={positions}
                    focus={graphFocus}
                    layer="foreground"
                  />
                )}
              </div>
            </div>
          )}
        </div>

        {selectedNode && (
          <NodeDetailsDrawer
            workspaceId={workspaceId}
            comparison={comparison}
            humanIntent={humanIntent}
            revision={graph!.revision}
            node={selectedNode}
            displayPath={
              selectedNode.path
                ? labelWorkspaceFolderInPath(
                    selectedNode.path,
                    workspaceNamesById,
                  )
                : undefined
            }
            nodes={nodes}
            edges={edges}
            focusedEvidence={focusedEdgeId === null ? null : focusedEvidence}
            onFocusEvidence={(selection) => {
              setFocusedEvidence((current) =>
                current?.evidenceId === selection.evidenceId &&
                current.edgeId === selection.edgeId &&
                focusedEdgeId !== null
                  ? null
                  : { ...selection, revision: graph!.revision },
              );
            }}
            evidenceTruncated={
              graph?.diagnostics.some(
                (diagnostic) => diagnostic.code === "evidence_budget_reached",
              ) ?? false
            }
            height={drawerHeight}
            onHeightChange={setDrawerHeight}
            onClose={() => {
              setFocusedEvidence(null);
              setSelectedNodeId(null);
            }}
            onOpenDocument={onOpenDocument}
            onOpenDiff={onOpenDiff}
            onOpenOntology={onOpenOntology}
          />
        )}
      </div>
    </section>
  );
};

/** Whether All changes includes saved uncommitted edits (g5.diff-graph.modes). */
function liveContentCopy(
  range: SourceControlChangeGraphRange | undefined,
): string | null {
  const repos = range?.liveContent ?? [];
  if (repos.length === 0) return null;
  if (repos.every((repo) => repo.includesUncommitted))
    return "Includes saved uncommitted edits.";
  const reasons = [...new Set(repos.map((repo) => repo.reason ?? ""))];
  if (
    repos.every((repo) => !repo.includesUncommitted) &&
    reasons.length === 1
  ) {
    const why = COMMITTED_REASON_COPY[reasons[0]];
    return `Committed changes only${why ? `: ${why}` : ""}.`;
  }
  const committed = repos
    .filter((repo) => !repo.includesUncommitted)
    .map((repo) => {
      const why = COMMITTED_REASON_COPY[repo.reason ?? ""];
      return `${repo.repoName} shows committed changes only${why ? ` (${why})` : ""}`;
    });
  // Every repository is committed-only, for different reasons.
  if (repos.every((repo) => !repo.includesUncommitted))
    return `Committed changes only: ${committed.join("; ")}.`;
  return `Includes saved uncommitted edits, except: ${committed.join("; ")}.`;
}

function edgeRelationship(
  edge: SourceControlChangeGraphEdge,
): ChangeGraphRelationship | "mixed" {
  const kinds = new Set(edge.evidence.map((item) => item.relationship));
  const [only] = [...kinds];
  return kinds.size === 1 && only in RELATIONSHIP_LABELS
    ? (only as ChangeGraphRelationship)
    : "mixed";
}

function RelationshipLegend() {
  return (
    <ul
      data-testid="change-graph-legend"
      aria-label="Link kinds"
      className="flex flex-wrap items-center gap-x-3 gap-y-1"
    >
      {RELATIONSHIP_ORDER.map((kind) => (
        <li key={kind} className="flex items-center gap-1">
          <svg width="22" height="6" aria-hidden="true">
            <line
              x1="0"
              y1="3"
              x2="22"
              y2="3"
              stroke="var(--g5-text1)"
              strokeWidth={1.5 + RELATIONSHIP_WIDTH_BONUS[kind]}
              strokeDasharray={RELATIONSHIP_DASHES[kind]}
            />
          </svg>
          {RELATIONSHIP_LABELS[kind]}
        </li>
      ))}
      <li className="text-[var(--g5-text2)]">
        Links cite their targets; they do not declare dependencies.
      </li>
    </ul>
  );
}

function MissingTargetStubs({
  nodes,
  positions,
}: {
  nodes: SourceControlChangeGraphNode[];
  positions: Map<string, GraphPosition>;
}) {
  const stubs = nodes.filter((node) => (node.missingTargets?.length ?? 0) > 0);
  if (stubs.length === 0) return null;
  return (
    <svg
      className="pointer-events-none absolute inset-0 z-10 h-full w-full"
      aria-hidden="true"
    >
      {stubs.map((node) => {
        const position = positions.get(node.id);
        if (!position) return null;
        const x = position.x + CARD_WIDTH;
        const y = position.y + CARD_HEIGHT * 0.75;
        return (
          <g key={node.id} data-testid="change-graph-missing-target-stub">
            <line
              x1={x}
              y1={y}
              x2={x + 26}
              y2={y}
              stroke="var(--g5-warn)"
              strokeWidth={1.5}
              strokeDasharray="3 3"
            />
            <circle
              cx={x + 31}
              cy={y}
              r={5}
              fill="var(--g5-bg0)"
              stroke="var(--g5-warn)"
              strokeWidth={1.5}
            />
          </g>
        );
      })}
    </svg>
  );
}

type GroupTop = {
  key: string;
  label: string;
  columnIndex: number;
  top: number;
};

function ontologyGroupLabel(group: string): string {
  return group === TOP_LEVEL_ONTOLOGY_GROUP ? "Top level" : group;
}

/** Intents group by kind and Ontology by folder; Source Code is one list. */
function columnGroups(
  column: ChangeGraphColumn,
  columnNodes: SourceControlChangeGraphNode[],
):
  | { key: string; label: string; nodes: SourceControlChangeGraphNode[] }[]
  | null {
  if (column === "intents") {
    return INTENT_GROUP_ORDER.map((group) => ({
      key: `intents:${group}`,
      label: INTENT_GROUP_LABELS[group],
      nodes: columnNodes.filter((node) => node.intentGroup === group),
    })).filter((group) => group.nodes.length > 0);
  }
  if (column === "ontology") {
    const folders = [
      ...new Set(
        columnNodes.map(
          (node) => node.ontologyGroup ?? TOP_LEVEL_ONTOLOGY_GROUP,
        ),
      ),
    ].sort((a, b) =>
      a === TOP_LEVEL_ONTOLOGY_GROUP
        ? -1
        : b === TOP_LEVEL_ONTOLOGY_GROUP
          ? 1
          : a.localeCompare(b),
    );
    return folders.map((folder) => ({
      key: `ontology:${folder}`,
      label: ontologyGroupLabel(folder),
      nodes: columnNodes.filter(
        (node) => (node.ontologyGroup ?? TOP_LEVEL_ONTOLOGY_GROUP) === folder,
      ),
    }));
  }
  return null;
}

function layoutNodes(nodes: SourceControlChangeGraphNode[]): {
  positions: Map<string, GraphPosition>;
  groupTops: GroupTop[];
  emptyLaneTops: Map<ChangeGraphColumn, number>;
  worldHeight: number;
} {
  const positions = new Map<string, GraphPosition>();
  const groupTops: GroupTop[] = [];
  const emptyLaneTops = new Map<ChangeGraphColumn, number>();
  let maxY = 160;
  COLUMN_ORDER.forEach((column, columnIndex) => {
    let y = COLUMN_HEADER_HEIGHT + COLUMN_CONTENT_GAP;
    const columnNodes = nodes.filter((node) => node.column === column);
    const groups = columnGroups(column, columnNodes);
    const place = (grouped: SourceControlChangeGraphNode[]) =>
      grouped.forEach((node, nodeIndex) => {
        positions.set(node.id, {
          x: WORLD_PADDING + columnIndex * COLUMN_WIDTH,
          y,
        });
        y += CARD_HEIGHT;
        if (nodeIndex < grouped.length - 1) y += ROW_GAP;
      });
    if (groups) {
      y = COLUMN_HEADER_HEIGHT + INTENT_FIRST_GROUP_GAP;
      groups.forEach((group, groupIndex) => {
        if (groupIndex > 0) y += INTENT_GROUP_GAP;
        groupTops.push({
          key: group.key,
          label: group.label,
          columnIndex,
          top: y,
        });
        y += INTENT_GROUP_HEADER_HEIGHT + INTENT_GROUP_CARD_GAP;
        place(group.nodes);
      });
    } else {
      place(columnNodes);
    }
    if (columnNodes.length === 0) {
      emptyLaneTops.set(column, y);
      y += EMPTY_LANE_HEIGHT;
    }
    maxY = Math.max(maxY, y + 24);
  });
  return { positions, groupTops, emptyLaneTops, worldHeight: maxY };
}

function ColumnFrames({ height }: { height: number }) {
  return (
    <>
      {COLUMN_ORDER.map((column, index) => (
        <div
          key={column}
          data-testid="change-graph-column"
          data-column={column}
          className="absolute top-0 border-r border-[var(--g5-stroke)]/35 bg-transparent"
          style={{
            left: index * COLUMN_WIDTH,
            width: COLUMN_WIDTH,
            height,
          }}
        >
          <ChangeGraphLaneHeading
            label={COLUMN_LABELS[column]}
            color={COLUMN_ACCENTS[column]}
            markerTestId="change-graph-lane-marker"
            className="sticky top-0 z-[60] bg-[var(--g5-bg0)]/90 px-4 text-[11px] backdrop-blur"
            style={{ height: COLUMN_HEADER_HEIGHT }}
          />
        </div>
      ))}
    </>
  );
}

function EmptyLanePlaceholders({
  emptyLaneTops,
}: {
  emptyLaneTops: Map<ChangeGraphColumn, number>;
}) {
  return (
    <>
      {COLUMN_ORDER.map((column, columnIndex) => {
        const top = emptyLaneTops.get(column);
        if (top === undefined) return null;
        return (
          <div
            key={column}
            data-testid="change-graph-empty-lane"
            data-column={column}
            aria-label={`${COLUMN_LABELS[column]}: no changed artifacts`}
            className="pointer-events-none absolute z-40 flex items-center justify-center gap-2 rounded-md border border-dashed border-[var(--g5-stroke)]/70 bg-[var(--g5-bg1)]/20 text-[10px] text-[var(--g5-text2)]"
            style={{
              left: WORLD_PADDING + columnIndex * COLUMN_WIDTH,
              top,
              width: CARD_WIDTH,
              height: EMPTY_LANE_HEIGHT,
            }}
          >
            <span
              aria-hidden="true"
              className="h-1.5 w-1.5 rounded-full"
              style={{ backgroundColor: COLUMN_ACCENTS[column] }}
            />
            No changed artifacts
          </div>
        );
      })}
    </>
  );
}

function GroupLabels({ groupTops }: { groupTops: GroupTop[] }) {
  return (
    <>
      {groupTops.map((group) => (
        <h4
          key={group.key}
          data-testid="change-graph-group-label"
          className="pointer-events-none absolute z-40 border-b border-[var(--g5-stroke)]/70 text-[11px] font-semibold uppercase leading-4 tracking-[0.08em] text-[var(--g5-text1)]"
          style={{
            left: WORLD_PADDING + group.columnIndex * COLUMN_WIDTH,
            top: group.top,
            width: CARD_WIDTH,
            height: INTENT_GROUP_HEADER_HEIGHT,
          }}
        >
          {group.label}
        </h4>
      ))}
    </>
  );
}

function deriveGraphFocus(
  selectedNodeId: string | null,
  edges: SourceControlChangeGraphEdge[],
  focusedEdgeId: string | null = null,
): GraphFocus | null {
  if (!selectedNodeId) return null;
  const neighborNodeIds = new Set<string>();
  const activeEdgeIds = new Set<string>();

  for (const edge of edges) {
    if (edge.sourceNodeId === selectedNodeId) {
      activeEdgeIds.add(edge.id);
      if (edge.targetNodeId !== selectedNodeId) {
        neighborNodeIds.add(edge.targetNodeId);
      }
    } else if (edge.targetNodeId === selectedNodeId) {
      activeEdgeIds.add(edge.id);
      neighborNodeIds.add(edge.sourceNodeId);
    }
  }
  return {
    selectedNodeId,
    neighborNodeIds,
    activeEdgeIds,
    highlightedEdgeId:
      focusedEdgeId && activeEdgeIds.has(focusedEdgeId) ? focusedEdgeId : null,
  };
}

function cardFocusState(
  nodeId: string,
  focus: GraphFocus | null,
): ChangeGraphCardFocusState {
  if (!focus) return "default";
  if (nodeId === focus.selectedNodeId) return "selected";
  if (focus.neighborNodeIds.has(nodeId)) return "neighbor";
  return "dimmed";
}

function edgeFocusState(
  edgeId: string,
  focus: GraphFocus | null,
): GraphEdgeFocusState {
  if (!focus) return "default";
  if (focus.highlightedEdgeId === edgeId) return "highlighted";
  return focus.activeEdgeIds.has(edgeId) ? "active" : "dimmed";
}

function displayArtifactLabel(node: SourceControlChangeGraphNode): string {
  return node.intentGroup === "other"
    ? "Other"
    : formatArtifactLabel(node.artifactType);
}

function GraphEdges({
  edges,
  positions,
  focus,
  layer,
}: {
  edges: SourceControlChangeGraphEdge[];
  positions: Map<string, GraphPosition>;
  focus: GraphFocus | null;
  layer: GraphEdgeLayer;
}) {
  const renderedEdges = edges
    .filter((edge) => {
      const state = edgeFocusState(edge.id, focus);
      return layer === "foreground"
        ? state === "active" || state === "highlighted"
        : state === "default" || state === "dimmed";
    })
    .sort((left, right) => {
      const rank = (edgeId: string) => {
        const state = edgeFocusState(edgeId, focus);
        return state === "highlighted" ? 2 : state === "active" ? 1 : 0;
      };
      return rank(left.id) - rank(right.id);
    });
  const markerPrefix = `change-graph-${layer}`;
  return (
    <svg
      data-testid={`change-graph-edge-layer-${layer}`}
      data-edge-layer={layer}
      className={classNames(
        "pointer-events-none absolute inset-0 h-full w-full",
        layer === "foreground" ? "z-30" : "z-10",
      )}
      aria-hidden="true"
    >
      <defs>
        <marker
          id={`${markerPrefix}-arrow`}
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth="5"
          markerHeight="5"
          orient="auto-start-reverse"
        >
          <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--g5-text2)" />
        </marker>
        <marker
          id={`${markerPrefix}-arrow-active`}
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth="5"
          markerHeight="5"
          orient="auto-start-reverse"
        >
          <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--g5-info)" />
        </marker>
        <marker
          id={`${markerPrefix}-arrow-dimmed`}
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth="5"
          markerHeight="5"
          orient="auto-start-reverse"
        >
          <path
            d="M 0 0 L 10 5 L 0 10 z"
            fill="var(--g5-text2)"
            fillOpacity={0.12}
          />
        </marker>
      </defs>
      {renderedEdges.map((edge) => {
        const source = positions.get(edge.sourceNodeId);
        const target = positions.get(edge.targetNodeId);
        if (!source || !target) return null;
        const focusState = edgeFocusState(edge.id, focus);
        const kind = edgeRelationship(edge);
        // Links may point back to an earlier column (provenance to intents).
        const leftward = target.x < source.x;
        const sx = leftward ? source.x : source.x + CARD_WIDTH;
        const sy = source.y + CARD_HEIGHT / 2;
        const tx = leftward ? target.x + CARD_WIDTH : target.x;
        const ty = target.y + CARD_HEIGHT / 2;
        const direction = leftward ? -1 : 1;
        const bend = Math.max(36, Math.abs(tx - sx) * 0.42) * direction;
        const path = `M ${sx} ${sy} C ${sx + bend} ${sy}, ${tx - bend} ${ty}, ${tx} ${ty}`;
        return (
          <g key={edge.id}>
            {focusState === "highlighted" && (
              <path
                d={path}
                fill="none"
                stroke="var(--g5-info)"
                strokeOpacity={0.22}
                strokeWidth={7}
                className="transition-[stroke,stroke-opacity,stroke-width] duration-150 motion-reduce:transition-none"
              />
            )}
            <path
              data-testid="change-graph-edge"
              data-edge-id={edge.id}
              data-relationship={kind}
              data-focus-state={focusState}
              strokeDasharray={RELATIONSHIP_DASHES[kind]}
              d={path}
              fill="none"
              stroke={
                focusState === "active" || focusState === "highlighted"
                  ? "var(--g5-info)"
                  : "var(--g5-text2)"
              }
              strokeOpacity={
                focusState === "highlighted"
                  ? 1
                  : focusState === "active"
                    ? 0.9
                    : focusState === "dimmed"
                      ? 0.12
                      : 0.5
              }
              strokeWidth={
                (focusState === "highlighted"
                  ? 3.5
                  : focusState === "active"
                    ? 2.5
                    : focusState === "dimmed"
                      ? 1.25
                      : 1.5) + RELATIONSHIP_WIDTH_BONUS[kind]
              }
              markerEnd={
                focusState === "active" || focusState === "highlighted"
                  ? `url(#${markerPrefix}-arrow-active)`
                  : focusState === "dimmed"
                    ? `url(#${markerPrefix}-arrow-dimmed)`
                    : `url(#${markerPrefix}-arrow)`
              }
              className="transition-[stroke,stroke-opacity,stroke-width] duration-150 motion-reduce:transition-none"
            />
          </g>
        );
      })}
    </svg>
  );
}

function SourceControlGraphCard({
  node,
  displayPath,
  position,
  focusState,
  onSelect,
}: {
  node: SourceControlChangeGraphNode;
  displayPath?: string;
  position: GraphPosition;
  focusState: ChangeGraphCardFocusState;
  onSelect: () => void;
}) {
  const ArtifactIcon = ARTIFACT_ICONS[node.artifactType] ?? File;
  return (
    <ChangeGraphCard
      ariaLabel={`${node.title}, ${displayArtifactLabel(node)}, ${formatLabel(node.changeState)}`}
      testId="change-graph-card"
      column={node.column}
      focusState={focusState}
      changeState={node.changeState}
      position={position}
      width={CARD_WIDTH}
      height={CARD_HEIGHT}
      accentColor={COLUMN_ACCENTS[node.column]}
      ArtifactIcon={ArtifactIcon}
      artifactLabel={displayArtifactLabel(node)}
      title={node.title}
      path={displayPath}
      additions={node.additions}
      deletions={node.deletions}
      notice={cardNotice(node)}
      onSelect={onSelect}
    />
  );
}

function cardNotice(node: SourceControlChangeGraphNode): string | undefined {
  if (node.comparisonWarning) return "Cannot compare statements";
  const missing = node.missingTargets?.length ?? 0;
  if (missing > 0)
    return `${missing} missing link ${missing === 1 ? "target" : "targets"}`;
  return undefined;
}

function NodeDetailsDrawer({
  workspaceId,
  comparison,
  humanIntent,
  revision,
  node,
  displayPath,
  nodes,
  edges,
  focusedEvidence,
  onFocusEvidence,
  evidenceTruncated,
  height,
  onHeightChange,
  onClose,
  onOpenDocument,
  onOpenDiff,
  onOpenOntology,
}: {
  workspaceId: string;
  comparison: SourceControlChangeGraphComparison;
  humanIntent: boolean;
  revision: string;
  node: SourceControlChangeGraphNode;
  displayPath?: string;
  nodes: SourceControlChangeGraphNode[];
  edges: SourceControlChangeGraphEdge[];
  focusedEvidence: EvidenceSelection | null;
  onFocusEvidence: (selection: EvidenceSelection) => void;
  evidenceTruncated: boolean;
  height: number;
  onHeightChange: (height: number) => void;
  onClose: () => void;
  onOpenDocument?: (route: SourceControlChangeGraphRoute) => void;
  onOpenDiff?: (route: SourceControlChangeGraphRoute) => void;
  onOpenOntology?: (target: ChangeGraphOntologyTarget) => void;
}) {
  const resizeRef = useRef<{ y: number; height: number } | null>(null);
  const queryClient = useQueryClient();
  const detailState = useSourceControlChangeGraphNodeDetails(
    workspaceId,
    comparison,
    revision,
    node.id,
    humanIntent,
  );
  const nodesById = useMemo(
    () => new Map(nodes.map((item) => [item.id, item])),
    [nodes],
  );
  const { changedEvidence, preExistingEvidence } = useMemo(() => {
    const evidence: IncidentEvidence[] = edges
      .filter(
        (edge) =>
          edge.sourceNodeId === node.id || edge.targetNodeId === node.id,
      )
      .flatMap((edge) =>
        edge.evidence.map((item) => ({
          evidence: item,
          edgeId: edge.id,
          sourceNode: nodesById.get(edge.sourceNodeId),
          targetNode: nodesById.get(edge.targetNodeId),
        })),
      );
    return {
      changedEvidence: evidence.filter(
        (item) => item.evidence.changeState !== "unchanged",
      ),
      preExistingEvidence: evidence.filter(
        (item) => item.evidence.changeState === "unchanged",
      ),
    };
  }, [edges, node.id, nodesById]);
  const action = secondaryAction(node, {
    onOpenDocument,
    onOpenDiff,
    onOpenOntology,
  });
  const retryDetails = useCallback(() => {
    void queryClient.invalidateQueries({
      queryKey: [
        "source-control-change-graph-node-details",
        workspaceId,
        comparison,
        revision,
        node.id,
        humanIntent,
      ],
      exact: true,
    });
  }, [comparison, humanIntent, node.id, queryClient, revision, workspaceId]);

  useEffect(() => {
    if (
      detailState.status === "error" &&
      detailState.error instanceof ApiError &&
      detailState.error.status === 409 &&
      detailState.error.code === "STALE_GRAPH_REVISION"
    ) {
      onClose();
    }
  }, [detailState, onClose]);

  useEffect(() => {
    const move = (event: PointerEvent) => {
      if (!resizeRef.current) return;
      onHeightChange(
        Math.min(
          520,
          Math.max(
            160,
            resizeRef.current.height + resizeRef.current.y - event.clientY,
          ),
        ),
      );
    };
    const up = () => {
      resizeRef.current = null;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
  }, [onHeightChange]);

  return (
    <aside
      className="relative min-h-40 shrink border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
      style={{ height }}
      aria-label="Node details"
      data-testid="change-graph-node-details"
    >
      <div
        role="separator"
        aria-label="Resize Node details"
        aria-orientation="horizontal"
        tabIndex={0}
        className="absolute -top-1.5 left-0 right-0 z-30 h-3 cursor-row-resize focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        onPointerDown={(event) => {
          resizeRef.current = { y: event.clientY, height };
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowUp")
            onHeightChange(Math.min(520, height + 20));
          if (event.key === "ArrowDown")
            onHeightChange(Math.max(160, height - 20));
        }}
      />
      <header className="flex h-11 items-center justify-between border-b border-[var(--g5-stroke)] px-3">
        <div className="min-w-0">
          <h3 className="text-xs font-semibold">Node details</h3>
          <p className="truncate text-[10px] text-[var(--g5-text2)]">
            {node.title}
          </p>
        </div>
        <div className="flex items-center gap-1">
          {action && (
            <button
              type="button"
              data-testid="change-graph-node-details-action"
              onClick={action.run}
              className="inline-flex items-center gap-1 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-xs hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
            >
              <ExternalLink size={12} /> {action.label}
            </button>
          )}
          <ChangeGraphControl label="Close Node details" onClick={onClose}>
            <X size={14} />
          </ChangeGraphControl>
        </div>
      </header>
      <div className="grid h-[calc(100%_-_2.75rem)] min-h-0 grid-cols-[minmax(220px,0.8fr)_minmax(300px,1.2fr)] overflow-hidden">
        <div
          data-testid="change-graph-node-details-left"
          className="overflow-auto border-r border-[var(--g5-stroke)] p-3 text-xs"
        >
          <DetailsRow label="Artifact" value={displayArtifactLabel(node)} />
          {displayPath && <DetailsRow label="Path" value={displayPath} mono />}
          <DetailsRow label="Change" value={formatLabel(node.changeState)} />
          {(node.additions !== undefined || node.deletions !== undefined) && (
            <DetailsRow
              label="Lines"
              value={`+${node.additions ?? 0} / −${node.deletions ?? 0}`}
            />
          )}
          {evidenceTruncated && (
            <p className="mt-4 flex items-start gap-1.5 rounded border border-[var(--g5-warn)]/35 bg-[var(--g5-warn)]/10 p-2 text-[10px] leading-4 text-[var(--g5-text1)]">
              <AlertTriangle
                size={12}
                className="mt-0.5 shrink-0 text-[var(--g5-warn)]"
              />
              <span>
                Some relationship evidence in this graph was truncated. Counts
                below reflect returned evidence rows.
              </span>
            </p>
          )}
          {node.comparisonWarning && (
            <p
              data-testid="change-graph-comparison-warning"
              className="mt-4 flex items-start gap-1.5 rounded border border-[var(--g5-warn)]/35 bg-[var(--g5-warn)]/10 p-2 text-[10px] leading-4 text-[var(--g5-text1)]"
            >
              <AlertTriangle
                size={12}
                className="mt-0.5 shrink-0 text-[var(--g5-warn)]"
              />
              <span>
                Cannot compare this file's statements, so none is classified.{" "}
                {node.comparisonWarning}
              </span>
            </p>
          )}
          {expectsSemanticDetails(node, workspaceId) &&
            !node.comparisonWarning && (
              <ChangedSemanticBlocksOverview
                state={detailState}
                humanIntent={humanIntent}
                onOpenStatement={
                  node.route.fileType === "ontology" && onOpenOntology
                    ? (statementId, deleted) =>
                        // A deleted file has no editor: open All changes there.
                        onOpenOntology(
                          node.changeState === "deleted"
                            ? { route: node.route, deleted: true }
                            : { route: node.route, statementId, deleted },
                        )
                    : undefined
                }
              />
            )}
          {(node.missingTargets?.length ?? 0) > 0 && (
            <section
              data-testid="change-graph-missing-targets"
              aria-labelledby="change-graph-missing-targets-heading"
              className="mt-4"
            >
              <h4
                id="change-graph-missing-targets-heading"
                className={NODE_DETAILS_SECTION_HEADING_CLASS}
              >
                Missing link targets
              </h4>
              <ul className="mt-2 space-y-1">
                {node.missingTargets!.map((missing) => (
                  <li
                    key={`${missing.statementId}|${missing.href}`}
                    className="rounded border border-[var(--g5-warn)]/35 bg-[var(--g5-bg0)] px-2 py-1 text-[10px]"
                  >
                    <span className="font-mono text-[var(--g5-text1)]">
                      {missing.statementId}
                    </span>{" "}
                    links to{" "}
                    <span className="break-all font-mono text-[var(--g5-text0)]">
                      {missing.href}
                    </span>
                    , which resolves in neither version.
                  </li>
                ))}
              </ul>
            </section>
          )}
          <section
            aria-labelledby="change-graph-grounding-edges-heading"
            data-testid="change-graph-grounding-edges-section"
            className="mt-4"
          >
            <h4
              id="change-graph-grounding-edges-heading"
              className={NODE_DETAILS_SECTION_HEADING_CLASS}
            >
              Relationships
            </h4>
            <GroundingEvidenceSubsections
              key={node.id}
              changedEvidence={changedEvidence}
              preExistingEvidence={preExistingEvidence}
              focusedEvidence={focusedEvidence}
              onFocusEvidence={onFocusEvidence}
            />
          </section>
        </div>
        <div
          data-testid="change-graph-node-details-right"
          className="overflow-auto p-3 text-xs"
        >
          <h4
            className={classNames(NODE_DETAILS_SECTION_HEADING_CLASS, "mb-1")}
          >
            What changed
          </h4>
          <SourceControlChangeGraphWhatChanged
            state={detailState}
            onRetry={retryDetails}
          />
        </div>
      </div>
    </aside>
  );
}

function expectsSemanticDetails(
  node: SourceControlChangeGraphNode,
  workspaceId: string,
): boolean {
  if (node.route.fileType === "ontology") return true;
  if (node.route.fileType !== "external_doc" || !node.route.filePath)
    return false;
  const prefix = `docs/${workspaceId}/`;
  if (!node.route.filePath.startsWith(prefix)) return false;
  const relative = node.route.filePath.slice(prefix.length);
  return (
    relative.endsWith(".md") &&
    !relative.startsWith("explainers/") &&
    relative
      .split("/")
      .every(
        (segment) => segment.length > 0 && segment !== "." && segment !== "..",
      )
  );
}

function formatProvenanceTimestamp(value: string | undefined): string {
  if (!value) return "unknown";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleString();
}

function ProvenanceSourceNote({
  source,
}: {
  source: string | undefined;
}): React.ReactElement | null {
  // Subtle, real-text indicator (never color-only) surfaced only when the value
  // came from a git-blame fallback or could not be resolved.
  if (source !== "git_blame" && source !== "unknown") return null;
  const label = source === "git_blame" ? "git blame" : "unknown";
  return <span className="ml-1 italic text-[var(--g5-text2)]">({label})</span>;
}

function ChangedSemanticBlocksOverview({
  state,
  humanIntent,
  onOpenStatement,
}: {
  state: AsyncState<SourceControlChangeGraphNodeDetails>;
  humanIntent: boolean;
  onOpenStatement?: (statementId: string, deleted: boolean) => void;
}) {
  let content: React.ReactNode;
  if (state.status === "loading") {
    content = (
      <p className="text-[10px] text-[var(--g5-text2)]">
        Loading changed statements…
      </p>
    );
  } else if (state.status === "error" || state.data.kind !== "semantic_diff") {
    content = (
      <p className="text-[10px] text-[var(--g5-text2)]">
        Changed statement overview unavailable.
      </p>
    );
  } else {
    const records = state.data.records.filter(
      // Under the "Human intent" lens, restrict to human statements only.
      (record) => !humanIntent || record.isHuman,
    );
    content =
      records.length === 0 ? (
        <p className="text-[10px] text-[var(--g5-text2)]">
          {humanIntent
            ? "No human statements changed."
            : "No statements changed."}
        </p>
      ) : (
        <ul className="space-y-1.5">
          {records.map((record) => (
            <li
              key={record.id}
              data-change-state={record.changeState}
              className={classNames(
                "flex min-w-0 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1.5",
                humanIntent ? "flex-col gap-1" : "items-center gap-1.5",
              )}
            >
              <div className="flex min-w-0 items-center gap-1.5">
                <span className="shrink-0 font-mono text-[10px] text-[var(--g5-text1)]">
                  {record.id}
                </span>
                <div className="flex min-w-0 flex-1 items-center gap-1.5">
                  <span
                    className="min-w-0 truncate text-[10px] text-[var(--g5-text1)]"
                    title={record.title}
                  >
                    {record.title || statementKindLabel(record.recordKind)}
                  </span>
                  {record.isHuman && <ChangeGraphHumanBadge />}
                </div>
                <ChangeGraphChangeBadge state={record.changeState} />
                {onOpenStatement && (
                  <button
                    type="button"
                    data-testid="change-graph-open-statement"
                    aria-label={`Open ${record.id} in the ontology editor`}
                    title={
                      record.changeState === "deleted"
                        ? "Open at its Before version"
                        : "Open in the ontology editor"
                    }
                    onClick={() =>
                      onOpenStatement(
                        record.id,
                        record.changeState === "deleted",
                      )
                    }
                    className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
                  >
                    <ExternalLink size={11} aria-hidden="true" />
                  </button>
                )}
              </div>
              {humanIntent && (
                <dl className="flex min-w-0 flex-wrap gap-x-3 gap-y-0.5 text-[10px] text-[var(--g5-text2)]">
                  <div className="flex min-w-0 items-baseline gap-1">
                    <dt className="shrink-0">Author</dt>
                    <dd className="min-w-0 text-[var(--g5-text1)]">
                      {record.author ?? "unknown"}
                      <ProvenanceSourceNote source={record.authorSource} />
                    </dd>
                  </div>
                  <div className="flex min-w-0 items-baseline gap-1">
                    <dt className="shrink-0">When</dt>
                    <dd className="min-w-0 text-[var(--g5-text1)]">
                      {formatProvenanceTimestamp(record.timestamp)}
                      <ProvenanceSourceNote source={record.timestampSource} />
                    </dd>
                  </div>
                </dl>
              )}
            </li>
          ))}
        </ul>
      );
  }

  return (
    <section
      aria-labelledby="change-graph-changed-semantic-blocks-heading"
      data-testid="change-graph-changed-semantic-blocks-section"
      className="mt-4"
    >
      <h4
        id="change-graph-changed-semantic-blocks-heading"
        className={NODE_DETAILS_SECTION_HEADING_CLASS}
      >
        Changed statements
      </h4>
      <div className="mt-2">{content}</div>
    </section>
  );
}

function GroundingEvidenceSubsections({
  changedEvidence,
  preExistingEvidence,
  focusedEvidence,
  onFocusEvidence,
}: {
  changedEvidence: IncidentEvidence[];
  preExistingEvidence: IncidentEvidence[];
  focusedEvidence: EvidenceSelection | null;
  onFocusEvidence: (selection: EvidenceSelection) => void;
}) {
  const [changedOpen, setChangedOpen] = useState(false);
  const [preExistingOpen, setPreExistingOpen] = useState(false);

  const handleOpenChange = (
    nextOpen: boolean,
    evidence: IncidentEvidence[],
    setOpen: (open: boolean) => void,
  ) => {
    setOpen(nextOpen);
    if (
      !nextOpen &&
      focusedEvidence &&
      evidenceIncludesSelection(evidence, focusedEvidence)
    ) {
      onFocusEvidence(focusedEvidence);
    }
  };

  return (
    <div data-testid="change-graph-grounding-edge-subsections" className="mt-2">
      <details
        open={changedOpen}
        onToggle={(event) =>
          handleOpenChange(
            event.currentTarget.open,
            changedEvidence,
            setChangedOpen,
          )
        }
        data-testid="change-graph-changed-grounding-edges"
        className="group"
      >
        <summary
          data-testid="change-graph-changed-grounding-edges-toggle"
          className="flex cursor-pointer select-none items-center justify-between gap-2 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        >
          <span className="flex items-center gap-1">
            <ChevronRight
              size={11}
              className="transition-transform duration-150 group-open:rotate-90 motion-reduce:transition-none"
            />
            <span
              id="change-graph-changed-grounding-edges-heading"
              role="heading"
              aria-level={5}
            >
              Changed in this Diff
            </span>
          </span>
          <span
            data-testid="change-graph-changed-grounding-edges-count"
            className="min-w-5 rounded-full bg-[var(--g5-bg2)] px-1.5 py-0.5 text-center text-[9px] font-semibold tabular-nums text-[var(--g5-text1)]"
          >
            {changedEvidence.length}
          </span>
        </summary>
        <div className="mt-2 ml-3">
          {changedEvidence.length === 0 ? (
            <p className="text-[var(--g5-text2)]">
              No links were added or removed.
              {preExistingEvidence.length > 0 && (
                <>
                  {" "}
                  Existing relationships are available under Pre-existing below.
                </>
              )}
            </p>
          ) : (
            <GroundingEvidenceList
              evidence={changedEvidence}
              focusedEvidence={focusedEvidence}
              onFocusEvidence={onFocusEvidence}
              testId="change-graph-grounding-edges"
            />
          )}
        </div>
      </details>

      <details
        open={preExistingOpen}
        onToggle={(event) =>
          handleOpenChange(
            event.currentTarget.open,
            preExistingEvidence,
            setPreExistingOpen,
          )
        }
        data-testid="change-graph-pre-existing-grounding-edges"
        className="group mt-3"
      >
        <summary
          data-testid="change-graph-pre-existing-grounding-edges-toggle"
          className="flex cursor-pointer select-none items-center justify-between gap-2 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        >
          <span className="flex items-center gap-1">
            <ChevronRight
              size={11}
              className="transition-transform duration-150 group-open:rotate-90 motion-reduce:transition-none"
            />
            <span
              id="change-graph-pre-existing-grounding-edges-heading"
              role="heading"
              aria-level={5}
            >
              Pre-existing
            </span>
          </span>
          <span
            data-testid="change-graph-pre-existing-grounding-edges-count"
            className="min-w-5 rounded-full bg-[var(--g5-bg2)] px-1.5 py-0.5 text-center text-[9px] font-semibold tabular-nums text-[var(--g5-text1)]"
          >
            {preExistingEvidence.length}
          </span>
        </summary>
        <div className="mt-2 ml-3">
          {preExistingEvidence.length === 0 ? (
            <p className="text-[var(--g5-text2)]">
              No unchanged links were returned for this artifact.
            </p>
          ) : (
            <GroundingEvidenceList
              evidence={preExistingEvidence}
              focusedEvidence={focusedEvidence}
              onFocusEvidence={onFocusEvidence}
            />
          )}
        </div>
      </details>
    </div>
  );
}

function GroundingEvidenceList({
  evidence,
  focusedEvidence,
  onFocusEvidence,
  testId,
}: {
  evidence: IncidentEvidence[];
  focusedEvidence: EvidenceSelection | null;
  onFocusEvidence: (selection: EvidenceSelection) => void;
  testId?: string;
}) {
  return (
    <ul className="space-y-1.5" data-testid={testId}>
      {evidence.map(({ evidence: item, edgeId, sourceNode, targetNode }) => {
        const sourceLabel = evidenceEndpointLabel(
          sourceNode,
          item.sourceFile,
          item.sourceId,
        );
        const targetLabel = evidenceEndpointLabel(
          targetNode,
          item.targetFile,
          item.targetId,
        );
        const selected =
          focusedEvidence?.evidenceId === item.id &&
          focusedEvidence.edgeId === edgeId;
        const technicalIdentity = `${item.sourceFile}#${item.sourceId} → ${item.targetFile}${item.targetId ? `#${item.targetId}` : ""}`;
        return (
          <li
            key={item.id}
            data-change-state={item.changeState}
            data-edge-id={edgeId}
            data-testid={`change-graph-evidence-${item.id}`}
            className={classNames(
              "overflow-hidden rounded border bg-[var(--g5-bg0)]",
              "transition-[background-color,border-color,box-shadow] duration-150 motion-reduce:transition-none",
              selected
                ? "border-[var(--g5-info)] bg-[var(--g5-bg2)] ring-1 ring-[var(--g5-info)]"
                : "border-[var(--g5-stroke)]",
            )}
          >
            <button
              type="button"
              data-testid="change-graph-evidence-item"
              aria-label={`${relationshipLabel(item.relationship)} from ${sourceLabel.replaceAll(" · ", " ")} to ${targetLabel.replaceAll(" · ", " ")}${item.targetDeleted ? " (target deleted)" : ""}`}
              aria-pressed={selected}
              onClick={() => onFocusEvidence({ evidenceId: item.id, edgeId })}
              className="block w-full px-2 py-1.5 text-left hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-[var(--g5-info)]"
            >
              <span className="flex min-w-0 items-center gap-1.5">
                <span className="min-w-0 flex-1 truncate font-medium">
                  {relationshipLabel(item.relationship)}
                </span>
                {item.targetDeleted && (
                  <span
                    data-testid="change-graph-evidence-target-deleted"
                    className="shrink-0 rounded border border-[var(--g5-blocked)]/45 px-1 text-[9px] font-semibold uppercase text-[var(--g5-blocked)]"
                  >
                    Target deleted
                  </span>
                )}
                <ChangeGraphChangeBadge
                  state={item.changeState}
                  label={
                    item.changeState === "unchanged"
                      ? "Pre-existing"
                      : undefined
                  }
                />
                {item.relevance !== undefined && (
                  <span className="shrink-0 text-[10px] tabular-nums text-[var(--g5-text2)]">
                    {item.relevance}% relevance
                  </span>
                )}
              </span>
              <span
                className="mt-1 block truncate text-[10px] leading-4 text-[var(--g5-text1)]"
                title={sourceLabel}
              >
                {sourceLabel}
              </span>
              <span
                className="block truncate text-[10px] leading-4 text-[var(--g5-text1)]"
                title={targetLabel}
              >
                → {targetLabel}
              </span>
            </button>
            <details className="border-t border-[var(--g5-stroke)]/70 px-2 py-1 text-[10px] text-[var(--g5-text2)]">
              <summary className="cursor-pointer select-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]">
                Technical details
              </summary>
              <p className="mt-1 select-text break-all font-mono leading-4 text-[var(--g5-text1)]">
                {technicalIdentity}
              </p>
            </details>
          </li>
        );
      })}
    </ul>
  );
}

function evidenceEndpointLabel(
  node: SourceControlChangeGraphNode | undefined,
  file: string,
  recordId: string,
): string {
  if (!node) return `${file}#${recordId}`;
  const recordLabel =
    node.artifactType === "conversation" ? null : evidenceRecordLabel(recordId);
  return [displayArtifactLabel(node), node.title, recordLabel]
    .filter((part): part is string => Boolean(part))
    .join(" · ");
}

function evidenceRecordLabel(recordId: string): string | null {
  return recordId || null;
}

function relationshipLabel(relationship: string): string {
  return (
    RELATIONSHIP_LABELS[relationship as ChangeGraphRelationship] ??
    formatLabel(relationship)
  );
}

function statementKindLabel(kind: string): string {
  return kind === "I"
    ? "Intent"
    : kind === "D"
      ? "Decision"
      : kind === "Q"
        ? "Question"
        : "Statement";
}

function secondaryAction(
  node: SourceControlChangeGraphNode,
  callbacks: Pick<
    SourceControlChangeGraphViewProps,
    "onOpenDocument" | "onOpenDiff" | "onOpenOntology"
  >,
): { label: string; run: () => void } | null {
  if (node.artifactType === "conversation") return null;
  // Ontology opens in the ontology editor with Compare on, never a Source
  // Control diff with a different baseline (g5.diff-graph.navigation).
  if (node.route.fileType === "ontology") {
    if (!callbacks.onOpenOntology) return null;
    const deleted = node.changeState === "deleted";
    return {
      label: deleted ? "Open in All changes" : "Open document",
      run: () => callbacks.onOpenOntology?.({ route: node.route, deleted }),
    };
  }
  if (
    node.artifactType === "document" ||
    node.artifactType === "view_document"
  ) {
    return callbacks.onOpenDocument
      ? {
          label: "Open document",
          run: () => callbacks.onOpenDocument?.(node.route),
        }
      : null;
  }
  return callbacks.onOpenDiff && node.route.filePath
    ? { label: "Open diff", run: () => callbacks.onOpenDiff?.(node.route) }
    : null;
}

function DetailsRow({
  label,
  value,
  mono = false,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="mb-2 grid grid-cols-[88px_1fr] gap-2">
      <dt className="text-[10px] uppercase tracking-wide text-[var(--g5-text2)]">
        {label}
      </dt>
      <dd
        className={classNames(
          "break-all text-[var(--g5-text1)]",
          mono && "font-mono text-[10px]",
        )}
      >
        {value}
      </dd>
    </div>
  );
}

function GraphSkeleton() {
  return (
    <div
      data-testid="change-graph-skeleton"
      className="grid h-full min-w-[840px] grid-cols-3 divide-x divide-[var(--g5-stroke)]/35"
    >
      {COLUMN_ORDER.map((column) => (
        <div key={column} className="p-4">
          <h3
            className="mb-8 flex items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.08em]"
            style={{ color: COLUMN_ACCENTS[column] }}
          >
            <span
              aria-hidden="true"
              className="h-1.5 w-1.5 rounded-full bg-current"
            />
            {COLUMN_LABELS[column]}
          </h3>
          {[0, 1, 2].map((index) => (
            <div
              key={index}
              className="mb-4 h-28 animate-pulse rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
            />
          ))}
        </div>
      ))}
    </div>
  );
}

function GraphMessage({
  title,
  detail,
  action,
}: {
  title: string;
  detail: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex h-full min-h-64 items-center justify-center p-8">
      <div className="max-w-md text-center">
        <h3 className="text-sm font-medium">{title}</h3>
        <p className="mb-3 mt-1 text-xs text-[var(--g5-text2)]">{detail}</p>
        {action}
      </div>
    </div>
  );
}

function formatLabel(value: string): string {
  return value
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function formatArtifactLabel(value: string): string {
  return value === "conversation" ? "Chat" : formatLabel(value);
}
