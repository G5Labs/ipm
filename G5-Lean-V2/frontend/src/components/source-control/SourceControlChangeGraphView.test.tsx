import { useState } from "react";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AsyncState,
  DiffHunk,
  SourceControlChangeGraphDiffAvailability,
  SourceControlChangeGraphDiffEvidence,
  SourceControlChangeGraphNode,
  SourceControlChangeGraphNodeDetails,
  SourceControlChangeGraphResponse,
  SourceControlChangeGraphSemanticRecordDiff,
} from "../../lib/types";
import { WorkspaceNamesProvider } from "../../hooks/useWorkspaceNames";
import { ApiError } from "../../lib/api";
import { renderApp } from "../../test/render";

const {
  mockUseSourceControlChangeGraph,
  mockUseSourceControlChangeGraphNodeDetails,
} = vi.hoisted(() => ({
  mockUseSourceControlChangeGraph: vi.fn(),
  mockUseSourceControlChangeGraphNodeDetails: vi.fn(),
}));

vi.mock("../../hooks/useSourceControlChangeGraph", () => ({
  useSourceControlChangeGraph: (
    workspaceId: string,
    comparison: string,
    humanIntent: boolean,
  ) => mockUseSourceControlChangeGraph(workspaceId, comparison, humanIntent),
  useSourceControlChangeGraphNodeDetails: (
    workspaceId: string,
    comparison: string,
    revision: string,
    nodeId: string,
    humanIntent: boolean,
  ) =>
    mockUseSourceControlChangeGraphNodeDetails(
      workspaceId,
      comparison,
      revision,
      nodeId,
      humanIntent,
    ),
}));

import { SourceControlChangeGraphView } from "./SourceControlChangeGraphView";
import { SourceControlChangeGraphWhatChanged } from "./SourceControlChangeGraphWhatChanged";

const CHAT_PATH = ".g5/chat_intent/build-current-graph/decisions.md";
const ADDED_CHAT_PATH = ".g5/chat_intent/add-exact-chat-cards/decisions.md";
const DOCUMENT_PATH = "docs/g5-in-g5/change_graph.md";
const CONCEPT_PATH = ".g5/ontology/features/collaborative_editing_ux.g5.md";
const ARCHITECTURE_PATH =
  ".g5/ontology/architecture/source-control-change-graph.g5.md";
const SOURCE_PATH =
  "frontend/src/components/source-control/SourceControlChangeGraphView.tsx";

const graph: SourceControlChangeGraphResponse = {
  revision: "abc123",
  comparison: "workspace",
  range: {
    baseKind: "merge_base",
    parentWorkspaceId: "parent-ws",
    parentWorkspaceName: "Base workspace",
    repositoryCount: 3,
  },
  truncated: false,
  diagnostics: [],
  nodes: [
    {
      id: "conversation",
      column: "intents",
      intentGroup: "conversations",
      title: "Build current graph",
      artifactType: "conversation",
      path: CHAT_PATH,
      changeState: "modified",
      connectionState: "connected",
      hasHumanBlocks: false,
      additions: 1,
      deletions: 1,
      changedRecords: [],
      route: {
        repoId: "code-repo-internal",
        workspaceId: "code-branch",
        fileType: "chat",
        filePath: CHAT_PATH,
      },
    },
    {
      id: "added-chat",
      column: "intents",
      intentGroup: "conversations",
      title: "Add exact chat cards",
      artifactType: "conversation",
      path: ADDED_CHAT_PATH,
      changeState: "added",
      connectionState: "unlinked",
      hasHumanBlocks: false,
      additions: 1,
      deletions: 0,
      changedRecords: [],
      route: {
        repoId: "code-repo-internal",
        workspaceId: "code-branch",
        fileType: "chat",
        filePath: ADDED_CHAT_PATH,
      },
    },
    {
      id: "document",
      column: "intents",
      intentGroup: "documents",
      title: "Change graph plan",
      artifactType: "document",
      path: DOCUMENT_PATH,
      changeState: "modified",
      connectionState: "connected",
      hasHumanBlocks: false,
      additions: 4,
      deletions: 1,
      changedRecords: [],
      route: {
        repoId: "docs-repo-internal",
        workspaceId: "docs-branch",
        fileType: "external_doc",
        filePath: DOCUMENT_PATH,
      },
    },
    {
      id: "concept",
      column: "ontology",
      ontologyGroup: "features",
      title: "collaborative_editing_ux",
      artifactType: "ontology",
      path: CONCEPT_PATH,
      changeState: "modified",
      connectionState: "connected",
      hasHumanBlocks: false,
      changedRecords: [
        {
          id: "g5.collab.edit-window",
          label: "g5.collab.edit-window",
          kind: "D",
          changeState: "modified",
          isHuman: false,
        },
      ],
      route: {
        repoId: "code-repo-internal",
        workspaceId: "code-branch",
        fileType: "ontology",
        filePath: CONCEPT_PATH,
      },
    },
    {
      id: "architecture",
      column: "ontology",
      ontologyGroup: "architecture",
      title: "source-control-change-graph",
      artifactType: "ontology",
      path: ARCHITECTURE_PATH,
      changeState: "added",
      connectionState: "connected",
      hasHumanBlocks: false,
      changedRecords: [
        {
          id: "g5.arch.change-graph",
          label: "g5.arch.change-graph",
          kind: "D",
          changeState: "added",
          isHuman: false,
        },
      ],
      route: {
        repoId: "code-repo-internal",
        workspaceId: "code-branch",
        fileType: "ontology",
        filePath: ARCHITECTURE_PATH,
      },
    },
    {
      id: "source",
      column: "source_code",
      title: "SourceControlChangeGraphView.tsx",
      artifactType: "source_file",
      path: SOURCE_PATH,
      changeState: "added",
      connectionState: "connected",
      hasHumanBlocks: false,
      additions: 120,
      deletions: 0,
      changedRecords: [],
      route: {
        repoId: "code-repo-internal",
        workspaceId: "code-branch",
        fileType: "code",
        filePath: SOURCE_PATH,
      },
    },
  ],
  edges: [
    {
      id: "edge-1",
      sourceNodeId: "concept",
      targetNodeId: "conversation",
      evidence: [
        {
          id: "evidence-1",
          sourceFileType: "ontology",
          sourceFile: CONCEPT_PATH,
          sourceId: "g5.collab.edit-window",
          targetFileType: "chat",
          targetFile: CHAT_PATH,
          targetId: "",
          relationship: "provenance",
          changeState: "added",
        },
        {
          id: "evidence-parallel",
          sourceFileType: "ontology",
          sourceFile: CONCEPT_PATH,
          sourceId: "g5.collab.retry-policy",
          targetFileType: "chat",
          targetFile: CHAT_PATH,
          targetId: "",
          relationship: "provenance",
          changeState: "added",
        },
        {
          id: "evidence-deleted",
          sourceFileType: "ontology",
          sourceFile: CONCEPT_PATH,
          sourceId: "g5.collab.legacy-lock",
          targetFileType: "chat",
          targetFile: CHAT_PATH,
          targetId: "",
          relationship: "provenance",
          changeState: "deleted",
        },
        {
          id: "evidence-unchanged",
          sourceFileType: "ontology",
          sourceFile: CONCEPT_PATH,
          sourceId: "g5.collab.presence",
          targetFileType: "chat",
          targetFile: CHAT_PATH,
          targetId: "",
          relationship: "provenance",
          changeState: "unchanged",
        },
      ],
    },
    {
      id: "edge-2",
      sourceNodeId: "architecture",
      targetNodeId: "document",
      evidence: [
        {
          id: "evidence-architecture-document",
          sourceFileType: "ontology",
          sourceFile: ARCHITECTURE_PATH,
          sourceId: "g5.arch.change-graph",
          targetFileType: "external_doc",
          targetFile: DOCUMENT_PATH,
          targetId: "",
          relationship: "project_document",
          changeState: "added",
        },
      ],
    },
    {
      id: "edge-3",
      sourceNodeId: "concept",
      targetNodeId: "architecture",
      evidence: [
        {
          id: "evidence-concept-architecture",
          sourceFileType: "ontology",
          sourceFile: CONCEPT_PATH,
          sourceId: "g5.collab.edit-window",
          targetFileType: "ontology",
          targetFile: ARCHITECTURE_PATH,
          targetId: "g5.arch.change-graph",
          relationship: "reference",
          changeState: "added",
        },
      ],
    },
    {
      id: "edge-neighbors",
      sourceNodeId: "architecture",
      targetNodeId: "conversation",
      evidence: [
        {
          id: "evidence-architecture-chat",
          sourceFileType: "ontology",
          sourceFile: ARCHITECTURE_PATH,
          sourceId: "g5.arch.change-graph",
          targetFileType: "chat",
          targetFile: CHAT_PATH,
          targetId: "",
          relationship: "provenance",
          changeState: "added",
        },
      ],
    },
    {
      id: "edge-implementation",
      sourceNodeId: "architecture",
      targetNodeId: "source",
      evidence: [
        {
          id: "evidence-implementation",
          sourceFileType: "ontology",
          sourceFile: ARCHITECTURE_PATH,
          sourceId: "g5.arch.change-graph",
          targetFileType: "code",
          targetFile: SOURCE_PATH,
          targetId: "",
          relationship: "implementation",
          changeState: "added",
        },
      ],
    },
  ],
};

const otherIntentNode: SourceControlChangeGraphResponse["nodes"][number] = {
  id: "other",
  column: "intents",
  intentGroup: "other",
  title: "plan.human-view.html",
  artifactType: "document",
  path: "cache/plan.human-view.html",
  changeState: "modified",
  connectionState: "unlinked",
  additions: 1,
  deletions: 1,
  changedRecords: [
    {
      id: "__file__",
      label: "File",
      changeState: "modified",
      isHuman: false,
    },
  ],
  hasHumanBlocks: false,
  route: {
    repoId: "code-repo-internal",
    workspaceId: "code-branch",
    fileType: "external_doc",
    filePath: "cache/plan.human-view.html",
  },
};

function graphNode(id: string): SourceControlChangeGraphNode {
  const node = graph.nodes.find((item) => item.id === id);
  if (!node) throw new Error(`missing fixture node ${id}`);
  return node;
}

function setGraph(state: AsyncState<SourceControlChangeGraphResponse>) {
  mockUseSourceControlChangeGraph.mockReturnValue(state);
}

const detailHunk = {
  startLine: 1,
  header: "@@ -1 +1 @@",
  lines: [
    { type: "deletion" as const, oldLine: 1, content: "old" },
    { type: "addition" as const, newLine: 1, content: "new" },
  ],
};

function availableDiff(
  hunks: DiffHunk[] = [detailHunk],
): SourceControlChangeGraphDiffEvidence {
  return {
    availability: "available",
    additions: 1,
    deletions: 1,
    hunks,
    returnedLineCount: 2,
    omittedLineCount: 0,
    truncated: false,
    partialFinalHunk: false,
  };
}

function statement(
  id: string,
  recordKind: "I" | "D" | "Q",
  title: string,
  changeState: SourceControlChangeGraphSemanticRecordDiff["changeState"],
  extra: Partial<SourceControlChangeGraphSemanticRecordDiff> = {},
): SourceControlChangeGraphSemanticRecordDiff {
  return {
    id,
    recordKind,
    title,
    changeState,
    isHuman: false,
    diff: availableDiff(),
    ...extra,
  };
}

function semanticDetails(
  nodeId: string,
  records: SourceControlChangeGraphSemanticRecordDiff[],
): SourceControlChangeGraphNodeDetails {
  return {
    revision: graph.revision,
    nodeId,
    kind: "semantic_diff",
    records,
    truncated: false,
  };
}

function defaultDetails(nodeId: string): SourceControlChangeGraphNodeDetails {
  // Chat intents are ordinary `.g5/chat_intent/**.md` files with file diffs.
  if (nodeId === "conversation" || nodeId === "added-chat") {
    const added = nodeId === "added-chat";
    return {
      revision: graph.revision,
      nodeId,
      kind: "file_diff",
      diff: availableDiff([
        {
          startLine: 1,
          header: added ? "@@ -0,0 +1 @@" : "@@ -1 +1 @@",
          lines: added
            ? [
                {
                  type: "addition" as const,
                  newLine: 1,
                  content: "Add one card for each registered chat intent.",
                },
              ]
            : [
                {
                  type: "deletion" as const,
                  oldLine: 1,
                  content: "Show the previous change graph.",
                },
                {
                  type: "addition" as const,
                  newLine: 1,
                  content: "Show the current change graph.",
                },
              ],
        },
      ]),
    };
  }
  if (nodeId === "concept") {
    return semanticDetails(nodeId, [
      statement("g5.collab.edit-window", "D", "Graph behavior", "modified"),
    ]);
  }
  if (nodeId === "architecture") {
    return semanticDetails(nodeId, [
      statement("g5.arch.change-graph", "D", "Graph behavior", "added"),
    ]);
  }
  return {
    revision: graph.revision,
    nodeId,
    kind: "file_diff",
    diff: availableDiff(),
  };
}

function RefetchHarness() {
  const [, setVersion] = useState(0);
  return (
    <>
      <button type="button" onClick={() => setVersion((value) => value + 1)}>
        Apply graph refetch
      </button>
      <SourceControlChangeGraphView workspaceId="ws-1" />
    </>
  );
}

function dispatchPointerEvent(
  element: HTMLElement,
  type: "pointerdown" | "pointermove" | "pointerup",
  init: {
    button?: number;
    pointerId: number;
    clientX: number;
    clientY: number;
  },
) {
  const event = new Event(type, { bubbles: true });
  Object.defineProperties(event, {
    button: { value: init.button ?? 0 },
    pointerId: { value: init.pointerId },
    clientX: { value: init.clientX },
    clientY: { value: init.clientY },
  });
  fireEvent(element, event);
}

