/**
 * Vitest unit tests for PublishBranchMenu — the labelled Publish button a
 * local-only, Publish-eligible branch shows on its repository card
 * (g5.workspace.remote-ui.actions). The button opens a dialog that offers the
 * Publish action (rendered by BranchAutoPullControl) with real pending/error
 * feedback.
 *
 * Scenarios:
 * 1. Renders one labelled "Publish" button and no dialog until clicked.
 * 2. Clicking the button opens the dialog with the Publish action.
 * 3. Escape closes the dialog and returns focus to the button.
 * 4. Publishing POSTs the per-child publish endpoint and closes the dialog
 *    once the publish confirms; a failure stays visible in the open dialog.
 * 5. A reader or a review copy is not offered the button.
 *
 * SOT:
 * - docs/.../auto_pull_publication_options_spec.md §6.2
 * - workspace_service D43
 */

import { describe, it, expect } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { renderApp } from "../../test/render";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { PublishBranchMenu } from "./PublishBranchMenu";
import {
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";

const WORKSPACE_ID = "ws-actions";
const REPO_ID = "repo-1";
const PUBLISH_URL = `/api/v1/workspaces/${WORKSPACE_ID}/repos/${REPO_ID}/publish`;

function workspaceDetail() {
  return {
    id: WORKSPACE_ID,
    orgId: "org-1",
    name: "demo",
    isBase: false,
    description: null,
    clonedFromId: null,
    docsRepoId: null,
    createdBy: "user-1",
    createdAt: "2026-05-28T00:00:00Z",
    updatedAt: "2026-05-28T00:00:00Z",
    pushMode: "manual",
    branches: [
      {
        workspaceId: "workspace-1",

        repoId: REPO_ID,
        repoName: "code",
        branchName: "feature",

        // Local-only child ⇒ the dialog offers Publish.
        trackingMode: "pinned",
        remoteEligible: true,
        headCommit: "abc123",
        divergence: 0,
        behindBase: 0,
        hasDraft: false,
        dirtyFileCount: 0,
        autoPullEnabled: false,
      },
    ],
  };
}

function mockWorkspace() {
  return http.get(`/api/v1/workspaces/${WORKSPACE_ID}`, () =>
    HttpResponse.json({ data: workspaceDetail() }),
  );
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

function render(gate?: WorkspaceLiveGate) {
  const menu = (
    <PublishBranchMenu
      workspaceId={WORKSPACE_ID}
      repoId={REPO_ID}
      repoLabel="acme/code"
    />
  );
  return renderApp(
    gate ? (
      <WorkspaceLiveGateProvider value={gate}>{menu}</WorkspaceLiveGateProvider>
    ) : (
      menu
    ),
    { user: createMockUser() },
  );
}

describe("PublishBranchMenu", () => {
  it("renders one labelled Publish button and no dialog until clicked", () => {
    server.use(mockWorkspace());
    render();

    const trigger = screen.getByRole("button", { name: "Publish" });
    expect(trigger).toHaveAttribute(
      "data-testid",
      "publish-branch-menu-button",
    );
    expect(trigger).toHaveAttribute("aria-haspopup", "dialog");
    expect(
      screen.queryByTestId("publish-branch-dialog"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("publish-branch-button"),
    ).not.toBeInTheDocument();
  });

  it("opens a dialog offering the Publish action", async () => {
    server.use(mockWorkspace());
    render();

    fireEvent.click(screen.getByRole("button", { name: "Publish" }));

    const dialog = await screen.findByTestId("publish-branch-dialog");
    expect(dialog).toHaveTextContent("Publish branch");
    expect(dialog).toHaveTextContent("acme/code");
    expect(
      await screen.findByTestId("publish-branch-button"),
    ).toHaveTextContent("Publish to Remote");
  });

  it("closes on Escape and returns focus to the Publish button", async () => {
    server.use(mockWorkspace());
    render();

    const trigger = screen.getByRole("button", { name: "Publish" });
    trigger.focus();
    fireEvent.click(trigger);

    // The shared Modal moves focus into the dialog.
    const dialog = await screen.findByTestId("publish-branch-dialog");
    await waitFor(
      () =>
        expect(dialog).toContainElement(document.activeElement as HTMLElement),
      {
        timeout: 10_000,
      },
    );

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(
        screen.queryByTestId("publish-branch-dialog"),
      ).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(trigger).toHaveFocus(), { timeout: 10_000 });
  });

  it("publishes the child and closes the dialog once the publish confirms", async () => {
    const publishCalls: string[] = [];
    server.use(
      mockWorkspace(),
      http.post(PUBLISH_URL, () => {
        publishCalls.push(REPO_ID);
        return HttpResponse.json({
          data: { repoId: REPO_ID, trackingMode: "remote_tracking" },
        });
      }),
    );
    render();

    fireEvent.click(screen.getByRole("button", { name: "Publish" }));
    fireEvent.click(await screen.findByTestId("publish-branch-button"));

    await waitFor(() => expect(publishCalls).toHaveLength(1));
    await waitFor(() =>
      expect(
        screen.queryByTestId("publish-branch-dialog"),
      ).not.toBeInTheDocument(),
    );
  });

  it("keeps the dialog open with the error when publishing fails", async () => {
    server.use(
      mockWorkspace(),
      http.post(PUBLISH_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "The remote branch already exists.",
              requestId: "req-publish",
            },
          },
          { status: 409 },
        ),
      ),
    );
    render();

    fireEvent.click(screen.getByRole("button", { name: "Publish" }));
    fireEvent.click(await screen.findByTestId("publish-branch-button"));

    expect(await screen.findByTestId("publish-branch-error")).toHaveTextContent(
      "The remote branch already exists.",
    );
    expect(screen.getByTestId("publish-branch-dialog")).toBeInTheDocument();
  });

  it.each([
    ["a reader", gateFor({ canWrite: false, owner: false })],
    ["a review copy", gateFor({ reviewCopy: true })],
    ["an unresolved review identity", gateFor({ reviewIdentityKnown: false })],
  ])("is not offered to %s", (_label, gate) => {
    server.use(mockWorkspace());
    render(gate);
    expect(screen.queryByRole("button", { name: "Publish" })).toBeNull();
  });

  it("is offered to the owner of a writable workspace", () => {
    server.use(mockWorkspace());
    render(gateFor());
    expect(screen.getByRole("button", { name: "Publish" })).toBeInTheDocument();
  });
});
