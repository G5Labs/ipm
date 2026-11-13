import { describe, expect, it, beforeEach, vi } from "vitest";
import { screen, fireEvent, waitFor, within } from "@testing-library/react";

import { ApiError } from "../../lib/api";
import { HttpResponse, delay, http } from "msw";
import {
  makeIssueSummary,
  serveExternalIssuesFeature,
  serveOrgExternalIssues,
} from "../../test/externalIssueFixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { MOCK_INSTANCE_INFO, createMockWorkspace } from "../../test/fixtures";
import { CloneDialog, forkDefaultAccountValue } from "./CloneDialog";
import type {
  CloneWorkspaceInput,
  OrgApprovalDefinitionDTO,
  ProviderAccountDTO,
  WorkspaceApprovalsStateDTO,
  WorkspaceSummary,
} from "../../lib/types";

// Mock the clone mutation hook so we can assert the request payload without a
// real network/query layer.
const useCloneWorkspaceMock = vi.fn();
// Org workspace list feeding the UI v3 target-workspace tree (top-bar D31).
const useWorkspacesMock = vi.fn();

vi.mock("../../hooks/useWorkspace", async () => {
  const actual = await vi.importActual<
    typeof import("../../hooks/useWorkspace")
  >("../../hooks/useWorkspace");
  return {
    ...actual,
    useCloneWorkspace: (...args: unknown[]) => useCloneWorkspaceMock(...args),
    useWorkspaces: (...args: unknown[]) => useWorkspacesMock(...args),
  };
});

// Mock the approvals-required summary query (top-bar D32).
const useWorkspaceApprovalsMock = vi.fn();

vi.mock("../../hooks/useWorkspaceApprovals", async () => {
  const actual = await vi.importActual<
    typeof import("../../hooks/useWorkspaceApprovals")
  >("../../hooks/useWorkspaceApprovals");
  return {
    ...actual,
    useWorkspaceApprovals: (...args: unknown[]) =>
      useWorkspaceApprovalsMock(...args),
  };
});

function approvalsStateWith(
  requiredInto: WorkspaceApprovalsStateDTO["requiredInto"],
): WorkspaceApprovalsStateDTO {
  return { requiredInto, attachmentHistory: [], outgoing: null };
}

function makeDefinition(
  overrides: Partial<OrgApprovalDefinitionDTO>,
): OrgApprovalDefinitionDTO {
  return {
    id: "def-1",
    latestVersion: 1,
    name: "Definition",
    description: null,
    humans: [],
    agentType: null,
    agentPrompt: null,
    invalidatesPrevious: false,
    createdBy: { userId: "u-creator", displayName: "Creator" },
    createdAt: "2026-08-01T00:00:00Z",
    firstCreatedBy: { userId: "u-creator", displayName: "Creator" },
    firstCreatedAt: "2026-08-01T00:00:00Z",
    ...overrides,
  };
}

beforeEach(() => {
  window.sessionStorage.clear();
  useWorkspacesMock.mockReset();
  useWorkspaceApprovalsMock.mockReset();
  useWorkspacesMock.mockReturnValue({ status: "ok", data: [] });
  useWorkspaceApprovalsMock.mockReturnValue({
    status: "ok",
    data: approvalsStateWith([]),
  });
});

describe("CloneDialog — recoverable fork requests", () => {
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
  });

  it("keeps the idempotency key for an unchanged retry and restores it after remount", async () => {
    const firstAttempt = vi.fn().mockRejectedValue(new Error("retryable"));
    mockCloneMutation(firstAttempt);
    const first = renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "feature/recoverable" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^fork$/i }));
    await waitFor(() => expect(firstAttempt).toHaveBeenCalledTimes(1));
    const firstKey = lastClonePayload(firstAttempt).idempotencyKey;
    first.unmount();

    const retry = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(retry);
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(/workspace name/i)).toHaveValue(
      "feature/recoverable",
    );
    fireEvent.click(screen.getByRole("button", { name: /^fork$/i }));
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(retry).idempotencyKey).toBe(firstKey);
  });

  it("uses a new idempotency key after the requested fork inputs change", async () => {
    const mutateAsync = vi
      .fn()
      .mockRejectedValueOnce(new Error("name unavailable"))
      .mockResolvedValueOnce({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "feature/taken" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^fork$/i }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));

    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "feature/available" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^fork$/i }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(2));
    expect(mutateAsync.mock.calls[1][0].idempotencyKey).not.toBe(
      mutateAsync.mock.calls[0][0].idempotencyKey,
    );
  });
});

function mockCloneMutation(mutateAsync: ReturnType<typeof vi.fn>) {
  useCloneWorkspaceMock.mockReturnValue({
    mutate: vi.fn(),
    mutateAsync,
    isLoading: false,
    error: null,
    data: undefined,
    reset: vi.fn(),
  });
}

/** A source workspace whose single repo is tracked in `remote_tracking` mode. */
function remoteTrackingSource(): WorkspaceSummary {
  return createMockWorkspace({
    id: "src-ws",
    name: "Source",
    branches: [
      {
        workspaceId: "workspace-1",

        repoId: "repo-1",
        repoName: "code",
        branchName: "feat/workspace-1",

        trackingMode: "remote_tracking",
        headCommit: "abc123def456",
        divergence: 0,
        behindBase: 0,
        hasDraft: false,
        dirtyFileCount: 0,
        remoteBranch: "main",
      },
    ],
  });
}

function lastClonePayload(
  mutateAsync: ReturnType<typeof vi.fn>,
): CloneWorkspaceInput {
  return mutateAsync.mock.calls[0][0] as CloneWorkspaceInput;
}

function clickClone() {
  const name = screen.getByLabelText(/workspace name/i);
  if (!(name as HTMLInputElement).value) {
    fireEvent.change(name, { target: { value: "feature/exact-name" } });
  }
  fireEvent.click(screen.getByTestId("clone-submit-button"));
}

/**
 * Every option except name/description lives inside the collapsed Advanced
 * section (top-bar D4), so tests that interact with any of them must expand
 * it first.
 */
function expandAdvanced() {
  fireEvent.click(screen.getByTestId("clone-advanced-toggle"));
}

