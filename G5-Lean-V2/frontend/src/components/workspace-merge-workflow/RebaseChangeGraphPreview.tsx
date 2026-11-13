import {
  AlertTriangle,
  FileCode2,
  FileText,
  Maximize2,
  MessageSquareText,
  Minus,
  Plus,
  RefreshCw,
  Shapes,
  X,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

import {
  useRebaseChangeGraph,
  useRebaseChangeGraphNodeDetails,
} from "../../hooks/useSourceControlChangeGraph";
import { ApiError } from "../../lib/api";
import { formatDiffGraphTooLargeMessage } from "../../lib/changeGraphErrors";
import type {
  ChangeGraphState,
  RebaseChangeGraphConflict,
  RebaseChangeGraphNodeDetailsResponse,
  RebaseChangeGraphResponse,
  RebaseChangeGraphSideDetails,
  SourceControlChangeGraphEdge,
  SourceControlChangeGraphNode,
  SourceControlChangeGraphResponse,
} from "../../lib/types";
import {
  CHANGE_GRAPH_ZOOM_CONTROL_STEP,
  useChangeGraphViewport,
} from "../source-control/useChangeGraphViewport";
import {
  ChangeGraphCard,
  ChangeGraphControl,
  ChangeGraphLaneHeading,
} from "../source-control/ChangeGraphPrimitives";
import { ChangeGraphDetailEvidence } from "../source-control/SourceControlChangeGraphWhatChanged";
import { classNames } from "../ui/UI";

type OptionalLayer = "code" | "chat" | "docs";
type GraphLayer = "ontology" | OptionalLayer;
type GraphSide = "source" | "target";
type SolidEdgeFilter = "all" | "red" | "gray";

type LayoutNode = SourceControlChangeGraphNode & {
  graphId: string;
  side: GraphSide;
  layer: GraphLayer;
  /** Every ontology card shares one lane per side. */
  ontologyKind?: "concept";
  x: number;
  y: number;
  width: number;
};

type LayoutRelationship = {
  id: string;
  source: string;
  target: string;
};

const BOARD_WIDTH = 1320;
const NODE_HEIGHT = 90;
const NODE_WIDTH = 210;

const LAYER_META: Record<
  GraphLayer,
  { label: string; icon: LucideIcon; color: string }
> = {
  ontology: {
    label: "Ontology",
    icon: Shapes,
    color: "var(--g5-graph-concept)",
  },
  code: {
    label: "Source Code",
    icon: FileCode2,
    color: "var(--g5-graph-source)",
  },
  chat: {
    label: "Chat",
    icon: MessageSquareText,
    color: "var(--g5-graph-intent)",
  },
  docs: {
    label: "Project Doc",
    icon: FileText,
    color: "var(--g5-graph-intent)",
  },
};

const OPTIONAL_LAYERS: Array<{ id: OptionalLayer; label: string }> = [
  { id: "code", label: "Code" },
  { id: "chat", label: "Chat" },
  { id: "docs", label: "Project Docs" },
];

export interface RebaseChangeGraphPreviewProps {
  sourceWorkspaceId: string;
  targetWorkspaceId: string;
  sourceWorkspaceName: string;
  targetWorkspaceName: string;
}

export function RebaseChangeGraphPreview({
  sourceWorkspaceId,
  targetWorkspaceId,
  sourceWorkspaceName,
  targetWorkspaceName,
}: RebaseChangeGraphPreviewProps) {
  const state = useRebaseChangeGraph(sourceWorkspaceId, targetWorkspaceId);

  if (state.status === "loading") {
    return (
      <GraphStatus message="Loading changes from the shared divergence…" />
    );
  }
  if (state.status === "error") {
    const message =
      state.error instanceof ApiError && state.error.is("DIFF_GRAPH_TOO_LARGE")
        ? formatDiffGraphTooLargeMessage(state.error, {
            subject: "This rebase comparison",
            fallback: "This rebase comparison exceeds the supported size.",
          })
        : state.error.message;
    return <GraphError message={message} onRetry={state.retry} />;
  }
  return (
    <RebaseChangeGraphContent
      key={state.data.revision}
      graph={state.data}
      sourceWorkspaceId={sourceWorkspaceId}
      targetWorkspaceId={targetWorkspaceId}
      sourceWorkspaceName={sourceWorkspaceName}
      targetWorkspaceName={targetWorkspaceName}
      onRefresh={state.retry}
    />
  );
}

function RebaseChangeGraphContent({
  graph,
  sourceWorkspaceId,
  targetWorkspaceId,
  sourceWorkspaceName,
  targetWorkspaceName,
  onRefresh,
}: RebaseChangeGraphPreviewProps & {
  graph: RebaseChangeGraphResponse;
  onRefresh: () => void;
}) {
  const [visibleLayers, setVisibleLayers] = useState<Set<OptionalLayer>>(
    () => new Set(),
  );
  const [solidEdgeFilter, setSolidEdgeFilter] =
    useState<SolidEdgeFilter>("all");
  const [selectedArtifactKey, setSelectedArtifactKey] = useState<string>();
  const layout = useMemo(
    () => buildLayout(graph, visibleLayers),
    [graph, visibleLayers],
  );
  const nodesByGraphId = useMemo(
    () => new Map(layout.nodes.map((node) => [node.graphId, node])),
    [layout.nodes],
  );
  const eligibleConflicts = useMemo(
    () =>
      graph.conflicts.filter(
        (conflict) =>
          nodesByGraphId.has(`source:${conflict.sourceNodeId}`) &&
          nodesByGraphId.has(`target:${conflict.targetNodeId}`),
      ),
    [graph.conflicts, nodesByGraphId],
  );
  const visibleConflicts = useMemo(() => {
    if (solidEdgeFilter === "red") {
      return eligibleConflicts.filter(
        (conflict) => conflict.kind === "same_block",
      );
    }
    // Gray is cumulative: same-node gray relationships plus same-block red
    // conflicts. All additionally retains unlinked and one-sided nodes.
    return eligibleConflicts;
  }, [eligibleConflicts, solidEdgeFilter]);
  const visibleNodes = useMemo(() => {
    if (solidEdgeFilter === "all") return layout.nodes;
    const conflictEndpointIds = new Set<string>();
    visibleConflicts.forEach((conflict) => {
      conflictEndpointIds.add(`source:${conflict.sourceNodeId}`);
      conflictEndpointIds.add(`target:${conflict.targetNodeId}`);
    });
    return layout.nodes.filter((node) => conflictEndpointIds.has(node.graphId));
  }, [layout.nodes, solidEdgeFilter, visibleConflicts]);
  const visibleNodeIds = useMemo(
    () => new Set(visibleNodes.map((node) => node.graphId)),
    [visibleNodes],
  );
  const visibleNodeCounts = useMemo(
    () => ({
      source: visibleNodes.filter((node) => node.side === "source").length,
      target: visibleNodes.filter((node) => node.side === "target").length,
    }),
    [visibleNodes],
  );
  const showSideStatuses =
    sideNeedsStatus(graph.source, visibleNodeCounts.source) ||
    sideNeedsStatus(graph.target, visibleNodeCounts.target);
  const conflictsByGraphId = useMemo(() => {
    const conflicts = new Map<string, RebaseChangeGraphConflict>();
    graph.conflicts.forEach((conflict) => {
      conflicts.set(`source:${conflict.sourceNodeId}`, conflict);
      conflicts.set(`target:${conflict.targetNodeId}`, conflict);
    });
    return conflicts;
  }, [graph.conflicts]);
  const detailState = useRebaseChangeGraphNodeDetails(
    sourceWorkspaceId,
    targetWorkspaceId,
    graph.revision,
    selectedArtifactKey,
  );
  const clearSelection = useCallback(
    () => setSelectedArtifactKey(undefined),
    [],
  );
  const { viewportRef, scale, changeScale, fitGraph, viewportPointerHandlers } =
    useChangeGraphViewport({
      worldWidth: BOARD_WIDTH,
      worldHeight: layout.height,
      onBackgroundClick: clearSelection,
    });

  const recoverFromStaleDetails = useCallback(() => {
    clearSelection();
    onRefresh();
  }, [clearSelection, onRefresh]);

  const toggleLayer = (layer: OptionalLayer) => {
    clearSelection();
    setVisibleLayers((current) => {
      const next = new Set(current);
      if (next.has(layer)) next.delete(layer);
      else next.add(layer);
      return next;
    });
    requestAnimationFrame(() => {
      viewportRef.current?.scrollIntoView?.({
        behavior: "smooth",
        block: "center",
        inline: "nearest",
      });
    });
  };

  return (
    <section
      aria-label="Rebase change comparison"
      data-testid="rebase-change-graph-preview"
      className="flex shrink-0 flex-col overflow-hidden rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)]"
    >
      <header className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2">
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-[var(--g5-text0)]">
            Rebase change comparison
          </h3>
          <p className="mt-0.5 truncate text-[11px] text-[var(--g5-text2)]">
            Each side is compared with the same persisted divergence across{" "}
            {graph.commonBaseRepositoryCount} repositories.
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <LayerChecklist
            visibleLayers={visibleLayers}
            onToggle={toggleLayer}
          />
          <EdgeTypeControls
            value={solidEdgeFilter}
            onChange={(value) => {
              clearSelection();
              setSolidEdgeFilter(value);
            }}
          />
          <GraphViewportControls
            scale={scale}
            onZoomOut={() => changeScale(-CHANGE_GRAPH_ZOOM_CONTROL_STEP)}
            onZoomIn={() => changeScale(CHANGE_GRAPH_ZOOM_CONTROL_STEP)}
            onFit={fitGraph}
          />
        </div>
      </header>

      <div className="grid grid-cols-[1fr_auto_1fr] border-b border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-[11px]">
        <BranchHeading
          side="source"
          name={graph.sourceWorkspace.name || sourceWorkspaceName}
          detail="Source changes since divergence"
        />
        <div className="flex items-center border-x border-[var(--g5-stroke)] px-3 text-[10px] font-medium uppercase tracking-[0.08em] text-[var(--g5-text2)]">
          Overlap
        </div>
        <BranchHeading
          side="target"
          name={graph.targetWorkspace.name || targetWorkspaceName}
          detail="Target changes since divergence"
        />
      </div>

      {showSideStatuses && (
        <div className="grid grid-cols-2 divide-x divide-[var(--g5-stroke)] border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)]/35">
          <SideGraphStatus
            side="source"
            graph={graph.source}
            visibleNodeCount={visibleNodeCounts.source}
          />
          <SideGraphStatus
            side="target"
            graph={graph.target}
            visibleNodeCount={visibleNodeCounts.target}
          />
        </div>
      )}

      <div
        ref={viewportRef}
        className="relative h-[clamp(320px,46vh,420px)] shrink-0 cursor-grab overflow-auto active:cursor-grabbing"
        data-testid="rebase-graph-scroll-region"
        {...viewportPointerHandlers}
      >
        {layout.nodes.length === 0 && (
          <p className="absolute inset-0 z-40 flex items-center justify-center text-xs text-[var(--g5-text2)]">
            {graph.source.nodes.length + graph.target.nodes.length === 0
              ? "No Source or Target changes from the shared divergence."
              : "No changes are available in the selected layers."}
          </p>
        )}
        {layout.nodes.length > 0 &&
          solidEdgeFilter !== "all" &&
          visibleNodes.length === 0 && (
            <p className="absolute inset-0 z-40 flex items-center justify-center text-xs text-[var(--g5-text2)]">
              No nodes are connected by the selected edge types.
            </p>
          )}
        <div
          className="relative select-none"
          style={{
            width: BOARD_WIDTH * scale,
            height: layout.height * scale,
            marginInline: "auto",
          }}
          data-testid="rebase-graph-footprint"
        >
          <div
            className="relative origin-top-left"
            style={{
              width: BOARD_WIDTH,
              height: layout.height,
              transform: `scale(${scale})`,
            }}
            data-testid="rebase-graph-board"
          >
            <div className="pointer-events-none absolute inset-y-0 left-0 w-[650px] border-r border-[var(--g5-stroke)]/45 bg-[var(--g5-bg1)]/10" />
            <div className="pointer-events-none absolute inset-y-0 right-0 w-[650px] border-l border-[var(--g5-stroke)]/45 bg-[var(--g5-bg1)]/10" />
            {/* One Source and one Target ontology lane (g5.diff-graph.rebase-conflicts). */}
            <OntologyLane side="source" kind="concept" x={380} />
            <OntologyLane side="target" kind="concept" x={690} />
            {visibleLayers.size > 0 && (
              <>
                <LaneLabel x={18} label="Source evidence" />
                <LaneLabel x={1150} label="Target evidence" />
              </>
            )}

            <svg
              aria-hidden="true"
              className="pointer-events-none absolute inset-0 z-10 h-full w-full"
              viewBox={`0 0 ${BOARD_WIDTH} ${layout.height}`}
            >
              <defs>
                <marker
                  id="rebase-relation-arrow"
                  viewBox="0 0 10 10"
                  refX="8"
                  refY="5"
                  markerWidth="5"
                  markerHeight="5"
                  orient="auto-start-reverse"
                >
                  <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--g5-text2)" />
                </marker>
              </defs>
              {layout.relationships.map((relationship) => {
                const source = nodesByGraphId.get(relationship.source);
                const target = nodesByGraphId.get(relationship.target);
                if (
                  !source ||
                  !target ||
                  !visibleNodeIds.has(source.graphId) ||
                  !visibleNodeIds.has(target.graphId)
                ) {
                  return null;
                }
                return (
                  <path
                    key={relationship.id}
                    data-testid="rebase-relationship-edge"
                    d={relationPath(source, target)}
                    fill="none"
                    stroke="var(--g5-text2)"
                    strokeDasharray="6 5"
                    strokeOpacity={0.48}
                    strokeWidth={1.5}
                    markerEnd="url(#rebase-relation-arrow)"
                  />
                );
              })}
              {visibleConflicts.map((conflict) => {
                const source = nodesByGraphId.get(
                  `source:${conflict.sourceNodeId}`,
                );
                const target = nodesByGraphId.get(
                  `target:${conflict.targetNodeId}`,
                );
                if (!source || !target) return null;
                const path = relationPath(source, target);
                const red = conflict.kind === "same_block";
                const count = conflict.overlapBlockCount ?? 0;
                const midpointX = (source.x + source.width + target.x) / 2;
                const midpointY = (source.y + target.y) / 2 + NODE_HEIGHT / 2;
                return (
                  <g key={conflict.id}>
                    <path
                      data-testid="rebase-conflict-edge"
                      data-conflict-kind={conflict.kind}
                      data-conflict-count={red ? count : undefined}
                      d={path}
                      fill="none"
                      stroke={red ? "var(--g5-blocked)" : "var(--g5-text2)"}
                      strokeOpacity={red ? 0.9 : 0.72}
                      strokeWidth={red ? Math.min(8, 1.8 + count * 1.2) : 2.5}
                    />
                    {red && (
                      <g data-testid="rebase-conflict-count-badge">
                        <circle
                          cx={midpointX}
                          cy={midpointY}
                          r={9}
                          fill="var(--g5-bg0)"
                          stroke="var(--g5-blocked)"
                          strokeWidth={1.5}
                        />
                        <text
                          x={midpointX}
                          y={midpointY + 3.5}
                          fill="var(--g5-blocked)"
                          fontSize="9"
                          fontWeight="700"
                          textAnchor="middle"
                        >
                          {count}
                        </text>
                      </g>
                    )}
                  </g>
                );
              })}
            </svg>

            {visibleNodes.map((node) => (
              <RebaseGraphCard
                key={node.graphId}
                node={node}
                selected={selectedArtifactKey === node.id}
                onSelect={() => setSelectedArtifactKey(node.id)}
                conflict={conflictsByGraphId.get(node.graphId)}
              />
            ))}
          </div>
        </div>
      </div>

      {selectedArtifactKey && (
        <NodeDetailsDrawer
          state={detailState}
          onClose={() => setSelectedArtifactKey(undefined)}
          onStaleRevision={recoverFromStaleDetails}
        />
      )}

      <footer className="flex min-h-12 shrink-0 flex-wrap items-center justify-between gap-3 border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-[10px] text-[var(--g5-text2)]">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
          <LegendLine
            color="var(--g5-blocked)"
            label="Red: both sides edited the same I/D/Q blocks × overlap count"
          />
          <LegendLine
            color="var(--g5-text2)"
            label="Gray: both sides edited the same node, but not the same blocks"
          />
          <LegendLine dashed color="var(--g5-text2)" label="Relationship" />
          <span>Red line weight = overlapping block count</span>
        </div>
        <p>
          {selectedArtifactKey
            ? "Paired source and target details are open below the graph."
            : "Select a node to inspect its exact changed I/D/Q blocks."}
        </p>
      </footer>
    </section>
  );
}

