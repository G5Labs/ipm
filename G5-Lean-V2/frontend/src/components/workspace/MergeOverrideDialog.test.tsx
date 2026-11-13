// Merge with override confirmation (g5.workspace.merge.admin-override.formal,
// g5.git.provider-override): the shown set grouped by authority, the
// reason + checkbox gate, not-overridable blockers, and the re-check refusal.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";

import { server } from "../../test/msw/server";
import type {
  MergeOverridePreview,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import { MERGE_OVERRIDE_CONFIRMATION } from "../../lib/mergeOverride";
import {
  MergeOverrideAction,
  MergeOverrideDialog,
} from "./MergeOverrideDialog";

const PREVIEW_URL =
  "/api/v1/workspaces/ws-target/pull-requests/pr-1/merge-override";

function previewFixture(
  overrides: Partial<MergeOverridePreview> = {},
): MergeOverridePreview {
  return {
    sourceHeads: [
      {
        repoId: "repo-code",
        branchName: "main",
        workspaceId: "ws-source",
        headCommit: "abc123",
      },
    ],
    blockers: {
      approvals: [
        {
          definitionId: "def-ie",
          definitionName: "Integration sign-off",
          definitionVersion: 2,
          state: "pending",
        },
        {
          definitionId: "def-sec",
          definitionName: "Security review",
          definitionVersion: 1,
          state: "rejected",
        },
      ],
      checks: [
        {
          repository: "acme/web",
          headRefOid: "abc123",
          availability: "ready",
          checks: [
            { name: "build", bucket: "fail", required: true },
            { name: "lint", bucket: "pending", required: false },
          ],
        },
        {
          repository: "acme/docs",
          headRefOid: null,
          availability: "unavailable",
          checks: [],
        },
      ],
      github: [
        {
          repoId: "repo-code",
          repository: "acme/web",
          headSha: "abc123",
          mergeState: "blocked",
        },
      ],
    },
    notOverridable: [],
    authority: { g5: { allowed: true }, github: { allowed: true } },
    selfOverride: false,
    digest: "digest-1",
    ...overrides,
  };
}

function mergedPullRequest(): WorkspacePullRequestSummary {
  return {
    id: "pr-1",
    sourceWorkspaceId: "ws-source",
    sourceWorkspaceName: "Fork",
    targetWorkspaceId: "ws-target",
    targetWorkspaceName: "Base",
    status: "merged",
    localMergeStatus: "merged",
    providerStatus: "local_only",
    title: "feat: thing",
    descriptionOverride: null,
    sourceHeads: [],
    targetHeads: [],
    preflight: {} as never,
    lastPreflightFingerprint: "fingerprint",
    blockingMessage: null,
    createdAt: "2026-10-27T00:00:00Z",
    updatedAt: "2026-10-27T00:00:00Z",
    mergedAt: "2026-10-27T00:00:00Z",
  };
}

function renderDialog() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  const onMerged = vi.fn();
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={queryClient}>
      <MergeOverrideDialog
        targetWorkspaceId="ws-target"
        targetWorkspaceName="Base"
        pullRequestId="pr-1"
        pullRequestTitle="feat: thing"
        onClose={onClose}
        onMerged={onMerged}
      />
    </QueryClientProvider>,
  );
  return { onMerged, onClose };
}

function servePreview(...previews: MergeOverridePreview[]) {
  let reads = 0;
  server.use(
    http.get(PREVIEW_URL, () => {
      const preview = previews[Math.min(reads, previews.length - 1)];
      reads += 1;
      return HttpResponse.json({ data: preview });
    }),
  );
  return () => reads;
}