describe("CloneDialog — Advanced section (top-bar D4)", () => {
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
  });

  it("collapses everything except the name by default, with no contents hint", () => {
    mockCloneMutation(
      vi.fn().mockResolvedValue({ workspace: createMockWorkspace() }),
    );
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    // The name renders at top level; there is no description field.
    expect(screen.getByLabelText(/workspace name/i)).toBeVisible();
    expect(screen.queryByLabelText(/description/i)).toBeNull();

    // The Advanced toggle is collapsed with no static contents hint and no
    // changed badge (everything at defaults).
    const toggle = screen.getByTestId("clone-advanced-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveTextContent(/^Advanced$/);
    expect(screen.queryByTestId("clone-advanced-badge")).toBeNull();

    // Every advanced option is hidden while collapsed.
    expect(screen.queryByText(/target workspace/i)).toBeNull();
    expect(screen.queryByTestId("clone-approvals-required")).toBeNull();
    expect(screen.queryByRole("checkbox", { name: /read-only/i })).toBeNull();
    expect(screen.queryByTestId("clone-fork-mode-select")).toBeNull();
    expect(screen.queryByText(/carry over chat sessions/i)).toBeNull();
    expect(screen.queryByText(/branch name overrides/i)).toBeNull();
    expect(screen.queryByTestId("clone-member-mapping")).toBeNull();

    // Expanding shows the per-repository branch names.
    fireEvent.click(toggle);
    expect(screen.getByTestId("clone-member-mapping")).toHaveTextContent(
      /repository branches/i,
    );
  });

  it("expanding reveals the advanced options in order", () => {
    mockCloneMutation(
      vi.fn().mockResolvedValue({ workspace: createMockWorkspace() }),
    );
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expandAdvanced();
    expect(screen.getByTestId("clone-advanced-toggle")).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByTestId("clone-target-workspace-toggle")).toBeVisible();
    // No governance-model selector: approvals bind to phases, not to a
    // per-workspace stage count (g5.workbench.gates).
    expect(
      screen.queryByRole("radiogroup", { name: /governance model/i }),
    ).toBeNull();
    expect(screen.getByTestId("clone-approvals-required")).toBeVisible();
    expect(screen.getByRole("checkbox", { name: /read-only/i })).toBeVisible();
    expect(screen.queryByTestId("clone-fork-mode-select")).toBeNull();
    expect(screen.queryByText(/carry over chat sessions/i)).toBeNull();
    expect(screen.queryByText(/branch name overrides/i)).toBeNull();
  });

  it("shows the changed-count badge exactly when advanced options differ from defaults", async () => {
    mockCloneMutation(
      vi.fn().mockResolvedValue({ workspace: createMockWorkspace() }),
    );
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.queryByTestId("clone-advanced-badge")).toBeNull();

    expandAdvanced();
    fireEvent.click(screen.getByRole("checkbox", { name: /read-only/i }));
    expect(screen.getByTestId("clone-advanced-badge")).toHaveTextContent(
      "1 changed",
    );

    // Reverting to defaults removes the badge.
    fireEvent.click(screen.getByRole("checkbox", { name: /read-only/i }));
    await waitFor(() =>
      expect(screen.queryByTestId("clone-advanced-badge")).toBeNull(),
    );
  });
});

describe("CloneDialog — read-only toggle", () => {
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
  });

  it("renders the read-only checkbox defaulting to off and omits readOnly in the payload", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={createMockWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expandAdvanced();
    const toggle = screen.getByRole("checkbox", { name: /read-only/i });
    expect(toggle).not.toBeChecked();

    clickClone();

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).readOnly).toBeUndefined();
  });

  it("sends readOnly=true when the checkbox is checked", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={createMockWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expandAdvanced();
    fireEvent.click(screen.getByRole("checkbox", { name: /read-only/i }));
    clickClone();

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).readOnly).toBe(true);
  });

  it("defaults the toggle off even when the source workspace is read-only (clone does not inherit)", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={createMockWorkspace({ readOnly: true })}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expandAdvanced();
    expect(
      screen.getByRole("checkbox", { name: /read-only/i }),
    ).not.toBeChecked();

    clickClone();

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).readOnly).toBeUndefined();
  });
});

describe("CloneDialog — target workspace (UI v3, top-bar D31)", () => {
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
  });

  /** Base → source (the fork parent) + a sibling, for the lineage tree. */
  function orgWorkspaces(): WorkspaceSummary[] {
    return [
      createMockWorkspace({ id: "base-ws", name: "Base", clonedFromId: null }),
      createMockWorkspace({
        id: "src-ws",
        name: "Source",
        clonedFromId: "base-ws",
      }),
      createMockWorkspace({
        id: "sibling-ws",
        name: "Sibling",
        clonedFromId: "base-ws",
      }),
    ];
  }

  function sourceWorkspace(): WorkspaceSummary {
    return createMockWorkspace({
      id: "src-ws",
      name: "Source",
      clonedFromId: "base-ws",
    });
  }

  function renderUiV3Dialog(mutateAsync: ReturnType<typeof vi.fn>) {
    mockCloneMutation(mutateAsync);
    useWorkspacesMock.mockReturnValue({ status: "ok", data: orgWorkspaces() });
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={sourceWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expandAdvanced();
  }


  it("offers no governance model and never sends a stage count (g5.workbench.gates)", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    renderUiV3Dialog(mutateAsync);

    expect(
      screen.queryByRole("radiogroup", { name: /governance model/i }),
    ).toBeNull();
    expect(screen.queryByTestId("clone-governance-one-stage")).toBeNull();

    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync)).not.toHaveProperty("stages");
    // Default target = parent, expressed by omission.
    expect(
      lastClonePayload(mutateAsync).mergeTargetWorkspaceId,
    ).toBeUndefined();
  });

  it("shows the parent as the default target and sends an explicit non-default pick", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    renderUiV3Dialog(mutateAsync);

    const toggle = screen.getByTestId("clone-target-workspace-toggle");
    expect(toggle).toHaveTextContent("Source (parent)");

    // Open the tree: Base is a root; Source and Sibling are its children.
    fireEvent.click(toggle);
    expect(
      await screen.findByTestId("clone-target-workspace-option-base-ws"),
    ).toBeVisible();
    expect(
      screen.getByTestId("clone-target-workspace-option-sibling-ws"),
    ).toBeVisible();

    fireEvent.click(
      screen.getByTestId("clone-target-workspace-option-sibling-ws"),
    );
    await waitFor(() => expect(toggle).toHaveTextContent("Sibling"));

    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).mergeTargetWorkspaceId).toBe(
      "sibling-ws",
    );
  });

  it("re-picking the parent returns to the default (field omitted)", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    renderUiV3Dialog(mutateAsync);

    const toggle = screen.getByTestId("clone-target-workspace-toggle");
    fireEvent.click(toggle);
    fireEvent.click(
      await screen.findByTestId("clone-target-workspace-option-sibling-ws"),
    );
    await waitFor(() => expect(toggle).toHaveTextContent("Sibling"));
    fireEvent.click(toggle);
    fireEvent.click(
      await screen.findByTestId("clone-target-workspace-option-src-ws"),
    );
    await waitFor(() => expect(toggle).toHaveTextContent("Source (parent)"));

    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(
      lastClonePayload(mutateAsync).mergeTargetWorkspaceId,
    ).toBeUndefined();
  });

  it("filters the target tree by the search input and selects a filtered row", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    renderUiV3Dialog(mutateAsync);

    fireEvent.click(screen.getByTestId("clone-target-workspace-toggle"));
    const search = await screen.findByTestId("clone-target-workspace-search");

    // Typing filters by case-insensitive substring on the name (D31).
    fireEvent.change(search, { target: { value: "sib" } });
    expect(
      screen.getByTestId("clone-target-workspace-option-sibling-ws"),
    ).toBeVisible();
    expect(
      screen.queryByTestId("clone-target-workspace-option-base-ws"),
    ).toBeNull();
    expect(
      screen.queryByTestId("clone-target-workspace-option-src-ws"),
    ).toBeNull();

    // A non-matching query shows the explicit empty note, not a bare panel.
    fireEvent.change(search, { target: { value: "zzz" } });
    expect(screen.getByText(/no workspaces match/i)).toBeVisible();

    // Selecting a filtered row works end to end.
    fireEvent.change(search, { target: { value: "sib" } });
    fireEvent.click(
      screen.getByTestId("clone-target-workspace-option-sibling-ws"),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("clone-target-workspace-toggle"),
      ).toHaveTextContent("Sibling"),
    );

    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).mergeTargetWorkspaceId).toBe(
      "sibling-ws",
    );
  });

  it("renders cycle-only lineage components flattened at root depth instead of dropping them", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    // A ↔ B form a lineage cycle (no root) beside a normal root; every live
    // workspace must still be selectable (top-bar D31).
    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [
        createMockWorkspace({
          id: "base-ws",
          name: "Base",
          clonedFromId: null,
        }),
        createMockWorkspace({
          id: "cycle-a",
          name: "Cycle A",
          clonedFromId: "cycle-b",
        }),
        createMockWorkspace({
          id: "cycle-b",
          name: "Cycle B",
          clonedFromId: "cycle-a",
        }),
      ],
    });
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={sourceWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expandAdvanced();
    fireEvent.click(screen.getByTestId("clone-target-workspace-toggle"));
    expect(
      await screen.findByTestId("clone-target-workspace-option-base-ws"),
    ).toBeVisible();
    expect(
      screen.getByTestId("clone-target-workspace-option-cycle-a"),
    ).toBeVisible();
    expect(
      screen.getByTestId("clone-target-workspace-option-cycle-b"),
    ).toBeVisible();

    // A cycle row must not just render — it must be selectable end to end.
    fireEvent.click(
      screen.getByTestId("clone-target-workspace-option-cycle-a"),
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("clone-target-workspace-toggle"),
      ).toHaveTextContent("Cycle A"),
    );
    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).mergeTargetWorkspaceId).toBe(
      "cycle-a",
    );
  });

  it("omits a previously selected target when the workspaces list stops resolving it", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    useWorkspacesMock.mockReturnValue({
      status: "ok",
      data: [
        createMockWorkspace({
          id: "base-ws",
          name: "Base",
          clonedFromId: null,
        }),
        createMockWorkspace({
          id: "sib-ws",
          name: "Sibling",
          clonedFromId: "base-ws",
        }),
      ],
    });
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={sourceWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expandAdvanced();
    const toggle = screen.getByTestId("clone-target-workspace-toggle");
    fireEvent.click(toggle);
    fireEvent.click(
      await screen.findByTestId("clone-target-workspace-option-sib-ws"),
    );
    await waitFor(() => expect(toggle).toHaveTextContent("Sibling"));

    // The list drops into an error state after the selection: the dialog now
    // displays the default parent, so the wire must omit the retained id
    // instead of persisting a target the UI no longer shows (top-bar D31).
    useWorkspacesMock.mockReturnValue({
      status: "error",
      error: new Error("boom"),
    });
    fireEvent.click(toggle);
    await waitFor(() =>
      expect(screen.getByText(/failed to load workspaces/i)).toBeVisible(),
    );

    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(
      lastClonePayload(mutateAsync).mergeTargetWorkspaceId,
    ).toBeUndefined();
  });

  it("a failed workspaces load never blocks the fork and falls back to the parent default", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    useWorkspacesMock.mockReturnValue({
      status: "error",
      error: new Error("boom"),
    });
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={sourceWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expandAdvanced();
    fireEvent.click(screen.getByTestId("clone-target-workspace-toggle"));
    expect(await screen.findByText(/failed to load workspaces/i)).toBeVisible();

    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "feature/exact-name" },
    });
    expect(screen.getByRole("button", { name: /^fork$/i })).toBeEnabled();
    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(
      lastClonePayload(mutateAsync).mergeTargetWorkspaceId,
    ).toBeUndefined();
    expect(lastClonePayload(mutateAsync)).not.toHaveProperty("stages");
  });
});

