/**
 * Vitest unit tests for MakeRemoteDialog + MakeRemoteMenu — Story 4 UX for the
 * gated Make-Remote feature (workspace_service D47).
 *
 * Scenarios:
 * 1. Validation — empty Git URL and/or empty remote branch keep submit blocked.
 * 2. Blast-radius confirmation gates submit — even with valid fields, submit is
 *    disabled until the org-admin acknowledgement checkbox is checked.
 * 3. Submit POSTs the promote endpoint WITH the `gitUrl` (and remote branch);
 *    a `bound` outcome closes the dialog.
 * 4. Outcome routing — a `merge_required` outcome starts the returned mergeRunId
 *    and, when a host handler is present, routes into the existing
 *    workspace-merge-workflow by invoking it with the run + resolution conversation.
 * 5. Malformed `merge_required` — a response missing `mergeRunId` keeps the
 *    dialog OPEN with an inline error and starts nothing.
 * 6. Visibility gate — MakeRemoteMenu's labelled Make Remote button is offered
 *    without the retired `make_remote` instance feature, to the owner of a
 *    writable workspace and on Base to its owner or an organization
 *    administrator, never to a plain member or on a review copy.
 * 7. Responsive rendering — long labels and structured backend diagnostics
 *    stay inside the modal and render as human-readable wrapped text.
 * 8. Reconciliation after a divergent bind — the reviewed provider history
 *    can be adopted (recommended for an initial-only history) beside Resolve
 *    with agent; Base offers adopting only; off Base adopting waits for the
 *    running VM (g5.workspace.remote.adopt-provider-history).
 * 9. Provider branch creation on Base — the
 *    `PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION` refusal shows a
 *    confirmation step; confirming resends the same branch and URL with
 *    `createProviderBranch: true`; Back keeps the draft and sends nothing.
 *
 * SOT:
 * - .semlift/specs/software_architecture/g5-api/services/workspace_service D47
 */

