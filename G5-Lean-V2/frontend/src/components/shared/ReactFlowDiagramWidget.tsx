import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  MarkerType,
  MiniMap,
  Position,
  ReactFlow,
  type Connection,
  type Edge,
  type Node,
  type NodeProps,
  useEdgesState,
  useNodesState,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import "./ReactFlowDiagramWidget.css";
import { Download, GitBranch, Plus, Sparkles } from "lucide-react";
import { classNames } from "../ui/UI";

type StoryTone = "info" | "ok" | "warn" | "blocked" | "neutral";

interface FlowNodeSpec {
  id: string;
  label: string;
  detail?: string;
  kicker?: string;
  tone?: StoryTone;
  score?: number;
  position?: { x: number; y: number };
}

interface FlowEdgeSpec {
  id?: string;
  source: string;
  target: string;
  label?: string;
  animated?: boolean;
}

interface FlowMetricSpec {
  label: string;
  value: string;
  tone?: StoryTone;
}

interface ReactFlowSpec {
  title?: string;
  subtitle?: string;
  allowlist?: string[];
  metrics?: FlowMetricSpec[];
  nodes?: FlowNodeSpec[];
  edges?: FlowEdgeSpec[];
}

interface StoryNodeData extends Record<string, unknown> {
  label: string;
  detail: string;
  kicker: string;
  tone: StoryTone;
  score: number;
}

type StoryNode = Node<StoryNodeData, "story">;
type StoryEdge = Edge<{ label?: string }>;

export interface ReactFlowDiagramWidgetProps {
  code: string;
}

const toneVar: Record<StoryTone, string> = {
  info: "var(--g5-info)",
  ok: "var(--g5-ok)",
  warn: "var(--g5-warn)",
  blocked: "var(--g5-blocked)",
  neutral: "var(--g5-text2)",
};