describe("CloneDialog — approvals-required summary (UI v3, top-bar D32)", () => {
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
  });

  function orgWorkspaces(): WorkspaceSummary[] {
    return [
      createMockWorkspace({ id: "base-ws", name: "Base", clonedFromId: null }),
      createMockWorkspace({
        id: "src-ws",
        name: "Source",
        clonedFromId: "base-ws",
      }),
      createMockWorkspace({
        id: "sibling-ws",
        name: "Sibling",
        clonedFromId: "base-ws",
      }),
    ];
  }

  function renderUiV3Dialog() {
    mockCloneMutation(
      vi.fn().mockResolvedValue({ workspace: createMockWorkspace() }),
    );
    useWorkspacesMock.mockReturnValue({ status: "ok", data: orgWorkspaces() });
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={createMockWorkspace({
          id: "src-ws",
          name: "Source",
          clonedFromId: "base-ws",
        })}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expandAdvanced();
  }

  it("lists each required definition with its name and approver avatars for the default parent target", () => {
    useWorkspaceApprovalsMock.mockReturnValue({
      status: "ok",
      data: approvalsStateWith([
        {
          definition: makeDefinition({
            id: "def-double",
            name: "Double signoff",
            humans: [
              {
                userId: "u-dvd",
                displayName: "Davide Pagano",
                email: "dvd@g5",
                avatarUrl: null,
                stillMember: true,
              },
              {
                userId: "u-kai",
                displayName: "Kaihui Zhang",
                email: "kai@g5",
                avatarUrl: null,
                stillMember: true,
              },
            ],
          }),
          attachedBy: { userId: "u-dvd", displayName: "Davide Pagano" },
          attachedAt: "2026-08-10T00:00:00Z",
        },
        {
          definition: makeDefinition({
            id: "def-sec",
            name: "Security review",
            agentType: "simple",
            agentPrompt: "Review for security issues",
          }),
          attachedBy: { userId: "u-dvd", displayName: "Davide Pagano" },
          attachedAt: "2026-08-10T00:00:00Z",
        },
      ]),
    });
    renderUiV3Dialog();

    // The summary queries the default target = the parent (source) workspace.
    expect(useWorkspaceApprovalsMock).toHaveBeenLastCalledWith("src-ws");

    const rows = screen.getAllByTestId("clone-approval-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Double signoff");
    // One initials avatar per human, titled with the display name.
    expect(screen.getByTitle("Davide Pagano")).toHaveTextContent("DP");
    expect(screen.getByTitle("Kaihui Zhang")).toHaveTextContent("KZ");
    // Agent-bearing definitions render the bot avatar.
    expect(rows[1]).toHaveTextContent("Security review");
    expect(screen.getByTitle("Agent check")).toBeInTheDocument();
  });

  it("re-queries the approvals for a newly selected target workspace", async () => {
    useWorkspaceApprovalsMock.mockReturnValue({
      status: "ok",
      data: approvalsStateWith([]),
    });
    renderUiV3Dialog();
    expect(useWorkspaceApprovalsMock).toHaveBeenLastCalledWith("src-ws");

    fireEvent.click(screen.getByTestId("clone-target-workspace-toggle"));
    fireEvent.click(
      await screen.findByTestId("clone-target-workspace-option-sibling-ws"),
    );

    await waitFor(() =>
      expect(useWorkspaceApprovalsMock).toHaveBeenLastCalledWith("sibling-ws"),
    );
  });

  it("shows the empty state when the target has no requirements", () => {
    useWorkspaceApprovalsMock.mockReturnValue({
      status: "ok",
      data: approvalsStateWith([]),
    });
    renderUiV3Dialog();
    expect(screen.getByText(/no approvals required/i)).toBeVisible();
  });

  it("shows a muted unavailable note on load failure and never blocks the fork", () => {
    useWorkspaceApprovalsMock.mockReturnValue({
      status: "error",
      error: new Error("boom"),
    });
    renderUiV3Dialog();

    // The failure must not masquerade as "no approvals required" (D32).
    expect(screen.getByText(/approvals could not be loaded/i)).toBeVisible();
    expect(screen.queryByText(/no approvals required/i)).toBeNull();
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "feature/exact-name" },
    });
    expect(screen.getByRole("button", { name: /^fork$/i })).toBeEnabled();
  });

  it("never shows approvals that could not be refreshed as the current list", () => {
    useWorkspaceApprovalsMock.mockReturnValue({
      status: "ok",
      data: approvalsStateWith([]),
      refreshError: new Error("The service is restarting"),
    });
    renderUiV3Dialog();

    // Kept from the last load, but not current: no "no approvals" (D32).
    expect(screen.getByText(/approvals could not be loaded/i)).toBeVisible();
    expect(screen.queryByText(/no approvals required/i)).toBeNull();
  });

});