function buildLayout(
  graph: RebaseChangeGraphResponse,
  visibleLayers: ReadonlySet<OptionalLayer>,
): {
  nodes: LayoutNode[];
  relationships: LayoutRelationship[];
  height: number;
} {
  const lanes: Record<string, LayoutNode[]> = {};
  const nodes: LayoutNode[] = [];
  const addSide = (
    side: GraphSide,
    sideNodes: SourceControlChangeGraphNode[],
  ) => {
    sideNodes.forEach((node) => {
      const layer = graphLayer(node);
      if (layer !== "ontology" && !visibleLayers.has(layer)) return;
      const ontologyKind = node.column === "ontology" ? "concept" : undefined;
      const lane = `${side}:${ontologyKind ?? "evidence"}`;
      const layoutNode: LayoutNode = {
        ...node,
        graphId: `${side}:${node.id}`,
        side,
        layer,
        ontologyKind,
        x: laneX(side, ontologyKind, layer),
        y: 58,
        width: NODE_WIDTH,
      };
      (lanes[lane] ??= []).push(layoutNode);
      nodes.push(layoutNode);
    });
  };
  addSide("source", graph.source.nodes);
  addSide("target", graph.target.nodes);

  let maxRows = 1;
  Object.values(lanes).forEach((laneNodes) => {
    const orderedLaneNodes = orderLaneNodes(laneNodes);
    maxRows = Math.max(maxRows, orderedLaneNodes.length);
    orderedLaneNodes.forEach((node, index) => {
      node.y = 58 + index * 116;
    });
  });

  return {
    nodes,
    relationships: [
      ...layoutRelationships("source", graph.source.edges),
      ...layoutRelationships("target", graph.target.edges),
    ],
    height: Math.max(520, 58 + maxRows * 116 + 24),
  };
}

