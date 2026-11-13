/**
 * CommitChangesButton — the owner commits the running workspace's working
 * tree from wherever the button is mounted (Reviews page, Changes view).
 */

import React from "react";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { CommitChangesButton } from "./CommitChangesButton";

const WORKSPACE_ID = "workspace-commit-test";

function report(overrides: Record<string, unknown> = {}) {
  return {
    workspace_id: WORKSPACE_ID,
    source: "live",
    attached: true,
    status: "succeeded",
    members: [
      {
        repo_id: "11111111-1111-1111-1111-111111111111",
        root_path: "code",
        errors: [],
        dirty: [
          { status: "M", path: "src/main.rs" },
          { status: "A", path: "docs/notes.md" },
        ],
      },
      {
        repo_id: "22222222-2222-2222-2222-222222222222",
        root_path: "docs",
        errors: [],
        dirty: [{ status: "M", path: "README.md" }],
      },
    ],
    ...overrides,
  };
}

function seedReport(data: Record<string, unknown>) {
  server.use(
    http.get(`/api/v1/workspaces/${WORKSPACE_ID}/changes`, () =>
      HttpResponse.json({ data }),
    ),
  );
}

describe("CommitChangesButton", () => {
  it("pushes the pending commits of a clean tree so a failed push can be retried", async () => {
    let pushed = 0;
    seedReport(
      report({
        members: [
          {
            repo_id: "r1",
            root_path: "code",
            errors: [],
            dirty: [],
            against_internal: { ahead: 2, behind: 0 },
          },
        ],
      }),
    );
    server.use(
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/changes/push`, () => {
        pushed += 1;
        return HttpResponse.json({
          data: report({
            members: [
              { repo_id: "r1", root_path: "code", errors: [], pushed: true },
            ],
          }),
        });
      }),
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/changes/commit`, () =>
        HttpResponse.json(
          { error: { message: "must not be called" } },
          { status: 500 },
        ),
      ),
    );
    renderApp(<CommitChangesButton workspaceId={WORKSPACE_ID} />);
    const button = await screen.findByTestId("workspace-commit");
    await waitFor(() => expect(button).toHaveTextContent("Push 2 commits"));
    expect(button).toBeEnabled();
    await userEvent.click(button);
    await waitFor(() => expect(pushed).toBe(1));
    expect(screen.queryByTestId("workspace-commit-dialog")).toBeNull();
    expect(
      await screen.findByTestId("workspace-commit-outcome"),
    ).toHaveTextContent("Pushed 2 commits");
  });

  it("is disabled while nothing is uncommitted", async () => {
    seedReport(
      report({
        members: [{ repo_id: "r1", root_path: "code", errors: [], dirty: [] }],
      }),
    );
    renderApp(<CommitChangesButton workspaceId={WORKSPACE_ID} />);
    const button = await screen.findByTestId("workspace-commit");
    await waitFor(() => expect(button).toBeDisabled());
    expect(button).toHaveAttribute("title", "Nothing to commit or push");
    expect(button).toHaveTextContent("Commit");
  });

  it("keeps an open dialog's Commit off once the workspace stops, and keeps the message (g5.workspace.ui.unavailable)", async () => {
    seedReport(report());
    function Host() {
      const [reason, setReason] = React.useState<string | undefined>();
      return (
        <>
          <button
            type="button"
            data-testid="stop-workspace"
            onClick={() => setReason("Start the workspace to commit or push.")}
          >
            stop
          </button>
          <CommitChangesButton
            workspaceId={WORKSPACE_ID}
            disabledReason={reason}
          />
        </>
      );
    }
    renderApp(<Host />);
    const user = userEvent.setup();
    const button = await screen.findByTestId("workspace-commit");
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    await user.type(screen.getByTestId("workspace-commit-message"), "wip");
    await user.click(screen.getByTestId("stop-workspace"));
    const submit = screen.getByTestId("workspace-commit-submit");
    expect(submit).toBeDisabled();
    expect(submit).toHaveAttribute(
      "title",
      "Start the workspace to commit or push.",
    );
    expect(screen.getByTestId("workspace-commit-blocked")).toHaveTextContent(
      "Start the workspace to commit or push.",
    );
    expect(screen.getByTestId("workspace-commit-message")).toHaveValue("wip");
  });

  it("is disabled while the workspace is not running", async () => {
    seedReport(report({ source: "durable", attached: false }));
    renderApp(<CommitChangesButton workspaceId={WORKSPACE_ID} />);
    const button = await screen.findByTestId("workspace-commit");
    await waitFor(() =>
      expect(button).toHaveAttribute("title", "Start the workspace to commit"),
    );
    expect(button).toBeDisabled();
  });

  it("commits with a message and reports what was committed", async () => {
    const bodies: unknown[] = [];
    seedReport(report());
    server.use(
      http.post(
        `/api/v1/workspaces/${WORKSPACE_ID}/changes/commit`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            data: report({
              members: [
                {
                  repo_id: "11111111-1111-1111-1111-111111111111",
                  committed: true,
                  head: "ab12cd34ef56",
                  files: 2,
                  pushed: true,
                },
                {
                  repo_id: "22222222-2222-2222-2222-222222222222",
                  committed: true,
                  head: "9876543210ab",
                  files: 1,
                  pushed: true,
                },
              ],
            }),
          });
        },
      ),
    );
    renderApp(
      <CommitChangesButton
        workspaceId={WORKSPACE_ID}
        defaultMessage="Apply review round 2"
      />,
    );

    const button = await screen.findByTestId("workspace-commit");
    await waitFor(() => expect(button).toBeEnabled());
    expect(button).toHaveTextContent("Commit 3 files");
    await userEvent.click(button);

    const dialog = await screen.findByTestId("workspace-commit-dialog");
    const message = within(dialog).getByTestId("workspace-commit-message");
    expect(message).toHaveValue("Apply review round 2");

    // A blank message never reaches the server.
    await userEvent.clear(message);
    await userEvent.click(
      within(dialog).getByTestId("workspace-commit-submit"),
    );
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "A commit needs a message.",
    );
    expect(bodies).toEqual([]);

    await userEvent.type(message, "Apply round 2: exit code and README");
    await userEvent.click(
      within(dialog).getByTestId("workspace-commit-submit"),
    );
    await waitFor(() =>
      expect(bodies).toEqual([
        { message: "Apply round 2: exit code and README" },
      ]),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("workspace-commit-dialog")).toBeNull(),
    );
    expect(screen.getByTestId("workspace-commit-outcome")).toHaveTextContent(
      "Committed 3 files as ab12cd3, 9876543",
    );
  });

  it("shows the server's refusal inside the dialog", async () => {
    seedReport(report());
    server.use(
      http.post(`/api/v1/workspaces/${WORKSPACE_ID}/changes/commit`, () =>
        HttpResponse.json(
          {
            error: {
              code: "WORKSPACE_NOT_RUNNING",
              message: "Start the workspace to commit its changes.",
            },
          },
          { status: 409 },
        ),
      ),
    );
    renderApp(<CommitChangesButton workspaceId={WORKSPACE_ID} />);
    const button = await screen.findByTestId("workspace-commit");
    await waitFor(() => expect(button).toBeEnabled());
    await userEvent.click(button);
    const dialog = await screen.findByTestId("workspace-commit-dialog");
    await userEvent.type(
      within(dialog).getByTestId("workspace-commit-message"),
      "Apply review",
    );
    await userEvent.click(
      within(dialog).getByTestId("workspace-commit-submit"),
    );
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Start the workspace to commit its changes.",
    );
    expect(screen.getByTestId("workspace-commit-dialog")).toBeInTheDocument();
  });
});