describe("CloneDialog — approvals grouped by phase (g5.workbench.gates)", () => {
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
  });

  /** IE-only (default), BE-only, inert (no phase), BE+IE. */
  function requiredIntoFixture(): WorkspaceApprovalsStateDTO["requiredInto"] {
    const attached = {
      attachedBy: { userId: "u-dvd", displayName: "Davide Pagano" },
      attachedAt: "2026-08-10T00:00:00Z",
    };
    return [
      {
        definition: makeDefinition({ id: "def-ie", name: "Merge sign-off" }),
        ...attached,
      },
      {
        definition: makeDefinition({
          id: "def-be",
          name: "Business sign-off",
          phases: ["be"],
        }),
        ...attached,
      },
      {
        definition: makeDefinition({
          id: "def-inert",
          name: "Inert",
          phases: [],
        }),
        ...attached,
      },
      {
        definition: makeDefinition({
          id: "def-both",
          name: "Both phases",
          phases: ["be", "ie"],
        }),
        ...attached,
      },
    ];
  }

  function renderGroupedDialog() {
    mockCloneMutation(
      vi.fn().mockResolvedValue({ workspace: createMockWorkspace() }),
    );
    useWorkspacesMock.mockReturnValue({ status: "ok", data: [] });
    useWorkspaceApprovalsMock.mockReturnValue({
      status: "ok",
      data: approvalsStateWith(requiredIntoFixture()),
    });
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={createMockWorkspace({ id: "src-ws", name: "Source" })}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expandAdvanced();
  }

  it("groups the target's requirements by phase in workbench order, chips each row's phases and omits inert definitions", () => {
    renderGroupedDialog();
    const panel = screen.getByTestId("clone-approvals-required");
    const groups = within(panel).getAllByTestId("clone-approval-group");
    expect(groups).toHaveLength(2);
    expect(groups[0]).toHaveTextContent("▲ Business Engineering");
    expect(groups[1]).toHaveTextContent("▲ Integration Engineering");
    expect(within(panel).queryByText("Inert")).toBeNull();

    expect(within(groups[0]).getByText("Business sign-off")).toBeVisible();
    expect(within(groups[0]).getByText("Both phases")).toBeVisible();
    expect(within(groups[0]).queryByText("Merge sign-off")).toBeNull();

    expect(within(groups[1]).getByText("Merge sign-off")).toBeVisible();
    expect(within(groups[1]).getByText("Both phases")).toBeVisible();
    expect(within(groups[1]).queryByText("Business sign-off")).toBeNull();

    // A two-phase definition is chipped with both phases wherever it lists.
    expect(
      within(panel).getAllByTestId("clone-approval-phase-chip-be"),
    ).toHaveLength(3);
    expect(
      within(panel).getAllByTestId("clone-approval-phase-chip-ie"),
    ).toHaveLength(3);
  });
});

/** A launch the resolver confirms, as the chat composer receives it. */
const RESOLVED_LAUNCH = {
  schemaVersion: 1,
  resolutionId: "alr_fork",
  configRevision: "cfg-test",
  catalogVersion: "agent-launch-catalog-v1",
  provider: { value: "claude_code", provenance: "explicit" },
  providerRoute: "anthropic_direct",
  model: { value: "anthropic/claude-opus-4.8", provenance: "explicit" },
  upstreamModel: "anthropic/claude-opus-4.8",
  permissionMode: { value: "bypassPermissions", provenance: "explicit" },
  effort: { value: "high", provenance: "explicit" },
  authMode: "anthropic_api_key",
  egressMode: "direct_provider",
  credentialReady: true,
  requiresInteractiveAuth: false,
} as const;