describe("MergeOverrideDialog", () => {
  it("lists G5 requirements, checks by repository and GitHub rules as separate groups", async () => {
    servePreview(previewFixture({ selfOverride: true }));
    renderDialog();

    const g5 = await screen.findByTestId("merge-override-g5-group");
    expect(within(g5).getByText("G5 requirements")).toBeInTheDocument();
    expect(
      screen.getByTestId("merge-override-approval-def-ie"),
    ).toHaveTextContent("Integration sign-off v2Waiting");
    expect(
      screen.getByTestId("merge-override-approval-def-sec"),
    ).toHaveTextContent("Rejected");
    const web = screen.getByTestId("merge-override-checks-acme/web");
    expect(web).toHaveTextContent("build · required");
    expect(web).toHaveTextContent("Failing");
    expect(web).toHaveTextContent("lint");
    expect(web).toHaveTextContent("Pending");
    expect(
      screen.getByTestId("merge-override-checks-acme/docs"),
    ).toHaveTextContent("Checks unreadable");

    const github = screen.getByTestId("merge-override-github-group");
    expect(within(github).getByText("GitHub rules")).toBeInTheDocument();
    expect(
      screen.getByTestId("merge-override-github-repo-code"),
    ).toHaveTextContent("acme/web");
    expect(github).toHaveTextContent("with your own GitHub account");
    expect(github).toHaveTextContent("GitHub decides whether you may bypass");

    expect(screen.getByTestId("merge-override-self")).toHaveTextContent(
      "Self-override",
    );
    expect(screen.queryByTestId("merge-override-not-overridable")).toBeNull();
  });

  it("enables Confirm only with a non-blank reason and the confirmation ticked, and posts the shown digest", async () => {
    servePreview(previewFixture());
    let sent: unknown = null;
    server.use(
      http.post(PREVIEW_URL, async ({ request }) => {
        sent = await request.json();
        return HttpResponse.json({
          data: { pullRequest: mergedPullRequest(), run: { id: "run-1" } },
        });
      }),
    );
    const user = userEvent.setup();
    const { onMerged } = renderDialog();
    await screen.findByTestId("merge-override-g5-group");

    const confirm = screen.getByTestId("merge-override-confirm");
    expect(confirm).toBeDisabled();
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    expect(confirm).toBeDisabled();
    await user.type(screen.getByTestId("merge-override-reason"), "   ");
    expect(confirm).toBeDisabled();
    await user.clear(screen.getByTestId("merge-override-reason"));
    await user.type(
      screen.getByTestId("merge-override-reason"),
      " Approver on leave; hotfix ",
    );
    expect(confirm).toBeEnabled();
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    expect(confirm).toBeDisabled();
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));

    await user.click(confirm);
    await waitFor(() => expect(onMerged).toHaveBeenCalledTimes(1));
    expect(sent).toEqual({
      reason: "Approver on leave; hotfix",
      confirmed: true,
      digest: "digest-1",
    });
  });

  it("lists mechanical blockers as not overridable and keeps Confirm disabled", async () => {
    servePreview(
      previewFixture({ notOverridable: ["The source has a merge conflict."] }),
    );
    const user = userEvent.setup();
    renderDialog();
    const block = await screen.findByTestId("merge-override-not-overridable");
    expect(block).toHaveTextContent("The source has a merge conflict.");
    expect(block).toHaveTextContent("Not overridable");
    await user.type(screen.getByTestId("merge-override-reason"), "urgent");
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    expect(screen.getByTestId("merge-override-confirm")).toBeDisabled();
  });

  it("names a group the server denies and keeps Confirm disabled", async () => {
    servePreview(
      previewFixture({
        authority: {
          g5: {
            allowed: false,
            reason: "Waiving G5 requirements needs an organization admin.",
          },
          github: { allowed: true },
        },
      }),
    );
    const user = userEvent.setup();
    renderDialog();
    expect(
      await screen.findByTestId("merge-override-g5-denied"),
    ).toHaveTextContent("needs an organization admin");
    await user.type(screen.getByTestId("merge-override-reason"), "urgent");
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    expect(screen.getByTestId("merge-override-confirm")).toBeDisabled();
  });

  it("on merge_override_changed re-reads the set, keeps the reason and clears the confirmation", async () => {
    const reads = servePreview(
      previewFixture(),
      previewFixture({
        blockers: {
          approvals: [
            {
              definitionId: "def-new",
              definitionName: "Release review",
              definitionVersion: 1,
              state: "pending",
            },
          ],
          checks: [],
          github: [],
        },
        digest: "digest-2",
      }),
    );
    const bodies: unknown[] = [];
    server.use(
      http.post(PREVIEW_URL, async ({ request }) => {
        bodies.push(await request.json());
        if (bodies.length === 1) {
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "What this override would waive changed.",
                details: {
                  code: "merge_override_changed",
                  digest: "digest-2",
                  blockers: {},
                  sourceHeads: [],
                },
              },
            },
            { status: 409 },
          );
        }
        return HttpResponse.json({
          data: { pullRequest: mergedPullRequest(), run: { id: "run-1" } },
        });
      }),
    );
    const user = userEvent.setup();
    const { onMerged } = renderDialog();
    await screen.findByTestId("merge-override-approval-def-ie");
    const readsBefore = reads();
    await user.type(screen.getByTestId("merge-override-reason"), "hotfix");
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    await user.click(screen.getByTestId("merge-override-confirm"));

    expect(
      await screen.findByTestId("merge-override-changed"),
    ).toHaveTextContent("What this override would waive changed");
    expect(
      await screen.findByTestId("merge-override-approval-def-new"),
    ).toBeInTheDocument();
    expect(reads()).toBeGreaterThan(readsBefore);
    expect(screen.queryByTestId("merge-override-approval-def-ie")).toBeNull();
    expect(screen.queryByTestId("merge-override-github-group")).toBeNull();
    expect(screen.getByTestId("merge-override-reason")).toHaveValue("hotfix");
    expect(
      screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION),
    ).not.toBeChecked();
    expect(screen.getByTestId("merge-override-confirm")).toBeDisabled();
    expect(onMerged).not.toHaveBeenCalled();

    // Confirming again sends the new set's digest.
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    await user.click(screen.getByTestId("merge-override-confirm"));
    await waitFor(() => expect(onMerged).toHaveBeenCalledTimes(1));
    expect(bodies[1]).toEqual({
      reason: "hotfix",
      confirmed: true,
      digest: "digest-2",
    });
  });

  it("tells the person to use the ordinary merge when nothing blocks any more", async () => {
    servePreview(previewFixture());
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "Nothing to override.",
              details: { code: "merge_override_not_needed" },
            },
          },
          { status: 409 },
        ),
      ),
    );
    const user = userEvent.setup();
    renderDialog();
    await screen.findByTestId("merge-override-g5-group");
    await user.type(screen.getByTestId("merge-override-reason"), "hotfix");
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    await user.click(screen.getByTestId("merge-override-confirm"));
    expect(
      await screen.findByTestId("merge-override-not-needed"),
    ).toHaveTextContent("Nothing blocks this merge any more");
    expect(screen.getByTestId("merge-override-confirm")).toBeDisabled();
  });

  it("shows GitHub's refusal in the dialog", async () => {
    servePreview(previewFixture());
    server.use(
      http.post(PREVIEW_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "Your GitHub account can't bypass these rules.",
            },
          },
          { status: 409 },
        ),
      ),
    );
    const user = userEvent.setup();
    const { onMerged } = renderDialog();
    await screen.findByTestId("merge-override-github-group");
    await user.type(screen.getByTestId("merge-override-reason"), "hotfix");
    await user.click(screen.getByLabelText(MERGE_OVERRIDE_CONFIRMATION));
    await user.click(screen.getByTestId("merge-override-confirm"));
    expect(await screen.findByTestId("merge-override-error")).toHaveTextContent(
      "Your GitHub account can't bypass these rules.",
    );
    expect(screen.getByTestId("merge-override-reason")).toHaveValue("hotfix");
    expect(onMerged).not.toHaveBeenCalled();
  });
});