function orderLaneNodes(laneNodes: LayoutNode[]): LayoutNode[] {
  const sortedNodes = [...laneNodes].sort((a, b) =>
    `${a.path ?? ""}|${a.title}|${a.id}`.localeCompare(
      `${b.path ?? ""}|${b.title}|${b.id}`,
    ),
  );
  if (sortedNodes.every((node) => node.layer === "ontology")) {
    return sortedNodes;
  }

  const nodesByLayer = new Map<OptionalLayer, LayoutNode[]>(
    OPTIONAL_LAYERS.map(({ id }) => [
      id,
      sortedNodes.filter((node) => node.layer === id),
    ]),
  );
  const orderedNodes: LayoutNode[] = [];
  const maxLayerRows = Math.max(
    0,
    ...Array.from(nodesByLayer.values(), (nodes) => nodes.length),
  );
  for (let row = 0; row < maxLayerRows; row += 1) {
    OPTIONAL_LAYERS.forEach(({ id }) => {
      const node = nodesByLayer.get(id)?.[row];
      if (node) orderedNodes.push(node);
    });
  }
  return orderedNodes;
}

function layoutRelationships(
  side: GraphSide,
  edges: SourceControlChangeGraphEdge[],
): LayoutRelationship[] {
  return edges.map((edge) => ({
    id: `${side}:${edge.id}`,
    source: `${side}:${edge.sourceNodeId}`,
    target: `${side}:${edge.targetNodeId}`,
  }));
}

