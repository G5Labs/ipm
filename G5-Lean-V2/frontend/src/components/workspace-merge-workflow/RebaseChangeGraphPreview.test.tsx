import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ApiError } from "../../lib/api";
import type {
  RebaseChangeGraphNodeDetailsResponse,
  RebaseChangeGraphResponse,
  SourceControlChangeGraphDiffEvidence,
  SourceControlChangeGraphNode,
} from "../../lib/types";
import { RebaseChangeGraphPreview } from "./RebaseChangeGraphPreview";

const hooks = vi.hoisted(() => ({
  graph: vi.fn(),
  details: vi.fn(),
}));

vi.mock("../../hooks/useSourceControlChangeGraph", () => ({
  useRebaseChangeGraph: (...args: unknown[]) => hooks.graph(...args),
  useRebaseChangeGraphNodeDetails: (...args: unknown[]) =>
    hooks.details(...args),
}));

function node(
  id: string,
  title: string,
  column: SourceControlChangeGraphNode["column"],
  changeState: SourceControlChangeGraphNode["changeState"],
  changedRecords: string[],
): SourceControlChangeGraphNode {
  return {
    id,
    column,
    title,
    artifactType: column === "ontology" ? "ontology" : "Source File",
    path: `.g5/ontology/${id}.g5.md`,
    changeState,
    connectionState: "connected",
    additions: changeState === "deleted" ? 0 : 12,
    deletions: changeState === "added" ? 0 : 4,
    changedRecords: changedRecords.map((recordId) => ({
      id: recordId,
      label: recordId,
      changeState: "modified",
      isHuman: false,
    })),
    hasHumanBlocks: false,
    route: {
      repoId: "repo-1",
      workspaceId: "branch-1",
      fileType: column === "source_code" ? "code" : "ontology",
      filePath: `.g5/ontology/${id}.g5.md`,
    },
  };
}

const sourceShared = node("shared", "Merge Workflow", "ontology", "modified", [
  "D27",
  "D33",
]);
const sourceGray = node("gray", "Graph Service", "ontology", "modified", [
  "D4",
]);
const sourceOnly = node(
  "source-only",
  "Source-only Concept",
  "ontology",
  "modified",
  ["D2"],
);
const sourceOnlyTwo = node(
  "source-only-two",
  "Source-only Architecture",
  "ontology",
  "modified",
  ["D6"],
);
const sourceAdded = node(
  "source-added",
  "New Source Concept",
  "ontology",
  "added",
  ["I1"],
);
const sourceDeleted = node(
  "delete-edit",
  "Deleted on Source",
  "ontology",
  "deleted",
  ["D1"],
);
const sourceCode = node(
  "source-code",
  "merge.ts",
  "source_code",
  "modified",
  [],
);
const sourceChat: SourceControlChangeGraphNode = {
  ...node("source-chat", "Rebase review request", "intents", "modified", []),
  intentGroup: "conversations",
  artifactType: "Chat",
  route: {
    repoId: "repo-1",
    workspaceId: "branch-1",
    fileType: "chat",
    sessionId: "rebase-review",
  },
};
const targetDoc: SourceControlChangeGraphNode = {
  ...node("target-doc", "Merge Graph Plan", "intents", "modified", ["D1"]),
  intentGroup: "documents",
  artifactType: "Project Doc",
  route: {
    repoId: "repo-1",
    workspaceId: "branch-1",
    fileType: "external_doc",
    filePath: "docs/merge_graph_view/pr_scope_and_fix_plan.md",
  },
};
const targetShared = {
  ...sourceShared,
  changedRecords: sourceShared.changedRecords,
};
const targetGray = {
  ...sourceGray,
  changedRecords: [
    { id: "D9", label: "D9", changeState: "modified" as const, isHuman: false },
  ],
};
const targetOnly = node(
  "target-only",
  "Target-only Architecture",
  "ontology",
  "modified",
  ["D8"],
);
const targetOnlyTwo = node(
  "target-only-two",
  "Target-only Concept",
  "ontology",
  "modified",
  ["D10"],
);
const targetEditedDeleted = {
  ...sourceDeleted,
  changeState: "modified" as const,
  changedRecords: [
    { id: "D2", label: "D2", changeState: "modified" as const, isHuman: false },
  ],
};

