// Home → Workspaces → Approvals cell (g5.ui.home-workspaces): stacked
// requirement counts from each requirement's one server status
// (g5.governance.approval.status), never an approver tally.
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import {
  approvalRequirement,
  approvalsState,
  approver,
} from "../../test/approvalFixtures";
import { ApprovalGates } from "./ApprovalGates";

describe("ApprovalGates", () => {
  it("counts an any-of approved by a human over an agent rejection as approved, once (the reported 1/1 rejected bug)", () => {
    // The agent's rejected decision stays in the log, but the server's
    // overall status is approved.
    const state = approvalsState([
      approvalRequirement({
        state: "approved",
        approvers: [approver("approved")],
        decisionLog: [
          {
            requestId: "agent-request",
            outcome: "rejected",
            comment: null,
            summary: "Needs a human UI approval.",
            actorType: "agent",
            actor: null,
            definitionVersion: 1,
            superseded: false,
            reviewedGitHeads: [],
            createdAt: "2026-10-30T09:00:00Z",
          },
        ],
      }),
    ]);
    render(
      <ApprovalGates read={{ status: "ok", state }} workspaceName="Child" />,
    );
    const cell = screen.getByRole("img", {
      name: "Child approvals: 1 approved, 0 rejected, 0 pending",
    });
    expect(cell).toHaveAttribute("data-state", "ok");
    expect(Array.from(cell.children).map((line) => line.textContent)).toEqual([
      "1 approved",
      "0 rejected",
      "0 pending",
    ]);
    expect(cell).not.toHaveTextContent("/");
    // Words carry the status in the theme foreground.
    expect(cell.className).toContain("text-[var(--g5-text0)]");
  });

  it("counts requirements, not approvers", () => {
    const state = approvalsState([
      approvalRequirement({
        state: "pending",
        approvers: [
          approver("approved"),
          approver("rejected", { userId: "u-2" }),
        ],
      }),
      approvalRequirement({ state: "rejected" }),
      approvalRequirement({ state: "approved" }),
    ]);
    render(
      <ApprovalGates read={{ status: "ok", state }} workspaceName="Mixed" />,
    );
    expect(
      screen.getByRole("img", {
        name: "Mixed approvals: 1 approved, 1 rejected, 1 pending",
      }),
    ).toHaveAttribute("data-state", "rej");
  });

  it("shows nothing when none is required, a loading placeholder, and an unavailable read with retry", async () => {
    const { rerender } = render(
      <ApprovalGates
        read={{ status: "ok", state: approvalsState([]) }}
        workspaceName="Base"
      />,
    );
    expect(
      screen.getByTestId("home-workspace-approvals-none"),
    ).toBeEmptyDOMElement();
    rerender(
      <ApprovalGates read={{ status: "loading" }} workspaceName="Base" />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "Loading Base approvals…",
    );
    const retry = vi.fn();
    rerender(
      <ApprovalGates read={{ status: "error", retry }} workspaceName="Base" />,
    );
    expect(
      screen.getByTestId("home-workspace-approvals-error"),
    ).toHaveTextContent("Unavailable");
    await userEvent.click(
      screen.getByRole("button", { name: "Retry loading Base approvals" }),
    );
    expect(retry).toHaveBeenCalledOnce();
  });
});