describe("CloneDialog — link an existing issue", () => {
  let resolvedPreferences: Record<string, unknown>[] = [];
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
    resolvedPreferences = [];
    // "Start work" resolves its agent launch against the source workspace.
    server.use(
      http.get("/api/v1/workspaces/:workspaceId/config", () =>
        HttpResponse.json({
          data: {
            scope: "workspace",
            settings: {},
            secretKeys: [],
            effectiveSettings: {},
          },
        }),
      ),
      http.post(
        "/api/v1/workspaces/:workspaceId/config/agent-launch/resolve",
        async ({ request }) => {
          const { preferences } = (await request.json()) as {
            preferences: Record<string, unknown>;
          };
          resolvedPreferences.push(preferences);
          return HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } });
        },
      ),
    );
  });

  function renderDialog(
    props: Partial<React.ComponentProps<typeof CloneDialog>> = {},
  ) {
    return renderApp(
      <CloneDialog
        orgId="org-g5labs"
        source={createMockWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
        {...props}
      />,
    );
  }

  it("hides the issue field and sends no issue while the feature is off", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    mockCloneMutation(mutateAsync);
    renderDialog({ initialIssueRef: "acme/api#12" });

    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId("clone-issue-field")).not.toBeInTheDocument();
    expect(lastClonePayload(mutateAsync).issue).toBeUndefined();
  });

  it("sends the entered issue and role, and a changed issue is a new request", async () => {
    serveExternalIssuesFeature();
    const mutateAsync = vi
      .fn()
      .mockRejectedValueOnce(new Error("retryable"))
      .mockResolvedValueOnce({
        workspace: createMockWorkspace(),
        issueLink: null,
      });
    mockCloneMutation(mutateAsync);
    renderDialog();

    const ref = await screen.findByTestId("clone-issue-ref");
    expect(screen.getByTestId("clone-issue-role")).toBeDisabled();
    fireEvent.change(ref, { target: { value: "  acme/api#12 " } });
    fireEvent.change(screen.getByTestId("clone-issue-role"), {
      target: { value: "related" },
    });
    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    const first = mutateAsync.mock.calls[0][0] as CloneWorkspaceInput;
    expect(first.issue).toEqual({ ref: "acme/api#12", role: "related" });

    fireEvent.change(ref, { target: { value: "acme/api#13" } });
    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(2));
    const second = mutateAsync.mock.calls[1][0] as CloneWorkspaceInput;
    expect(second.issue).toEqual({ ref: "acme/api#13", role: "related" });
    expect(second.idempotencyKey).not.toBe(first.idempotencyKey);
  });

  it("starts over without the issue after the organization turned external issues off", async () => {
    serveExternalIssuesFeature();
    const refused = vi.fn().mockRejectedValue(
      new ApiError({
        code: "CONFLICT",
        message: "external issues are not enabled for this organization",
        status: 409,
        details: { code: "external_issues_disabled" },
      }),
    );
    mockCloneMutation(refused);
    const first = renderDialog();
    fireEvent.change(await screen.findByTestId("clone-issue-ref"), {
      target: { value: "acme/api#12" },
    });
    clickClone();
    await waitFor(() => expect(refused).toHaveBeenCalledTimes(1));
    const refusedRequest = lastClonePayload(refused);
    expect(refusedRequest.issue).toEqual({
      ref: "acme/api#12",
      role: "primary",
    });
    first.unmount();

    // Reopened with the switch off: the issue field is gone, and the refused
    // request (never admitted) is not replayed under its key.
    serveOrgExternalIssues(false);
    const retry = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    mockCloneMutation(retry);
    renderDialog();
    await waitFor(() =>
      expect(screen.queryByTestId("clone-issue-ref")).not.toBeInTheDocument(),
    );
    clickClone();
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
    const retried = lastClonePayload(retry);
    expect(retried.issue).toBeUndefined();
    expect(retried.idempotencyKey).not.toBe(refusedRequest.idempotencyKey);
  });

  it("preselects an issue reference passed by the caller", async () => {
    serveExternalIssuesFeature();
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    mockCloneMutation(mutateAsync);
    renderDialog({ initialIssueRef: "acme/api#12" });

    expect(await screen.findByTestId("clone-issue-ref")).toHaveValue(
      "acme/api#12",
    );
    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).issue).toEqual({
      ref: "acme/api#12",
      role: "primary",
    });
  });

  it("explains a primary claim another workspace holds", async () => {
    serveExternalIssuesFeature();
    useCloneWorkspaceMock.mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isLoading: false,
      error: new ApiError({
        code: "CONFLICT",
        message: "the issue already has a primary workspace",
        status: 409,
        details: {
          code: "issue_has_primary",
          ownerWorkspace: { visible: true, workspaceId: "ws-9", name: "fix/x" },
        },
      }),
      data: undefined,
      reset: vi.fn(),
    });
    renderDialog();

    expect(await screen.findByTestId("clone-error")).toHaveTextContent(
      "This issue is already the primary issue of fix/x. Choose Related",
    );
  });

  it("explains a typed issue error", async () => {
    serveExternalIssuesFeature();
    useCloneWorkspaceMock.mockReturnValue({
      mutate: vi.fn(),
      mutateAsync: vi.fn(),
      isLoading: false,
      error: new ApiError({
        code: "VALIDATION_ERROR",
        message: "other/repo is not a configured issue domain",
        status: 422,
        details: { issueError: "outside_domain" },
      }),
      data: undefined,
      reset: vi.fn(),
    });
    renderDialog();

    const error = await screen.findByTestId("clone-error");
    expect(error).toHaveTextContent(
      "other/repo is not a configured issue domain",
    );
  });

  it("explains a primary claim lost while the fork ran before opening the workspace", async () => {
    serveExternalIssuesFeature();
    const created = createMockWorkspace({ name: "fix/issue-12" });
    const mutateAsync = vi.fn().mockResolvedValue({
      workspace: created,
      issueLink: {
        linkId: "link-1",
        displayKey: "acme/api#12",
        canonicalUrl: "https://github.com/acme/api/issues/12",
        requestedRole: "primary",
        role: "related",
      },
    });
    mockCloneMutation(mutateAsync);
    const onCloned = vi.fn();
    const onClose = vi.fn();
    renderDialog({ onCloned, onClose, initialIssueRef: "acme/api#12" });

    await screen.findByTestId("clone-issue-ref");
    clickClone();
    const notice = await screen.findByTestId("clone-issue-linked-as-related");
    expect(screen.getByTestId("clone-ready-notice")).toHaveTextContent(
      "Workspace fix/issue-12 is ready.",
    );
    expect(notice).toHaveTextContent("acme/api#12");
    expect(notice).toHaveTextContent("linked as related");
    expect(onCloned).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("clone-open-workspace-button"));
    expect(onClose).toHaveBeenCalled();
    expect(onCloned).toHaveBeenCalledWith(created, {
      chatSessionId: null,
      draft: null,
    });
  });

  function serveResolve(titles: Record<string, [number, string]>) {
    const requested: string[] = [];
    server.use(
      http.get("/api/v1/orgs/org-g5labs/issues/resolve", ({ request }) => {
        const ref = new URL(request.url).searchParams.get("ref") ?? "";
        requested.push(ref);
        const match = titles[ref];
        if (!match) {
          return HttpResponse.json(
            {
              error: {
                code: "VALIDATION_ERROR",
                message: "other/repo is not a configured issue domain",
                details: { issueError: "outside_domain" },
                requestId: "",
              },
            },
            { status: 422 },
          );
        }
        const [number, title] = match;
        return HttpResponse.json({
          data: makeIssueSummary({
            displayKey: `acme/api#${number}`,
            title,
            locator: { repositoryId: "11", number },
          }),
        });
      }),
    );
    return requested;
  }

  it("suggests a workspace name from the issue until the user edits the name", async () => {
    serveExternalIssuesFeature();
    serveResolve({
      "acme/api#12": [12, "Crash on save"],
      "acme/api#13": [13, "Slow search"],
    });
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    mockCloneMutation(mutateAsync);
    renderDialog();
    const nameField = screen.getByLabelText(/workspace name/i);

    fireEvent.change(await screen.findByTestId("clone-issue-ref"), {
      target: { value: "acme/api#12" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("clone-issue-lookup")).toHaveTextContent(
        "acme/api#12 · Crash on save",
      ),
    );
    expect(nameField).toHaveValue("issue-12-crash-on-save");

    // Another issue replaces a suggestion the user has not touched.
    fireEvent.change(screen.getByTestId("clone-issue-ref"), {
      target: { value: "acme/api#13" },
    });
    await waitFor(() => expect(nameField).toHaveValue("issue-13-slow-search"));

    // Once edited, the user's name stays.
    fireEvent.change(nameField, { target: { value: "fix/search" } });
    fireEvent.change(screen.getByTestId("clone-issue-ref"), {
      target: { value: "acme/api#12" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("clone-issue-lookup")).toHaveTextContent(
        "Crash on save",
      ),
    );
    expect(nameField).toHaveValue("fix/search");

    fireEvent.click(screen.getByRole("button", { name: /^fork$/i }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).name).toBe("fix/search");
    // A name the user typed is exact.
    expect(lastClonePayload(mutateAsync).uniquifyName).toBeUndefined();
  });

  it("submits the suggested name for a preselected issue", async () => {
    serveExternalIssuesFeature();
    serveResolve({ "acme/api#12": [12, "Crash on save"] });
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    mockCloneMutation(mutateAsync);
    renderDialog({ initialIssueRef: "acme/api#12" });
    const nameField = screen.getByLabelText(/workspace name/i);

    await waitFor(() =>
      expect(nameField).toHaveValue("issue-12-crash-on-save"),
    );
    // Starting from an issue starts work, with a message naming the issue.
    expect(screen.getByTestId("clone-start-work-checkbox")).toBeChecked();
    expect(screen.getByTestId("clone-start-message")).toHaveValue(
      "Work on acme/api#12: Crash on save\n\nRead the issue with `g5d issue show` before you start.",
    );
    const startButton = screen.getByRole("button", {
      name: /^fork & start work$/i,
    });
    await waitFor(() => expect(startButton).toBeEnabled());
    fireEvent.click(startButton);
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).name).toBe("issue-12-crash-on-save");
    // A kept suggestion may take a free numbered alternative on the server.
    expect(lastClonePayload(mutateAsync).uniquifyName).toBe(true);
    const startChat = lastClonePayload(mutateAsync).startChat;
    expect(startChat?.content).toBe(
      "Work on acme/api#12: Crash on save\n\nRead the issue with `g5d issue show` before you start.",
    );
    // The confirmed launch choices travel with the message.
    expect(startChat?.launchPreferences).toEqual(resolvedPreferences.at(-1));
    expect(startChat?.launchPreferences).toMatchObject({
      permissionMode: { source: expect.any(String) },
    });
  });

  it("opens the new workspace on the chat the fork started", async () => {
    const created = createMockWorkspace({ name: "fix/dark-mode" });
    mockCloneMutation(
      vi.fn().mockResolvedValue({
        workspace: created,
        issueLink: null,
        startedChat: {
          sessionId: "session-7",
          error: null,
          workspaceStartError: null,
        },
      }),
    );
    const onCloned = vi.fn();
    renderDialog({ onCloned });

    fireEvent.click(screen.getByTestId("clone-start-work-checkbox"));
    fireEvent.change(screen.getByTestId("clone-start-message"), {
      target: { value: "Add a dark mode toggle." },
    });
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "fix/dark-mode" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    clickClone();
    await waitFor(() =>
      expect(onCloned).toHaveBeenCalledWith(created, {
        chatSessionId: "session-7",
        draft: null,
      }),
    );
  });

  // Scenario 2 of the fork landing (ui g5.workspace.ui.fork-landing): an
  // issue without Start work lands on a composer pre-typed with the issue's
  // suggested first message, unsent.
  it("lands a fork from an issue without Start work on the issue's suggested message", async () => {
    serveExternalIssuesFeature();
    serveResolve({ "acme/api#12": [12, "Crash on save"] });
    const created = createMockWorkspace({ name: "issue-12-crash-on-save" });
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: created, issueLink: null });
    mockCloneMutation(mutateAsync);
    const onCloned = vi.fn();
    renderDialog({ onCloned, initialIssueRef: "acme/api#12" });

    await waitFor(() =>
      expect(screen.getByLabelText(/workspace name/i)).toHaveValue(
        "issue-12-crash-on-save",
      ),
    );
    fireEvent.click(screen.getByTestId("clone-start-work-checkbox"));
    expect(screen.queryByTestId("clone-start-message")).not.toBeInTheDocument();
    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(lastClonePayload(mutateAsync).startChat).toBeUndefined();
    await waitFor(() =>
      expect(onCloned).toHaveBeenCalledWith(created, {
        chatSessionId: null,
        draft:
          "Work on acme/api#12: Crash on save\n\nRead the issue with `g5d issue show` before you start.",
      }),
    );
  });

  it("starts work only when asked, with the message the user wrote", async () => {
    const mutateAsync = vi
      .fn()
      .mockRejectedValueOnce(new Error("retryable"))
      .mockResolvedValueOnce({
        workspace: createMockWorkspace(),
        issueLink: null,
      });
    mockCloneMutation(mutateAsync);
    const onCloned = vi.fn();
    renderDialog({ onCloned });

    const checkbox = screen.getByTestId("clone-start-work-checkbox");
    expect(checkbox).not.toBeChecked();
    expect(screen.queryByTestId("clone-start-message")).not.toBeInTheDocument();
    clickClone();
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    const first = mutateAsync.mock.calls[0][0] as CloneWorkspaceInput;
    expect(first.startChat).toBeUndefined();
    // The first attempt failed: nothing landed yet.
    expect(onCloned).not.toHaveBeenCalled();

    fireEvent.click(checkbox);
    fireEvent.change(screen.getByTestId("clone-start-message"), {
      target: { value: "  Add a dark mode toggle. " },
    });
    const startButton = screen.getByRole("button", {
      name: /^fork & start work$/i,
    });
    await waitFor(() => expect(startButton).toBeEnabled());
    fireEvent.click(startButton);
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(2));
    const second = mutateAsync.mock.calls[1][0] as CloneWorkspaceInput;
    expect(second.startChat?.content).toBe("Add a dark mode toggle.");
    expect(second.startChat?.launchPreferences).toEqual(
      resolvedPreferences.at(-1),
    );
    // The message is part of the request: a new key.
    expect(second.idempotencyKey).not.toBe(first.idempotencyKey);
  });

  it("explains a chat that did not start before opening the workspace", async () => {
    const created = createMockWorkspace({ name: "fix/dark-mode" });
    mockCloneMutation(
      vi.fn().mockResolvedValue({
        workspace: created,
        issueLink: null,
        startedChat: {
          sessionId: null,
          error: "no agent provider is configured",
          workspaceStartError: null,
        },
      }),
    );
    const onCloned = vi.fn();
    renderDialog({ onCloned });

    fireEvent.click(screen.getByTestId("clone-start-work-checkbox"));
    fireEvent.change(screen.getByTestId("clone-start-message"), {
      target: { value: "Add a dark mode toggle." },
    });
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "fix/dark-mode" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    clickClone();
    const notice = await screen.findByTestId("clone-ready-notice");
    expect(notice).toHaveTextContent("Workspace fix/dark-mode is ready.");
    expect(screen.getByTestId("clone-chat-not-started")).toHaveTextContent(
      "Its chat did not start: no agent provider is configured",
    );
    expect(onCloned).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("clone-open-workspace-button"));
    // The unsent message lands in the fresh composer instead.
    expect(onCloned).toHaveBeenCalledWith(created, {
      chatSessionId: null,
      draft: "Add a dark mode toggle.",
    });
  });

  it("does not replace a name restored from a retry draft", async () => {
    serveExternalIssuesFeature();
    serveResolve({ "acme/api#12": [12, "Crash on save"] });
    const firstAttempt = vi.fn().mockRejectedValue(new Error("retryable"));
    mockCloneMutation(firstAttempt);
    const first = renderDialog();
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "fix/mine" },
    });
    fireEvent.change(await screen.findByTestId("clone-issue-ref"), {
      target: { value: "acme/api#12" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^fork$/i }));
    await waitFor(() => expect(firstAttempt).toHaveBeenCalledTimes(1));
    first.unmount();

    mockCloneMutation(vi.fn());
    renderDialog();
    await waitFor(() =>
      expect(screen.getByTestId("clone-issue-lookup")).toHaveTextContent(
        "Crash on save",
      ),
    );
    expect(screen.getByLabelText(/workspace name/i)).toHaveValue("fix/mine");
  });

  it("reports a reference that cannot be resolved and suggests no name", async () => {
    serveExternalIssuesFeature();
    serveResolve({});
    mockCloneMutation(vi.fn());
    renderDialog();

    fireEvent.change(await screen.findByTestId("clone-issue-ref"), {
      target: { value: "other/repo#1" },
    });
    expect(
      await screen.findByTestId("clone-issue-lookup-error"),
    ).toHaveTextContent(/not in a configured issue domain|issue domain/i);
    expect(screen.getByLabelText(/workspace name/i)).toHaveValue("");
  });
});