import { beforeEach, describe, it, expect, vi } from "vitest";
import { useState } from "react";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { renderApp } from "../../test/render";
import { createMockUser, createMockWorkspace } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { MOCK_INSTANCE_INFO } from "../../data/mock";
import { MakeRemoteDialog, MakeRemoteMenu } from "./MakeRemoteDialog";
import {
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import type { AgentLaunchSelectionV1 } from "../../lib/types";

const WORKSPACE_ID = "ws-make-remote";
const REPO_ID = "repo-local";

const PROMOTE_URL = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/remote_tracking`;
const PREVIEW_URL = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/provider_history_preview`;
const ACCEPT_URL = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/accept_provider_history`;
const PROVIDER_TIP = "cafebabe00000000000000000000000000000000";
const LOCAL_HEAD = "deadbeef00000000000000000000000000000000";

function previewBody(overrides: Record<string, unknown> = {}) {
  return {
    workspaceId: WORKSPACE_ID,
    repoId: REPO_ID,
    remoteBranch: "main",
    head: LOCAL_HEAD,
    providerTip: PROVIDER_TIP,
    discardedCommits: 1,
    initialOnly: false,
    ...overrides,
  };
}

function gateFor(over: Partial<WorkspaceLiveGate> = {}): WorkspaceLiveGate {
  return {
    kind: "live",
    canWrite: true,
    live: true,
    canStart: false,
    reviewCopy: false,
    reviewIdentityKnown: true,
    owner: true,
    orgAdmin: false,
    workspaceNoun: "the workspace",
    ...over,
  };
}

const BASE_ADMIN_GATE = gateFor({
  kind: "base",
  canWrite: false,
  live: false,
  owner: false,
  orgAdmin: true,
  workspaceNoun: "Base",
});

beforeEach(() => {
  server.use(
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
      HttpResponse.json({
        data: { ...createMockWorkspace({ id: WORKSPACE_ID }), orgId: "org-1" },
      }),
    ),
    http.post(
      `/api/v1/workspaces/${WORKSPACE_ID}/config/agent-launch/resolve`,
      () =>
        HttpResponse.json({
          data: {
            resolved: {
              schemaVersion: 1,
              resolutionId: "alr_make_remote",
              configRevision: "cfg-test",
              provider: { value: "claude_code", provenance: "explicit" },
              model: {
                value: "anthropic/claude-opus-4.8",
                provenance: "explicit",
              },
              permissionMode: {
                value: "bypassPermissions",
                provenance: "explicit",
              },
              effort: { value: "high", provenance: "explicit" },
              credentialReady: true,
              requiresInteractiveAuth: false,
            },
          },
        }),
    ),
    http.post(PREVIEW_URL, () => HttpResponse.json({ data: previewBody() })),
  );
});

async function confirmResolution(runId: string) {
  const start = await screen.findByTestId(`start-merge-${runId}`);
  await waitFor(() => expect(start).toBeEnabled());
  fireEvent.click(start);
}

function promoteResponse(
  outcome: "bound" | "published" | "merge_required",
  mergeRunId?: string,
) {
  return {
    branch: {
      workspaceId: "workspace-1",

      repoId: REPO_ID,
      repoName: "code",
      branchName: "feature",

      trackingMode: "remote_tracking",
      headCommit: "abc123",
      divergence: 0,
      behindBase: 0,
      hasDraft: false,
      dirtyFileCount: 0,
    },
    coordinated: { repos: [] },
    outcome,
    ...(mergeRunId ? { mergeRunId } : {}),
  };
}

function renderDialog(
  props: Partial<React.ComponentProps<typeof MakeRemoteDialog>> = {},
  gate?: WorkspaceLiveGate,
) {
  const onClose = vi.fn();
  const dialog = (
    <MakeRemoteDialog
      open
      workspaceId={WORKSPACE_ID}
      repoId={REPO_ID}
      repoLabel="acme/code"
      onClose={onClose}
      {...props}
    />
  );
  renderApp(
    gate ? (
      <WorkspaceLiveGateProvider value={gate}>
        {dialog}
      </WorkspaceLiveGateProvider>
    ) : (
      dialog
    ),
    { user: createMockUser() },
  );
  return { onClose };
}

async function submitDivergentBind(mergeRunId = "merge-run-7") {
  server.use(
    http.post(PROMOTE_URL, () =>
      HttpResponse.json({
        data: promoteResponse("merge_required", mergeRunId),
      }),
    ),
  );
  fillValidForm();
  fireEvent.click(screen.getByTestId("make-remote-ack"));
  await waitFor(() =>
    expect(screen.getByTestId("make-remote-submit")).toBeEnabled(),
  );
  fireEvent.click(screen.getByTestId("make-remote-submit"));
  return screen.findByTestId("make-remote-reconcile");
}

function serveBase() {
  server.use(
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
      HttpResponse.json({
        data: {
          ...createMockWorkspace({ id: WORKSPACE_ID }),
          orgId: "org-1",
          isBase: true,
        },
      }),
    ),
  );
}

function fillValidForm() {
  fireEvent.change(screen.getByTestId("make-remote-git-url"), {
    target: { value: "https://github.com/acme/code.git" },
  });
  fireEvent.change(screen.getByTestId("make-remote-branch"), {
    target: { value: "main" },
  });
}

describe("MakeRemoteDialog", () => {
  it("keeps an open dialog but stops its submit once publication is no longer the viewer's (g5.workspace.ui.reader-publication)", async () => {
    let posted = 0;
    server.use(
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/remote_tracking`,
        () => {
          posted += 1;
          return HttpResponse.json({ data: { outcome: "bound" } });
        },
      ),
    );
    // The viewer stays an organization administrator throughout; losing
    // ownership alone must stop Make Remote off Base (owner-and-admin rule).
    const gate = (owner: boolean) => ({
      kind: owner ? ("live" as const) : ("reader" as const),
      canWrite: owner,
      live: owner,
      canStart: false,
      reviewCopy: false,
      reviewIdentityKnown: true,
      owner,
      orgAdmin: true,
      workspaceNoun: "the workspace",
    });
    function Host() {
      const [owner, setOwner] = useState(true);
      return (
        <WorkspaceLiveGateProvider value={gate(owner)}>
          <button
            type="button"
            data-testid="lose-ownership"
            onClick={() => setOwner(false)}
          >
            transfer
          </button>
          <MakeRemoteDialog
            open
            workspaceId={WORKSPACE_ID}
            repoId={REPO_ID}
            repoLabel="acme/code"
            onClose={vi.fn()}
          />
        </WorkspaceLiveGateProvider>
      );
    }
    renderApp(<Host />, { user: createMockUser() });
    fillValidForm();
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    const submit = screen.getByRole("button", { name: /^Make remote$/ });
    expect(submit).toBeEnabled();
    fireEvent.click(screen.getByTestId("lose-ownership"));
    expect(submit).toBeDisabled();
    expect(screen.getByTestId("make-remote-blocked")).toBeVisible();
    // The draft is kept.
    expect(screen.getByTestId("make-remote-git-url")).toHaveValue(
      "https://github.com/acme/code.git",
    );
    fireEvent.click(submit);
    await waitFor(() => expect(posted).toBe(0));
  });

  it("keeps submit disabled while the Git URL or remote branch is empty", () => {
    renderDialog();
    const submit = screen.getByTestId("make-remote-submit");
    expect(submit).toBeDisabled();

    // URL only, still blocked (empty branch).
    fireEvent.change(screen.getByTestId("make-remote-git-url"), {
      target: { value: "https://github.com/acme/code.git" },
    });
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    expect(submit).toBeDisabled();

    // Branch only, still blocked (empty URL).
    fireEvent.change(screen.getByTestId("make-remote-git-url"), {
      target: { value: "" },
    });
    fireEvent.change(screen.getByTestId("make-remote-branch"), {
      target: { value: "main" },
    });
    expect(submit).toBeDisabled();
  });

  it("gates submit behind the org-admin blast-radius acknowledgement", () => {
    renderDialog();
    const submit = screen.getByTestId("make-remote-submit");
    fillValidForm();
    // Valid fields but acknowledgement unchecked → still blocked.
    expect(submit).toBeDisabled();
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    expect(submit).toBeEnabled();
  });

  it("keeps long unbroken dialog content inside the responsive modal", () => {
    renderDialog({
      repoLabel: `acme/${"very-long-repository-name".repeat(8)}`,
    });

    expect(screen.getByTestId("make-remote-dialog")).toHaveClass(
      "min-w-0",
      "[overflow-wrap:anywhere]",
    );
    expect(
      screen.getByTestId("make-remote-dialog-overlay"),
    ).toBeInTheDocument();
  });

  it("renders coordinated API failures as wrapped human-readable evidence, not JSON", async () => {
    const detailMessage =
      "ls-remote could not authenticate to this remote because the connected credential lacks repository access";
    const details = {
      code: "REMOTE_BIND_REF_LOOKUP_FAILED",
      operation: "remote_bind",
      message: "The selected remote branch could not be resolved.",
      modal: {
        severity: "error",
        title: "Coordinated repo sync failed",
        message: "The selected remote branch could not be resolved.",
        repos: [
          {
            workspaceId: "workspace-1",
            repoId: REPO_ID,
            repoName: "code",
            branchName: "feature",
            operation: "remote_bind",
            status: "failed",
            error: detailMessage,
          },
        ],
      },
    };
    server.use(
      http.post(PROMOTE_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: `Validation error: ${JSON.stringify(details)}`,
              requestId: "req-make-remote",
              details,
            },
          },
          { status: 422 },
        ),
      ),
    );

    renderDialog();
    fillValidForm();
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    fireEvent.click(screen.getByTestId("make-remote-submit"));

    const error = await screen.findByTestId("make-remote-error");
    expect(error).toHaveTextContent(
      "The selected remote branch could not be resolved.",
    );
    expect(error).not.toHaveTextContent("Validation error:");
    expect(error).not.toHaveTextContent('"modal"');
    expect(screen.getByTestId("make-remote-error-repo-code")).toHaveTextContent(
      detailMessage,
    );
    expect(screen.getByTestId("make-remote-dialog")).toBeInTheDocument();
  });

  it("POSTs the promote endpoint with the gitUrl and closes on a bound outcome", async () => {
    const bodies: Array<{ remoteBranch?: string; gitUrl?: string }> = [];
    server.use(
      http.post(PROMOTE_URL, async ({ request }) => {
        bodies.push(
          (await request.json()) as { remoteBranch?: string; gitUrl?: string },
        );
        return HttpResponse.json({ data: promoteResponse("bound") });
      }),
    );
    const { onClose } = renderDialog();
    fillValidForm();
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    await waitFor(() =>
      expect(screen.getByTestId("make-remote-submit")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("make-remote-submit"));

    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).toEqual({
      remoteBranch: "main",
      gitUrl: "https://github.com/acme/code.git",
    });
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it("starts the returned mergeRunId with confirmed settings after merge_required", async () => {
    const MERGE_RUN_ID = "merge-run-42";
    let startHit = false;
    let launch: AgentLaunchSelectionV1 | undefined;
    server.use(
      http.post(PROMOTE_URL, () =>
        HttpResponse.json({
          data: promoteResponse("merge_required", MERGE_RUN_ID),
        }),
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs/${MERGE_RUN_ID}/start`,
        async ({ request }) => {
          launch = (
            (await request.json()) as { agentLaunch: AgentLaunchSelectionV1 }
          ).agentLaunch;
          startHit = true;
          return HttpResponse.json({
            data: {
              run: {
                id: MERGE_RUN_ID,
                status: "running",
              },
            },
          });
        },
      ),
    );
    const { onClose } = renderDialog();
    fillValidForm();
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    await waitFor(() =>
      expect(screen.getByTestId("make-remote-submit")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("make-remote-submit"));

    await screen.findByTestId(`start-merge-${MERGE_RUN_ID}`);
    expect(startHit).toBe(false);
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.queryByTestId("make-remote-submit")).toBeNull();
    await confirmResolution(MERGE_RUN_ID);
    await waitFor(() => expect(startHit).toBe(true));
    expect(launch?.preferences.permissionMode).toEqual({
      source: "explicit",
      value: "bypassPermissions",
    });
    expect(launch?.confirmedResolutionId).toBe("alr_make_remote");
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it("starts resolution and hands the workspace session to the host for navigation", async () => {
    const MERGE_RUN_ID = "merge-run-99";
    const RESOLUTION_SESSION_ID = "resolution-session-99";
    let startHit = false;
    const onMergeRequired = vi.fn();
    server.use(
      http.post(PROMOTE_URL, () =>
        HttpResponse.json({
          data: promoteResponse("merge_required", MERGE_RUN_ID),
        }),
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs/${MERGE_RUN_ID}/start`,
        () => {
          startHit = true;
          return HttpResponse.json({
            data: {
              run: {
                id: MERGE_RUN_ID,
                status: "running",
                resolutionSessionId: RESOLUTION_SESSION_ID,
              },
            },
          });
        },
      ),
    );
    const { onClose } = renderDialog({ onMergeRequired });
    fillValidForm();
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    await waitFor(() =>
      expect(screen.getByTestId("make-remote-submit")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("make-remote-submit"));

    await confirmResolution(MERGE_RUN_ID);
    // Navigation follows the confirmed launch, using its workspace session.
    await waitFor(() => expect(startHit).toBe(true));
    await waitFor(() =>
      expect(onMergeRequired).toHaveBeenCalledWith({
        mergeRunId: MERGE_RUN_ID,
        resolutionSessionId: RESOLUTION_SESSION_ID,
      }),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
  });

  it("keeps the dialog open with an error when a merge_required response omits mergeRunId", async () => {
    let startHit = false;
    const onMergeRequired = vi.fn();
    server.use(
      // Malformed: merge_required with NO mergeRunId.
      http.post(PROMOTE_URL, () =>
        HttpResponse.json({ data: promoteResponse("merge_required") }),
      ),
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/merge_runs/:mergeRunId/start`,
        () => {
          startHit = true;
          return HttpResponse.json({ data: { run: { id: "x" } } });
        },
      ),
    );
    const { onClose } = renderDialog({ onMergeRequired });
    fillValidForm();
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    await waitFor(() =>
      expect(screen.getByTestId("make-remote-submit")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("make-remote-submit"));

    // The inline error appears and the dialog stays mounted; nothing is routed.
    await waitFor(() =>
      expect(screen.getByTestId("make-remote-error")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("make-remote-dialog")).toBeInTheDocument();
    expect(startHit).toBe(false);
    expect(onMergeRequired).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("MakeRemoteDialog reconciliation after a divergent bind (g5.workspace.remote.adopt-provider-history)", () => {
  it("offers adopting the reviewed provider history beside Resolve with agent, recommended for an initial-only history", async () => {
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json({ data: previewBody({ initialOnly: true }) }),
      ),
    );
    renderDialog({}, gateFor({ orgAdmin: true }));
    await submitDivergentBind();

    const adopt = screen.getByTestId("make-remote-adopt");
    const review = await screen.findByTestId("accept-provider-history-preview");
    expect(adopt).toContainElement(review);
    expect(
      screen.getByTestId("accept-provider-history-recommended"),
    ).toHaveTextContent("Recommended");
    expect(review).toHaveTextContent("cafebab");
    expect(review).toHaveTextContent("1 local commit not on the provider");
    expect(review).toHaveTextContent("only the repository's initial commit");
    expect(review).toHaveTextContent("Nothing is pushed to the provider.");
    expect(
      screen.getByTestId("accept-provider-history-confirm"),
    ).toHaveTextContent("Discard and adopt provider history");
    expect(screen.getByTestId("make-remote-resolve")).toContainElement(
      await screen.findByTestId("start-merge-merge-run-7"),
    );
  });

  it("does not recommend adopting when real local commits would be discarded", async () => {
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json({ data: previewBody({ discardedCommits: 3 }) }),
      ),
    );
    renderDialog({}, gateFor({ orgAdmin: true }));
    await submitDivergentBind();

    const review = await screen.findByTestId("accept-provider-history-preview");
    expect(review).toHaveTextContent("3 local commits not on the provider");
    expect(
      screen.queryByTestId("accept-provider-history-recommended"),
    ).toBeNull();
    expect(screen.getByTestId("make-remote-resolve")).toBeInTheDocument();
  });

  it("adopts exactly the reviewed tip and head, then notifies and closes", async () => {
    let accepted: unknown = null;
    server.use(
      http.post(ACCEPT_URL, async ({ request }) => {
        accepted = await request.json();
        return HttpResponse.json({
          data: {
            workspaceId: WORKSPACE_ID,
            repoId: REPO_ID,
            head: PROVIDER_TIP,
          },
        });
      }),
    );
    const onNotify = vi.fn();
    const { onClose } = renderDialog({ onNotify }, gateFor({ orgAdmin: true }));
    await submitDivergentBind();

    fireEvent.click(
      await screen.findByTestId("accept-provider-history-confirm"),
    );
    await waitFor(() =>
      expect(accepted).toEqual({
        expectedProviderTip: PROVIDER_TIP,
        expectedHead: LOCAL_HEAD,
      }),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(onNotify).toHaveBeenCalledWith(
      "info",
      "Provider history adopted",
      expect.stringContaining("reset to the provider tip"),
    );
  });

  it("keeps adopting visible but waiting, with the reason, while the VM is stopped", async () => {
    let accepted = 0;
    server.use(
      http.post(ACCEPT_URL, () => {
        accepted += 1;
        return HttpResponse.json({ data: {} });
      }),
    );
    renderDialog({}, gateFor({ kind: "stopped", live: false, canStart: true, orgAdmin: true }));
    await submitDivergentBind();

    const confirm = await screen.findByTestId(
      "accept-provider-history-confirm",
    );
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute(
      "title",
      "Start the workspace to adopt the provider history.",
    );
    fireEvent.click(
      screen.getByTestId("accept-provider-history-confirm-hint-trigger"),
    );
    expect(
      screen.getByTestId("accept-provider-history-confirm-hint"),
    ).toHaveTextContent("Start the workspace to adopt the provider history.");
    expect(
      screen.getByTestId("accept-provider-history-confirm-hint-start"),
    ).toBeInTheDocument();
    expect(accepted).toBe(0);
  });

  it("offers Base only adopting, to an organization administrator, and lands it without a VM", async () => {
    serveBase();
    let accepted: unknown = null;
    server.use(
      http.post(ACCEPT_URL, async ({ request }) => {
        accepted = await request.json();
        return HttpResponse.json({
          data: {
            workspaceId: WORKSPACE_ID,
            repoId: REPO_ID,
            head: PROVIDER_TIP,
          },
        });
      }),
    );
    const onNotify = vi.fn();
    const { onClose } = renderDialog({ onNotify }, BASE_ADMIN_GATE);
    const reconcile = await submitDivergentBind();

    expect(reconcile).toHaveTextContent("Base has no workspace VM");
    const review = await screen.findByTestId("accept-provider-history-preview");
    expect(review).toHaveTextContent("1 Base commit not on the provider");
    expect(screen.queryByTestId("make-remote-resolve")).toBeNull();
    expect(screen.queryByTestId("start-merge-merge-run-7")).toBeNull();

    const confirm = screen.getByTestId("accept-provider-history-confirm");
    expect(confirm).toBeEnabled();
    fireEvent.click(confirm);
    await waitFor(() =>
      expect(accepted).toEqual({
        expectedProviderTip: PROVIDER_TIP,
        expectedHead: LOCAL_HEAD,
      }),
    );
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(onNotify).toHaveBeenCalledWith(
      "info",
      "Provider history adopted",
      expect.stringContaining("Base now matches the provider branch"),
    );
  });

  it("shows a refused adoption inline and lets the viewer review again", async () => {
    server.use(
      http.post(ACCEPT_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "PROVIDER_HISTORY_CHANGED",
              message:
                "The provider tip or workspace head moved since the review.",
              requestId: "req-adopt",
            },
          },
          { status: 409 },
        ),
      ),
    );
    const { onClose } = renderDialog({}, gateFor({ orgAdmin: true }));
    await submitDivergentBind();

    fireEvent.click(
      await screen.findByTestId("accept-provider-history-confirm"),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "moved since the review",
    );
    expect(
      screen.getByTestId("accept-provider-history-button"),
    ).toHaveTextContent("Adopt provider history");
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("MakeRemoteDialog provider branch creation on Base (g5.workspace.remote.base-parity)", () => {
  type PromoteBody = {
    remoteBranch?: string;
    gitUrl?: string;
    createProviderBranch?: boolean;
  };

  function creationRefusal() {
    const details = {
      code: "PROVIDER_BRANCH_CREATION_REQUIRES_CONFIRMATION",
      operation: "remote_bind",
      message:
        "The provider branch does not exist. Confirm to create it from Base's current history.",
      modal: {
        severity: "error",
        title: "Coordinated repo sync failed",
        message:
          "The provider branch does not exist. Confirm to create it from Base's current history.",
        repos: [
          {
            workspaceId: WORKSPACE_ID,
            repoId: REPO_ID,
            repoName: "code",
            branchName: "main",
            operation: "remote_bind",
            status: "blocked",
            error: "provider branch 'release' does not exist",
          },
        ],
      },
    };
    return HttpResponse.json(
      {
        error: {
          code: "VALIDATION_ERROR",
          message: `Validation error: ${JSON.stringify(details)}`,
          requestId: "req-create-branch",
          details,
        },
      },
      { status: 422 },
    );
  }

  /** Refuses an unconfirmed request; binds a confirmed one. */
  function serveCreationFlow(bodies: PromoteBody[]) {
    server.use(
      http.post(PROMOTE_URL, async ({ request }) => {
        const body = (await request.json()) as PromoteBody;
        bodies.push(body);
        return body.createProviderBranch
          ? HttpResponse.json({ data: promoteResponse("published") })
          : creationRefusal();
      }),
    );
  }

  async function submitAbsentBranch() {
    serveBase();
    fireEvent.change(screen.getByTestId("make-remote-git-url"), {
      target: { value: "https://github.com/acme/code.git" },
    });
    fireEvent.change(screen.getByTestId("make-remote-branch"), {
      target: { value: "release" },
    });
    fireEvent.click(screen.getByTestId("make-remote-ack"));
    await waitFor(() =>
      expect(screen.getByTestId("make-remote-submit")).toBeEnabled(),
    );
    fireEvent.click(screen.getByTestId("make-remote-submit"));
    return screen.findByTestId("make-remote-create-branch");
  }

  it("asks to create the absent branch instead of showing an error", async () => {
    const bodies: PromoteBody[] = [];
    serveCreationFlow(bodies);
    renderDialog({}, BASE_ADMIN_GATE);

    const confirm = await submitAbsentBranch();
    expect(confirm).toHaveTextContent(
      "Branch release doesn't exist on the provider.",
    );
    expect(confirm).toHaveTextContent(
      "Create it from Base's current history? This pushes Base's history to the provider once to create the branch.",
    );
    expect(screen.queryByTestId("make-remote-error")).toBeNull();
    expect(screen.queryByTestId("make-remote-submit")).toBeNull();
    const create = screen.getByRole("button", {
      name: "Create branch and Make Remote",
    });
    await waitFor(() => expect(create).toHaveFocus());
    // The draft is locked while the confirmation is pending.
    expect(screen.getByTestId("make-remote-branch")).toBeDisabled();
    expect(bodies).toEqual([
      { remoteBranch: "release", gitUrl: "https://github.com/acme/code.git" },
    ]);
  });

  it("resends the same branch and URL with createProviderBranch on confirm", async () => {
    const bodies: PromoteBody[] = [];
    serveCreationFlow(bodies);
    const onNotify = vi.fn();
    const { onClose } = renderDialog({ onNotify }, BASE_ADMIN_GATE);

    await submitAbsentBranch();
    fireEvent.click(
      screen.getByRole("button", { name: "Create branch and Make Remote" }),
    );

    await waitFor(() => expect(bodies).toHaveLength(2));
    expect(bodies[1]).toEqual({
      remoteBranch: "release",
      gitUrl: "https://github.com/acme/code.git",
      createProviderBranch: true,
    });
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(onNotify).toHaveBeenCalledWith(
      "info",
      "Repository is now remote",
      expect.any(String),
    );
  });

  it("shows a failure of the confirmed request as an error", async () => {
    const bodies: PromoteBody[] = [];
    server.use(
      http.post(PROMOTE_URL, async ({ request }) => {
        const body = (await request.json()) as PromoteBody;
        bodies.push(body);
        return body.createProviderBranch
          ? HttpResponse.json(
              {
                error: {
                  code: "FORBIDDEN",
                  message: "Your provider credential cannot push here.",
                  requestId: "req-create-branch-denied",
                },
              },
              { status: 403 },
            )
          : creationRefusal();
      }),
    );
    const { onClose } = renderDialog({}, BASE_ADMIN_GATE);

    await submitAbsentBranch();
    fireEvent.click(
      screen.getByRole("button", { name: "Create branch and Make Remote" }),
    );

    expect(await screen.findByTestId("make-remote-error")).toHaveTextContent(
      "Your provider credential cannot push here.",
    );
    expect(screen.queryByTestId("make-remote-create-branch")).toBeNull();
    expect(screen.getByTestId("make-remote-branch")).toHaveValue("release");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("returns to the form with the draft intact on Back and sends nothing", async () => {
    const bodies: PromoteBody[] = [];
    serveCreationFlow(bodies);
    const { onClose } = renderDialog({}, BASE_ADMIN_GATE);

    await submitAbsentBranch();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));

    expect(screen.queryByTestId("make-remote-create-branch")).toBeNull();
    expect(screen.queryByTestId("make-remote-error")).toBeNull();
    expect(screen.getByTestId("make-remote-git-url")).toHaveValue(
      "https://github.com/acme/code.git",
    );
    expect(screen.getByTestId("make-remote-branch")).toHaveValue("release");
    expect(screen.getByTestId("make-remote-branch")).toBeEnabled();
    expect(screen.getByTestId("make-remote-ack")).toBeChecked();
    const submit = screen.getByTestId("make-remote-submit");
    expect(submit).toBeEnabled();
    await waitFor(() => expect(submit).toHaveFocus());
    expect(onClose).not.toHaveBeenCalled();
    // Only the refused request was sent.
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(bodies).toHaveLength(1);
    expect(bodies[0].createProviderBranch).toBeUndefined();
  });
});

describe("MakeRemoteMenu visibility gate", () => {
  it("keeps the open dialog mounted when promotion removes the menu trigger", async () => {
    function Parent() {
      const [available, setAvailable] = useState(true);
      return (
        <>
          <button onClick={() => setAvailable(false)}>
            Remote tracking refreshed
          </button>
          <MakeRemoteMenu
            workspaceId={WORKSPACE_ID}
            repoId={REPO_ID}
            available={available}
          />
        </>
      );
    }
    renderApp(<Parent />);
    fireEvent.click(screen.getByTestId("make-remote-menu-button"));
    fillValidForm();
    fireEvent.click(screen.getByText("Remote tracking refreshed"));
    expect(screen.queryByTestId("make-remote-menu-button")).toBeNull();
    expect(screen.getByTestId("make-remote-git-url")).toHaveValue(
      "https://github.com/acme/code.git",
    );
    fireEvent.click(screen.getByTestId("make-remote-cancel"));
    expect(screen.queryByTestId("make-remote-dialog")).toBeNull();
  });

  function mockInstanceFeatures(features: string[]) {
    server.use(
      http.get("/api/v1/instance/info", () =>
        HttpResponse.json({
          data: { ...MOCK_INSTANCE_INFO, features },
        }),
      ),
    );
  }

  it("offers Make Remote when instance metadata omits the retired feature marker", async () => {
    mockInstanceFeatures(["chat"]);
    renderApp(<MakeRemoteMenu workspaceId={WORKSPACE_ID} repoId={REPO_ID} />, {
      user: createMockUser(),
    });
    // Availability does not depend on the instance-info feature marker.
    await waitFor(() => {
      expect(
        screen.queryByTestId("make-remote-menu-button"),
      ).toBeInTheDocument();
    });
  });

  it.each([
    [
      "the owner-administrator of a writable workspace",
      gateFor({ orgAdmin: true }),
      true,
    ],
    ["a non-admin owner of a writable workspace", gateFor(), false],
    [
      "Base's owner, an administrator",
      gateFor({ kind: "base", canWrite: false, live: false, orgAdmin: true }),
      true,
    ],
    ["an organization administrator on Base", BASE_ADMIN_GATE, true],
    ["a plain member on Base", { ...BASE_ADMIN_GATE, orgAdmin: false }, false],
    [
      "the owner-administrator of a review copy",
      gateFor({ reviewCopy: true, orgAdmin: true }),
      false,
    ],
    [
      "an organization administrator who does not own an ordinary workspace",
      gateFor({
        kind: "reader",
        canWrite: false,
        owner: false,
        orgAdmin: true,
      }),
      false,
    ],
  ])(
    "offers the labelled Make Remote button to %s: %s",
    (_label, gate, offered) => {
      renderApp(
        <WorkspaceLiveGateProvider value={gate}>
          <MakeRemoteMenu workspaceId={WORKSPACE_ID} repoId={REPO_ID} />
        </WorkspaceLiveGateProvider>,
        { user: createMockUser() },
      );
      const trigger = screen.queryByRole("button", { name: "Make Remote" });
      if (offered) {
        expect(trigger).toHaveAttribute(
          "data-testid",
          "make-remote-menu-button",
        );
      } else {
        expect(trigger).toBeNull();
      }
    },
  );

  it("renders the trigger and opens the dialog when make_remote is present", async () => {
    mockInstanceFeatures(["chat", "make_remote"]);
    renderApp(<MakeRemoteMenu workspaceId={WORKSPACE_ID} repoId={REPO_ID} />, {
      user: createMockUser(),
    });
    const trigger = await screen.findByTestId("make-remote-menu-button");
    expect(screen.queryByTestId("make-remote-dialog")).not.toBeInTheDocument();
    fireEvent.click(trigger);
    expect(await screen.findByTestId("make-remote-dialog")).toBeInTheDocument();
  });
});