function graphLayer(node: SourceControlChangeGraphNode): GraphLayer {
  if (node.column === "ontology") return "ontology";
  if (node.column === "source_code") return "code";
  return node.intentGroup === "conversations" ? "chat" : "docs";
}

function laneX(
  side: GraphSide,
  kind: LayoutNode["ontologyKind"],
  layer: GraphLayer,
): number {
  if (kind === "concept") return side === "source" ? 380 : 690;
  if (layer !== "ontology") return side === "source" ? 18 : 1092;
  return side === "source" ? 380 : 690;
}

function relationPath(source: LayoutNode, target: LayoutNode): string {
  const sourceCenterX = source.x + source.width / 2;
  const targetCenterX = target.x + target.width / 2;
  const sourceCenterY = source.y + NODE_HEIGHT / 2;
  const targetCenterY = target.y + NODE_HEIGHT / 2;
  const sourceOnLeft = sourceCenterX < targetCenterX;
  const startX = sourceOnLeft ? source.x + source.width : source.x;
  const endX = sourceOnLeft ? target.x : target.x + target.width;
  const bend = Math.max(34, Math.abs(endX - startX) * 0.42);
  return `M ${startX} ${sourceCenterY} C ${startX + (sourceOnLeft ? bend : -bend)} ${sourceCenterY}, ${endX + (sourceOnLeft ? -bend : bend)} ${targetCenterY}, ${endX} ${targetCenterY}`;
}