describe("CloneDialog — source tracking advisories", () => {
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
  });

  it("lists held, paused and behind repositories without blocking the fork", async () => {
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace() });
    mockCloneMutation(mutateAsync);
    server.use(
      http.get("/api/v1/workspaces/:id/fork_advisories", () =>
        HttpResponse.json({
          data: [
            {
              repoId: "repo-code",
              repoName: "code",
              kind: "import_held",
              pullRequest: {
                pullRequestId: "pr-1",
                title: "Cross-repo change",
              },
            },
            {
              repoId: "repo-docs",
              repoName: "docs",
              kind: "paused",
              pauseReason: "auth_required",
            },
            {
              repoId: "repo-code",
              repoName: "code",
              kind: "behind",
              providerTip: "abc123",
            },
          ],
        }),
      ),
    );
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    const notice = await screen.findByTestId("fork-source-advisories");
    const items = within(notice).getAllByTestId("fork-source-advisory");
    expect(items.map((item) => item.getAttribute("data-kind"))).toEqual([
      "import_held",
      "paused",
      "behind",
    ]);
    expect(notice).toHaveTextContent(/Cross-repo change/);
    expect(notice).toHaveTextContent(/OAuth credential missing/);
    // The advisories never block: a named fork can still be submitted.
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "fix/advised" },
    });
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
  });

  it("shows nothing when the source has no advisories", async () => {
    mockCloneMutation(vi.fn());
    let requested = false;
    server.use(
      http.get("/api/v1/workspaces/:id/fork_advisories", () => {
        requested = true;
        return HttpResponse.json({ data: [] });
      }),
    );
    renderApp(
      <CloneDialog
        orgId="org-1"
        source={remoteTrackingSource()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    await waitFor(() => expect(requested).toBe(true));
    expect(screen.queryByTestId("fork-source-advisories")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Account for the first chat (#987; g5.chat.configuration): the fork's
// "Start work" row offers the Account next to Agent, Model, Effort and Mode.
// ---------------------------------------------------------------------------

function makeAccount(
  overrides: Partial<ProviderAccountDTO> & Pick<ProviderAccountDTO, "id">,
): ProviderAccountDTO {
  return {
    orgId: "org-g5labs",
    name: overrides.id,
    kind: "claude_subscription",
    family: "claude",
    scope: "org",
    workspaceId: null,
    userId: null,
    baseUrl: null,
    hasSecret: true,
    secretHint: "sk-ant-oat01",
    createdBy: "user-alex",
    createdAt: "2026-10-30T00:00:00Z",
    updatedAt: "2026-10-30T00:00:00Z",
    usage: null,
    ...overrides,
  };
}

const ORG_ACCOUNT = makeAccount({ id: "acct-org", name: "Team subscription" });
const USER_ACCOUNT = makeAccount({
  id: "acct-user",
  name: "My key",
  kind: "anthropic_api_key",
  scope: "user",
  userId: "user-alex",
});
// Belongs to the source workspace: the fork cannot see it.
const SOURCE_WORKSPACE_ACCOUNT = makeAccount({
  id: "acct-source",
  name: "Source only",
  scope: "workspace",
  workspaceId: "test-workspace",
});
const CODEX_ACCOUNT = makeAccount({
  id: "acct-codex",
  name: "Codex org",
  kind: "chatgpt_subscription",
  family: "codex",
});

function scopedConfig(
  scope: string,
  settings: Record<string, string>,
  secretKeys: string[] = scope === "org" ? ["ANTHROPIC_API_KEY"] : [],
) {
  return {
    data: { scope, settings, secretKeys, effectiveSettings: settings },
  };
}

describe("CloneDialog — account for the first chat (#987)", () => {
  let resolvedPreferences: Record<string, unknown>[] = [];

  function serveAccounts(
    assignments: {
      org?: Record<string, string>;
      workspace?: Record<string, string>;
      user?: Record<string, string>;
    },
    accounts: {
      delayMs?: number;
      fail?: boolean;
      /** No raw Claude key anywhere (chat g5.chat.account-defaults). */
      noRawKeys?: boolean;
    } = {},
  ) {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...MOCK_INSTANCE_INFO,
            features: [...MOCK_INSTANCE_INFO.features, "provider_accounts"],
          },
        }),
      ),
      http.get("/api/v1/orgs/:orgId/provider-accounts", async () => {
        if (accounts.delayMs) await delay(accounts.delayMs);
        if (accounts.fail) {
          return HttpResponse.json(
            { error: { code: "INTERNAL", message: "accounts unavailable" } },
            { status: 500 },
          );
        }
        return HttpResponse.json({
          data: [
            ORG_ACCOUNT,
            USER_ACCOUNT,
            SOURCE_WORKSPACE_ACCOUNT,
            CODEX_ACCOUNT,
          ],
        });
      }),
      http.get("/api/v1/orgs/:orgId/config", () =>
        HttpResponse.json(
          scopedConfig(
            "org",
            assignments.org ?? {},
            accounts.noRawKeys ? [] : undefined,
          ),
        ),
      ),
      http.get("/api/v1/workspaces/:workspaceId/config", () =>
        HttpResponse.json(
          scopedConfig("workspace", assignments.workspace ?? {}),
        ),
      ),
      http.get("/api/v1/users/me/config", () =>
        HttpResponse.json(scopedConfig("user", assignments.user ?? {})),
      ),
    );
  }

  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
    resolvedPreferences = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/config/agent-launch/resolve",
        async ({ request }) => {
          const { preferences } = (await request.json()) as {
            preferences: Record<string, unknown>;
          };
          resolvedPreferences.push(preferences);
          return HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } });
        },
      ),
    );
  });

  function renderStartWork(
    mutateAsync: ReturnType<typeof vi.fn> = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null }),
  ) {
    mockCloneMutation(mutateAsync);
    renderApp(
      <CloneDialog
        orgId="org-g5labs"
        source={createMockWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "feature/dark-mode" },
    });
    fireEvent.click(screen.getByTestId("clone-start-work-checkbox"));
    fireEvent.change(screen.getByTestId("clone-start-message"), {
      target: { value: "Add a dark mode toggle." },
    });
    return mutateAsync;
  }

  async function forkAndStart(mutateAsync: ReturnType<typeof vi.fn>) {
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    return lastClonePayload(mutateAsync);
  }

  it("shows no Account field and sends no account while provider accounts are off", async () => {
    const mutateAsync = renderStartWork();
    const payload = await forkAndStart(mutateAsync);
    expect(
      screen.queryByTestId("clone-start-account-select"),
    ).not.toBeInTheDocument();
    expect(payload.startChat?.launchPreferences).not.toHaveProperty("account");
  });

  it("offers the accounts the new workspace will see and starts the chat under the source's assignment", async () => {
    serveAccounts({ workspace: { claude_account_id: "acct-org" } });
    const mutateAsync = renderStartWork();

    const select = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("Team subscription"));
    const labels = Array.from(
      document.querySelectorAll("label > span:first-child"),
    ).map((node) => node.textContent);
    expect(labels).toEqual(["Agent", "Account", "Model", "Effort", "Mode"]);
    fireEvent.click(select);
    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual(
      expect.arrayContaining([
        expect.stringContaining("Runtime variables"),
        expect.stringContaining("Team subscription"),
        expect.stringContaining("My key"),
      ]),
    );
    expect(
      screen.queryByRole("option", { name: /Source only/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: /Codex org/ }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("option", { name: /Team subscription/ }));

    const payload = await forkAndStart(mutateAsync);
    expect(payload.startChat?.launchPreferences?.account).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: "acct-org" },
    });
    expect(resolvedPreferences.at(-1)?.account).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: "acct-org" },
    });
  });

  it("skips a source assignment the fork cannot use and carries the person's pick", async () => {
    serveAccounts({
      org: { claude_account_id: "acct-org" },
      workspace: { claude_account_id: "acct-source" },
    });
    const mutateAsync = renderStartWork();

    const select = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("Team subscription"));
    fireEvent.click(select);
    fireEvent.click(screen.getByRole("option", { name: /My key/ }));
    await waitFor(() => expect(select).toHaveTextContent("My key"));

    const payload = await forkAndStart(mutateAsync);
    expect(payload.startChat?.launchPreferences?.account).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: "acct-user" },
    });
    // A different account is a different request.
    expect(payload.idempotencyKey).toBeTruthy();
  });

  it("falls back to Runtime variables when no usable assignment exists", async () => {
    serveAccounts({ workspace: { claude_account_id: "acct-source" } });
    const mutateAsync = renderStartWork();
    const select = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("Runtime variables"));
    const payload = await forkAndStart(mutateAsync);
    expect(payload.startChat?.launchPreferences?.account).toEqual({
      source: "explicit",
      value: { kind: "runtime_config" },
    });
  });

  it("reads No account without raw keys and offers no unassigned choice once an account is the default", async () => {
    // chat g5.chat.account-defaults: "Runtime variables" is offered only
    // while the organization's or the person's scope carries a raw key.
    serveAccounts({}, { noRawKeys: true });
    renderStartWork();
    const select = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("No account"));
    fireEvent.click(select);
    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual(
      expect.arrayContaining([
        expect.stringContaining("No account"),
        expect.stringContaining("Team subscription"),
      ]),
    );
    fireEvent.click(screen.getByRole("option", { name: /Team subscription/ }));
    await waitFor(() => expect(select).toHaveTextContent("Team subscription"));
    fireEvent.click(select);
    expect(
      screen.queryByRole("option", { name: /No account|Runtime variables/ }),
    ).not.toBeInTheDocument();
  });

  it("starts Auto on the agent whose account the fork can use", async () => {
    // chat g5.chat.account-defaults: with no configured default agent, Auto
    // prefers an agent that can authenticate; only a Codex account exists.
    serveAccounts({ org: { codex_account_id: "acct-codex" } }, { noRawKeys: true });
    renderStartWork();
    const select = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("Codex org"));
    expect(screen.getByTestId("clone-start-agent-select")).toHaveTextContent(
      "Codex",
    );
  });
});