const emptyGraphSide = {
  revision: "side-rev",
  comparison: "workspace" as const,
  range: { baseKind: "merge_base" as const, repositoryCount: 1 },
  edges: [],
  diagnostics: [],
  truncated: false,
};

const graph: RebaseChangeGraphResponse = {
  revision: "pair-rev",
  sourceWorkspace: { id: "source-1", name: "feature" },
  targetWorkspace: { id: "target-1", name: "main" },
  commonBaseRepositoryCount: 1,
  source: {
    ...emptyGraphSide,
    nodes: [
      sourceShared,
      sourceGray,
      sourceOnly,
      sourceOnlyTwo,
      sourceAdded,
      sourceDeleted,
      sourceCode,
      sourceChat,
    ],
    edges: [
      {
        id: "relationship-1",
        sourceNodeId: "shared",
        targetNodeId: "gray",
        evidence: [],
      },
    ],
  },
  target: {
    ...emptyGraphSide,
    nodes: [
      targetShared,
      targetGray,
      targetOnly,
      targetOnlyTwo,
      targetEditedDeleted,
      targetDoc,
    ],
  },
  conflicts: [
    {
      id: "red",
      artifactKey: "shared",
      sourceNodeId: "shared",
      targetNodeId: "shared",
      kind: "same_block",
      overlappingBlockIds: ["D27", "D33"],
      overlapBlockCount: 2,
    },
    {
      id: "gray",
      artifactKey: "gray",
      sourceNodeId: "gray",
      targetNodeId: "gray",
      kind: "same_node",
      overlappingBlockIds: [],
    },
    {
      id: "delete-edit",
      artifactKey: "delete-edit",
      sourceNodeId: "delete-edit",
      targetNodeId: "delete-edit",
      kind: "same_node",
      overlappingBlockIds: [],
    },
  ],
};

const details: RebaseChangeGraphNodeDetailsResponse = {
  revision: "pair-rev",
  artifactKey: "shared",
  title: "Merge Workflow",
  artifact: "Ontology Concept",
  path: "core_workflow/merge_workflow",
  source: {
    node: sourceShared,
    changedLineCount: 16,
    changedBlockCount: 5,
    detail: {
      kind: "semantic_diff",
      records: sourceShared.changedRecords.map((record) => ({
        id: record.id,
        recordKind: "D",
        title: `Decision ${record.id}`,
        changeState: "modified",
        isHuman: false,
        diff: {
          availability: "available",
          additions: 1,
          deletions: 1,
          hunks: [
            {
              startLine: 1,
              header: "@@ -1 +1 @@",
              lines: [
                { type: "deletion", content: "before" },
                { type: "addition", content: "after" },
              ],
            },
          ],
          returnedLineCount: 2,
          omittedLineCount: 0,
          truncated: false,
          partialFinalHunk: false,
        },
      })),
      truncated: false,
    },
  },
  target: {
    node: targetShared,
    changedLineCount: 16,
    changedBlockCount: 4,
    detail: {
      kind: "semantic_diff",
      records: [],
      truncated: false,
    },
  },
  overlapLineCount: 4,
  overlapBlockIds: ["D27", "D33"],
  overlapBlockCount: 7,
};

function availableDiff(content: string): SourceControlChangeGraphDiffEvidence {
  return {
    availability: "available",
    additions: 1,
    deletions: 0,
    hunks: [
      {
        startLine: 4,
        header: "@@ -4,0 +5 @@",
        lines: [{ type: "addition", content }],
      },
    ],
    returnedLineCount: 1,
    omittedLineCount: 0,
    truncated: false,
    partialFinalHunk: false,
  };
}

function renderPreview() {
  return render(previewElement());
}

function previewElement() {
  return (
    <RebaseChangeGraphPreview
      sourceWorkspaceId="source-1"
      targetWorkspaceId="target-1"
      sourceWorkspaceName="feature"
      targetWorkspaceName="main"
    />
  );
}