function BranchHeading({
  side,
  name,
  detail,
}: {
  side: GraphSide;
  name: string;
  detail: string;
}) {
  return (
    <div
      className={classNames(
        "flex min-w-0 items-center gap-2 px-3 py-2",
        side === "target" && "justify-end text-right",
      )}
    >
      <span className="h-2 w-2 shrink-0 rounded-full bg-[var(--g5-info)]" />
      <span className="min-w-0">
        <span className="block truncate font-semibold text-[var(--g5-text0)]">
          {name}
        </span>
        <span className="block truncate text-[10px] text-[var(--g5-text2)]">
          {detail}
        </span>
      </span>
    </div>
  );
}

function LayerChecklist({
  visibleLayers,
  onToggle,
}: {
  visibleLayers: Set<OptionalLayer>;
  onToggle: (layer: OptionalLayer) => void;
}) {
  return (
    <fieldset
      aria-label="Visible rebase graph layers"
      className="flex items-center gap-2 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1"
    >
      <legend className="sr-only">Visible graph layers</legend>
      <label className="inline-flex items-center gap-1 text-[10px] font-medium text-[var(--g5-text0)]">
        <input
          type="checkbox"
          aria-label="Ontology"
          checked
          disabled
          readOnly
        />
        Ontology
      </label>
      {OPTIONAL_LAYERS.map((layer) => (
        <label
          key={layer.id}
          className="inline-flex cursor-pointer items-center gap-1 text-[10px] font-medium text-[var(--g5-text1)]"
        >
          <input
            type="checkbox"
            aria-label={layer.label}
            checked={visibleLayers.has(layer.id)}
            onChange={() => onToggle(layer.id)}
          />
          {layer.label}
        </label>
      ))}
    </fieldset>
  );
}