describe("CloneDialog — account data readiness and retries (#987)", () => {
  let resolvedPreferences: Record<string, unknown>[] = [];
  beforeEach(() => {
    useCloneWorkspaceMock.mockReset();
    window.sessionStorage.clear();
    resolvedPreferences = [];
    server.use(
      http.post(
        "/api/v1/workspaces/:workspaceId/config/agent-launch/resolve",
        async ({ request }) => {
          const { preferences } = (await request.json()) as {
            preferences: Record<string, unknown>;
          };
          resolvedPreferences.push(preferences);
          return HttpResponse.json({ data: { resolved: RESOLVED_LAUNCH } });
        },
      ),
    );
  });

  function serve(
    assignments: Record<string, string>,
    accounts: { delayMs?: number; fail?: boolean } = {},
  ) {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: {
            ...MOCK_INSTANCE_INFO,
            features: [...MOCK_INSTANCE_INFO.features, "provider_accounts"],
          },
        }),
      ),
      http.get("/api/v1/orgs/:orgId/provider-accounts", async () => {
        if (accounts.delayMs) await delay(accounts.delayMs);
        if (accounts.fail) {
          return HttpResponse.json(
            { error: { code: "INTERNAL", message: "accounts unavailable" } },
            { status: 500 },
          );
        }
        return HttpResponse.json({ data: [ORG_ACCOUNT, USER_ACCOUNT] });
      }),
      http.get("/api/v1/orgs/:orgId/config", () =>
        HttpResponse.json(scopedConfig("org", assignments)),
      ),
      http.get("/api/v1/workspaces/:workspaceId/config", () =>
        HttpResponse.json(scopedConfig("workspace", {})),
      ),
      http.get("/api/v1/users/me/config", () =>
        HttpResponse.json(scopedConfig("user", {})),
      ),
    );
  }

  function render(mutateAsync: ReturnType<typeof vi.fn>) {
    mockCloneMutation(mutateAsync);
    const view = renderApp(
      <CloneDialog
        orgId="org-g5labs"
        source={createMockWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    fireEvent.change(screen.getByLabelText(/workspace name/i), {
      target: { value: "feature/dark-mode" },
    });
    fireEvent.click(screen.getByTestId("clone-start-work-checkbox"));
    fireEvent.change(screen.getByTestId("clone-start-message"), {
      target: { value: "Add a dark mode toggle." },
    });
    return view;
  }

  it("waits for the accounts before the fork can start, then keeps the configured account", async () => {
    serve({ claude_account_id: "acct-org" }, { delayMs: 600 });
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    render(mutateAsync);

    const select = await screen.findByTestId("clone-start-account-select");
    expect(select).toHaveTextContent("Loading accounts…");
    // The launch resolves meanwhile, but the fork still waits for the data.
    await waitFor(() => expect(resolvedPreferences.length).toBeGreaterThan(0));
    expect(select).toHaveTextContent("Loading accounts…");
    expect(screen.getByTestId("clone-submit-button")).toBeDisabled();

    await waitFor(
      () => expect(select).toHaveTextContent("Team subscription"),
      { timeout: 3000 },
    );
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(
      lastClonePayload(mutateAsync).startChat?.launchPreferences?.account,
    ).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: "acct-org" },
    });
  });

  it("shows a failed account load and still lets the person fork with what is offered", async () => {
    serve({ claude_account_id: "acct-org" }, { fail: true });
    const mutateAsync = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    render(mutateAsync);

    expect(
      await screen.findByTestId("clone-start-accounts-error"),
    ).toHaveTextContent(/Accounts could not be loaded/);
    const select = screen.getByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("Runtime variables"));
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(
      lastClonePayload(mutateAsync).startChat?.launchPreferences?.account,
    ).toEqual({ source: "explicit", value: { kind: "runtime_config" } });
  });

  it("pins the default account an attempt used so a refreshed assignment cannot change the retry", async () => {
    serve({ claude_account_id: "acct-org" });
    const mutateAsync = vi
      .fn()
      .mockRejectedValueOnce(new Error("retryable"))
      .mockResolvedValueOnce({
        workspace: createMockWorkspace(),
        issueLink: null,
      });
    const view = render(mutateAsync);

    const select = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("Team subscription"));
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    const firstPayload = lastClonePayload(mutateAsync);

    // The organization's assignment moves to the person's account meanwhile.
    server.use(
      http.get("/api/v1/orgs/:orgId/config", () =>
        HttpResponse.json(
          scopedConfig("org", { claude_account_id: "acct-user" }),
        ),
      ),
    );
    await view.queryClient.invalidateQueries({ queryKey: ["scoped-config"] });
    await waitFor(() =>
      expect(
        view.queryClient.getQueryData(["scoped-config", "org", "org-g5labs"]),
      ).toEqual(
        expect.objectContaining({
          settings: { claude_account_id: "acct-user" },
        }),
      ),
    );
    expect(select).toHaveTextContent("Team subscription");

    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(2));
    const retried = mutateAsync.mock.calls[1][0] as CloneWorkspaceInput;
    expect(retried.idempotencyKey).toBe(firstPayload.idempotencyKey);
    expect(retried.startChat?.launchPreferences?.account).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: "acct-org" },
    });
  });

  it("restores the picked account for an unchanged retry and keeps the request identity", async () => {
    serve({ claude_account_id: "acct-org" });
    const firstAttempt = vi.fn().mockRejectedValue(new Error("retryable"));
    const first = render(firstAttempt);

    const select = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(select).toHaveTextContent("Team subscription"));
    fireEvent.click(select);
    fireEvent.click(screen.getByRole("option", { name: /My key/ }));
    await waitFor(() => expect(select).toHaveTextContent("My key"));
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(firstAttempt).toHaveBeenCalledTimes(1));
    const firstPayload = lastClonePayload(firstAttempt);
    expect(firstPayload.startChat?.launchPreferences?.account).toEqual({
      source: "explicit",
      value: { kind: "account", accountId: "acct-user" },
    });
    first.unmount();

    const retry = vi
      .fn()
      .mockResolvedValue({ workspace: createMockWorkspace(), issueLink: null });
    mockCloneMutation(retry);
    renderApp(
      <CloneDialog
        orgId="org-g5labs"
        source={createMockWorkspace()}
        onCloned={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    const restored = await screen.findByTestId("clone-start-account-select");
    await waitFor(() => expect(restored).toHaveTextContent("My key"));
    await waitFor(() =>
      expect(screen.getByTestId("clone-submit-button")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("clone-submit-button"));
    await waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
    const retried = lastClonePayload(retry);
    expect(retried.idempotencyKey).toBe(firstPayload.idempotencyKey);
    expect(retried.startChat?.launchPreferences?.account).toEqual(
      firstPayload.startChat?.launchPreferences?.account,
    );
  });
});

describe("forkDefaultAccountValue", () => {
  const visible = [ORG_ACCOUNT, USER_ACCOUNT];

  it("follows user over workspace over organization among usable accounts", () => {
    expect(
      forkDefaultAccountValue(
        { user: "acct-user", workspace: "acct-org", org: "" },
        visible,
      ),
    ).toBe("acct-user");
    expect(
      forkDefaultAccountValue(
        { user: "", workspace: "acct-org", org: "acct-user" },
        visible,
      ),
    ).toBe("acct-org");
  });

  it("skips assignments the new workspace cannot see", () => {
    expect(
      forkDefaultAccountValue(
        { user: "", workspace: "acct-source", org: "acct-org" },
        visible,
      ),
    ).toBe("acct-org");
    expect(
      forkDefaultAccountValue(
        { user: "", workspace: "acct-source", org: "" },
        visible,
      ),
    ).toBe("runtime_config");
  });
});
