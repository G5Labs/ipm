import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { MergePreflightResponse } from "../../lib/types";
import {
  WorkspaceLiveGateProvider,
  type LiveGateKind,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import { DirectCleanMergeChoiceModal } from "./DirectCleanMergeChoiceModal";

function mockPreflight(
  decision: MergePreflightResponse["decision"],
  sourceKind: MergePreflightResponse["sourceKind"] = "workspace",
): MergePreflightResponse {
  return {
    sourceKind,
    sourceWorkspaceId: "source-workspace",
    targetWorkspaceId: "target-workspace",
    decision,
    canDirectMerge: decision !== "workflowRequired",
    preflightFingerprint: "sha256:test",
    message: "No Git conflicts detected.",
    repoResults: [
      {
        repoId: "repo-1",
        sourceWorkspaceBranchId: "source-branch-1",
        targetWorkspaceBranchId: "target-branch-1",
        sourceBranchName: "feature",
        targetBranchName: "main",
        sourceHead: "abc",
        targetHead: "def",
        outcome:
          decision === "fastForwardOrNoOp"
            ? "fastForward"
            : decision === "gitConflicts"
              ? "gitConflicts"
              : decision === "workflowRequired"
                ? "workflowRequired"
                : "cleanMerge",
        conflictPaths: [],
        message: null,
      },
    ],
  };
}

describe("DirectCleanMergeChoiceModal", () => {
  it("shows fast-forward workspace review with preview and approval only", () => {
    render(
      <DirectCleanMergeChoiceModal
        open
        title="Review Workspace Merge"
        preflight={mockPreflight("fastForwardOrNoOp")}
        operationKind="workspace-rebase"
        previewAction={<button type="button">Preview rebased state</button>}
        onCancel={vi.fn()}
        onApprove={vi.fn()}
        onFullWorkflow={vi.fn()}
      />,
    );

    expect(screen.getByText("Preview rebased state")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Rebase Workspace" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Run Full Rebase Workflow" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText(/the target workspace will not be advanced/i),
    ).toBeVisible();
  });

  it("keeps clean divergent workspace approval source-only", () => {
    render(
      <DirectCleanMergeChoiceModal
        open
        title="No Git Conflicts"
        preflight={mockPreflight("cleanMergeNoGitConflicts")}
        operationKind="workspace-rebase"
        onCancel={vi.fn()}
        onApprove={vi.fn()}
        onFullWorkflow={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("button", { name: "Rebase Workspace" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Direct Merge" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Run Full Rebase Workflow" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText(/the target workspace will not be advanced/i),
    ).toBeVisible();
  });

  it("preserves direct-clean and full-workflow choices for task merges", () => {
    render(
      <DirectCleanMergeChoiceModal
        open
        title="No Git Conflicts"
        preflight={mockPreflight("cleanMergeNoGitConflicts", "workspace")}
        operationKind="workspace-merge"
        onCancel={vi.fn()}
        onApprove={vi.fn()}
        onFullWorkflow={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Direct Merge" })).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Run Full Rebase Workflow" }),
    ).toBeVisible();
  });

  it("offers an agent when direct merging is unavailable when direct clean merge is unavailable", () => {
    render(
      <DirectCleanMergeChoiceModal
        open
        title="Full Rebase Workflow Required"
        preflight={mockPreflight("workflowRequired")}
        operationKind="workspace-rebase"
        onCancel={vi.fn()}
        onApprove={vi.fn()}
        onFullWorkflow={vi.fn()}
      />,
    );

    expect(
      screen.getByText(
        /An agent will rebase the current workspace onto the target/i,
      ),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Resolve with Agent" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Direct Merge" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Approve & Merge" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Rebase Workspace" }),
    ).not.toBeInTheDocument();
  });

  // Start and resolve with agent (g5.git.stopped-rebase.resolve-with-agent,
  // g5.chat.start-and-send): the owner's stopped or failed VM makes the
  // agent choice the compound Start; a Start under way is joined; a running
  // VM and a non-owner see the ordinary choice.
  describe("Start and resolve with agent", () => {
    const gate = (kind: LiveGateKind, owner = true): WorkspaceLiveGate => ({
      kind,
      canWrite: owner,
      live: kind === "live",
      canStart: owner && (kind === "stopped" || kind === "failed"),
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner,
      workspaceNoun: "the workspace",
    });
    function renderConflicts(
      value: WorkspaceLiveGate,
      onFullWorkflow = vi.fn(),
    ) {
      render(
        <WorkspaceLiveGateProvider value={value}>
          <DirectCleanMergeChoiceModal
            open
            title="Resolve Git Conflicts"
            preflight={mockPreflight("gitConflicts")}
            operationKind="workspace-rebase"
            startWaitReason="all hosts are busy"
            onCancel={vi.fn()}
            onApprove={vi.fn()}
            onFullWorkflow={onFullWorkflow}
          />
        </WorkspaceLiveGateProvider>,
      );
      return onFullWorkflow;
    }

    it.each(["stopped", "failed"] as const)(
      "on the owner's %s VM reads Start and resolve with agent, explains it and asks for the compound Start",
      (kind) => {
        const onFullWorkflow = renderConflicts(gate(kind));
        const button = screen.getByRole("button", {
          name: "Start and resolve with agent",
        });
        const note = screen.getByTestId(
          "direct-clean-merge-start-and-resolve-note",
        );
        expect(note).toHaveTextContent(
          kind === "failed"
            ? "The workspace failed to start."
            : "The workspace is stopped.",
        );
        expect(note).toHaveTextContent(
          "starts it and queues the agent’s first message, sent once it is running",
        );
        expect(
          screen.getByText(/An agent will rebase the current workspace/),
        ).toBeVisible();
        expect(screen.queryByText(/Start its workspace VM first/)).toBeNull();
        fireEvent.click(button);
        expect(onFullWorkflow).toHaveBeenCalledWith({ startCompute: true });
      },
    );

    it.each(["starting", "queued"] as const)(
      "while the owner's Start is %s stays Resolve with Agent, shows the Start's progress and joins the Start",
      (kind) => {
        const onFullWorkflow = renderConflicts(gate(kind));
        expect(
          screen.queryByRole("button", {
            name: "Start and resolve with agent",
          }),
        ).toBeNull();
        const progress = screen.getByTestId("start-progress");
        if (kind === "queued") {
          expect(progress).toHaveTextContent("all hosts are busy");
        }
        expect(
          screen.queryByTestId("direct-clean-merge-start-and-resolve-note"),
        ).toBeNull();
        fireEvent.click(
          screen.getByRole("button", { name: "Resolve with Agent" }),
        );
        expect(onFullWorkflow).toHaveBeenCalledWith({ startCompute: true });
      },
    );

    it.each([
      ["a running VM", gate("live")],
      ["a non-owner", gate("reader", false)],
    ] as const)("keeps the ordinary choice for %s", (_label, value) => {
      const onFullWorkflow = renderConflicts(value);
      expect(
        screen.queryByTestId("direct-clean-merge-start-and-resolve-note"),
      ).toBeNull();
      expect(screen.queryByTestId("start-progress")).toBeNull();
      fireEvent.click(
        screen.getByRole("button", { name: "Resolve with Agent" }),
      );
      expect(onFullWorkflow).toHaveBeenCalledWith({ startCompute: false });
    });
  });
});