describe("SourceControlChangeGraphView", () => {
  beforeEach(() => {
    mockUseSourceControlChangeGraph.mockReset();
    mockUseSourceControlChangeGraphNodeDetails.mockReset();
    mockUseSourceControlChangeGraphNodeDetails.mockImplementation(
      (
        _workspaceId: string,
        _comparison: string,
        _revision: string,
        nodeId: string,
      ) => ({
        status: "ok",
        data: defaultDetails(nodeId),
      }),
    );
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      configurable: true,
      value: vi.fn(),
    });
    Object.defineProperty(HTMLElement.prototype, "setPointerCapture", {
      configurable: true,
      value: vi.fn(),
    });
  });

  it("renders Chats, Documents, and Other in order", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: [...graph.nodes, otherIntentNode],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(
      screen.getByRole("region", { name: "Source Control Diff Graph" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Diff Graph" }),
    ).toBeInTheDocument();
    for (const label of ["Intents", "Ontology", "Source Code"]) {
      expect(screen.getByRole("heading", { name: label })).toBeInTheDocument();
    }
    expect(
      screen.queryByRole("heading", { name: "Ontology Concepts" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Software Architecture" }),
    ).not.toBeInTheDocument();
    const documentsLabel = screen.getByRole("heading", {
      level: 4,
      name: "Documents",
    });
    const chatsLabel = screen.getByRole("heading", {
      level: 4,
      name: "Chats",
    });
    const otherLabel = screen.getByRole("heading", {
      level: 4,
      name: "Other",
    });
    const documentCard = screen.getByRole("button", {
      name: /Change graph plan, Document, Modified/i,
    });
    const chatCard = screen.getByRole("button", {
      name: /Build current graph, Chat, Modified/i,
    });
    const lastChatCard = screen.getByRole("button", {
      name: /Add exact chat cards, Chat, Added/i,
    });
    const otherCard = screen.getByRole("button", {
      name: /plan\.human-view\.html, Other, Modified/i,
    });
    const documentsTop = Number.parseFloat(documentsLabel.style.top);
    const chatsTop = Number.parseFloat(chatsLabel.style.top);
    const documentTop = Number.parseFloat(documentCard.style.top);
    const chatTop = Number.parseFloat(chatCard.style.top);
    const lastChatTop = Number.parseFloat(lastChatCard.style.top);
    const otherTop = Number.parseFloat(otherLabel.style.top);
    const otherCardTop = Number.parseFloat(otherCard.style.top);

    expect(chatsTop).toBe(56);
    expect(Number.parseFloat(chatsLabel.style.height)).toBe(20);
    expect(Number.parseFloat(chatsLabel.style.width)).toBe(236);
    expect(chatTop - (chatsTop + 20)).toBe(12);
    expect(documentsTop - (lastChatTop + 112)).toBe(24);
    expect(documentTop - (documentsTop + 20)).toBe(12);
    expect(otherTop - (documentTop + 112)).toBe(24);
    expect(otherCardTop - (otherTop + 20)).toBe(12);
    expect(documentsLabel).toHaveClass(
      "border-b",
      "font-semibold",
      "text-[var(--g5-text1)]",
    );
    expect(screen.queryByText("Conversations")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("change-graph-card")).toHaveLength(7);
    expect(screen.getAllByTestId("change-graph-card")[0]).toHaveAttribute(
      "data-ui",
      "change-graph-card",
    );
    expect(screen.queryByText("docs-repo-internal")).not.toBeInTheDocument();
    expect(screen.queryByText("code-repo-internal")).not.toBeInTheDocument();
    expect(
      Number.parseFloat(
        screen.getByRole("button", {
          name: /SourceControlChangeGraphView\.tsx, Source File, Added/i,
        }).style.top,
      ),
    ).toBe(56);
    for (const label of [documentsLabel, chatsLabel, otherLabel]) {
      expect(label).toHaveAttribute("data-testid", "change-graph-group-label");
    }
    expect(
      within(otherCard).getByTestId("change-graph-card-eyebrow"),
    ).toHaveTextContent("Other");

    await user.click(otherCard);
    expect(
      within(screen.getByTestId("change-graph-node-details")).getByText(
        "Other",
        { exact: true },
      ),
    ).toBeInTheDocument();
  });

  it("omits empty Intent subgroup headings without reserving layout space", () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.filter(
          (node) =>
            node.column !== "intents" || node.intentGroup === "conversations",
        ),
      },
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(
      screen.queryByRole("heading", { level: 4, name: "Documents" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { level: 4, name: "Other" }),
    ).not.toBeInTheDocument();

    const chatsLabel = screen.getByRole("heading", {
      level: 4,
      name: "Chats",
    });
    const firstChatCard = screen.getByRole("button", {
      name: /Build current graph, Chat, Modified/i,
    });
    expect(Number.parseFloat(chatsLabel.style.top)).toBe(56);
    expect(
      Number.parseFloat(firstChatCard.style.top) -
        (Number.parseFloat(chatsLabel.style.top) + 20),
    ).toBe(12);
  });

  it("starts Documents at the top when it is the only Intent subgroup", () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.filter(
          (node) =>
            node.column !== "intents" || node.intentGroup === "documents",
        ),
      },
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(
      screen.queryByRole("heading", { level: 4, name: "Chats" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { level: 4, name: "Other" }),
    ).not.toBeInTheDocument();

    const documentsLabel = screen.getByRole("heading", {
      level: 4,
      name: "Documents",
    });
    const documentCard = screen.getByRole("button", {
      name: /Change graph plan, Document, Modified/i,
    });
    expect(Number.parseFloat(documentsLabel.style.top)).toBe(56);
    expect(
      Number.parseFloat(documentCard.style.top) -
        (Number.parseFloat(documentsLabel.style.top) + 20),
    ).toBe(12);
  });

  it("places Documents before Other without leading space when Chats is absent", () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: [
          ...graph.nodes.filter(
            (node) =>
              node.column !== "intents" || node.intentGroup === "documents",
          ),
          otherIntentNode,
        ],
      },
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(
      screen.queryByRole("heading", { level: 4, name: "Chats" }),
    ).not.toBeInTheDocument();
    const documentsLabel = screen.getByRole("heading", {
      level: 4,
      name: "Documents",
    });
    const otherLabel = screen.getByRole("heading", {
      level: 4,
      name: "Other",
    });
    const documentCard = screen.getByRole("button", {
      name: /Change graph plan, Document, Modified/i,
    });
    const otherCard = screen.getByRole("button", {
      name: /plan\.human-view\.html, Other, Modified/i,
    });

    expect(Number.parseFloat(documentsLabel.style.top)).toBe(56);
    expect(
      Number.parseFloat(otherLabel.style.top) -
        (Number.parseFloat(documentCard.style.top) + 112),
    ).toBe(24);
    expect(
      Number.parseFloat(otherCard.style.top) -
        (Number.parseFloat(otherLabel.style.top) + 20),
    ).toBe(12);
  });

  it("shows only the empty-lane placeholder when Intents has no cards", () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.filter((node) => node.column !== "intents"),
      },
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    for (const label of ["Documents", "Chats", "Other"]) {
      expect(
        screen.queryByRole("heading", { level: 4, name: label }),
      ).not.toBeInTheDocument();
    }
    const emptyIntents = screen.getByLabelText("Intents: no changed artifacts");
    expect(emptyIntents).toHaveTextContent("No changed artifacts");
    expect(Number.parseFloat(emptyIntents.style.top)).toBe(56);
  });

  it("offers All changes without an Uncommitted only mode, even when asked for one", async () => {
    const user = userEvent.setup();
    setGraph({ status: "ok", data: graph });
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        defaultComparison="uncommitted"
      />,
    );

    const group = screen.getByRole("group", { name: "Diff Graph comparison" });
    expect(
      within(group)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["All changes", "Human intent"]);
    expect(
      screen.queryByRole("button", { name: "Uncommitted only" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "All changes" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(
      screen.getByText("Base workspace → Current workspace"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/Current commit → Working changes/),
    ).not.toBeInTheDocument();
    for (const call of mockUseSourceControlChangeGraph.mock.calls) {
      expect(call).toEqual(["ws-1", "workspace", false]);
    }

    await user.click(
      screen.getByRole("button", { name: /SourceControlChangeGraphView/i }),
    );
    expect(mockUseSourceControlChangeGraphNodeDetails).toHaveBeenLastCalledWith(
      "ws-1",
      "workspace",
      "abc123",
      "source",
      false,
    );
    // Re-selecting the active base keeps the selection.
    await user.click(screen.getByRole("button", { name: "All changes" }));
    expect(screen.getByTestId("change-graph-node-details")).toBeInTheDocument();
  });

  it("layers a Human intent lens over All changes and clears selection", async () => {
    const user = userEvent.setup();
    setGraph({ status: "ok", data: graph });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const group = screen.getByRole("group", { name: "Diff Graph comparison" });
    const humanButton = within(group).getByRole("button", {
      name: "Human intent",
    });
    expect(humanButton).toHaveAttribute("aria-pressed", "false");
    expect(humanButton).toBeEnabled();
    // Default All changes base with the lens off.
    expect(mockUseSourceControlChangeGraph).toHaveBeenCalledWith(
      "ws-1",
      "workspace",
      false,
    );

    // Open a node so we can prove enabling the lens clears the selection.
    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );
    expect(screen.getByTestId("change-graph-node-details")).toBeInTheDocument();

    await user.click(humanButton);
    await waitFor(() => {
      expect(humanButton).toHaveAttribute("aria-pressed", "true");
      expect(
        screen.queryByTestId("change-graph-node-details"),
      ).not.toBeInTheDocument();
      // The lens layers over All changes.
      expect(mockUseSourceControlChangeGraph).toHaveBeenLastCalledWith(
        "ws-1",
        "workspace",
        true,
      );
    });

    // Re-selecting the active lens is a no-op that keeps a new selection.
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    expect(mockUseSourceControlChangeGraphNodeDetails).toHaveBeenLastCalledWith(
      "ws-1",
      "workspace",
      "abc123",
      "concept",
      true,
    );
    await user.click(humanButton);
    expect(screen.getByTestId("change-graph-node-details")).toBeInTheDocument();

    // All changes turns the lens back off and resets the scoped selection.
    await user.click(screen.getByRole("button", { name: "All changes" }));
    await waitFor(() => {
      expect(humanButton).toHaveAttribute("aria-pressed", "false");
      expect(
        screen.queryByTestId("change-graph-node-details"),
      ).not.toBeInTheDocument();
      expect(mockUseSourceControlChangeGraph).toHaveBeenLastCalledWith(
        "ws-1",
        "workspace",
        false,
      );
    });
    expect(mockUseSourceControlChangeGraph).not.toHaveBeenCalledWith(
      "ws-1",
      "uncommitted",
      expect.anything(),
    );
  });

  it("disables the Human intent lens when All changes is unavailable", async () => {
    const user = userEvent.setup();
    setGraph({
      status: "error",
      error: new ApiError({
        code: "DIFF_GRAPH_UNAVAILABLE",
        status: 409,
        message: "Workspace Diff Graph comparison is unavailable",
        details: { reason: "no_common_ancestor", missingRepoCount: 1 },
      }),
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const humanButton = screen.getByRole("button", { name: "Human intent" });
    expect(humanButton).toBeDisabled();
    expect(screen.getByRole("button", { name: "All changes" })).toBeDisabled();
    await user.click(humanButton);
    expect(humanButton).toHaveAttribute("aria-pressed", "false");
    // There is no uncommitted base to fall back to.
    for (const call of mockUseSourceControlChangeGraph.mock.calls) {
      expect(call).toEqual(["ws-1", "workspace", false]);
    }
  });

  it("shows the human-intent empty state when the lens yields no nodes", async () => {
    const user = userEvent.setup();
    setGraph({ status: "ok", data: { ...graph, nodes: [], edges: [] } });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(screen.getByText("No workspace changes")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Human intent" }));
    await waitFor(() =>
      expect(screen.getByText("No human-authored changes")).toBeInTheDocument(),
    );
    expect(
      screen.getByText(
        "No human-tagged intent blocks changed in this comparison.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("No workspace changes")).not.toBeInTheDocument();
  });

  it("renders responsive range metadata with the full revision accessible", () => {
    const fullRevision = "f55a0bf0e2b9bcf8";
    setGraph({
      status: "ok",
      data: { ...graph, revision: fullRevision },
    });
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        defaultComparison="workspace"
      />,
    );

    const title = screen.getByRole("heading", { level: 2, name: "Diff Graph" });
    expect(title.closest("header")).toHaveClass("flex-wrap");
    expect(title.parentElement).toHaveClass(
      "min-w-0",
      "basis-48",
      "grow",
      "flex-col",
      "sm:flex-row",
    );

    const range = screen.getByText("Base workspace → Current workspace");
    expect(range).toHaveClass(
      "min-w-0",
      "flex-1",
      "truncate",
      "text-[11px]",
      "text-[var(--g5-text1)]",
    );
    expect(range).toHaveAttribute(
      "title",
      "Base workspace → Current workspace",
    );
    expect(range.parentElement).toHaveClass(
      "flex",
      "w-full",
      "min-w-0",
      "max-w-full",
      "overflow-hidden",
      "sm:w-auto",
      "sm:flex-1",
    );

    const revision = screen.getByLabelText(`Revision ${fullRevision}`);
    expect(revision).toHaveTextContent("rev f55a0bf");
    expect(revision).toHaveAttribute("title", `Revision ${fullRevision}`);
    expect(revision).toHaveAttribute("tabindex", "0");
    expect(
      screen.getByRole("group", { name: "Diff Graph comparison" })
        .parentElement,
    ).toHaveClass("ml-auto", "shrink-0");
  });

  it("preserves a pointer-interactive graph band while Node details is open", async () => {
    const user = userEvent.setup();
    setGraph({ status: "ok", data: graph });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(screen.getByTestId("change-graph-viewport")).toHaveClass("min-h-44");

    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );

    expect(screen.getByTestId("change-graph-node-details")).toHaveClass(
      "min-h-40",
      "shrink",
    );
    expect(screen.getByTestId("change-graph-node-details")).not.toHaveClass(
      "shrink-0",
    );
  });

  it("always exposes comparison controls and honors the Workspace default", () => {
    setGraph({ status: "ok", data: graph });
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        defaultComparison="workspace"
      />,
    );

    expect(mockUseSourceControlChangeGraph).toHaveBeenCalledWith(
      "ws-1",
      "workspace",
      false,
    );
    expect(
      screen.getByRole("group", { name: "Diff Graph comparison" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Base workspace → Current workspace"),
    ).toBeInTheDocument();
  });

  it("explains an unrelated-history failure without an Uncommitted escape", () => {
    setGraph({
      status: "error",
      error: new ApiError({
        code: "DIFF_GRAPH_UNAVAILABLE",
        status: 409,
        message: "Workspace Diff Graph comparison is unavailable",
        details: { reason: "no_common_ancestor", missingRepoCount: 1 },
      }),
    });
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        defaultComparison="workspace"
      />,
    );

    expect(screen.getByText("All changes unavailable")).toBeInTheDocument();
    expect(
      screen.getByText(
        "The workspace and its parent have no common Git ancestor.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Uncommitted changes remain/)).toBeNull();
    expect(screen.queryByText("Loading workspace range…")).toBeNull();
    expect(screen.queryByTestId("change-graph-message-loader-icon")).toBeNull();
    expect(
      screen.queryByRole("button", { name: "View uncommitted changes" }),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("change-graph-retry")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "All changes" })).toBeDisabled();
  });

  it.each([
    [
      "uncommitted_unsupported",
      "Uncommitted only is not available yet. All changes includes saved uncommitted edits.",
    ],
    ["not_forked", "The All changes comparison requires a parent workspace."],
    ["base_workspace", "Diff Graph is not available for Base workspaces."],
  ])("explains the %s unavailable reason", (reason, copy) => {
    setGraph({
      status: "error",
      error: new ApiError({
        code: "DIFF_GRAPH_UNAVAILABLE",
        status: 409,
        message: "Workspace Diff Graph comparison is unavailable",
        details: { reason },
      }),
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(screen.getByText("All changes unavailable")).toBeInTheDocument();
    expect(screen.getByText(copy)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "View uncommitted changes" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Human intent" })).toBeDisabled();
  });

  it("formats each crossed Diff Graph size limit and keeps terminal errors settled", () => {
    const cases = [
      {
        details: {
          observedRoots: 501,
          maxRoots: 500,
          observedBytes: 1024,
          maxBytes: 100 * 1024 * 1024,
        },
        expected:
          "This comparison has 501 changed artifacts; the limit is 500.",
      },
      {
        details: {
          observedRoots: 128,
          maxRoots: 500,
          observedBytes: 130_000_000,
          maxBytes: 100 * 1024 * 1024,
        },
        expected:
          "This comparison requires 124 MiB of diff input; the limit is 100 MiB.",
      },
      {
        details: {
          observedRoots: 501,
          maxRoots: 500,
          observedBytes: 130_000_000,
          maxBytes: 100 * 1024 * 1024,
        },
        expected:
          "This comparison exceeds both supported limits: 501 changed artifacts (limit 500) and 124 MiB of diff input (limit 100 MiB).",
      },
      {
        details: { observedRoots: 128, maxRoots: 500 },
        expected:
          "This complete workspace comparison exceeds the supported size.",
      },
    ];

    for (const testCase of cases) {
      setGraph({
        status: "error",
        error: new ApiError({
          code: "DIFF_GRAPH_TOO_LARGE",
          status: 413,
          message: "Workspace Diff Graph comparison is too large",
          details: testCase.details,
        }),
      });
      const view = renderApp(
        <SourceControlChangeGraphView
          workspaceId="ws-1"
          defaultComparison="workspace"
        />,
      );

      expect(
        screen.getByText("All changes comparison is too large"),
      ).toBeInTheDocument();
      expect(screen.getByText(testCase.expected)).toBeInTheDocument();
      expect(screen.queryByText(/uncommitted/i)).toBeNull();
      expect(
        screen.queryByRole("button", { name: "View uncommitted changes" }),
      ).not.toBeInTheDocument();
      expect(screen.queryByTestId("change-graph-retry")).toBeNull();
      expect(screen.queryByText("Loading workspace range…")).toBeNull();
      expect(
        screen.queryByTestId("change-graph-message-loader-icon"),
      ).toBeNull();
      view.unmount();
    }
  });

  it("labels Project Doc paths with workspace names without changing document routes", async () => {
    const workspaceUuid = "019fecb3-0701-7170-aabe-da851b3e4c42";
    const canonicalPath = `docs/${workspaceUuid}/change_graph.md`;
    const documentRoute = {
      ...graph.nodes.find((node) => node.id === "document")!.route,
      filePath: canonicalPath,
    };
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) =>
          node.id === "document"
            ? { ...node, path: canonicalPath, route: documentRoute }
            : node,
        ),
      },
    });
    const onOpenDocument = vi.fn();
    const user = userEvent.setup();
    renderApp(
      <WorkspaceNamesProvider value={{ [workspaceUuid]: "Review Workspace" }}>
        <SourceControlChangeGraphView
          workspaceId="ws-1"
          onOpenDocument={onOpenDocument}
        />
      </WorkspaceNamesProvider>,
    );

    const documentCard = screen.getByRole("button", {
      name: /Change graph plan, Document, Modified/i,
    });
    expect(documentCard).toHaveTextContent(
      "docs/Review Workspace/change_graph.md",
    );
    expect(documentCard).not.toHaveTextContent(workspaceUuid);

    await user.click(documentCard);
    const details = screen.getByTestId("change-graph-node-details");
    expect(details).toHaveTextContent("docs/Review Workspace/change_graph.md");
    expect(details).not.toHaveTextContent(workspaceUuid);

    await user.click(screen.getByRole("button", { name: "Open document" }));
    expect(onOpenDocument).toHaveBeenCalledWith(documentRoute);
  });

  it("renders semantic lane hierarchy with compact rows and empty-lane placeholders", () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes
          .filter((node) => node.column === "intents")
          .map((node) =>
            node.id === "document"
              ? {
                  ...node,
                  title:
                    "A deliberately long document title that needs two bounded lines",
                }
              : node,
          ),
        edges: [],
      },
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const intentColumn = screen
      .getByRole("heading", { name: "Intents" })
      .closest("[data-testid='change-graph-column']");
    expect(intentColumn).toHaveAttribute("data-column", "intents");
    expect(intentColumn).toHaveClass(
      "border-[var(--g5-stroke)]/35",
      "bg-transparent",
    );
    const intentHeading = screen.getByRole("heading", { name: "Intents" });
    expect(intentHeading).toHaveStyle({ color: "var(--g5-graph-intent)" });
    expect(
      within(intentHeading).getByTestId("change-graph-lane-marker"),
    ).toBeInTheDocument();

    const documentCard = screen.getByRole("button", {
      name: /A deliberately long document title.*Document.*Modified/i,
    });
    expect(documentCard).toHaveAttribute("data-column", "intents");
    expect(
      within(documentCard).getByTestId("change-graph-card-icon"),
    ).toBeInTheDocument();
    expect(
      within(documentCard).getByTestId("change-graph-card-eyebrow"),
    ).toHaveTextContent("Document");
    expect(
      within(documentCard).getByTestId("change-graph-card-title"),
    ).toHaveClass("line-clamp-2");
    expect(
      within(documentCard).getByText("Modified", { exact: true }),
    ).toHaveClass("border-[var(--g5-warn)]/45", "font-semibold", "uppercase");

    const unlinkedChat = screen.getByRole("button", {
      name: /Add exact chat cards, Chat, Added/i,
    });
    expect(
      within(unlinkedChat).getByTestId("change-graph-card-eyebrow"),
    ).toHaveTextContent(/^Chat$/);

    const chats = [
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
      screen.getByRole("button", {
        name: /Add exact chat cards, Chat, Added/i,
      }),
    ];
    expect(
      Number.parseFloat(chats[1].style.top) -
        (Number.parseFloat(chats[0].style.top) + 112),
    ).toBe(16);

    const placeholders = screen.getAllByTestId("change-graph-empty-lane");
    expect(placeholders).toHaveLength(2);
    expect(placeholders.map((item) => item.dataset.column)).toEqual([
      "ontology",
      "source_code",
    ]);
    expect(placeholders.map((item) => item.getAttribute("aria-label"))).toEqual(
      ["Ontology: no changed artifacts", "Source Code: no changed artifacts"],
    );
    for (const placeholder of placeholders) {
      expect(placeholder).toHaveTextContent("No changed artifacts");
      expect(Number.parseFloat(placeholder.style.top)).toBe(56);
    }
  });

  it("keeps the graph mounted while selection opens Node details", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );
    expect(
      screen.getByTestId("source-control-change-graph"),
    ).toBeInTheDocument();
    const details = screen.getByTestId("change-graph-node-details");
    expect(details).toBeInTheDocument();
    expect(
      within(details).getByText("Show the current change graph."),
    ).toBeInTheDocument();
    expect(
      within(details).getByText("Chat", { exact: true }),
    ).toBeInTheDocument();
    expect(within(details).getByText(CHAT_PATH)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open document/i })).toBeNull();
    expect(
      screen.queryByTestId("change-graph-node-details-action"),
    ).not.toBeInTheDocument();
  });

  it("loads exact type-specific What Changed evidence for the selected node", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );
    expect(mockUseSourceControlChangeGraphNodeDetails).toHaveBeenCalledWith(
      "ws-1",
      "workspace",
      "abc123",
      "conversation",
      false,
    );
    // A chat intent is a file: its What changed is the file diff.
    const chatDiff = screen.getByTestId("change-graph-file-diff");
    const previous = within(chatDiff).getByText(
      "Show the previous change graph.",
    );
    const current = within(chatDiff).getByText(
      "Show the current change graph.",
    );
    expect(
      previous.compareDocumentPosition(current) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      screen.queryByTestId("change-graph-changed-semantic-blocks-section"),
    ).not.toBeInTheDocument();

    await user.click(
      screen.getByRole("button", {
        name: /Change graph plan, Document, Modified/i,
      }),
    );
    expect(screen.getByTestId("change-graph-file-diff")).toBeInTheDocument();
    expect(screen.getByText("old")).toBeInTheDocument();
    expect(screen.getByText("new")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    const semanticDiff = screen.getByTestId("change-graph-semantic-diff");
    expect(semanticDiff).toBeInTheDocument();
    expect(
      within(semanticDiff).getByText("g5.collab.edit-window"),
    ).toBeInTheDocument();
    expect(
      within(semanticDiff).getByText("Graph behavior"),
    ).toBeInTheDocument();
  });

  it("lists every changed statement kind for ontology and Project Docs without showing Origin", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) => {
          if (node.id === "concept") return { ...node, changeOrigin: "both" };
          if (node.id === "document") {
            return {
              ...node,
              path: "docs/ws-1/change_graph.md",
              route: {
                ...node.route,
                filePath: "docs/ws-1/change_graph.md",
              },
            };
          }
          return node;
        }),
      },
    });
    mockUseSourceControlChangeGraphNodeDetails.mockImplementation(
      (
        _workspaceId: string,
        _comparison: string,
        _revision: string,
        nodeId: string,
      ) => {
        if (nodeId === "concept") {
          return {
            status: "ok",
            data: semanticDetails(nodeId, [
              statement(
                "g5.collab.intent",
                "I",
                "Exact Project Doc intent",
                "modified",
              ),
              statement(
                "g5.collab.decision",
                "D",
                "Full authored block diff",
                "added",
              ),
              statement(
                "g5.collab.question",
                "Q",
                "Question is a statement too",
                "modified",
              ),
              statement("g5.collab.untitled-intent", "I", "", "deleted"),
              statement("g5.collab.untitled-decision", "D", "", "added"),
              statement("g5.collab.untitled-question", "Q", "", "modified"),
            ]),
          };
        }
        if (nodeId === "architecture") {
          return { status: "ok", data: semanticDetails(nodeId, []) };
        }
        if (nodeId === "document") {
          return {
            status: "ok",
            data: semanticDetails(nodeId, [
              statement("D1", "D", "Project Doc decision", "modified"),
            ]),
          };
        }
        return { status: "ok", data: defaultDetails(nodeId) };
      },
    );
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    const left = screen.getByTestId("change-graph-node-details-left");
    const overview = within(left).getByTestId(
      "change-graph-changed-semantic-blocks-section",
    );
    const relationships = within(left).getByTestId(
      "change-graph-grounding-edges-section",
    );
    expect(
      overview.compareDocumentPosition(relationships) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const overviewHeading = within(overview).getByRole("heading", {
      level: 4,
      name: "Changed statements",
    });
    const rows = within(overview).getAllByRole("listitem");
    expect(
      rows.map((row) => [
        row.getAttribute("data-change-state"),
        row.textContent,
      ]),
    ).toEqual([
      ["modified", "g5.collab.intentExact Project Doc intentModified"],
      ["added", "g5.collab.decisionFull authored block diffAdded"],
      ["modified", "g5.collab.questionQuestion is a statement tooModified"],
      ["deleted", "g5.collab.untitled-intentIntentDeleted"],
      ["added", "g5.collab.untitled-decisionDecisionAdded"],
      ["modified", "g5.collab.untitled-questionQuestionModified"],
    ]);
    expect(within(left).queryByText("Origin", { exact: true })).toBeNull();
    expect(
      within(left).queryByText("Changed I/D Blocks"),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId("change-graph-node-details-right")).getByText(
        "Question is a statement too",
      ),
    ).toBeInTheDocument();
    const relationshipsHeading = within(relationships).getByRole("heading", {
      level: 4,
      name: "Relationships",
    });
    expect(within(left).queryByText("Grounding Edges")).not.toBeInTheDocument();
    for (const className of [
      "text-[10px]",
      "font-semibold",
      "uppercase",
      "tracking-[0.08em]",
      "text-[var(--g5-text1)]",
    ]) {
      expect(overviewHeading).toHaveClass(className);
      expect(relationshipsHeading).toHaveClass(className);
    }

    await user.click(
      screen.getByRole("button", {
        name: /Change graph plan, Document, Modified/i,
      }),
    );
    expect(
      within(
        screen.getByTestId("change-graph-changed-semantic-blocks-section"),
      ).getByText("Project Doc decision"),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", {
        name: /source-control-change-graph, Ontology, Added/i,
      }),
    );
    expect(
      within(
        screen.getByTestId("change-graph-changed-semantic-blocks-section"),
      ).getByText("No statements changed."),
    ).toBeInTheDocument();
  });

  it("marks human statements in both detail columns without requiring the lens", async () => {
    setGraph({ status: "ok", data: graph });
    mockUseSourceControlChangeGraphNodeDetails.mockImplementation(
      (
        _workspaceId: string,
        _comparison: string,
        _revision: string,
        nodeId: string,
      ) => {
        if (nodeId !== "concept") {
          return { status: "ok", data: defaultDetails(nodeId) };
        }
        return {
          status: "ok",
          data: semanticDetails(nodeId, [
            statement(
              "g5.collab.approval-window",
              "I",
              "Human-set approval window",
              "modified",
              { isHuman: true },
            ),
            statement(
              "g5.collab.retry-policy",
              "D",
              "Agent-set retry policy",
              "added",
            ),
            statement(
              "g5.collab.answered-question",
              "Q",
              "Human-answered question",
              "added",
              { isHuman: true },
            ),
          ]),
        };
      },
    );
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(
      screen.getByRole("button", { name: "Human intent" }),
    ).toHaveAttribute("aria-pressed", "false");
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );

    const overview = screen.getByTestId(
      "change-graph-changed-semantic-blocks-section",
    );
    const overviewRow = (title: string) => {
      const row = within(overview).getByText(title).closest("li");
      expect(row).not.toBeNull();
      return row!;
    };
    for (const title of [
      "Human-set approval window",
      "Human-answered question",
    ]) {
      expect(within(overviewRow(title)).getByText("Human")).toHaveClass(
        "bg-[var(--g5-ok)]/15",
        "text-[var(--g5-ok)]",
      );
    }
    expect(
      within(overviewRow("Agent-set retry policy")).queryByText("Human"),
    ).toBeNull();

    const semanticDiff = screen.getByTestId("change-graph-semantic-diff");
    const humanDiffHeader = within(semanticDiff)
      .getByText("Human-set approval window")
      .closest("header");
    const agentDiffHeader = within(semanticDiff)
      .getByText("Agent-set retry policy")
      .closest("header");
    expect(humanDiffHeader).not.toBeNull();
    expect(agentDiffHeader).not.toBeNull();
    expect(within(humanDiffHeader!).getByText("Human")).toHaveClass(
      "bg-[var(--g5-ok)]/15",
      "text-[var(--g5-ok)]",
    );
    expect(within(agentDiffHeader!).queryByText("Human")).toBeNull();
    // A Question answered by a person is human in the diff column too.
    const questionDiffHeader = within(semanticDiff)
      .getByText("Human-answered question")
      .closest("header");
    expect(within(questionDiffHeader!).getByText("Human")).toBeVisible();
  });

  it("under the lens shows only human statements with author + timestamp", async () => {
    setGraph({ status: "ok", data: graph });
    mockUseSourceControlChangeGraphNodeDetails.mockImplementation(
      (
        _workspaceId: string,
        _comparison: string,
        _revision: string,
        nodeId: string,
      ) => {
        if (nodeId === "concept") {
          return {
            status: "ok",
            data: semanticDetails(nodeId, [
              statement(
                "g5.collab.human-decision",
                "D",
                "Human authored decision",
                "modified",
                {
                  isHuman: true,
                  author: "Ada Lovelace",
                  authorSource: "record",
                  timestamp: "2026-01-02T03:04:05Z",
                  timestampSource: "record",
                },
              ),
              statement(
                "g5.collab.no-provenance",
                "D",
                "Human block missing provenance",
                "added",
                {
                  isHuman: true,
                  author: undefined,
                  authorSource: "unknown",
                  timestamp: undefined,
                  timestampSource: "unknown",
                },
              ),
              statement(
                "g5.collab.deleted-human",
                "D",
                "Deleted human block last-known",
                "deleted",
                {
                  isHuman: true,
                  author: "Grace Hopper",
                  authorSource: "git_blame",
                  timestamp: "2025-12-31T00:00:00Z",
                  timestampSource: "git_blame",
                },
              ),
              statement(
                "g5.collab.answered-question",
                "Q",
                "Question answered by a person",
                "modified",
                { isHuman: true, author: "Ada Lovelace" },
              ),
              statement(
                "g5.collab.agent-intent",
                "I",
                "Agent authored block",
                "modified",
              ),
            ]),
          };
        }
        return { status: "ok", data: defaultDetails(nodeId) };
      },
    );
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const humanButton = screen.getByRole("button", { name: "Human intent" });
    await user.click(humanButton);
    await waitFor(() =>
      expect(humanButton).toHaveAttribute("aria-pressed", "true"),
    );
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    expect(mockUseSourceControlChangeGraphNodeDetails).toHaveBeenLastCalledWith(
      "ws-1",
      "workspace",
      "abc123",
      "concept",
      true,
    );
    const overview = within(
      screen.getByTestId("change-graph-node-details-left"),
    ).getByTestId("change-graph-changed-semantic-blocks-section");

    // Human rows render with author + timestamp.
    expect(
      within(overview).getByText("g5.collab.human-decision"),
    ).toBeInTheDocument();
    expect(
      within(overview).getByText("Human authored decision"),
    ).toBeInTheDocument();
    expect(within(overview).getAllByText("Ada Lovelace")).toHaveLength(2);
    // A deleted human row shows its last-known author, flagged as git blame.
    expect(
      within(overview).getByText("g5.collab.deleted-human"),
    ).toBeInTheDocument();
    expect(within(overview).getByText("Grace Hopper")).toBeInTheDocument();
    expect(within(overview).getAllByText("(git blame)").length).toBeGreaterThan(
      0,
    );
    // A human block without provenance shows an explicit "unknown", never faked.
    expect(
      within(overview).getByText("g5.collab.no-provenance"),
    ).toBeInTheDocument();
    expect(within(overview).getAllByText("unknown").length).toBeGreaterThan(0);
    expect(within(overview).getAllByText("(unknown)").length).toBeGreaterThan(
      0,
    );
    // Human Questions are kept: the lens has no I/D-only filter.
    expect(
      within(overview).getByText("g5.collab.answered-question"),
    ).toBeInTheDocument();
    // The agent-authored statement is filtered out under the lens.
    expect(within(overview).queryByText("g5.collab.agent-intent")).toBeNull();
    expect(within(overview).queryByText("Agent authored block")).toBeNull();
  });

  it("closes stale Node details instead of showing mismatched evidence", async () => {
    setGraph({ status: "ok", data: graph });
    mockUseSourceControlChangeGraphNodeDetails.mockReturnValue({
      status: "error",
      error: new ApiError({
        code: "STALE_GRAPH_REVISION",
        status: 409,
        message: "Diff Graph revision changed; reload the graph",
      }),
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("change-graph-node-details")).toBeNull(),
    );
    expect(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    ).toHaveAttribute("data-focus-state", "default");
  });

  it.each([
    ["binary", "This binary change has no textual diff."],
    ["oversized", "This diff exceeds the 10 MiB input limit"],
    ["unavailable", "Exact diff evidence is unavailable"],
    ["available", "No textual diff hunks were returned"],
  ] satisfies Array<[SourceControlChangeGraphDiffAvailability, string]>)(
    "renders the %s file-detail state accurately",
    (availability, message) => {
      renderApp(
        <SourceControlChangeGraphWhatChanged
          state={{
            status: "ok",
            data: {
              revision: graph.revision,
              nodeId: "document",
              kind: "file_diff",
              diff: {
                availability,
                reason:
                  availability === "binary"
                    ? "binary_content"
                    : availability === "oversized"
                      ? "input_limit_exceeded"
                      : availability === "unavailable"
                        ? "retained_evidence_missing"
                        : undefined,
                additions: 4,
                deletions: 1,
                hunks: [],
                returnedLineCount: 0,
                omittedLineCount: 0,
                truncated: false,
                partialFinalHunk: false,
              },
            },
          }}
        />,
      );
      expect(screen.getByText(new RegExp(message))).toBeInTheDocument();
    },
  );

  it("gives every available What Changed diff body an accessible horizontal scroller", () => {
    const longLine = `long-line-${"x".repeat(240)}`;
    const longHunk = {
      ...detailHunk,
      lines: [
        { type: "deletion" as const, oldLine: 1, content: longLine },
        { type: "addition" as const, newLine: 1, content: `${longLine}-new` },
      ],
    };
    const diff = {
      availability: "available" as const,
      additions: 1,
      deletions: 1,
      hunks: [longHunk],
      returnedLineCount: 2,
      omittedLineCount: 0,
      truncated: false,
      partialFinalHunk: false,
    };
    const { rerender } = renderApp(
      <SourceControlChangeGraphWhatChanged
        state={{
          status: "ok",
          data: {
            revision: graph.revision,
            nodeId: "concept",
            kind: "semantic_diff",
            records: [
              {
                id: "D23",
                recordKind: "D",
                title: "Long semantic record",
                changeState: "modified",
                isHuman: false,
                diff: { ...diff, truncated: true, omittedLineCount: 2 },
              },
            ],
            otherFileChanges: diff,
            truncated: false,
          },
        }}
      />,
    );

    const recordScroller = screen.getByRole("region", { name: "D23 diff" });
    const otherScroller = screen.getByRole("region", {
      name: "Other file changes diff",
    });
    for (const scroller of [recordScroller, otherScroller]) {
      expect(scroller).toHaveAttribute("tabindex", "0");
      expect(scroller).toHaveClass("min-w-0", "overflow-x-auto");
      expect(within(scroller).getByText(longLine)).toBeInTheDocument();
    }
    expect(
      within(recordScroller).queryByText("Long semantic record"),
    ).toBeNull();
    expect(within(otherScroller).queryByText("Other file changes")).toBeNull();
    expect(
      within(recordScroller).queryByText(
        "Additional diff hunks were omitted at the response limit.",
      ),
    ).toBeNull();
    expect(
      screen.getByText(
        "Additional diff hunks were omitted at the response limit.",
      ),
    ).toBeInTheDocument();

    rerender(
      <SourceControlChangeGraphWhatChanged
        state={{
          status: "ok",
          data: {
            revision: graph.revision,
            nodeId: "source",
            kind: "file_diff",
            diff,
          },
        }}
      />,
    );
    const fileScroller = screen.getByRole("region", { name: "File diff" });
    expect(fileScroller).toHaveAttribute("tabindex", "0");
    expect(fileScroller).toHaveClass("min-w-0", "overflow-x-auto");
    expect(within(fileScroller).getByText(longLine)).toBeInTheDocument();
  });

  it("renders output truncation and retries an ordinary details failure", async () => {
    const retry = vi.fn();
    const { rerender } = renderApp(
      <SourceControlChangeGraphWhatChanged
        state={{ status: "error", error: new Error("temporary failure") }}
        onRetry={retry}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);

    rerender(
      <SourceControlChangeGraphWhatChanged
        state={{
          status: "ok",
          data: {
            revision: graph.revision,
            nodeId: "document",
            kind: "file_diff",
            diff: {
              availability: "available",
              reason: "output_limit_reached",
              additions: 2,
              deletions: 1,
              hunks: [detailHunk],
              returnedLineCount: 2,
              omittedLineCount: 3,
              truncated: true,
              partialFinalHunk: false,
            },
          },
        }}
      />,
    );
    expect(
      screen.getByText(
        "Additional diff hunks were omitted at the response limit.",
      ),
    ).toBeInTheDocument();
  });

  it("partitions returned incident evidence in Node details", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const conversation = screen.getByRole("button", {
      name: /Build current graph, Chat, Modified/i,
    });
    await user.click(conversation);

    const details = screen.getByTestId("change-graph-node-details");
    const left = within(details).getByTestId("change-graph-node-details-left");
    const right = within(details).getByTestId(
      "change-graph-node-details-right",
    );
    expect(left.parentElement).toHaveClass(
      "grid-cols-[minmax(220px,0.8fr)_minmax(300px,1.2fr)]",
    );
    expect(within(left).queryByText("Connection", { exact: true })).toBeNull();
    const groundingEdges = within(left).getByTestId(
      "change-graph-grounding-edges-section",
    );
    const groundingEdgesHeading = within(groundingEdges).getByRole("heading", {
      level: 4,
      name: "Relationships",
    });
    expect(groundingEdgesHeading).toBeInTheDocument();
    expect(
      within(groundingEdges).queryByTestId(
        "change-graph-grounding-edges-count",
      ),
    ).toBeNull();
    expect(groundingEdges).not.toHaveClass(
      "overflow-hidden",
      "rounded",
      "border",
      "bg-[var(--g5-bg0)]",
    );
    const groundingSubsections = within(groundingEdges).getByTestId(
      "change-graph-grounding-edge-subsections",
    );
    expect(groundingSubsections).not.toHaveClass("ml-3");
    expect(groundingSubsections).not.toHaveClass(
      "border-l-2",
      "border-[var(--g5-stroke)]",
      "pl-2.5",
    );
    expect(
      within(groundingSubsections).getByRole("heading", {
        level: 5,
        name: "Changed in this Diff",
      }),
    ).toBeInTheDocument();
    expect(
      within(groundingSubsections).getByTestId(
        "change-graph-changed-grounding-edges-count",
      ),
    ).toHaveTextContent("4");
    const whatChangedHeading = within(right).getByRole("heading", {
      level: 4,
      name: "What changed",
    });
    for (const className of [
      "text-[10px]",
      "font-semibold",
      "uppercase",
      "tracking-[0.08em]",
      "text-[var(--g5-text1)]",
    ]) {
      expect(groundingEdgesHeading).toHaveClass(className);
      expect(whatChangedHeading).toHaveClass(className);
    }

    const changed = within(groundingSubsections).getByTestId(
      "change-graph-changed-grounding-edges",
    );
    const changedToggle = within(changed).getByTestId(
      "change-graph-changed-grounding-edges-toggle",
    );
    expect(changedToggle).not.toHaveClass("ml-3");
    expect(changedToggle.nextElementSibling).toHaveClass("ml-3");
    expect(changed).not.toHaveAttribute("open");
    expect(
      within(changed).queryByTestId("change-graph-grounding-edges"),
    ).not.toBeVisible();
    await user.click(changedToggle);
    expect(changed).toHaveAttribute("open");

    const current = within(left).getByTestId("change-graph-grounding-edges");
    expect(
      within(current)
        .getAllByTestId("change-graph-evidence-item")
        .map((item) => item.closest("li")?.dataset.testid),
    ).toEqual([
      "change-graph-evidence-evidence-1",
      "change-graph-evidence-evidence-parallel",
      "change-graph-evidence-evidence-deleted",
      "change-graph-evidence-evidence-architecture-chat",
    ]);
    expect(
      within(current).getByText(
        `${CONCEPT_PATH}#g5.collab.legacy-lock → ${CHAT_PATH}`,
      ),
    ).toBeInTheDocument();
    expect(
      within(current).queryByTestId("change-graph-evidence-evidence-unchanged"),
    ).toBeNull();
    await user.click(changedToggle);
    expect(changed).not.toHaveAttribute("open");
    expect(
      within(changed).queryByTestId("change-graph-grounding-edges"),
    ).not.toBeVisible();
    await user.click(changedToggle);
    expect(changed).toHaveAttribute("open");

    const preExisting = within(groundingSubsections).getByTestId(
      "change-graph-pre-existing-grounding-edges",
    );
    const preExistingToggle = within(preExisting).getByTestId(
      "change-graph-pre-existing-grounding-edges-toggle",
    );
    expect(preExistingToggle).not.toHaveClass("ml-3");
    expect(preExistingToggle.nextElementSibling).toHaveClass("ml-3");
    expect(preExisting).not.toHaveAttribute("open");
    expect(
      within(preExistingToggle).getByText("Pre-existing"),
    ).toBeInTheDocument();
    expect(
      within(preExistingToggle).getByRole("heading", {
        level: 5,
        name: "Pre-existing",
      }),
    ).toBeInTheDocument();
    expect(
      within(preExisting).getByTestId(
        "change-graph-pre-existing-grounding-edges-count",
      ),
    ).toHaveTextContent("1");
    await user.click(preExistingToggle);
    expect(preExisting).toHaveAttribute("open");
    expect(
      within(preExisting).getByTestId(
        "change-graph-evidence-evidence-unchanged",
      ),
    ).toHaveAttribute("data-change-state", "unchanged");
    expect(
      within(preExisting).getByText(
        `${CONCEPT_PATH}#g5.collab.presence → ${CHAT_PATH}`,
      ),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    await user.click(conversation);
    expect(
      within(details).getByTestId("change-graph-changed-grounding-edges"),
    ).not.toHaveAttribute("open");
    expect(
      within(details).getByTestId("change-graph-pre-existing-grounding-edges"),
    ).not.toHaveAttribute("open");
  });

  it("renders compact human-readable relationship evidence with exact technical details", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );

    const evidence = screen.getByTestId("change-graph-evidence-evidence-1");
    const focus = within(evidence).getByRole("button", {
      name: "Provenance from Ontology collaborative_editing_ux g5.collab.edit-window to Chat Build current graph",
    });
    expect(focus).toHaveAttribute("aria-pressed", "false");
    expect(within(evidence).getByText("Provenance")).toBeInTheDocument();
    expect(within(evidence).getByText("Added")).toBeInTheDocument();
    expect(within(evidence).queryByText(/relevance/)).toBeNull();
    expect(
      within(evidence).queryByTestId("change-graph-evidence-target-deleted"),
    ).toBeNull();
    expect(
      within(evidence).getByText(
        "Ontology · collaborative_editing_ux · g5.collab.edit-window",
      ),
    ).toHaveAttribute(
      "title",
      "Ontology · collaborative_editing_ux · g5.collab.edit-window",
    );
    // A whole-file chat target carries no statement label.
    expect(
      within(evidence).getByText("→ Chat · Build current graph"),
    ).toHaveAttribute("title", "Chat · Build current graph");

    const technical = within(evidence).getByText("Technical details");
    const disclosure = technical.closest("details");
    expect(disclosure).not.toHaveAttribute("open");
    await user.click(technical);
    expect(disclosure).toHaveAttribute("open");
    // A whole-file target omits the `#` fragment.
    expect(
      within(evidence).getByText(
        `${CONCEPT_PATH}#g5.collab.edit-window → ${CHAT_PATH}`,
      ),
    ).toBeInTheDocument();

    // A statement target keeps its ID in both the label and technical identity.
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    const reference = screen.getByTestId(
      "change-graph-evidence-evidence-concept-architecture",
    );
    expect(
      within(reference).getByRole("button", {
        name: "Reference from Ontology collaborative_editing_ux g5.collab.edit-window to Ontology source-control-change-graph g5.arch.change-graph",
      }),
    ).toBeInTheDocument();
    expect(
      within(reference).getByText(
        "→ Ontology · source-control-change-graph · g5.arch.change-graph",
      ),
    ).toBeInTheDocument();
    expect(
      within(reference).getByText(
        `${CONCEPT_PATH}#g5.collab.edit-window → ${ARCHITECTURE_PATH}#g5.arch.change-graph`,
      ),
    ).toBeInTheDocument();
  });

  it.each([
    ["reference", "Reference"],
    ["expands", "Expands"],
    ["provenance", "Provenance"],
    ["implementation", "Implementation"],
    ["project_document", "Project document"],
  ])("labels %s evidence rows by relationship kind", (relationship, label) => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        edges: [
          {
            id: "edge-kind",
            sourceNodeId: "concept",
            targetNodeId: "architecture",
            evidence: [
              {
                id: "evidence-kind",
                sourceFileType: "ontology",
                sourceFile: CONCEPT_PATH,
                sourceId: "g5.collab.edit-window",
                targetFileType: "ontology",
                targetFile: ARCHITECTURE_PATH,
                targetId: "g5.arch.change-graph",
                relationship,
                changeState: "added",
              },
            ],
          },
        ],
      },
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);
    fireEvent.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );

    const evidence = screen.getByTestId("change-graph-evidence-evidence-kind");
    expect(within(evidence).getByText(label)).toBeInTheDocument();
    expect(within(evidence).getByRole("button")).toHaveAccessibleName(
      new RegExp(`^${label} from Ontology collaborative_editing_ux`),
    );
    expect(
      document.querySelector('[data-edge-id="edge-kind"]'),
    ).toHaveAttribute("data-relationship", relationship);
  });

  it("marks evidence whose target was deleted and shows statement IDs as-is", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) =>
          node.id === "architecture"
            ? { ...node, changeState: "deleted" as const }
            : node,
        ),
        edges: [
          {
            id: "edge-deleted-target",
            sourceNodeId: "concept",
            targetNodeId: "architecture",
            evidence: [
              {
                id: "evidence-deleted-target",
                sourceFileType: "ontology",
                sourceFile: CONCEPT_PATH,
                sourceId: "D7",
                targetFileType: "ontology",
                targetFile: ARCHITECTURE_PATH,
                targetId: "g5.arch.change-graph",
                relationship: "reference",
                changeState: "unchanged",
                targetDeleted: true,
              },
            ],
          },
        ],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    await user.click(
      screen.getByTestId("change-graph-pre-existing-grounding-edges-toggle"),
    );
    const evidence = screen.getByTestId(
      "change-graph-evidence-evidence-deleted-target",
    );
    expect(
      within(evidence).getByTestId("change-graph-evidence-target-deleted"),
    ).toHaveTextContent("Target deleted");
    expect(within(evidence).getByRole("button")).toHaveAccessibleName(
      "Reference from Ontology collaborative_editing_ux D7 to Ontology source-control-change-graph g5.arch.change-graph (target deleted)",
    );
    // Short IDs are not reinterpreted as legacy I/D record labels.
    expect(
      within(evidence).getByText("Ontology · collaborative_editing_ux · D7"),
    ).toBeInTheDocument();
    expect(within(evidence).queryByText(/Decision D7/)).toBeNull();
  });

  it("drills into one evidence edge and returns to ordinary node focus", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const conversation = screen.getByRole("button", {
      name: /Build current graph, Chat, Modified/i,
    });
    const documentCard = screen.getByRole("button", {
      name: /Change graph plan, Document, Modified/i,
    });
    const concept = screen.getByRole("button", {
      name: /collaborative_editing_ux, Ontology, Modified/i,
    });
    const architecture = screen.getByRole("button", {
      name: /source-control-change-graph, Ontology, Added/i,
    });
    const edgeState = (id: string) =>
      document
        .querySelector(`[data-edge-id="${id}"]`)
        ?.getAttribute("data-focus-state");

    await user.click(conversation);
    expect(edgeState("edge-1")).toBe("active");
    expect(edgeState("edge-neighbors")).toBe("active");
    expect(edgeState("edge-2")).toBe("dimmed");

    const changedDisclosure = screen.getByTestId(
      "change-graph-changed-grounding-edges",
    );
    const changedDisclosureToggle = within(changedDisclosure).getByTestId(
      "change-graph-changed-grounding-edges-toggle",
    );
    expect(changedDisclosure).not.toHaveAttribute("open");
    await user.click(changedDisclosureToggle);
    expect(changedDisclosure).toHaveAttribute("open");

    const first = screen.getByTestId("change-graph-evidence-evidence-1");
    const firstButton = within(first).getByRole("button");
    await user.click(firstButton);
    expect(firstButton).toHaveAttribute("aria-pressed", "true");
    expect(edgeState("edge-1")).toBe("highlighted");
    expect(edgeState("edge-neighbors")).toBe("active");
    expect(conversation).toHaveAttribute("data-focus-state", "selected");
    expect(concept).toHaveAttribute("data-focus-state", "neighbor");
    expect(architecture).toHaveAttribute("data-focus-state", "neighbor");
    expect(documentCard).toHaveAttribute("data-focus-state", "dimmed");

    await user.click(changedDisclosureToggle);
    expect(changedDisclosure).not.toHaveAttribute("open");
    expect(edgeState("edge-1")).toBe("active");
    expect(conversation).toHaveAttribute("data-focus-state", "selected");
    await user.click(changedDisclosureToggle);

    const parallel = screen.getByTestId(
      "change-graph-evidence-evidence-parallel",
    );
    const parallelButton = within(parallel).getByRole("button");
    await user.click(parallelButton);
    expect(firstButton).toHaveAttribute("aria-pressed", "false");
    expect(parallelButton).toHaveAttribute("aria-pressed", "true");
    expect(edgeState("edge-1")).toBe("highlighted");

    await user.click(parallelButton);
    expect(parallelButton).toHaveAttribute("aria-pressed", "false");
    expect(edgeState("edge-1")).toBe("active");
    expect(edgeState("edge-neighbors")).toBe("active");

    const architectureEvidence = screen.getByTestId(
      "change-graph-evidence-evidence-architecture-chat",
    );
    const architectureEvidenceButton =
      within(architectureEvidence).getByRole("button");
    architectureEvidenceButton.focus();
    await user.keyboard("{Enter}");
    expect(architectureEvidenceButton).toHaveAttribute("aria-pressed", "true");
    expect(edgeState("edge-1")).toBe("active");
    expect(edgeState("edge-neighbors")).toBe("highlighted");
    expect(architecture).toHaveAttribute("data-focus-state", "neighbor");
    expect(concept).toHaveAttribute("data-focus-state", "neighbor");

    await user.click(concept);
    expect(edgeState("edge-1")).toBe("active");
    expect(edgeState("edge-3")).toBe("active");
    expect(edgeState("edge-neighbors")).toBe("dimmed");
  });

  it("clears evidence drill-down when a refetch removes its response edge", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<RefetchHarness />);

    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );
    await user.click(
      within(screen.getByTestId("change-graph-evidence-evidence-1")).getByRole(
        "button",
      ),
    );
    expect(document.querySelector('[data-edge-id="edge-1"]')).toHaveAttribute(
      "data-focus-state",
      "highlighted",
    );

    setGraph({
      status: "ok",
      data: {
        ...graph,
        revision: "abc124",
        edges: graph.edges.filter((edge) => edge.id !== "edge-1"),
      },
    });
    await user.click(
      screen.getByRole("button", { name: "Apply graph refetch" }),
    );

    expect(screen.queryByTestId("change-graph-evidence-evidence-1")).toBeNull();
    expect(
      document.querySelector('[data-edge-id="edge-neighbors"]'),
    ).toHaveAttribute("data-focus-state", "active");
    expect(document.querySelector('[data-edge-id="edge-2"]')).toHaveAttribute(
      "data-focus-state",
      "dimmed",
    );
    expect(
      document.querySelectorAll('[data-focus-state="highlighted"]'),
    ).toHaveLength(0);
  });

  it("shows accurate Node details empty and truncation messages", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: [...graph.nodes, otherIntentNode],
        edges: [
          ...graph.edges,
          {
            id: "edge-pre-existing-only",
            sourceNodeId: "architecture",
            targetNodeId: "added-chat",
            evidence: [
              {
                id: "evidence-pre-existing-only",
                sourceFileType: "ontology",
                sourceFile: ARCHITECTURE_PATH,
                sourceId: "g5.arch.change-graph",
                targetFileType: "chat",
                targetFile: ADDED_CHAT_PATH,
                targetId: "",
                relationship: "provenance",
                changeState: "unchanged",
              },
            ],
          },
        ],
        diagnostics: [{ code: "evidence_budget_reached", count: 9 }],
        truncated: true,
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: /Add exact chat cards, Chat, Added/i,
      }),
    );
    const details = screen.getByTestId("change-graph-node-details");
    expect(
      within(details).queryByTestId("change-graph-grounding-edges-count"),
    ).toBeNull();
    expect(
      within(details).getByTestId("change-graph-changed-grounding-edges-count"),
    ).toHaveTextContent("0");
    expect(
      within(details).getByText(
        "No links were added or removed. Existing relationships are available under Pre-existing below.",
      ),
    ).toBeInTheDocument();
    expect(
      within(details).getByTestId(
        "change-graph-pre-existing-grounding-edges-count",
      ),
    ).toHaveTextContent("1");
    await user.click(
      within(details).getByTestId(
        "change-graph-pre-existing-grounding-edges-toggle",
      ),
    );
    expect(
      within(details).getByText(
        `${ARCHITECTURE_PATH}#g5.arch.change-graph → ${ADDED_CHAT_PATH}`,
      ),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", {
        name: /plan\.human-view\.html, Other, Modified/i,
      }),
    );
    expect(
      within(details).queryByTestId("change-graph-grounding-edges-count"),
    ).toBeNull();
    expect(
      within(details).getByTestId("change-graph-changed-grounding-edges-count"),
    ).toHaveTextContent("0");
    expect(
      within(details).getByText("No links were added or removed.", {
        exact: true,
      }),
    ).toBeInTheDocument();
    expect(
      within(details).getByTestId(
        "change-graph-pre-existing-grounding-edges-count",
      ),
    ).toHaveTextContent("0");
    await user.click(
      within(details).getByTestId(
        "change-graph-pre-existing-grounding-edges-toggle",
      ),
    );
    expect(
      within(details).getByText(
        "No unchanged links were returned for this artifact.",
      ),
    ).toBeInTheDocument();
    expect(
      within(details).getByText(
        "Some relationship evidence in this graph was truncated. Counts below reflect returned evidence rows.",
      ),
    ).toBeInTheDocument();
  });

  it("focuses only the selected node direct neighbors and incident edges", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const backgroundLayer = screen.getByTestId(
      "change-graph-edge-layer-background",
    );
    expect(backgroundLayer).toHaveClass("z-10");
    expect(
      within(backgroundLayer).getAllByTestId("change-graph-edge"),
    ).toHaveLength(graph.edges.length);
    expect(
      screen.queryByTestId("change-graph-edge-layer-foreground"),
    ).not.toBeInTheDocument();

    const conversation = screen.getByRole("button", {
      name: /Build current graph, Chat, Modified/i,
    });
    const documentCard = screen.getByRole("button", {
      name: /Change graph plan, Document, Modified/i,
    });
    const concept = screen.getByRole("button", {
      name: /collaborative_editing_ux, Ontology, Modified/i,
    });
    const architecture = screen.getByRole("button", {
      name: /source-control-change-graph, Ontology, Added/i,
    });
    const source = screen.getByRole("button", {
      name: /SourceControlChangeGraphView.tsx, Source File, Added/i,
    });

    await user.click(concept);

    expect(concept).toHaveAttribute("data-focus-state", "selected");
    expect(conversation).toHaveAttribute("data-focus-state", "neighbor");
    expect(architecture).toHaveAttribute("data-focus-state", "neighbor");
    expect(documentCard).toHaveAttribute("data-focus-state", "dimmed");
    expect(source).toHaveAttribute("data-focus-state", "dimmed");
    expect(concept).toHaveAttribute("aria-pressed", "true");
    expect(documentCard).toHaveClass(
      "opacity-[0.55]",
      "hover:opacity-100",
      "focus-visible:!opacity-100",
      "hover:z-50",
      "focus-visible:z-50",
    );
    expect(documentCard).toHaveClass("focus-visible:ring-[var(--g5-info)]");
    expect(concept).toHaveClass("z-50");
    expect(conversation).toHaveClass("z-40");
    expect(architecture).toHaveClass("z-40");
    expect(screen.getByRole("heading", { name: "Documents" })).toHaveClass(
      "z-40",
    );
    expect(screen.getByRole("heading", { name: "features" })).toHaveClass(
      "z-40",
    );
    expect(screen.getByRole("heading", { name: "Source Code" })).toHaveClass(
      "z-[60]",
    );

    const foregroundLayer = screen.getByTestId(
      "change-graph-edge-layer-foreground",
    );
    expect(foregroundLayer).toHaveClass("z-30");
    expect(
      within(backgroundLayer).getAllByTestId("change-graph-edge"),
    ).toHaveLength(3);
    expect(
      within(foregroundLayer).getAllByTestId("change-graph-edge"),
    ).toHaveLength(2);
    for (const edge of graph.edges) {
      expect(
        document.querySelectorAll(
          `[data-testid="change-graph-edge"][data-edge-id="${edge.id}"]`,
        ),
      ).toHaveLength(1);
    }
    const markerIds = Array.from(document.querySelectorAll("marker[id]")).map(
      (marker) => marker.id,
    );
    expect(new Set(markerIds).size).toBe(markerIds.length);

    const edgeState = (id: string) =>
      document
        .querySelector(`[data-edge-id="${id}"]`)
        ?.getAttribute("data-focus-state");
    expect(edgeState("edge-1")).toBe("active");
    expect(edgeState("edge-3")).toBe("active");
    expect(edgeState("edge-2")).toBe("dimmed");
    expect(edgeState("edge-neighbors")).toBe("dimmed");
    expect(edgeState("edge-implementation")).toBe("dimmed");
    expect(
      Array.from(document.querySelectorAll("[data-testid='change-graph-edge']"))
        .map((edge) => edge.getAttribute("data-focus-state"))
        .slice(-2),
    ).toEqual(["active", "active"]);

    source.focus();
    await user.keyboard("{Enter}");
    expect(source).toHaveAttribute("data-focus-state", "selected");
    expect(source).toHaveAttribute("aria-pressed", "true");
    expect(edgeState("edge-implementation")).toBe("active");
    expect(architecture).toHaveAttribute("data-focus-state", "neighbor");
    expect(concept).toHaveAttribute("data-focus-state", "dimmed");

    await user.click(
      screen.getByRole("button", {
        name: /Add exact chat cards, Chat, Added/i,
      }),
    );
    expect(
      screen.getByRole("button", {
        name: /Add exact chat cards, Chat, Added/i,
      }),
    ).toHaveAttribute("data-focus-state", "selected");
    expect(
      document.querySelectorAll(
        '[data-testid="change-graph-edge"][data-focus-state="active"]',
      ),
    ).toHaveLength(0);
    expect(
      screen.queryByTestId("change-graph-edge-layer-foreground"),
    ).not.toBeInTheDocument();
  });

  it("clears focus on close or background click but not after a pan", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);
    const selected = screen.getByRole("button", {
      name: /Build current graph, Chat, Modified/i,
    });
    const viewport = screen.getByTestId("change-graph-viewport");

    await user.click(selected);
    await user.click(
      screen.getByRole("button", { name: "Close Node details" }),
    );
    expect(selected).toHaveAttribute("data-focus-state", "default");
    expect(
      screen.queryByTestId("change-graph-edge-layer-foreground"),
    ).not.toBeInTheDocument();
    expect(
      within(
        screen.getByTestId("change-graph-edge-layer-background"),
      ).getAllByTestId("change-graph-edge"),
    ).toHaveLength(graph.edges.length);

    await user.click(selected);
    dispatchPointerEvent(viewport, "pointerdown", {
      button: 0,
      pointerId: 1,
      clientX: 20,
      clientY: 20,
    });
    dispatchPointerEvent(viewport, "pointerup", {
      pointerId: 1,
      clientX: 20,
      clientY: 20,
    });
    expect(selected).toHaveAttribute("data-focus-state", "default");
    expect(screen.queryByTestId("change-graph-node-details")).toBeNull();

    await user.click(selected);
    dispatchPointerEvent(viewport, "pointerdown", {
      button: 0,
      pointerId: 2,
      clientX: 20,
      clientY: 20,
    });
    dispatchPointerEvent(viewport, "pointermove", {
      pointerId: 2,
      clientX: 23,
      clientY: 20,
    });
    dispatchPointerEvent(viewport, "pointerup", {
      pointerId: 2,
      clientX: 23,
      clientY: 20,
    });
    expect(selected).toHaveAttribute("data-focus-state", "selected");
    expect(screen.getByTestId("change-graph-node-details")).toBeInTheDocument();
  });

  it("drops stale focus when the selected node disappears on refetch", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<RefetchHarness />);
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );

    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.filter((node) => node.id !== "concept"),
        edges: graph.edges.filter(
          (edge) =>
            edge.sourceNodeId !== "concept" && edge.targetNodeId !== "concept",
        ),
      },
    });
    await user.click(
      screen.getByRole("button", { name: "Apply graph refetch" }),
    );

    expect(screen.queryByTestId("change-graph-node-details")).toBeNull();
    for (const card of screen.getAllByTestId("change-graph-card")) {
      expect(card).toHaveAttribute("data-focus-state", "default");
    }
  });

  it("renders each chat intent as a file node with its own state and file diff", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        onOpenDocument={vi.fn()}
        onOpenDiff={vi.fn()}
        onOpenOntology={vi.fn()}
      />,
    );

    const modified = screen.getByRole("button", {
      name: /Build current graph, Chat, Modified/i,
    });
    const added = screen.getByRole("button", {
      name: /Add exact chat cards, Chat, Added/i,
    });
    expect(modified).toHaveTextContent(CHAT_PATH);
    expect(added).toHaveTextContent(ADDED_CHAT_PATH);

    await user.click(added);
    expect(mockUseSourceControlChangeGraphNodeDetails).toHaveBeenLastCalledWith(
      "ws-1",
      "workspace",
      "abc123",
      "added-chat",
      false,
    );
    const details = screen.getByTestId("change-graph-node-details");
    const left = within(details).getByTestId("change-graph-node-details-left");
    const whatChanged = within(details).getByTestId(
      "change-graph-node-details-right",
    );
    expect(within(left).getByText(ADDED_CHAT_PATH)).toBeInTheDocument();
    expect(
      within(left).getByText("Added", { exact: true }),
    ).toBeInTheDocument();
    expect(
      within(whatChanged).getByTestId("change-graph-file-diff"),
    ).toBeInTheDocument();
    expect(
      within(whatChanged).getByText(
        "Add one card for each registered chat intent.",
      ),
    ).toBeInTheDocument();
    expect(
      within(details).queryByText("Show the current change graph."),
    ).not.toBeInTheDocument();
    // Chats have no statement overview and no navigation action.
    expect(
      within(details).queryByTestId(
        "change-graph-changed-semantic-blocks-section",
      ),
    ).not.toBeInTheDocument();
    expect(
      within(details).queryByTestId("change-graph-node-details-action"),
    ).not.toBeInTheDocument();

    await user.click(modified);
    expect(within(left).getByText(CHAT_PATH)).toBeInTheDocument();
    expect(
      within(left).getByText("Modified", { exact: true }),
    ).toBeInTheDocument();
    expect(
      within(details).getByText("Show the current change graph."),
    ).toBeInTheDocument();
  });

  it("navigates only through the explicit type-specific drawer action", async () => {
    setGraph({ status: "ok", data: graph });
    const onOpenDiff = vi.fn();
    const user = userEvent.setup();
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        onOpenDiff={onOpenDiff}
      />,
    );

    await user.click(
      screen.getByRole("button", {
        name: /SourceControlChangeGraphView.tsx, Source File, Added/i,
      }),
    );
    expect(onOpenDiff).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Open diff" }));
    expect(onOpenDiff).toHaveBeenCalledWith(graphNode("source").route);
  });

  it("keeps Project Doc actions and never offers a Source Control diff for ontology", async () => {
    setGraph({ status: "ok", data: graph });
    const onOpenDocument = vi.fn();
    const onOpenDiff = vi.fn();
    const user = userEvent.setup();
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        onOpenDocument={onOpenDocument}
        onOpenDiff={onOpenDiff}
      />,
    );

    await user.click(
      screen.getByRole("button", {
        name: /Change graph plan, Document, Modified/i,
      }),
    );
    expect(onOpenDocument).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Open document" }));
    expect(onOpenDocument).toHaveBeenCalledWith(graphNode("document").route);

    // Without an ontology editor callback, ontology nodes have no action.
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    expect(
      screen.queryByTestId("change-graph-node-details-action"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Open diff" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("change-graph-open-statement"),
    ).not.toBeInTheDocument();
    expect(onOpenDiff).not.toHaveBeenCalled();
    expect(onOpenDocument).toHaveBeenCalledTimes(1);
  });

  it("opens ontology files in the ontology editor and deleted files in All changes", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) =>
          node.id === "architecture"
            ? { ...node, changeState: "deleted" as const }
            : node,
        ),
      },
    });
    const onOpenOntology = vi.fn();
    const onOpenDocument = vi.fn();
    const onOpenDiff = vi.fn();
    const user = userEvent.setup();
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        onOpenOntology={onOpenOntology}
        onOpenDocument={onOpenDocument}
        onOpenDiff={onOpenDiff}
      />,
    );

    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    expect(
      screen.queryByRole("button", { name: "Open diff" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open document" }));
    expect(onOpenOntology).toHaveBeenLastCalledWith({
      route: graphNode("concept").route,
      deleted: false,
    });

    await user.click(
      screen.getByRole("button", {
        name: /source-control-change-graph, Ontology, Deleted/i,
      }),
    );
    expect(
      screen.queryByRole("button", { name: "Open document" }),
    ).not.toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Open in All changes" }),
    );
    expect(onOpenOntology).toHaveBeenLastCalledWith({
      route: graphNode("architecture").route,
      deleted: true,
    });
    expect(onOpenOntology).toHaveBeenCalledTimes(2);
    expect(onOpenDocument).not.toHaveBeenCalled();
    expect(onOpenDiff).not.toHaveBeenCalled();
  });

  it("opens each changed statement row at its statement, deleted rows at Before", async () => {
    mockUseSourceControlChangeGraphNodeDetails.mockImplementation(
      (
        _workspaceId: string,
        _comparison: string,
        _revision: string,
        nodeId: string,
      ) =>
        nodeId === "concept"
          ? {
              status: "ok",
              data: semanticDetails(nodeId, [
                statement(
                  "g5.collab.edit-window",
                  "D",
                  "Edit window",
                  "modified",
                ),
                statement(
                  "g5.collab.legacy-lock",
                  "D",
                  "Legacy lock",
                  "deleted",
                ),
              ]),
            }
          : nodeId === "document"
            ? {
                status: "ok",
                data: semanticDetails(nodeId, [
                  statement("D1", "D", "Project Doc decision", "modified"),
                ]),
              }
            : { status: "ok", data: defaultDetails(nodeId) },
    );
    const documentPath = "docs/ws-1/change_graph.md";
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) =>
          node.id === "document"
            ? {
                ...node,
                path: documentPath,
                route: { ...node.route, filePath: documentPath },
              }
            : node,
        ),
      },
    });
    const onOpenOntology = vi.fn();
    const user = userEvent.setup();
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        onOpenOntology={onOpenOntology}
      />,
    );

    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    const overview = screen.getByTestId(
      "change-graph-changed-semantic-blocks-section",
    );
    const openButtons = within(overview).getAllByTestId(
      "change-graph-open-statement",
    );
    expect(openButtons.map((button) => button.getAttribute("title"))).toEqual([
      "Open in the ontology editor",
      "Open at its Before version",
    ]);
    await user.click(
      within(overview).getByRole("button", {
        name: "Open g5.collab.edit-window in the ontology editor",
      }),
    );
    expect(onOpenOntology).toHaveBeenLastCalledWith({
      route: graphNode("concept").route,
      statementId: "g5.collab.edit-window",
      deleted: false,
    });
    await user.click(
      within(overview).getByRole("button", {
        name: "Open g5.collab.legacy-lock in the ontology editor",
      }),
    );
    expect(onOpenOntology).toHaveBeenLastCalledWith({
      route: graphNode("concept").route,
      statementId: "g5.collab.legacy-lock",
      deleted: true,
    });

    // Project Doc statements are listed without ontology-editor navigation.
    await user.click(
      screen.getByRole("button", {
        name: /Change graph plan, Document, Modified/i,
      }),
    );
    const documentOverview = screen.getByTestId(
      "change-graph-changed-semantic-blocks-section",
    );
    expect(
      within(documentOverview).getByText("Project Doc decision"),
    ).toBeInTheDocument();
    expect(
      within(documentOverview).queryByTestId("change-graph-open-statement"),
    ).not.toBeInTheDocument();
    expect(onOpenOntology).toHaveBeenCalledTimes(2);
  });

  it("opens statements of a deleted ontology file in All changes, never a missing editor", async () => {
    mockUseSourceControlChangeGraphNodeDetails.mockImplementation(
      (
        _workspaceId: string,
        _comparison: string,
        _revision: string,
        nodeId: string,
      ) =>
        nodeId === "concept"
          ? {
              status: "ok",
              data: semanticDetails(nodeId, [
                statement(
                  "g5.collab.gone",
                  "I",
                  "Gone with its file",
                  "deleted",
                ),
              ]),
            }
          : { status: "ok", data: defaultDetails(nodeId) },
    );
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) =>
          node.id === "concept" ? { ...node, changeState: "deleted" } : node,
        ),
      },
    });
    const onOpenOntology = vi.fn();
    const user = userEvent.setup();
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        onOpenOntology={onOpenOntology}
      />,
    );
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Deleted/i,
      }),
    );
    await user.click(
      screen.getByRole("button", {
        name: "Open g5.collab.gone in the ontology editor",
      }),
    );
    expect(onOpenOntology).toHaveBeenLastCalledWith({
      route: graphNode("concept").route,
      deleted: true,
    });
  });

  it("explains a reached link-target budget among display limits", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        truncated: true,
        diagnostics: [{ code: "link_target_budget_reached", count: 3 }],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);
    await user.click(
      screen.getByRole("button", {
        name: "Explain unavailable Diff Graph relationship details",
      }),
    );
    expect(
      screen.getByRole("dialog", { name: "Diff Graph relationship details" }),
    ).toHaveTextContent("Showing links to up to 1,024 unchanged files.");
  });

  it("uses five-point zoom controls and fits the graph to the full viewport", async () => {
    setGraph({ status: "ok", data: graph });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(screen.getByText("100%")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByText("105%")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(screen.getByText("100%")).toBeInTheDocument();

    const viewport = screen.getByTestId("change-graph-viewport");
    // Three 280px columns make an 840px world; fitting a 700px viewport
    // scales it to 700 / 840.
    Object.defineProperties(viewport, {
      clientWidth: { configurable: true, value: 700 },
      clientHeight: { configurable: true, value: 1000 },
    });
    await user.click(screen.getByRole("button", { name: "Fit graph" }));
    expect(screen.getByText("83%")).toBeInTheDocument();
    const footprint = screen.getByTestId("change-graph-footprint");
    expect(footprint.style.marginInline).toBe("auto");
    expect(Number.parseFloat(footprint.style.width)).toBeCloseTo(700);
  });

  it("claims modifier-wheel zoom exactly once and removes its native listener", () => {
    setGraph({ status: "ok", data: graph });
    const view = renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);
    const viewport = screen.getByTestId("change-graph-viewport");

    const ctrlWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      deltaY: -100,
    });
    fireEvent(viewport, ctrlWheel);
    expect(ctrlWheel.defaultPrevented).toBe(true);
    expect(screen.getByText("108%")).toBeInTheDocument();

    const metaWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      metaKey: true,
      deltaY: 100,
    });
    fireEvent(viewport, metaWheel);
    expect(metaWheel.defaultPrevented).toBe(true);
    expect(screen.getByText("100%")).toBeInTheDocument();

    const plainWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      deltaY: -100,
    });
    fireEvent(viewport, plainWheel);
    expect(plainWheel.defaultPrevented).toBe(false);
    expect(screen.getByText("100%")).toBeInTheDocument();

    const removeWheelListener = vi.spyOn(viewport, "removeEventListener");
    view.unmount();
    expect(removeWheelListener).toHaveBeenCalledWith(
      "wheel",
      expect.any(Function),
    );
  });

  it("preserves the exact column frame while loading", () => {
    setGraph({ status: "loading" });
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        defaultComparison="workspace"
      />,
    );

    expect(screen.getByText("Loading workspace range…")).toBeInTheDocument();
    const skeleton = screen.getByTestId("change-graph-skeleton");
    expect(skeleton).toHaveClass("grid-cols-3");
    expect(
      within(skeleton)
        .getAllByRole("heading")
        .map((heading) => heading.textContent),
    ).toEqual(["Intents", "Ontology", "Source Code"]);
  });

  it("renders terminal empty comparisons without loader glyphs", () => {
    setGraph({
      status: "ok",
      data: { ...graph, comparison: "workspace", nodes: [], edges: [] },
    });
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        defaultComparison="uncommitted"
      />,
    );

    // An Uncommitted only default is not offered; the view stays on All changes.
    expect(mockUseSourceControlChangeGraph).toHaveBeenLastCalledWith(
      "ws-1",
      "workspace",
      false,
    );
    expect(screen.getByText("No workspace changes")).toBeInTheDocument();
    expect(
      screen.getByText("The current workspace matches its parent baseline."),
    ).toBeInTheDocument();
    expect(screen.queryByText("No uncommitted changes")).toBeNull();
    expect(
      screen.queryByText(
        "The working state matches the current commit vector.",
      ),
    ).toBeNull();
    expect(screen.queryByTestId("change-graph-message-loader-icon")).toBeNull();
    expect(screen.queryByTestId("change-graph-legend")).toBeNull();
  });

  it("discloses completeness-affecting diagnostics without a warning strip", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        truncated: true,
        diagnostics: [
          { code: "unresolved_changed_relationship_endpoint", count: 2 },
          { code: "ambiguous_grounding_owner", count: 1 },
          { code: "malformed_grounding_edge", count: 3 },
          { code: "ontology_comparison_refused", count: 5 },
          { code: "relationship_context_budget_reached", count: 7 },
          { code: "edge_budget_reached", count: 8 },
          { code: "evidence_budget_reached", count: 9 },
        ],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(
      screen.queryByText(/Optional graph evidence was bounded/i),
    ).not.toBeInTheDocument();
    const trigger = screen.getByRole("button", {
      name: "Explain unavailable Diff Graph relationship details",
    });
    expect(trigger).toHaveAttribute("aria-haspopup", "dialog");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveAttribute(
      "title",
      "About unavailable Diff Graph relationship details",
    );
    expect(
      trigger.compareDocumentPosition(
        screen.getByRole("button", { name: "Zoom out" }),
      ) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    trigger.focus();
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const dialog = screen.getByRole("dialog", {
      name: "Diff Graph relationship details",
    });
    // Ledger-era codes are no longer special causes; refused ontology
    // comparisons are informational and not counted.
    expect(dialog).toHaveTextContent(
      "Some relationships or supporting evidence weren’t shown because this graph reached its display limits.",
    );
    expect(dialog).not.toHaveTextContent(/couldn’t be matched/);
    expect(dialog).not.toHaveTextContent(/single repository/);
    expect(dialog).not.toHaveTextContent(/supporting records?/);
    expect(dialog).toHaveTextContent(
      "Showing up to 256 related context artifacts.",
    );
    expect(dialog).toHaveTextContent("Showing up to 1,024 connections.");
    expect(dialog).toHaveTextContent("Showing up to 4,096 evidence rows.");
    expect(dialog).toHaveTextContent(
      "6 additional relationship or supporting-evidence details couldn’t be displayed.",
    );
    expect(dialog).toHaveTextContent(
      "Underlying files and saved changes are unaffected.",
    );
    expect(screen.getAllByTestId("change-graph-card")).toHaveLength(6);

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", {
        name: "Diff Graph relationship details",
      }),
    ).not.toBeInTheDocument();
    expect(trigger).toHaveAttribute("aria-expanded", "false");

    await user.click(trigger);
    fireEvent.pointerDown(document.body);
    expect(
      screen.queryByRole("dialog", {
        name: "Diff Graph relationship details",
      }),
    ).not.toBeInTheDocument();
  });

  it.each([
    "unresolved_grounding_edge",
    "ambiguous_grounding_owner",
    "malformed_intent_record",
    "unresolved_changed_relationship_endpoint",
  ])(
    "treats the ledger-era %s diagnostic as an unknown detail",
    async (code) => {
      setGraph({
        status: "ok",
        data: { ...graph, diagnostics: [{ code, count: 1 }] },
      });
      const user = userEvent.setup();
      renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

      await user.click(
        screen.getByRole("button", {
          name: "Explain unavailable Diff Graph relationship details",
        }),
      );
      const dialog = screen.getByRole("dialog", {
        name: "Diff Graph relationship details",
      });
      expect(dialog).toHaveTextContent(
        "Some relationship or supporting evidence details couldn’t be displayed.",
      );
      expect(dialog).toHaveTextContent(
        "1 additional relationship or supporting-evidence detail couldn’t be displayed.",
      );
      expect(dialog).not.toHaveTextContent(/couldn’t be matched/);
    },
  );

  it("uses cause-specific disclosure copy for limits only", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        truncated: true,
        diagnostics: [{ code: "edge_budget_reached", count: 12 }],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: "Explain unavailable Diff Graph relationship details",
      }),
    );
    const dialog = screen.getByRole("dialog", {
      name: "Diff Graph relationship details",
    });
    expect(dialog).toHaveTextContent(
      "Some relationships or supporting evidence weren’t shown because this graph reached its display limits.",
    );
    expect(dialog).toHaveTextContent("Showing up to 1,024 connections.");
    expect(dialog).not.toHaveTextContent(/additional relationship/);
  });

  it("keeps informational-only diagnostics quiet and preserves the Node details budget note", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        diagnostics: [{ code: "ontology_comparison_refused", count: 1 }],
      },
    });
    const informational = renderApp(
      <SourceControlChangeGraphView workspaceId="ws-1" />,
    );
    expect(
      screen.queryByRole("button", {
        name: "Explain unavailable Diff Graph relationship details",
      }),
    ).not.toBeInTheDocument();
    informational.unmount();

    setGraph({
      status: "ok",
      data: {
        ...graph,
        truncated: true,
        diagnostics: [{ code: "evidence_budget_reached", count: 9 }],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);
    await user.click(
      screen.getByRole("button", {
        name: /SourceControlChangeGraphView\.tsx, Source File, Added/i,
      }),
    );
    expect(
      within(screen.getByTestId("change-graph-node-details")).getByText(
        "Some relationship evidence in this graph was truncated. Counts below reflect returned evidence rows.",
      ),
    ).toBeInTheDocument();
  });

  it("shows a neutral fallback for an unknown projection diagnostic", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        diagnostics: [{ code: "future_projection_condition", count: 4 }],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    await user.click(
      screen.getByRole("button", {
        name: "Explain unavailable Diff Graph relationship details",
      }),
    );
    expect(
      screen.getByRole("dialog", { name: "Diff Graph relationship details" }),
    ).toHaveTextContent(
      "4 additional relationship or supporting-evidence details couldn’t be displayed.",
    );
  });

  it("groups the Ontology column by folder with top-level pages first", () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: [
          ...graph.nodes,
          {
            ...graphNode("concept"),
            id: "presence",
            title: "presence",
            path: ".g5/ontology/features/presence.g5.md",
            changeState: "added",
            route: {
              ...graphNode("concept").route,
              filePath: ".g5/ontology/features/presence.g5.md",
            },
          },
          {
            ...graphNode("concept"),
            id: "overview",
            ontologyGroup: undefined,
            title: "overview",
            path: ".g5/ontology/overview.g5.md",
            route: {
              ...graphNode("concept").route,
              filePath: ".g5/ontology/overview.g5.md",
            },
          },
        ],
        edges: [],
      },
    });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const labels = screen.getAllByTestId("change-graph-group-label");
    const columnLabels = (left: string) =>
      labels.filter((label) => label.style.left === left);
    // Column 0 (Intents) starts at 24px and column 1 (Ontology) at 304px.
    expect(columnLabels("24px").map((label) => label.textContent)).toEqual([
      "Chats",
      "Documents",
    ]);
    const ontologyLabels = columnLabels("304px");
    expect(ontologyLabels.map((label) => label.textContent)).toEqual([
      "Top level",
      "architecture",
      "features",
    ]);
    expect(labels).toHaveLength(5);
    for (const label of ontologyLabels) {
      expect(label.tagName).toBe("H4");
      expect(label).toHaveClass("border-b", "font-semibold", "uppercase");
    }

    const top = (element: HTMLElement) => Number.parseFloat(element.style.top);
    const card = (name: RegExp) => screen.getByRole("button", { name });
    const [topLevel, architecture, features] = ontologyLabels;
    expect(top(topLevel)).toBe(56);
    expect(top(card(/^overview, Ontology, Modified/))).toBe(top(topLevel) + 32);
    expect(top(architecture)).toBe(
      top(card(/^overview, Ontology, Modified/)) + 112 + 24,
    );
    expect(top(card(/^source-control-change-graph, Ontology, Added/))).toBe(
      top(architecture) + 32,
    );
    expect(top(features)).toBe(
      top(card(/^source-control-change-graph, Ontology, Added/)) + 112 + 24,
    );
    const conceptTop = top(card(/^collaborative_editing_ux, Ontology/));
    expect(conceptTop).toBe(top(features) + 32);
    expect(top(card(/^presence, Ontology, Added/))).toBe(conceptTop + 112 + 16);
    for (const ontologyCard of [
      card(/^overview, Ontology/),
      card(/^collaborative_editing_ux, Ontology/),
    ]) {
      expect(ontologyCard).toHaveAttribute("data-column", "ontology");
      expect(
        within(ontologyCard).getByTestId("change-graph-card-eyebrow"),
      ).toHaveTextContent(/^Ontology$/);
    }
    // Source Code is one ungrouped list.
    expect(top(card(/SourceControlChangeGraphView\.tsx, Source File/))).toBe(
      56,
    );
  });

  it.each([
    {
      label: "every repository live",
      liveContent: [
        { repo: "code", includesUncommitted: true },
        { repo: "docs", includesUncommitted: true },
      ],
      copy: "Includes saved uncommitted edits.",
      warns: false,
    },
    {
      label: "committed only for a non-owner",
      liveContent: [
        { repo: "code", includesUncommitted: false, reason: "not_owner" },
        { repo: "docs", includesUncommitted: false, reason: "not_owner" },
      ],
      copy: "Committed changes only: only the workspace owner sees saved edits.",
      warns: false,
    },
    {
      label: "committed only with unpublished commits",
      liveContent: [
        {
          repo: "code",
          includesUncommitted: false,
          reason: "unpublished_commits",
        },
      ],
      copy: "Committed changes only: local commits are not published yet; ontology files are still current.",
      warns: true,
    },
    {
      label: "committed only for an unknown reason",
      liveContent: [
        { repo: "code", includesUncommitted: false, reason: "future_reason" },
      ],
      copy: "Committed changes only.",
      warns: true,
    },
    {
      label: "mixed with a stopped VM",
      liveContent: [
        { repo: "code", includesUncommitted: true },
        { repo: "docs", includesUncommitted: false, reason: "vm_not_running" },
      ],
      copy: "Includes saved uncommitted edits, except: docs shows committed changes only (the workspace VM is not running).",
      warns: false,
    },
    {
      label: "mixed with unreadable status",
      liveContent: [
        { repo: "code", includesUncommitted: true },
        {
          repo: "docs",
          includesUncommitted: false,
          reason: "status_unavailable",
        },
        {
          repo: "infra",
          includesUncommitted: false,
          reason: "workspace_synchronizing",
        },
      ],
      copy: "Includes saved uncommitted edits, except: docs shows committed changes only (the workspace status is unavailable); infra shows committed changes only (the workspace is synchronizing).",
      warns: true,
    },
    {
      label: "committed only for different reasons",
      liveContent: [
        {
          repo: "code",
          includesUncommitted: false,
          reason: "unpublished_commits",
        },
        {
          repo: "docs",
          includesUncommitted: false,
          reason: "status_unavailable",
        },
      ],
      copy: "Committed changes only: code shows committed changes only (local commits are not published yet; ontology files are still current); docs shows committed changes only (the workspace status is unavailable).",
      warns: true,
    },
  ])(
    "states whether All changes includes live edits: $label",
    ({ liveContent, copy, warns }) => {
      setGraph({
        status: "ok",
        data: {
          ...graph,
          range: {
            ...graph.range!,
            liveContent: liveContent.map(
              ({ repo, includesUncommitted, reason }) => ({
                repoId: `${repo}-id`,
                repoName: repo,
                includesUncommitted,
                reason,
                ontologyLive: includesUncommitted,
              }),
            ),
          },
        },
      });
      renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

      const line = screen.getByTestId("change-graph-live-content");
      expect(line).toHaveTextContent(copy, { normalizeWhitespace: true });
      expect(line.textContent).toBe(copy);
      if (warns) {
        expect(line).toHaveClass("text-[var(--g5-warn)]");
      } else {
        expect(line).not.toHaveClass("text-[var(--g5-warn)]");
      }
    },
  );

  it("omits the live-content line when the range does not report it", () => {
    setGraph({ status: "ok", data: graph });
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    expect(screen.queryByTestId("change-graph-live-content")).toBeNull();
    expect(screen.getByTestId("change-graph-legend")).toBeInTheDocument();
  });

  it("draws each link kind distinctly with a legend and marks mixed edges", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        edges: [
          ...graph.edges,
          {
            id: "edge-mixed",
            sourceNodeId: "concept",
            targetNodeId: "source",
            evidence: [
              {
                id: "evidence-mixed-implementation",
                sourceFileType: "ontology",
                sourceFile: CONCEPT_PATH,
                sourceId: "g5.collab.edit-window",
                targetFileType: "code",
                targetFile: SOURCE_PATH,
                targetId: "",
                relationship: "implementation",
                changeState: "added",
              },
              {
                id: "evidence-mixed-reference",
                sourceFileType: "ontology",
                sourceFile: CONCEPT_PATH,
                sourceId: "g5.collab.presence",
                targetFileType: "code",
                targetFile: SOURCE_PATH,
                targetId: "",
                relationship: "reference",
                changeState: "unchanged",
              },
            ],
          },
          {
            id: "edge-legacy-kind",
            sourceNodeId: "architecture",
            targetNodeId: "added-chat",
            evidence: [
              {
                id: "evidence-legacy-kind",
                sourceFileType: "ontology",
                sourceFile: ARCHITECTURE_PATH,
                sourceId: "g5.arch.change-graph",
                targetFileType: "chat",
                targetFile: ADDED_CHAT_PATH,
                targetId: "",
                relationship: "requests",
                changeState: "added",
              },
            ],
          },
        ],
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const legend = screen.getByTestId("change-graph-legend");
    expect(legend).toHaveAccessibleName("Link kinds");
    expect(
      within(legend)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual([
      "Reference",
      "Expands",
      "Provenance",
      "Implementation",
      "Project document",
      "Links cite their targets; they do not declare dependencies.",
    ]);
    const legendDashes = Array.from(legend.querySelectorAll("line")).map(
      (line) => line.getAttribute("stroke-dasharray"),
    );
    expect(legendDashes).toEqual([null, null, "6 4", "2 3", "8 3 2 3"]);
    const legendWidths = Array.from(legend.querySelectorAll("line")).map(
      (line) => Number(line.getAttribute("stroke-width")),
    );
    // Expands is drawn heavier than Reference, which shares its solid stroke.
    expect(legendWidths[1]).toBeGreaterThan(legendWidths[0]);

    const edge = (id: string) =>
      document.querySelector(
        `[data-testid="change-graph-edge"][data-edge-id="${id}"]`,
      );
    const expected: Array<[string, string, string | null]> = [
      ["edge-1", "provenance", "6 4"],
      ["edge-2", "project_document", "8 3 2 3"],
      ["edge-3", "reference", null],
      ["edge-neighbors", "provenance", "6 4"],
      ["edge-implementation", "implementation", "2 3"],
      ["edge-mixed", "mixed", null],
      ["edge-legacy-kind", "mixed", null],
    ];
    for (const [id, relationship, dash] of expected) {
      expect(edge(id)).toHaveAttribute("data-relationship", relationship);
      expect(edge(id)?.getAttribute("stroke-dasharray") ?? null).toBe(dash);
    }

    // Focused edges keep their kind in the foreground layer.
    await user.click(
      screen.getByRole("button", {
        name: /collaborative_editing_ux, Ontology, Modified/i,
      }),
    );
    const foreground = screen.getByTestId("change-graph-edge-layer-foreground");
    expect(foreground.querySelector('[data-edge-id="edge-1"]')).toHaveAttribute(
      "data-relationship",
      "provenance",
    );
    expect(
      foreground.querySelector('[data-edge-id="edge-mixed"]'),
    ).toHaveAttribute("data-relationship", "mixed");
  });

  it("flags added links with missing targets on the card, in details, and as a stub", async () => {
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) =>
          node.id === "concept"
            ? {
                ...node,
                missingTargets: [
                  {
                    statementId: "g5.collab.edit-window",
                    href: "../architecture/missing.g5.md#g5.arch.gone",
                    text: "gone",
                  },
                ],
              }
            : node.id === "architecture"
              ? {
                  ...node,
                  missingTargets: [
                    {
                      statementId: "g5.arch.change-graph",
                      href: "typo.g5.md",
                      text: "typo",
                    },
                    {
                      statementId: "g5.arch.change-graph",
                      href: "../features/never.g5.md#g5.never",
                      text: "never",
                    },
                  ],
                }
              : node,
        ),
      },
    });
    const user = userEvent.setup();
    renderApp(<SourceControlChangeGraphView workspaceId="ws-1" />);

    const concept = screen.getByRole("button", {
      name: /collaborative_editing_ux, Ontology, Modified/i,
    });
    const architecture = screen.getByRole("button", {
      name: /source-control-change-graph, Ontology, Added/i,
    });
    expect(
      within(concept).getByTestId("change-graph-card-notice"),
    ).toHaveTextContent("1 missing link target");
    expect(
      within(architecture).getByTestId("change-graph-card-notice"),
    ).toHaveTextContent("2 missing link targets");
    expect(
      within(
        screen.getByRole("button", {
          name: /SourceControlChangeGraphView\.tsx, Source File/i,
        }),
      ).queryByTestId("change-graph-card-notice"),
    ).toBeNull();

    const stubs = screen.getAllByTestId("change-graph-missing-target-stub");
    expect(stubs).toHaveLength(2);
    // Each stub dangles off the right edge of its card.
    const conceptStubLine = stubs[0].querySelector("line")!;
    expect(Number(conceptStubLine.getAttribute("x1"))).toBe(
      Number.parseFloat(concept.style.left) + 236,
    );
    expect(Number(conceptStubLine.getAttribute("y1"))).toBe(
      Number.parseFloat(concept.style.top) + 112 * 0.75,
    );
    expect(stubs[0].closest("svg")).toHaveAttribute("aria-hidden", "true");

    await user.click(concept);
    const section = screen.getByTestId("change-graph-missing-targets");
    expect(
      within(section).getByRole("heading", {
        level: 4,
        name: "Missing link targets",
      }),
    ).toBeInTheDocument();
    expect(within(section).getAllByRole("listitem")).toHaveLength(1);
    expect(within(section).getByRole("listitem")).toHaveTextContent(
      "g5.collab.edit-window links to ../architecture/missing.g5.md#g5.arch.gone, which resolves in neither version.",
    );
    // Missing targets never block the statement overview.
    expect(
      screen.getByTestId("change-graph-changed-semantic-blocks-section"),
    ).toBeInTheDocument();

    await user.click(architecture);
    expect(
      within(screen.getByTestId("change-graph-missing-targets")).getAllByRole(
        "listitem",
      ),
    ).toHaveLength(2);

    await user.click(
      screen.getByRole("button", {
        name: /Build current graph, Chat, Modified/i,
      }),
    );
    expect(screen.queryByTestId("change-graph-missing-targets")).toBeNull();
  });

  it("shows a Cannot compare warning instead of classifying statements", async () => {
    const warning = "Statement ID g5.collab.edit-window appears twice.";
    setGraph({
      status: "ok",
      data: {
        ...graph,
        nodes: graph.nodes.map((node) =>
          node.id === "concept"
            ? {
                ...node,
                changedRecords: [],
                comparisonWarning: warning,
                missingTargets: [
                  {
                    statementId: "g5.collab.edit-window",
                    href: "typo.g5.md",
                    text: "typo",
                  },
                ],
              }
            : node,
        ),
      },
    });
    mockUseSourceControlChangeGraphNodeDetails.mockImplementation(
      (
        _workspaceId: string,
        _comparison: string,
        _revision: string,
        nodeId: string,
      ) =>
        nodeId === "concept"
          ? {
              status: "ok",
              data: {
                revision: graph.revision,
                nodeId,
                kind: "file_diff",
                diff: availableDiff(),
              } satisfies SourceControlChangeGraphNodeDetails,
            }
          : { status: "ok", data: defaultDetails(nodeId) },
    );
    const user = userEvent.setup();
    renderApp(
      <SourceControlChangeGraphView
        workspaceId="ws-1"
        onOpenOntology={vi.fn()}
      />,
    );

    const concept = screen.getByRole("button", {
      name: /collaborative_editing_ux, Ontology, Modified/i,
    });
    // The comparison warning outranks the missing-target notice.
    expect(
      within(concept).getByTestId("change-graph-card-notice"),
    ).toHaveTextContent("Cannot compare statements");
    expect(
      within(
        screen.getByRole("button", {
          name: /source-control-change-graph, Ontology, Added/i,
        }),
      ).queryByTestId("change-graph-card-notice"),
    ).toBeNull();

    await user.click(concept);
    const left = screen.getByTestId("change-graph-node-details-left");
    expect(
      within(left).getByTestId("change-graph-comparison-warning"),
    ).toHaveTextContent(
      `Cannot compare this file's statements, so none is classified. ${warning}`,
    );
    expect(
      within(left).queryByTestId(
        "change-graph-changed-semantic-blocks-section",
      ),
    ).toBeNull();
    expect(
      within(left).queryByTestId("change-graph-open-statement"),
    ).toBeNull();
    expect(
      within(left).getByTestId("change-graph-grounding-edges-section"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId("change-graph-node-details-right")).getByTestId(
        "change-graph-file-diff",
      ),
    ).toBeInTheDocument();
    // The file itself still opens in the ontology editor.
    expect(
      screen.getByRole("button", { name: "Open document" }),
    ).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", {
        name: /source-control-change-graph, Ontology, Added/i,
      }),
    );
    expect(screen.queryByTestId("change-graph-comparison-warning")).toBeNull();
    expect(
      screen.getByTestId("change-graph-changed-semantic-blocks-section"),
    ).toBeInTheDocument();
  });
});