describe("RebaseChangeGraphPreview", () => {
  beforeEach(() => {
    hooks.graph.mockReset();
    hooks.details.mockReset();
    hooks.graph.mockReturnValue({ status: "ok", data: graph, retry: vi.fn() });
    hooks.details.mockReturnValue({ status: "ok", data: details });
  });

  it("keeps typed graph errors local and retries on request", () => {
    const retry = vi.fn();
    hooks.graph.mockReturnValue({
      status: "error",
      error: new Error("missing divergence base"),
      retry,
    });

    renderPreview();

    expect(screen.getByText(/missing divergence base/i)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });

  it("formats each crossed Diff Graph size limit and falls back safely", () => {
    const cases = [
      {
        details: {
          observedRoots: 501,
          maxRoots: 500,
          observedBytes: 1024,
          maxBytes: 100 * 1024 * 1024,
        },
        expected:
          "Unable to load the rebase comparison: This rebase comparison has 501 changed artifacts; the limit is 500.",
      },
      {
        details: {
          observedRoots: 128,
          maxRoots: 500,
          observedBytes: 130_000_000,
          maxBytes: 100 * 1024 * 1024,
        },
        expected:
          "Unable to load the rebase comparison: This rebase comparison requires 124 MiB of diff input; the limit is 100 MiB.",
      },
      {
        details: {
          observedRoots: 501,
          maxRoots: 500,
          observedBytes: 130_000_000,
          maxBytes: 100 * 1024 * 1024,
        },
        expected:
          "Unable to load the rebase comparison: This rebase comparison exceeds both supported limits: 501 changed artifacts (limit 500) and 124 MiB of diff input (limit 100 MiB).",
      },
      {
        details: { observedRoots: 128, maxRoots: 500 },
        expected:
          "Unable to load the rebase comparison: This rebase comparison exceeds the supported size.",
      },
      {
        details: {
          observedRoots: 128,
          maxRoots: 500,
          observedBytes: 1024,
          maxBytes: 100 * 1024 * 1024,
        },
        expected:
          "Unable to load the rebase comparison: This rebase comparison exceeds the supported size.",
      },
    ];

    for (const testCase of cases) {
      hooks.graph.mockReturnValue({
        status: "error",
        error: new ApiError({
          code: "DIFF_GRAPH_TOO_LARGE",
          status: 413,
          message: "Workspace Diff Graph comparison is too large",
          details: testCase.details,
        }),
        retry: vi.fn(),
      });

      const view = renderPreview();

      expect(screen.getByText(testCase.expected)).toBeVisible();
      expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
      view.unmount();
    }
  });

  it("renders one-sided added deleted and edited ontology nodes", () => {
    renderPreview();

    expect(screen.getByLabelText("Ontology")).toBeChecked();
    expect(screen.getByLabelText("Ontology")).toBeDisabled();
    expect(screen.getAllByTestId("rebase-ontology-lane")).toHaveLength(2);
    expect(screen.getByTestId("rebase-graph-scroll-region")).toHaveClass(
      "h-[clamp(320px,46vh,420px)]",
      "overflow-auto",
    );
    expect(screen.getByTestId("rebase-graph-scroll-region")).not.toHaveClass(
      "overscroll-contain",
    );
    expect(
      screen.queryByRole("button", { name: /merge\.ts/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /New Source Concept/i }),
    ).toHaveAttribute("data-change-state", "added");
    expect(
      screen.getByRole("button", { name: /Source-only Concept/i }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: /Source-only Architecture/i }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: /Target-only Architecture/i }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: /Target-only Concept/i }),
    ).toBeVisible();
    expect(
      screen.getAllByRole("button", { name: /Deleted on Source/i })[0],
    ).toHaveAttribute("data-change-state", "deleted");
  });

  it("exposes side, lifecycle, and overlap semantics to keyboard users", async () => {
    const user = userEvent.setup();
    renderPreview();

    const sourceConflict = screen.getByRole("button", {
      name: /Source Merge Workflow, Ontology, Modified, same-block overlap, 2 overlapping blocks/i,
    });
    expect(
      screen.getByRole("button", {
        name: /Target Merge Workflow, Ontology, Modified, same-block overlap, 2 overlapping blocks/i,
      }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", {
        name: /Source Graph Service, Ontology, Modified, same-node overlap/i,
      }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", {
        name: /Source Source-only Concept, Ontology, Modified, no cross-side overlap/i,
      }),
    ).toBeVisible();

    sourceConflict.focus();
    await user.keyboard("{Enter}");
    expect(screen.getByTestId("rebase-graph-node-details")).toBeVisible();
  });

  it("renders shared graph cards, selected layers, and induced edge-type subgraphs", () => {
    renderPreview();

    expect(screen.getByRole("group", { name: "Edge types" })).toBeVisible();
    expect(screen.getAllByTestId("rebase-conflict-edge")).toHaveLength(3);
    expect(screen.getAllByTestId("rebase-graph-node")).toHaveLength(11);
    expect(screen.getAllByTestId("rebase-graph-node")[0]).toHaveAttribute(
      "data-ui",
      "change-graph-card",
    );
    expect(screen.getByTestId("rebase-relationship-edge")).toHaveAttribute(
      "stroke-dasharray",
      "6 5",
    );
    expect(screen.getByTestId("rebase-conflict-count-badge")).toHaveTextContent(
      "2",
    );

    fireEvent.click(screen.getByRole("button", { name: "Red only" }));
    expect(screen.getAllByTestId("rebase-conflict-edge")).toHaveLength(1);
    expect(screen.getAllByTestId("rebase-graph-node")).toHaveLength(2);
    expect(
      screen.queryByRole("button", { name: /Source-only Concept/i }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Gray only" }));
    expect(screen.getAllByTestId("rebase-conflict-edge")).toHaveLength(3);
    expect(screen.getAllByTestId("rebase-graph-node")).toHaveLength(6);
    expect(
      screen.getAllByRole("button", { name: /Merge Workflow/i })[0],
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /Source-only Concept/i }),
    ).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "All edge types" }));
    expect(
      screen.getByRole("button", { name: /Source-only Concept/i }),
    ).toBeVisible();

    fireEvent.click(screen.getByLabelText("Code"));
    expect(screen.getByRole("button", { name: /merge\.ts/i })).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /Rebase review request/i }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Chat"));
    expect(
      screen.getByRole("button", { name: /Rebase review request/i }),
    ).toBeVisible();
    fireEvent.click(screen.getByLabelText("Project Docs"));
    expect(
      screen.getByRole("button", { name: /Merge Graph Plan/i }),
    ).toBeVisible();
  });

  it("opens paired semantic details and marks shared blocks as overlap", () => {
    renderPreview();
    fireEvent.click(
      screen.getAllByRole("button", { name: /Merge Workflow/i })[0],
    );

    expect(hooks.details).toHaveBeenLastCalledWith(
      "source-1",
      "target-1",
      "pair-rev",
      "shared",
    );
    const drawer = screen.getByTestId("rebase-graph-node-details");
    expect(within(drawer).getByText("Artifact")).toBeVisible();
    expect(within(drawer).getByText("Path")).toBeVisible();
    expect(within(drawer).getByText("Source changed lines")).toBeVisible();
    expect(within(drawer).getByText("Target changed blocks")).toBeVisible();
    expect(within(drawer).getByText("Overlap lines")).toBeVisible();
    expect(within(drawer).getByText("Source changes")).toBeVisible();
    expect(within(drawer).getByText("Target changes")).toBeVisible();
    expect(within(drawer).getAllByText("Overlap")).toHaveLength(2);
    for (const block of within(drawer).getAllByTestId("rebase-changed-block")) {
      expect(block).toHaveAttribute("data-overlap", "true");
    }

    const summary = within(drawer).getByTestId("rebase-node-pair-summary");
    expect(
      within(summary).getByText("Source changed lines").parentElement,
    ).toHaveTextContent("Source changed lines16");
    expect(
      within(summary).getByText("Source changed blocks").parentElement,
    ).toHaveTextContent("Source changed blocks5");
    expect(
      within(summary).getByText("Source returned block IDs").parentElement,
    ).toHaveTextContent("Source returned block IDsD27, D33");
    expect(
      within(summary).getByText("Target changed lines").parentElement,
    ).toHaveTextContent("Target changed lines16");
    expect(
      within(summary).getByText("Target changed blocks").parentElement,
    ).toHaveTextContent("Target changed blocks4");
    expect(
      within(summary).getByText("Target returned block IDs").parentElement,
    ).toHaveTextContent("Target returned block IDsNone returned");
    expect(
      within(summary).getByText("Overlap blocks").parentElement,
    ).toHaveTextContent("Overlap blocks7");
    expect(
      within(summary).getByText("Overlap returned block IDs").parentElement,
    ).toHaveTextContent("Overlap returned block IDsD27, D33");
  });

  it("renders Source residual file evidence beside Target semantic evidence", () => {
    const source = details.source!;
    const target = details.target!;
    if (source.detail.kind !== "semantic_diff") {
      throw new Error("Expected semantic Source fixture");
    }
    const targetRecord = source.detail.records[0];
    hooks.graph.mockReturnValue({
      status: "ok",
      data: {
        ...graph,
        conflicts: graph.conflicts.map((conflict) =>
          conflict.id === "red"
            ? {
                ...conflict,
                kind: "same_node" as const,
                overlappingBlockIds: [],
                overlapBlockCount: undefined,
              }
            : conflict,
        ),
      },
      retry: vi.fn(),
    });
    hooks.details.mockReturnValue({
      status: "ok",
      data: {
        ...details,
        source: {
          ...source,
          changedLineCount: 1,
          changedBlockCount: 0,
          detail: {
            kind: "semantic_diff",
            records: [],
            otherFileChanges: availableDiff(
              "  - generation/g5d/sidecars/claude/provenance_env.mjs",
            ),
            truncated: false,
          },
        },
        target: {
          ...target,
          changedLineCount: 2,
          changedBlockCount: 1,
          detail: {
            kind: "semantic_diff",
            records: [targetRecord],
            truncated: false,
          },
        },
        overlapLineCount: 0,
        overlapBlockIds: [],
        overlapBlockCount: 0,
      } satisfies RebaseChangeGraphNodeDetailsResponse,
    });

    renderPreview();
    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );

    const sourceChanges = screen.getByTestId("rebase-source-changes");
    const targetChanges = screen.getByTestId("rebase-target-changes");
    expect(
      within(sourceChanges).getByText(
        "Other changes (metadata, headings or prose)",
      ),
    ).toBeVisible();
    expect(
      within(sourceChanges).getByRole("region", {
        name: "Other file changes diff",
      }),
    ).toHaveAttribute("tabindex", "0");
    expect(
      within(sourceChanges).getByTestId("rebase-semantic-other-file-diff"),
    ).toHaveTextContent("- generation/g5d/sidecars/claude/provenance_env.mjs");
    expect(
      within(sourceChanges).queryByText("No changed statements."),
    ).toBeNull();
    expect(within(targetChanges).getByText("Decision D27")).toBeVisible();
    expect(screen.getAllByTestId("rebase-conflict-edge")[0]).toHaveAttribute(
      "data-conflict-kind",
      "same_node",
    );
    const summary = screen.getByTestId("rebase-node-pair-summary");
    expect(
      within(summary).getByText("Source changed lines").parentElement,
    ).toHaveTextContent("Source changed lines1");
    expect(
      within(summary).getByText("Source changed blocks").parentElement,
    ).toHaveTextContent("Source changed blocks0");
    expect(
      within(summary).getByText("Overlap blocks").parentElement,
    ).toHaveTextContent("Overlap blocks0");
  });

  it("renders mirrored and mixed residual evidence without false empty states", () => {
    const source = details.source!;
    const target = details.target!;
    if (source.detail.kind !== "semantic_diff") {
      throw new Error("Expected semantic Source fixture");
    }
    const semanticRecord = source.detail.records[0];
    hooks.graph.mockReturnValue({
      status: "ok",
      data: {
        ...graph,
        conflicts: graph.conflicts.map((conflict) =>
          conflict.id === "red"
            ? {
                ...conflict,
                kind: "same_node" as const,
                overlappingBlockIds: [],
                overlapBlockCount: undefined,
              }
            : conflict,
        ),
      },
      retry: vi.fn(),
    });
    hooks.details.mockReturnValue({
      status: "ok",
      data: {
        ...details,
        source: {
          ...source,
          detail: {
            kind: "semantic_diff",
            records: [semanticRecord],
            truncated: false,
          },
        },
        target: {
          ...target,
          changedLineCount: 1,
          changedBlockCount: 0,
          detail: {
            kind: "semantic_diff",
            records: [],
            otherFileChanges: availableDiff("  - generation/target-only.ts"),
            truncated: false,
          },
        },
        overlapLineCount: 0,
        overlapBlockIds: [],
        overlapBlockCount: 0,
      } satisfies RebaseChangeGraphNodeDetailsResponse,
    });

    const view = renderPreview();
    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );
    const targetChanges = screen.getByTestId("rebase-target-changes");
    expect(
      within(targetChanges).getByText(
        "Other changes (metadata, headings or prose)",
      ),
    ).toBeVisible();
    expect(
      within(targetChanges).queryByText("No changed statements."),
    ).toBeNull();
    const summary = screen.getByTestId("rebase-node-pair-summary");
    expect(
      within(summary).getByText("Target changed lines").parentElement,
    ).toHaveTextContent("Target changed lines1");
    expect(
      within(summary).getByText("Target changed blocks").parentElement,
    ).toHaveTextContent("Target changed blocks0");
    expect(
      within(summary).getByText("Overlap blocks").parentElement,
    ).toHaveTextContent("Overlap blocks0");
    expect(screen.getAllByTestId("rebase-conflict-edge")[0]).toHaveAttribute(
      "data-conflict-kind",
      "same_node",
    );

    view.unmount();
    hooks.details.mockReturnValue({
      status: "ok",
      data: {
        ...details,
        source: {
          ...source,
          detail: {
            kind: "semantic_diff",
            records: [semanticRecord],
            otherFileChanges: availableDiff("  - generation/mixed.ts"),
            truncated: false,
          },
        },
      } satisfies RebaseChangeGraphNodeDetailsResponse,
    });
    renderPreview();
    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );
    const sourceChanges = screen.getByTestId("rebase-source-changes");
    expect(within(sourceChanges).getByText("Decision D27")).toBeVisible();
    expect(
      within(sourceChanges).getByText(
        "Other changes (metadata, headings or prose)",
      ),
    ).toBeVisible();
  });

  it("preserves typed residual availability truncation and true-empty states", () => {
    const source = details.source!;
    const target = details.target!;
    hooks.details.mockReturnValue({
      status: "ok",
      data: {
        ...details,
        source: {
          ...source,
          detail: {
            kind: "semantic_diff",
            records: [],
            otherFileChanges: {
              ...availableDiff(""),
              availability: "binary",
              reason: "binary_content",
              hunks: [],
              returnedLineCount: 0,
            },
            truncated: false,
          },
        },
        target: {
          ...target,
          detail: {
            kind: "semantic_diff",
            records: [],
            otherFileChanges: {
              ...availableDiff("  - generation/truncated.ts"),
              truncated: true,
              omittedLineCount: 3,
            },
            truncated: true,
          },
        },
      } satisfies RebaseChangeGraphNodeDetailsResponse,
    });

    const view = renderPreview();
    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );
    expect(
      within(screen.getByTestId("rebase-source-changes")).getByText(
        "This binary change has no textual diff.",
      ),
    ).toBeVisible();
    expect(
      within(screen.getByTestId("rebase-target-changes")).getByText(
        "Additional diff hunks were omitted at the response limit.",
      ),
    ).toBeVisible();
    expect(
      within(screen.getByTestId("rebase-target-changes")).getByText(
        "Some statement change evidence was truncated at the response limit.",
      ),
    ).toBeVisible();

    view.unmount();
    hooks.details.mockReturnValue({
      status: "ok",
      data: {
        ...details,
        source: {
          ...source,
          detail: { kind: "semantic_diff", records: [], truncated: false },
        },
        target: {
          ...target,
          detail: { kind: "semantic_diff", records: [], truncated: false },
        },
      } satisfies RebaseChangeGraphNodeDetailsResponse,
    });
    renderPreview();
    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );
    expect(screen.getAllByText("No changed statements.")).toHaveLength(2);
  });

  it("marks human I/D semantic records in paired graph details", () => {
    const source = details.source!;
    const sourceDetail = source.detail;
    hooks.details.mockReturnValue({
      status: "ok",
      data: {
        ...details,
        source: {
          ...source,
          detail:
            sourceDetail.kind === "semantic_diff"
              ? {
                  ...sourceDetail,
                  records: sourceDetail.records.map((record, index) => ({
                    ...record,
                    isHuman: index === 0,
                  })),
                }
              : sourceDetail,
        },
      } satisfies RebaseChangeGraphNodeDetailsResponse,
    });
    renderPreview();

    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );
    const sourceChanges = screen.getByTestId("rebase-source-changes");
    const blocks = within(sourceChanges).getAllByTestId("rebase-changed-block");
    expect(within(blocks[0]).getByText("Human")).toHaveClass(
      "bg-[var(--g5-ok)]/15",
      "text-[var(--g5-ok)]",
    );
    expect(within(blocks[1]).queryByText("Human")).toBeNull();
  });

  it("closes stale details, refetches the paired graph, and requires reselection", async () => {
    const retry = vi.fn();
    hooks.graph.mockReturnValue({ status: "ok", data: graph, retry });
    hooks.details.mockReturnValue({
      status: "error",
      error: new ApiError({
        code: "STALE_GRAPH_REVISION",
        status: 409,
        message: "The graph changed",
      }),
    });
    renderPreview();

    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );

    await waitFor(() =>
      expect(screen.queryByTestId("rebase-graph-node-details")).toBeNull(),
    );
    expect(screen.queryByText("The graph changed")).not.toBeInTheDocument();
    expect(retry).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    ).toHaveAttribute("aria-pressed", "false");
  });

  it("does not treat an ordinary 409 detail error as a stale graph", () => {
    const retry = vi.fn();
    hooks.graph.mockReturnValue({ status: "ok", data: graph, retry });
    hooks.details.mockReturnValue({
      status: "error",
      error: new ApiError({
        code: "CONFLICT",
        status: 409,
        message: "A different conflict",
      }),
    });
    renderPreview();

    fireEvent.click(
      screen.getByRole("button", {
        name: /Source Merge Workflow, Ontology, Modified/i,
      }),
    );

    expect(screen.getByTestId("rebase-graph-node-details")).toBeVisible();
    expect(screen.getByText("A different conflict")).toBeVisible();
    expect(retry).not.toHaveBeenCalled();
  });

  it("provides zoom, Fit, pointer-pan, modifier-wheel, and listener cleanup", () => {
    const view = renderPreview();
    const viewport = screen.getByTestId("rebase-graph-scroll-region");
    Object.defineProperties(viewport, {
      clientWidth: { configurable: true, value: 660 },
      clientHeight: { configurable: true, value: 360 },
      scrollLeft: { configurable: true, writable: true, value: 120 },
      scrollTop: { configurable: true, writable: true, value: 90 },
    });
    Object.assign(viewport, {
      scrollTo: vi.fn(),
      setPointerCapture: vi.fn(),
      releasePointerCapture: vi.fn(),
      hasPointerCapture: vi.fn(() => true),
    });
    const removeEventListener = vi.spyOn(viewport, "removeEventListener");

    expect(screen.getByText("100%")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByText("105%")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(screen.getByText("100%")).toBeVisible();

    const plainWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      deltaY: -1,
    });
    fireEvent(viewport, plainWheel);
    expect(plainWheel.defaultPrevented).toBe(false);
    expect(screen.getByText("100%")).toBeVisible();

    const zoomWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      deltaY: -1,
    });
    fireEvent(viewport, zoomWheel);
    expect(zoomWheel.defaultPrevented).toBe(true);
    expect(screen.getByText("108%")).toBeVisible();

    const metaZoomWheel = new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      metaKey: true,
      deltaY: 1,
    });
    fireEvent(viewport, metaZoomWheel);
    expect(metaZoomWheel.defaultPrevented).toBe(true);
    expect(screen.getByText("100%")).toBeVisible();

    const pointerEvent = (
      type: string,
      { clientX, clientY }: { clientX: number; clientY: number },
    ) => {
      const event = new MouseEvent(type, {
        bubbles: true,
        button: 0,
        clientX,
        clientY,
      });
      Object.defineProperty(event, "pointerId", { value: 7 });
      fireEvent(viewport, event);
    };
    pointerEvent("pointerdown", { clientX: 100, clientY: 100 });
    pointerEvent("pointermove", { clientX: 80, clientY: 70 });
    expect(viewport.scrollLeft).toBe(140);
    expect(viewport.scrollTop).toBe(120);
    pointerEvent("pointerup", { clientX: 80, clientY: 70 });
    expect(viewport.releasePointerCapture).toHaveBeenCalledWith(7);

    fireEvent.click(screen.getByRole("button", { name: "Fit graph" }));
    // One ontology lane per side stacks every ontology card, so the board is
    // taller and Fit is height-bound.
    expect(screen.getByText("46%")).toBeVisible();
    expect(viewport.scrollTo).toHaveBeenCalledWith({
      left: 0,
      top: 0,
      behavior: "smooth",
    });

    view.unmount();
    expect(removeEventListener).toHaveBeenCalledWith(
      "wheel",
      expect.any(Function),
    );
  });

  it("renders typed Source partial/truncated and Target filter-empty states", () => {
    hooks.graph.mockReturnValue({
      status: "ok",
      data: {
        ...graph,
        source: {
          ...graph.source,
          diagnostics: [{ code: "repository_unavailable", count: 2 }],
          truncated: true,
        },
        target: { ...emptyGraphSide, nodes: [targetDoc] },
      },
      retry: vi.fn(),
    });
    renderPreview();

    const sourceStatus = screen.getByLabelText("Source graph status");
    expect(sourceStatus).toHaveTextContent("Source projection is partial");
    expect(
      within(sourceStatus).getByTestId(
        "source-diagnostic-repository_unavailable",
      ),
    ).toHaveTextContent("Repository unavailable: 2");
    expect(sourceStatus).toHaveTextContent(
      "Source graph evidence is truncated",
    );
    expect(screen.getByLabelText("Target graph status")).toHaveTextContent(
      "Target has no changes in the selected layers",
    );
  });

  it("distinguishes Source-empty and Target-empty projections", () => {
    hooks.graph.mockReturnValue({
      status: "ok",
      data: {
        ...graph,
        source: { ...emptyGraphSide, nodes: [] },
        target: { ...emptyGraphSide, nodes: [] },
      },
      retry: vi.fn(),
    });
    renderPreview();

    expect(screen.getByLabelText("Source graph status")).toHaveTextContent(
      "Source has no changes from the shared divergence",
    );
    expect(screen.getByLabelText("Target graph status")).toHaveTextContent(
      "Target has no changes from the shared divergence",
    );
  });

  it("clears revision-bound details when the paired graph revision changes", () => {
    const view = renderPreview();
    fireEvent.click(
      screen.getAllByRole("button", { name: /Merge Workflow/i })[0],
    );
    expect(screen.getByTestId("rebase-graph-node-details")).toBeVisible();

    hooks.graph.mockReturnValue({
      status: "ok",
      data: { ...graph, revision: "pair-rev-2" },
      retry: vi.fn(),
    });
    view.rerender(previewElement());

    expect(
      screen.queryByTestId("rebase-graph-node-details"),
    ).not.toBeInTheDocument();
  });
});
