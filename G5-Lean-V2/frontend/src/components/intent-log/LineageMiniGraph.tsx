// =============================================================================
// LineageMiniGraph — circular lineage renderer for intent-log details.
// Spec: g5/client/workspace-page/intent-log-table-view [D3],
//       g5/client/workspace-page/intent-log-lineage-graph [D2][D3].
//
// `focus` mode (intent-log row): centers + zooms on the current intent, enlarges
// the center node, and renders large intent-title labels wrapped to ≤3 lines.
// Clicking a node opens the inline IntentLogBlockInspector (content + optional
// diff + "Go to I/D block"); default (non-focus) behavior is unchanged.
// =============================================================================

import {
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { Crosshair, ScanSearch, ZoomIn, ZoomOut } from "lucide-react";

import {
  IntentLogBlockInspector,
  type BlockInspectorData,
} from "./IntentLogBlockInspector";

export interface LineageRenderNode {
  id: string;
  depth: number;
  label: string;
  sublabel: string;
  colorVar: string;
  labelColorVar?: string;
  // Inspector payload (focus mode):
  content?: string | null;
  fileName?: string | null;
  kindName?: string;
  diff?: string | null;
  /** false → the block has no openable I/D markdown (e.g. chat / supervisor). */
  navigable?: boolean;
}

export interface LineageMiniGraphProps {
  centerId: string;
  nodes: LineageRenderNode[];
  edges: Array<{ from: string; to: string }>;
  ancestorCount: number;
  descendantCount: number;
  loading?: boolean;
  maxHeight?: number;
  /** Navigate to the I/D block (markdown) — invoked from the inspector only. */
  onJump?: (id: string) => void;
  /** Intent-log row mode: center + zoom the current node, large wrapped titles, inline inspector. */
  focus?: boolean;
}

interface LayoutConfig {
  COL_GAP: number;
  ROW_GAP: number;
  BAND_GAP: number;
  MAX_PER_ROW: number;
  R: number;
  CR: number;
}

const DEFAULT_CFG: LayoutConfig = {
  COL_GAP: 144,
  ROW_GAP: 92,
  BAND_GAP: 36,
  MAX_PER_ROW: 5,
  R: 22,
  CR: 28,
};

const FOCUS_CFG: LayoutConfig = {
  COL_GAP: 220,
  ROW_GAP: 250,
  BAND_GAP: 64,
  MAX_PER_ROW: 5,
  R: 46,
  CR: 84,
};

function truncateLabel(label: string, max = 18): string {
  return label.length > max ? `${label.slice(0, max - 3)}...` : label;
}

/** Greedy word-wrap into ≤ maxLines lines of ~maxChars; ellipsizes overflow. */
function wrapTitle(text: string, maxChars: number, maxLines: number): string[] {
  const words = (text || "").trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [text || ""];
  const lines: string[] = [];
  let current = "";
  let index = 0;
  for (; index < words.length; index += 1) {
    const word = words[index];
    const candidate = current ? `${current} ${word}` : word;
    if (candidate.length <= maxChars || !current) {
      current = candidate;
    } else {
      lines.push(current);
      current = word;
      if (lines.length === maxLines) break;
    }
  }
  if (lines.length < maxLines) {
    if (current) lines.push(current);
    return lines;
  }
  if (index < words.length - 1 || current) {
    let last = lines[maxLines - 1];
    if (last.length > maxChars - 1) {
      last = last.slice(0, maxChars - 1).replace(/\s+\S*$/, "");
    }
    lines[maxLines - 1] = `${last}…`;
  }
  return lines;
}

function layout(nodes: LineageRenderNode[], cfg: LayoutConfig) {
  const levels = new Map<number, LineageRenderNode[]>();
  for (const node of nodes) {
    const level = levels.get(node.depth) ?? [];
    level.push(node);
    levels.set(node.depth, level);
  }

  const depths = [...levels.keys()].sort((a, b) => a - b);
  const pos: Record<string, { x: number; y: number }> = {};
  let y = cfg.CR + 24;

  for (const depth of depths) {
    const level = levels.get(depth) ?? [];
    const rowCount = Math.max(1, Math.ceil(level.length / cfg.MAX_PER_ROW));
    for (let rowIndex = 0; rowIndex < rowCount; rowIndex += 1) {
      const chunk = level.slice(
        rowIndex * cfg.MAX_PER_ROW,
        (rowIndex + 1) * cfg.MAX_PER_ROW,
      );
      const totalWidth = (chunk.length - 1) * cfg.COL_GAP;
      chunk.forEach((node, index) => {
        pos[node.id] = {
          x: -totalWidth / 2 + index * cfg.COL_GAP,
          y: y + rowIndex * cfg.ROW_GAP,
        };
      });
    }
    y += rowCount * cfg.ROW_GAP + cfg.BAND_GAP;
  }

  const xs = Object.values(pos).map((p) => p.x);
  const minX = xs.length > 0 ? Math.min(...xs) - cfg.CR - 36 : 0;
  const maxX = xs.length > 0 ? Math.max(...xs) + cfg.CR + 36 : 320;

  return {
    pos,
    minX,
    worldW: Math.max(320, maxX - minX),
    worldH: Math.max(140, y),
  };
}

function handleNodeKey(
  event: KeyboardEvent<SVGGElement>,
  nodeId: string,
  onSelect: (id: string) => void,
) {
  if (event.key !== "Enter" && event.key !== " ") return;
  event.preventDefault();
  onSelect(nodeId);
}

export function LineageMiniGraph({
  centerId,
  nodes,
  edges,
  ancestorCount,
  descendantCount,
  loading = false,
  maxHeight = 340,
  onJump,
  focus = false,
}: LineageMiniGraphProps) {
  const cfg = focus ? FOCUS_CFG : DEFAULT_CFG;
  const graphLayout = useMemo(() => layout(nodes, cfg), [nodes, cfg]);
  const byId = useMemo(() => {
    const map: Record<string, LineageRenderNode> = {};
    for (const node of nodes) map[node.id] = node;
    return map;
  }, [nodes]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const dragRef = useRef<{
    sx: number;
    sy: number;
    px: number;
    py: number;
  } | null>(null);
  const didDragRef = useRef(false);
  const svgRef = useRef<SVGSVGElement | null>(null);

  // No wheel handler on purpose: scrolling over the graph bubbles to the
  // intent-log page so the user can scroll to the next record. Zoom is done
  // only via the on-screen +/- / reset controls.

  if (loading) {
    return (
      <div className="space-y-1.5 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-3">
        {[1, 2, 3].map((i) => (
          <div
            key={i}
            className="h-8 animate-pulse rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg1)]"
          />
        ))}
      </div>
    );
  }

  if (ancestorCount === 0 && descendantCount === 0) {
    return (
      <div className="flex items-center gap-2 rounded-[var(--g5-radius-md)] border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3.5 py-3 text-[11.5px] text-[var(--g5-text2)]">
        <ScanSearch className="h-3.5 w-3.5" />
        <span>
          No trace edges returned for this block; it is a chain endpoint.
        </span>
      </div>
    );
  }

  const x = (id: string) => (graphLayout.pos[id]?.x ?? 0) - graphLayout.minX;
  const y = (id: string) => graphLayout.pos[id]?.y ?? 0;

  // Base viewBox: focus mode frames the current node at the center and zooms in.
  let baseX = 0;
  let baseY = 0;
  let baseW = graphLayout.worldW;
  let baseH = graphLayout.worldH;
  if (focus && graphLayout.pos[centerId]) {
    const focusZoom = 1.55;
    baseW = graphLayout.worldW / focusZoom;
    baseH = graphLayout.worldH / focusZoom;
    baseX = x(centerId) - baseW / 2;
    baseY = y(centerId) - baseH / 2;
  }
  // Interactive zoom (about center) + pan, applied in focus mode only.
  const vbW = focus ? baseW / zoom : baseW;
  const vbH = focus ? baseH / zoom : baseH;
  const vbX = focus ? baseX + (baseW - vbW) / 2 + pan.x : baseX;
  const vbY = focus ? baseY + (baseH - vbH) / 2 + pan.y : baseY;
  const viewBox = `${vbX} ${vbY} ${vbW} ${vbH}`;

  // Drag-to-pan. A drag past a small threshold suppresses the node click so
  // panning over a node does not also open its inspector.
  const onPointerDown = (event: ReactPointerEvent<SVGSVGElement>) => {
    if (!focus || event.button !== 0) return;
    didDragRef.current = false;
    dragRef.current = {
      sx: event.clientX,
      sy: event.clientY,
      px: pan.x,
      py: pan.y,
    };
    setDragging(true);
  };
  const onPointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
    const start = dragRef.current;
    if (!start) return;
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0 || rect.height === 0) return;
    const dx = event.clientX - start.sx;
    const dy = event.clientY - start.sy;
    if (!didDragRef.current && Math.hypot(dx, dy) < 3) return;
    didDragRef.current = true;
    setPan({
      x: start.px - (dx * vbW) / rect.width,
      y: start.py - (dy * vbH) / rect.height,
    });
  };
  const endDrag = () => {
    dragRef.current = null;
    setDragging(false);
  };

  const selected = selectedId ? byId[selectedId] : null;
  const inspectorData: BlockInspectorData | null = selected
    ? {
        id: selected.label,
        title: selected.sublabel,
        fileName: selected.fileName ?? null,
        content: selected.content ?? null,
        diff: selected.diff ?? null,
        colorVar: selected.colorVar,
        kindName: selected.kindName,
        isCenter: selected.id === centerId,
        navigable: selected.navigable ?? true,
      }
    : null;

  return (
    <div
      className="relative flex flex-col overflow-hidden rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 pb-1 pt-2"
      style={{ height: maxHeight }}
    >
      {focus && (
        <div className="absolute right-2 top-2 z-10 flex flex-col gap-1">
          {[
            {
              key: "in",
              title: "Zoom in",
              icon: <ZoomIn size={14} />,
              onPress: () => setZoom((z) => Math.min(8, z * 1.2)),
            },
            {
              key: "out",
              title: "Zoom out",
              icon: <ZoomOut size={14} />,
              onPress: () => setZoom((z) => Math.max(0.4, z * 0.83)),
            },
            {
              key: "reset",
              title: "Reset view",
              icon: <Crosshair size={14} />,
              onPress: () => {
                setZoom(1);
                setPan({ x: 0, y: 0 });
              },
            },
          ].map((btn) => (
            <button
              key={btn.key}
              type="button"
              data-testid="lineage-graph-viewport-control"
              aria-label={btn.title}
              title={btn.title}
              onClick={btn.onPress}
              className="flex h-7 w-7 items-center justify-center rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
            >
              {btn.icon}
            </button>
          ))}
        </div>
      )}
      <svg
        ref={svgRef}
        width="100%"
        height={maxHeight - 28}
        viewBox={viewBox}
        preserveAspectRatio="xMidYMid meet"
        className="block"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerLeave={endDrag}
        style={
          focus
            ? { cursor: dragging ? "grabbing" : "grab", touchAction: "none" }
            : undefined
        }
      >
        <defs>
          <marker
            id="intent-log-lineage-arrow"
            viewBox="0 0 10 10"
            refX="8.5"
            refY="5"
            markerWidth="6.5"
            markerHeight="6.5"
            orient="auto-start-reverse"
          >
            <path d="M0 0 L10 5 L0 10 z" fill="var(--g5-text2)" />
          </marker>
        </defs>

        {edges.map((edge, index) => {
          const from = graphLayout.pos[edge.from];
          const to = graphLayout.pos[edge.to];
          if (!from || !to) return null;
          const fromX = x(edge.from);
          const fromY = y(edge.from);
          const toX = x(edge.to);
          const toY = y(edge.to);
          const flowsDown = fromY < toY;
          const startY = fromY + (flowsDown ? cfg.R : -cfg.R);
          const endY = toY + (flowsDown ? -cfg.R : cfg.R);
          const midY = (startY + endY) / 2;
          const touchesCenter = edge.from === centerId || edge.to === centerId;
          const color = touchesCenter
            ? (byId[edge.from]?.colorVar ??
              byId[edge.to]?.colorVar ??
              "var(--g5-text2)")
            : "var(--g5-text2)";

          return (
            <g key={`${edge.from}:${edge.to}:${index}`}>
              {touchesCenter && (
                <path
                  d={`M ${fromX} ${startY} C ${fromX} ${midY} ${toX} ${midY} ${toX} ${endY}`}
                  fill="none"
                  stroke={color}
                  strokeLinecap="round"
                  strokeWidth={focus ? 9 : 6}
                  opacity={0.12}
                />
              )}
              <path
                d={`M ${fromX} ${startY} C ${fromX} ${midY} ${toX} ${midY} ${toX} ${endY}`}
                fill="none"
                stroke={color}
                strokeWidth={touchesCenter ? (focus ? 2.6 : 1.9) : 1.3}
                opacity={touchesCenter ? 0.72 : 0.4}
                markerEnd="url(#intent-log-lineage-arrow)"
              />
            </g>
          );
        })}

        {nodes.map((node) => {
          const isCenter = node.id === centerId;
          const radius = isCenter ? cfg.CR : cfg.R;
          const color = node.colorVar;
          const labelFs = focus
            ? isCenter
              ? node.label.length > 3
                ? 24
                : 28
              : 18
            : node.label.length > 3
              ? 12
              : 13;
          const titleFs = focus ? (isCenter ? 20 : 16) : 10;
          const titleLines = focus
            ? wrapTitle(
                node.sublabel,
                Math.max(
                  8,
                  Math.round((isCenter ? 300 : 240) / (titleFs * 0.56)),
                ),
                3,
              )
            : [truncateLabel(node.sublabel)];

          return (
            <g
              key={node.id}
              transform={`translate(${x(node.id)} ${y(node.id)})`}
              role="button"
              data-testid="lineage-graph-node"
              tabIndex={0}
              aria-label={`Inspect intent block ${node.label} ${node.sublabel}`}
              onClick={() => {
                if (didDragRef.current) {
                  didDragRef.current = false;
                  return;
                }
                setSelectedId(node.id);
              }}
              onKeyDown={(event) =>
                handleNodeKey(event, node.id, setSelectedId)
              }
              style={{ cursor: "pointer" }}
            >
              {isCenter && (
                <>
                  <circle
                    r={radius + (focus ? 36 : 20)}
                    fill={color}
                    opacity={0.14}
                    className="animate-pulse motion-reduce:animate-none"
                  />
                  <circle
                    r={radius + (focus ? 15 : 12)}
                    fill="none"
                    stroke={color}
                    strokeWidth={focus ? 4 : 2.6}
                    opacity={0.95}
                  />
                  <circle
                    r={radius + (focus ? 8 : 7)}
                    fill="none"
                    stroke={color}
                    strokeWidth={1.4}
                    opacity={0.45}
                  />
                </>
              )}
              <circle
                r={radius + 3}
                fill="none"
                stroke={color}
                strokeWidth={1}
                opacity={0.3}
              />
              <circle
                r={radius}
                fill={`color-mix(in srgb, ${color} ${isCenter ? 42 : 17}%, var(--g5-bg1))`}
                stroke={color}
                strokeWidth={isCenter ? (focus ? 4.5 : 3.2) : focus ? 2.2 : 1.6}
              />
              <text
                textAnchor="middle"
                dy={focus ? (isCenter ? 8 : 6) : 4.5}
                fontFamily="var(--g5-font-mono)"
                fontSize={labelFs}
                fontWeight={700}
                fill={node.labelColorVar ?? color}
              >
                {node.label}
              </text>
              <text
                textAnchor="middle"
                x={0}
                y={radius + (focus ? (isCenter ? 26 : 21) : 13)}
                fontSize={titleFs}
                fontWeight={focus ? (isCenter ? 700 : 600) : 400}
                fill={focus && isCenter ? "var(--g5-text0)" : "var(--g5-text2)"}
              >
                {titleLines.map((line, lineIndex) => (
                  <tspan
                    key={lineIndex}
                    x={0}
                    dy={lineIndex === 0 ? 0 : titleFs * 1.16}
                  >
                    {line}
                  </tspan>
                ))}
              </text>
            </g>
          );
        })}
      </svg>
      <div className="mt-1 flex items-center justify-center gap-2 text-[10.5px] text-[var(--g5-text2)]">
        <span>
          {ancestorCount} ancestor{ancestorCount === 1 ? "" : "s"}
        </span>
        <span className="opacity-50">.</span>
        <span>
          {descendantCount} descendant{descendantCount === 1 ? "" : "s"}
        </span>
        <span className="ml-2 italic opacity-70">
          {focus
            ? "drag to pan · ± to zoom · click a block to inspect"
            : "click a block to inspect"}
        </span>
      </div>

      {inspectorData && (
        <IntentLogBlockInspector
          data={inspectorData}
          half={focus}
          onClose={() => setSelectedId(null)}
          onGoto={
            onJump && selected
              ? () => {
                  onJump(selected.id);
                  setSelectedId(null);
                }
              : undefined
          }
        />
      )}
    </div>
  );
}