const fallbackSpec: Required<Pick<ReactFlowSpec, "nodes" | "edges">> = {
  nodes: [
    {
      id: "author",
      label: "Markdown author",
      detail: "Requests an interactive embedded app.",
      kicker: "intent",
      tone: "info",
      score: 84,
      position: { x: 0, y: 80 },
    },
    {
      id: "sandbox",
      label: "Sandboxed JS",
      detail: "Runs networkless code in a unique-origin iframe.",
      kicker: "isolation",
      tone: "ok",
      score: 92,
      position: { x: 310, y: 20 },
    },
    {
      id: "allowlist",
      label: "Allowlisted library",
      detail: "Host renders approved runtimes like React Flow.",
      kicker: "capability",
      tone: "warn",
      score: 76,
      position: { x: 310, y: 190 },
    },
    {
      id: "ship",
      label: "Production directive",
      detail: "Promote the prototype to a typed markdown node.",
      kicker: "promotion",
      tone: "blocked",
      score: 68,
      position: { x: 640, y: 110 },
    },
  ],
  edges: [
    { source: "author", target: "sandbox", label: "prototype", animated: true },
    { source: "author", target: "allowlist", label: "rich tool" },
    { source: "sandbox", target: "ship", label: "events" },
    { source: "allowlist", target: "ship", label: "schema" },
  ],
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isStoryTone(value: unknown): value is StoryTone {
  return (
    value === "info" ||
    value === "ok" ||
    value === "warn" ||
    value === "blocked" ||
    value === "neutral"
  );
}

function isFlowNodeSpec(value: unknown): value is FlowNodeSpec {
  if (!isRecord(value)) return false;
  if (typeof value.id !== "string" || value.id.trim() === "") return false;
  if (typeof value.label !== "string" || value.label.trim() === "") {
    return false;
  }
  if (value.detail !== undefined && typeof value.detail !== "string") {
    return false;
  }
  if (value.kicker !== undefined && typeof value.kicker !== "string") {
    return false;
  }
  if (value.tone !== undefined && !isStoryTone(value.tone)) return false;
  if (value.score !== undefined && typeof value.score !== "number") {
    return false;
  }
  if (value.position !== undefined) {
    if (!isRecord(value.position)) return false;
    if (
      typeof value.position.x !== "number" ||
      typeof value.position.y !== "number"
    ) {
      return false;
    }
  }
  return true;
}

function isFlowEdgeSpec(value: unknown): value is FlowEdgeSpec {
  if (!isRecord(value)) return false;
  if (value.id !== undefined && typeof value.id !== "string") return false;
  if (typeof value.source !== "string" || value.source.trim() === "") {
    return false;
  }
  if (typeof value.target !== "string" || value.target.trim() === "") {
    return false;
  }
  if (value.label !== undefined && typeof value.label !== "string") {
    return false;
  }
  if (value.animated !== undefined && typeof value.animated !== "boolean") {
    return false;
  }
  return true;
}

function isFlowMetricSpec(value: unknown): value is FlowMetricSpec {
  if (!isRecord(value)) return false;
  if (typeof value.label !== "string" || value.label.trim() === "") {
    return false;
  }
  if (typeof value.value !== "string" || value.value.trim() === "") {
    return false;
  }
  if (value.tone !== undefined && !isStoryTone(value.tone)) return false;
  return true;
}

function parseFlowSpec(code: string): ReactFlowSpec {
  try {
    const parsed = JSON.parse(code) as Record<string, unknown>;
    const rawNodes = parsed.nodes;
    const rawEdges = parsed.edges;
    const hasParsedNodes = Array.isArray(rawNodes);
    const parsedNodes = hasParsedNodes
      ? rawNodes.filter(isFlowNodeSpec)
      : fallbackSpec.nodes;
    const nodes = parsedNodes.length > 0 ? parsedNodes : fallbackSpec.nodes;
    const nodeIds = new Set(nodes.map((node) => node.id));
    const edgeCandidates = Array.isArray(rawEdges)
      ? rawEdges
      : hasParsedNodes
        ? []
        : fallbackSpec.edges;
    const parsedEdges = edgeCandidates.filter(
      (edge): edge is FlowEdgeSpec =>
        isFlowEdgeSpec(edge) &&
        nodeIds.has(edge.source) &&
        nodeIds.has(edge.target),
    );

    return {
      title: typeof parsed.title === "string" ? parsed.title : undefined,
      subtitle:
        typeof parsed.subtitle === "string" ? parsed.subtitle : undefined,
      allowlist: Array.isArray(parsed.allowlist)
        ? parsed.allowlist.filter(
            (item): item is string => typeof item === "string",
          )
        : undefined,
      metrics: Array.isArray(parsed.metrics)
        ? parsed.metrics.filter(isFlowMetricSpec)
        : undefined,
      nodes,
      edges: parsedEdges,
    };
  } catch {
    return {
      title: "React Flow diagram",
      subtitle:
        "The graph spec could not be parsed, so a fallback flow is shown.",
      nodes: fallbackSpec.nodes,
      edges: fallbackSpec.edges,
    };
  }
}

function toNode(spec: FlowNodeSpec, index: number): StoryNode {
  return {
    id: spec.id,
    type: "story",
    position: spec.position ?? {
      x: index * 260,
      y: index % 2 === 0 ? 60 : 220,
    },
    data: {
      label: spec.label,
      detail: spec.detail ?? "No detail provided.",
      kicker: spec.kicker ?? "step",
      tone: spec.tone ?? "neutral",
      score: Math.min(100, Math.max(0, spec.score ?? 72)),
    },
  };
}

function toEdge(spec: FlowEdgeSpec): StoryEdge {
  return {
    id: spec.id ?? `${spec.source}-${spec.target}`,
    source: spec.source,
    target: spec.target,
    label: spec.label,
    animated: Boolean(spec.animated),
    type: "smoothstep",
    markerEnd: { type: MarkerType.ArrowClosed },
    style: { strokeWidth: 2 },
  };
}

const StoryFlowNode: React.FC<NodeProps<StoryNode>> = ({ data, selected }) => {
  const accent = toneVar[data.tone] ?? toneVar.neutral;
  return (
    <div
      className={classNames("g5-reactflow-node", selected && "is-selected")}
      style={{ "--node-accent": accent } as React.CSSProperties}
    >
      <Handle type="target" position={Position.Left} />
      <div className="g5-reactflow-node__top">
        <span className="g5-reactflow-node__kicker">{data.kicker}</span>
        <span className="g5-reactflow-node__badge">{data.score}</span>
      </div>
      <div className="g5-reactflow-node__body">
        <div className="g5-reactflow-node__title">{data.label}</div>
        <div className="g5-reactflow-node__detail">{data.detail}</div>
        <div className="g5-reactflow-node__meter">
          <span style={{ width: `${data.score}%` }} />
        </div>
      </div>
      <Handle type="source" position={Position.Right} />
    </div>
  );
};

const ReactFlowDiagramWidgetInner: React.FC<ReactFlowDiagramWidgetProps> = ({
  code,
}) => {
  const spec = useMemo(() => parseFlowSpec(code), [code]);
  const initialNodes = useMemo(
    () => (spec.nodes ?? fallbackSpec.nodes).map(toNode),
    [spec.nodes],
  );
  const initialEdges = useMemo(
    () => (spec.edges ?? fallbackSpec.edges).map(toEdge),
    [spec.edges],
  );
  const nodeTypes = useMemo(() => ({ story: StoryFlowNode }), []);
  const [nodes, setNodes, onNodesChange] =
    useNodesState<StoryNode>(initialNodes);
  const [edges, setEdges, onEdgesChange] =
    useEdgesState<StoryEdge>(initialEdges);
  const [selectedNodeId, setSelectedNodeId] = useState(
    initialNodes[0]?.id ?? "",
  );
  const [pulse, setPulse] = useState(true);
  const [exported, setExported] = useState("");

  useEffect(() => {
    setNodes(initialNodes);
    setEdges(initialEdges);
    setSelectedNodeId(initialNodes[0]?.id ?? "");
    setExported("");
  }, [initialEdges, initialNodes, setEdges, setNodes]);

  const selectedNode =
    nodes.find((node) => node.id === selectedNodeId) ?? nodes[0] ?? null;

  const onConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target) return;
      setEdges((current) => [
        ...current,
        toEdge({
          source: connection.source ?? "",
          target: connection.target ?? "",
          label: "new edge",
          animated: pulse,
        }),
      ]);
    },
    [pulse, setEdges],
  );

  const addNode = useCallback(() => {
    const nextNumber = nodes.length + 1;
    const sourceId = selectedNode?.id ?? nodes[0]?.id ?? "";
    const nextNode = toNode(
      {
        id: `review-${nextNumber}`,
        label: `Review gate ${nextNumber}`,
        detail: "A newly added human or policy checkpoint.",
        kicker: "new",
        tone: "warn",
        score: 64,
        position: { x: 180 + nextNumber * 70, y: 310 },
      },
      nextNumber,
    );
    setNodes((current) => [...current, nextNode]);
    if (sourceId) {
      setEdges((current) => [
        ...current,
        toEdge({
          source: sourceId,
          target: nextNode.id,
          label: "review",
          animated: pulse,
        }),
      ]);
    }
    setSelectedNodeId(nextNode.id);
  }, [nodes, pulse, selectedNode?.id, setEdges, setNodes]);

  const togglePulse = useCallback(() => {
    setPulse((current) => !current);
    setEdges((current) =>
      current.map((edge) => ({ ...edge, animated: !pulse })),
    );
  }, [pulse, setEdges]);

  const exportGraph = useCallback(() => {
    setExported(
      JSON.stringify(
        {
          nodes: nodes.map((node) => ({
            id: node.id,
            label: node.data.label,
            position: node.position,
          })),
          edges: edges.map((edge) => ({
            source: edge.source,
            target: edge.target,
            label: edge.label,
          })),
        },
        null,
        2,
      ),
    );
  }, [edges, nodes]);

  return (
    <section
      data-testid="react-flow-diagram-widget"
      className="g5-reactflow-widget my-4 overflow-hidden rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
      style={{ borderWidth: "var(--g5-border-w)" }}
    >
      <div className="border-b border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-4 py-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="text-[10px] font-semibold uppercase tracking-normal text-[var(--g5-text2)]">
              Allowlisted library: @xyflow/react
            </div>
            <h3 className="mt-1 text-lg font-semibold leading-tight text-[var(--g5-text0)]">
              {spec.title ?? "React Flow capability map"}
            </h3>
            {spec.subtitle && (
              <p className="mt-1 max-w-2xl text-sm leading-relaxed text-[var(--g5-text1)]">
                {spec.subtitle}
              </p>
            )}
          </div>
          {spec.allowlist && spec.allowlist.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {spec.allowlist.map((library) => (
                <span
                  key={library}
                  className="rounded-full border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 text-[11px] font-medium text-[var(--g5-text1)]"
                >
                  {library}
                </span>
              ))}
            </div>
          )}
        </div>

        {spec.metrics && spec.metrics.length > 0 && (
          <div className="mt-3 grid gap-2 sm:grid-cols-3">
            {spec.metrics.map((metric) => (
              <div
                key={metric.label}
                className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2"
              >
                <div className="text-[10px] font-semibold uppercase tracking-normal text-[var(--g5-text2)]">
                  {metric.label}
                </div>
                <div
                  className="mt-1 text-lg font-semibold"
                  style={{ color: toneVar[metric.tone ?? "neutral"] }}
                >
                  {metric.value}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="grid min-h-[520px] grid-cols-1 lg:grid-cols-[minmax(0,1fr)_260px]">
        <div className="min-h-[520px] bg-[var(--g5-bg0)]">
          <ReactFlow<StoryNode, StoryEdge>
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeClick={(_, node) => setSelectedNodeId(node.id)}
            fitView
            fitViewOptions={{ padding: 0.18 }}
            minZoom={0.45}
            maxZoom={1.7}
            preventScrolling={false}
            zoomOnScroll={false}
            zoomOnPinch={false}
            panOnScroll={false}
            nodesDraggable
            nodesConnectable
            elementsSelectable
          >
            <Background
              color="var(--g5-stroke)"
              gap={22}
              size={1.2}
              variant={BackgroundVariant.Dots}
            />
            <MiniMap
              pannable
              zoomable
              nodeColor={(node) => {
                const tone = node.data.tone;
                return typeof tone === "string" && tone in toneVar
                  ? toneVar[tone as StoryTone]
                  : toneVar.neutral;
              }}
            />
            <Controls showInteractive={false} />
          </ReactFlow>
        </div>

        <aside className="grid content-start gap-3 border-t border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4 lg:border-l lg:border-t-0">
          <div>
            <div className="text-[10px] font-semibold uppercase tracking-normal text-[var(--g5-text2)]">
              Selected step
            </div>
            <h4 className="mt-1 text-base font-semibold text-[var(--g5-text0)]">
              {selectedNode?.data.label ?? "No node selected"}
            </h4>
            <p className="mt-1 text-sm leading-relaxed text-[var(--g5-text1)]">
              {selectedNode?.data.detail ??
                "Click a node in the canvas to inspect it."}
            </p>
          </div>

          <div className="grid gap-2">
            <button
              type="button"
              data-testid="reactflow-add-gate"
              onClick={addNode}
              className="inline-flex items-center justify-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-2 text-sm font-medium text-[var(--g5-text0)] hover:bg-[color-mix(in_srgb,var(--g5-info)_16%,var(--g5-bg2))]"
            >
              <Plus size={15} />
              Add gate
            </button>
            <button
              type="button"
              data-testid="reactflow-toggle-pulse"
              onClick={togglePulse}
              className="inline-flex items-center justify-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-2 text-sm font-medium text-[var(--g5-text0)] hover:bg-[color-mix(in_srgb,var(--g5-ok)_16%,var(--g5-bg2))]"
            >
              <Sparkles size={15} />
              {pulse ? "Calm edges" : "Animate path"}
            </button>
            <button
              type="button"
              data-testid="reactflow-export-graph"
              onClick={exportGraph}
              className="inline-flex items-center justify-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-2 text-sm font-medium text-[var(--g5-text0)] hover:bg-[color-mix(in_srgb,var(--g5-warn)_16%,var(--g5-bg2))]"
            >
              <Download size={15} />
              Export graph
            </button>
          </div>

          <div className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-3">
            <div className="mb-2 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-normal text-[var(--g5-text2)]">
              <GitBranch size={13} />
              Host capability
            </div>
            <p className="text-xs leading-relaxed text-[var(--g5-text1)]">
              Generated markdown declares graph data. The host app supplies the
              approved React Flow runtime.
            </p>
          </div>

          {exported && (
            <pre className="max-h-56 overflow-auto rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 text-[10px] leading-relaxed text-[var(--g5-text1)]">
              {exported}
            </pre>
          )}
        </aside>
      </div>
    </section>
  );
};

export const ReactFlowDiagramWidget: React.FC<ReactFlowDiagramWidgetProps> = ({
  code,
}) => <ReactFlowDiagramWidgetInner key={code} code={code} />;
