import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ReactFlowDiagramWidget } from "./ReactFlowDiagramWidget";

vi.mock("@xyflow/react", async () => {
  const React = await vi.importActual<typeof import("react")>("react");

  return {
    Background: () => <div data-testid="react-flow-background" />,
    BackgroundVariant: { Dots: "dots" },
    Controls: () => <div data-testid="react-flow-controls" />,
    Handle: () => <span data-testid="react-flow-handle" />,
    MarkerType: { ArrowClosed: "arrowclosed" },
    MiniMap: () => <div data-testid="react-flow-minimap" />,
    Position: { Left: "left", Right: "right" },
    ReactFlow: ({
      children,
      nodes,
    }: {
      children: React.ReactNode;
      nodes: Array<{ id: string; data: { label: string } }>;
    }) => (
      <div data-testid="react-flow-canvas">
        {nodes.map((node) => (
          <div key={node.id}>{node.data.label}</div>
        ))}
        {children}
      </div>
    ),
    useEdgesState: <T,>(initialEdges: T[]) => [
      ...React.useState(initialEdges),
      vi.fn(),
    ],
    useNodesState: <T,>(initialNodes: T[]) => [
      ...React.useState(initialNodes),
      vi.fn(),
    ],
  };
});

const firstGraph = JSON.stringify({
  title: "First graph",
  nodes: [{ id: "alpha", label: "Alpha", detail: "First node" }],
  edges: [],
});

const secondGraph = JSON.stringify({
  title: "Second graph",
  nodes: [{ id: "beta", label: "Beta", detail: "Second node" }],
  edges: [],
});

describe("ReactFlowDiagramWidget", () => {
  it("resets interactive graph state when the markdown graph spec changes", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<ReactFlowDiagramWidget code={firstGraph} />);

    expect(screen.getByRole("heading", { name: "First graph" })).toBeVisible();
    expect(screen.getAllByText("Alpha").length).toBeGreaterThan(0);

    await user.click(screen.getByRole("button", { name: "Add gate" }));
    expect(screen.getAllByText("Review gate 2").length).toBeGreaterThan(0);

    await user.click(screen.getByRole("button", { name: "Export graph" }));
    expect(screen.getByText(/"label": "Review gate 2"/)).toBeVisible();

    rerender(<ReactFlowDiagramWidget code={secondGraph} />);

    expect(screen.getByRole("heading", { name: "Second graph" })).toBeVisible();
    expect(screen.queryByText("Alpha")).not.toBeInTheDocument();
    expect(screen.queryByText("Review gate 2")).not.toBeInTheDocument();
    expect(
      screen.queryByText(/"label": "Review gate 2"/),
    ).not.toBeInTheDocument();
    expect(screen.getAllByText("Beta").length).toBeGreaterThan(0);
  });
});