function EdgeTypeControls({
  value,
  onChange,
}: {
  value: SolidEdgeFilter;
  onChange: (value: SolidEdgeFilter) => void;
}) {
  const filters: Array<[SolidEdgeFilter, string]> = [
    ["all", "All"],
    ["red", "Red only"],
    ["gray", "Gray only"],
  ];
  return (
    <div
      role="group"
      aria-label="Edge types"
      className="flex items-center gap-0.5 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-0.5"
    >
      <span className="px-1 text-[9px] font-semibold uppercase tracking-wide text-[var(--g5-text2)]">
        Edge types
      </span>
      {filters.map(([id, label]) => (
        <button
          key={id}
          type="button"
          data-testid={`rebase-graph-edge-filter-${id}`}
          aria-label={label === "All" ? "All edge types" : label}
          aria-pressed={value === id}
          onClick={() => onChange(id)}
          className={classNames(
            "rounded px-2 py-1 text-[10px] font-medium",
            value === id
              ? "bg-[var(--g5-bg2)] text-[var(--g5-text0)]"
              : "text-[var(--g5-text2)] hover:text-[var(--g5-text0)]",
          )}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

function GraphViewportControls({
  scale,
  onZoomOut,
  onZoomIn,
  onFit,
}: {
  scale: number;
  onZoomOut: () => void;
  onZoomIn: () => void;
  onFit: () => void;
}) {
  return (
    <div
      aria-label="Paired graph controls"
      className="flex items-center gap-1"
      role="group"
    >
      <ChangeGraphControl label="Zoom out" onClick={onZoomOut}>
        <Minus aria-hidden="true" size={14} />
      </ChangeGraphControl>
      <span className="w-12 text-center text-[10px] tabular-nums text-[var(--g5-text2)]">
        {Math.round(scale * 100)}%
      </span>
      <ChangeGraphControl label="Zoom in" onClick={onZoomIn}>
        <Plus aria-hidden="true" size={14} />
      </ChangeGraphControl>
      <ChangeGraphControl label="Fit graph" onClick={onFit} text="Fit">
        <Maximize2 aria-hidden="true" size={13} />
      </ChangeGraphControl>
    </div>
  );
}

function SideGraphStatus({
  side,
  graph,
  visibleNodeCount,
}: {
  side: GraphSide;
  graph: SourceControlChangeGraphResponse;
  visibleNodeCount: number;
}) {
  const label = side === "source" ? "Source" : "Target";
  const isEmpty = graph.nodes.length === 0;
  const isFilterEmpty = !isEmpty && visibleNodeCount === 0;
  const isPartial = graph.diagnostics.length > 0;

  return (
    <section
      aria-label={`${label} graph status`}
      className="min-h-8 px-3 py-1.5 text-[10px] leading-4 text-[var(--g5-text2)]"
    >
      {isEmpty ? (
        <p>{label} has no changes from the shared divergence.</p>
      ) : isFilterEmpty ? (
        <p>{label} has no changes in the selected layers.</p>
      ) : !isPartial && !graph.truncated ? (
        <p>{label} projection is complete.</p>
      ) : null}
      {isPartial && (
        <div className="text-[var(--g5-warn)]">
          <p>{label} projection is partial.</p>
          <ul>
            {graph.diagnostics.map((diagnostic) => (
              <li
                key={diagnostic.code}
                data-testid={`${side}-diagnostic-${diagnostic.code}`}
              >
                {formatDiagnosticCode(diagnostic.code)}: {diagnostic.count}
              </li>
            ))}
          </ul>
        </div>
      )}
      {graph.truncated && (
        <p className="text-[var(--g5-warn)]">
          {label} graph evidence is truncated.
        </p>
      )}
    </section>
  );
}

function sideNeedsStatus(
  graph: SourceControlChangeGraphResponse,
  visibleNodeCount: number,
): boolean {
  return (
    graph.nodes.length === 0 ||
    visibleNodeCount === 0 ||
    graph.diagnostics.length > 0 ||
    graph.truncated
  );
}

function formatDiagnosticCode(code: string): string {
  const label = code.replaceAll("_", " ");
  return label.charAt(0).toUpperCase() + label.slice(1);
}

function LaneLabel({ x, label }: { x: number; label: string }) {
  return (
    <span
      className="pointer-events-none absolute top-4 z-20 text-[9px] font-semibold uppercase tracking-[0.08em] text-[var(--g5-text2)]"
      style={{ left: x }}
    >
      {label}
    </span>
  );
}

function OntologyLane({
  side,
  kind,
  x,
}: {
  side: GraphSide;
  kind: "concept";
  x: number;
}) {
  const label = "Ontology";
  const color = "var(--g5-graph-concept)";
  return (
    <section
      data-testid="rebase-ontology-lane"
      data-side={side}
      data-ontology-kind={kind}
      aria-label={`${side === "source" ? "Source" : "Target"} ${label}`}
      className="pointer-events-none absolute inset-y-0 z-0 border-x border-[var(--g5-stroke)]/30 bg-[var(--g5-bg0)]/15"
      style={{ left: x - 12, width: 234 }}
    >
      <ChangeGraphLaneHeading
        as="h4"
        label={label}
        color={color}
        className="h-9 gap-1.5 border-[var(--g5-stroke)]/35 px-3 text-[9px]"
      />
    </section>
  );
}

function RebaseGraphCard({
  node,
  selected,
  onSelect,
  conflict,
}: {
  node: LayoutNode;
  selected: boolean;
  onSelect: () => void;
  conflict?: RebaseChangeGraphConflict;
}) {
  const meta = LAYER_META[node.layer];
  const Icon = meta.icon;
  const sideLabel = node.side === "source" ? "Source" : "Target";
  const overlapLabel =
    conflict?.kind === "same_block"
      ? `same-block overlap, ${conflict.overlapBlockCount ?? conflict.overlappingBlockIds.length} overlapping blocks`
      : conflict?.kind === "same_node"
        ? "same-node overlap"
        : "no cross-side overlap";
  return (
    <ChangeGraphCard
      ariaLabel={`${sideLabel} ${node.title}, ${displayKind(node)}, ${stateLabel(node.changeState)}, ${overlapLabel}`}
      testId="rebase-graph-node"
      focusState={selected ? "selected" : "default"}
      layer={node.layer}
      side={node.side}
      ontologyKind={node.ontologyKind}
      changeState={node.changeState}
      position={node}
      width={node.width}
      height={NODE_HEIGHT}
      accentColor={meta.color}
      ArtifactIcon={Icon}
      artifactLabel={displayKind(node)}
      title={node.title}
      path={node.path ?? node.route.filePath ?? node.id}
      additions={node.additions}
      deletions={node.deletions}
      compact
      onSelect={onSelect}
    />
  );
}

function NodeDetailsDrawer({
  state,
  onClose,
  onStaleRevision,
}: {
  state:
    | { status: "loading" }
    | { status: "error"; error: Error }
    | { status: "ok"; data: RebaseChangeGraphNodeDetailsResponse };
  onClose: () => void;
  onStaleRevision: () => void;
}) {
  const isStaleRevision =
    state.status === "error" &&
    state.error instanceof ApiError &&
    state.error.status === 409 &&
    state.error.code === "STALE_GRAPH_REVISION";

  useEffect(() => {
    if (isStaleRevision) onStaleRevision();
  }, [isStaleRevision, onStaleRevision]);

  if (isStaleRevision) return null;

  return (
    <aside
      aria-label="Node details"
      data-testid="rebase-graph-node-details"
      className="h-72 shrink-0 border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
    >
      <header className="flex h-11 items-center justify-between border-b border-[var(--g5-stroke)] px-3">
        <div className="min-w-0">
          <h3 className="text-xs font-semibold text-[var(--g5-text0)]">
            Node details
          </h3>
          {state.status === "ok" && (
            <p className="truncate text-[10px] text-[var(--g5-text2)]">
              {state.data.title}
            </p>
          )}
        </div>
        <button
          type="button"
          data-testid="rebase-graph-node-details-close-button"
          aria-label="Close Node details"
          onClick={onClose}
          className="rounded p-1.5 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)]"
        >
          <X aria-hidden="true" size={14} />
        </button>
      </header>
      {state.status === "loading" ? (
        <p className="p-4 text-xs text-[var(--g5-text2)]">
          Loading exact changed blocks…
        </p>
      ) : state.status === "error" ? (
        <p className="p-4 text-xs text-[var(--g5-blocked)]">
          {state.error.message}
        </p>
      ) : (
        <NodeDetailColumns details={state.data} />
      )}
    </aside>
  );
}

function NodeDetailColumns({
  details,
}: {
  details: RebaseChangeGraphNodeDetailsResponse;
}) {
  return (
    <div className="grid h-[calc(100%_-_2.75rem)] min-h-0 grid-cols-[minmax(150px,0.42fr)_minmax(340px,1fr)_minmax(340px,1fr)] overflow-hidden">
      <div
        data-testid="rebase-node-pair-summary"
        className="overflow-auto border-r border-[var(--g5-stroke)] p-3 text-[10px]"
      >
        <DetailsRow label="Artifact" value={details.artifact} />
        <DetailsRow label="Path" value={details.path ?? "No path"} />
        <DetailsRow
          label="Source changed lines"
          value={authoritativeCount(details.source?.changedLineCount)}
        />
        <DetailsRow
          label="Source changed blocks"
          value={authoritativeCount(details.source?.changedBlockCount)}
        />
        <DetailsRow
          label="Source returned block IDs"
          value={changedBlockIds(details.source).join(", ") || "None returned"}
        />
        <DetailsRow
          label="Target changed lines"
          value={authoritativeCount(details.target?.changedLineCount)}
        />
        <DetailsRow
          label="Target changed blocks"
          value={authoritativeCount(details.target?.changedBlockCount)}
        />
        <DetailsRow
          label="Target returned block IDs"
          value={changedBlockIds(details.target).join(", ") || "None returned"}
        />
        <DetailsRow
          label="Overlap lines"
          value={String(details.overlapLineCount)}
        />
        <DetailsRow
          label="Overlap blocks"
          value={String(details.overlapBlockCount)}
        />
        <DetailsRow
          label="Overlap returned block IDs"
          value={details.overlapBlockIds.join(", ") || "None returned"}
        />
      </div>
      <DetailSide
        title="Source changes"
        side={details.source}
        overlapIds={details.overlapBlockIds}
      />
      <DetailSide
        title="Target changes"
        side={details.target}
        overlapIds={details.overlapBlockIds}
        last
      />
    </div>
  );
}

function authoritativeCount(count: number | undefined): string {
  return count === undefined ? "No change" : String(count);
}

function changedBlockIds(
  side: RebaseChangeGraphSideDetails | undefined,
): string[] {
  return side?.detail.kind === "semantic_diff"
    ? side.detail.records.map((record) => record.id)
    : (side?.node.changedRecords.map((record) => record.id) ?? []);
}

function DetailSide({
  title,
  side,
  overlapIds,
  last = false,
}: {
  title: string;
  side: RebaseChangeGraphSideDetails | undefined;
  overlapIds: string[];
  last?: boolean;
}) {
  return (
    <div
      data-testid={
        title === "Source changes"
          ? "rebase-source-changes"
          : "rebase-target-changes"
      }
      className={classNames(
        "overflow-auto p-3 text-xs",
        !last && "border-r border-[var(--g5-stroke)]",
      )}
    >
      <h4 className="mb-2 text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-text1)]">
        {title}
      </h4>
      {side ? (
        <ChangeGraphDetailEvidence
          details={side.detail}
          overlapIds={overlapIds}
          presentation="paired"
        />
      ) : (
        <p className="py-3 text-[var(--g5-text2)]">
          No change on this side, or the node does not exist in this workspace.
        </p>
      )}
    </div>
  );
}

function DetailsRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="mb-2">
      <span className="block text-[var(--g5-text2)]">{label}</span>
      <span className="block min-w-0 break-words font-medium text-[var(--g5-text0)]">
        {value}
      </span>
    </div>
  );
}

function displayKind(node: LayoutNode): string {
  if (node.layer !== "ontology") return LAYER_META[node.layer].label;
  return "Ontology";
}

function stateLabel(state: ChangeGraphState): string {
  return state.charAt(0).toUpperCase() + state.slice(1);
}

function LegendLine({
  color,
  label,
  dashed = false,
}: {
  color: string;
  label: string;
  dashed?: boolean;
}) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <svg aria-hidden="true" width="24" height="8" viewBox="0 0 24 8">
        <path
          d="M 1 4 L 23 4"
          fill="none"
          stroke={color}
          strokeDasharray={dashed ? "4 3" : undefined}
          strokeWidth={dashed ? 1.5 : 3}
        />
      </svg>
      {label}
    </span>
  );
}

function GraphStatus({ message }: { message: string }) {
  return (
    <section
      data-testid="rebase-change-graph-preview"
      className="flex h-48 shrink-0 items-center justify-center rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] text-xs text-[var(--g5-text2)]"
    >
      <RefreshCw aria-hidden="true" size={14} className="mr-2 animate-spin" />
      {message}
    </section>
  );
}

function GraphError({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <section
      data-testid="rebase-change-graph-preview"
      className="flex shrink-0 items-center gap-2 rounded-md border border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 p-3 text-xs text-[var(--g5-text1)]"
    >
      <AlertTriangle
        aria-hidden="true"
        size={15}
        className="shrink-0 text-[var(--g5-blocked)]"
      />
      <span className="min-w-0 flex-1">
        Unable to load the rebase comparison: {message}
      </span>
      <button
        type="button"
        data-testid="rebase-graph-error-retry-button"
        onClick={onRetry}
        className="rounded border border-[var(--g5-blocked)]/50 px-2 py-1 font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
      >
        Retry
      </button>
    </section>
  );
}