describe("MergeOverrideAction", () => {
  function renderAction() {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <MergeOverrideAction
          targetWorkspaceId="ws-target"
          pullRequestId="pr-1"
          enabled
          testIdPrefix="ship"
          onMerged={vi.fn()}
        />
      </QueryClientProvider>,
    );
  }

  it("asks a person whose session cannot override to sign in again", async () => {
    server.use(
      http.get(PREVIEW_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "Insufficient permissions",
              details: { missing_scopes: ["browser_session_required"] },
              requestId: "",
            },
          },
          { status: 403 },
        ),
      ),
    );
    renderAction();
    expect(
      await screen.findByTestId("ship-merge-override-sign-in"),
    ).toHaveTextContent("Sign in again to use Merge with override.");
    expect(screen.queryByTestId("ship-merge-override")).toBeNull();
  });

  it("offers nothing to someone who may not merge", async () => {
    server.use(
      http.get(PREVIEW_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "Insufficient permissions",
              details: { missing_scopes: ["merge_authority"] },
              requestId: "",
            },
          },
          { status: 403 },
        ),
      ),
    );
    renderAction();
    await waitFor(() =>
      expect(screen.queryByTestId("ship-merge-override-sign-in")).toBeNull(),
    );
    expect(screen.queryByTestId("ship-merge-override")).toBeNull();
  });

  it("offers the button when the server allows the present groups", async () => {
    servePreview(previewFixture());
    renderAction();
    expect(
      await screen.findByTestId("ship-merge-override"),
    ).toBeInTheDocument();
  });
});
