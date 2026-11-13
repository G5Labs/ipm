// =============================================================================
// CreateProjectDocModal — the open dialog follows the workspace's live gate
// (`g5.workspace.ui.unavailable`): a stop after the dialog opened disables
// submission with the state's reason and Start, keeps the draft, and sends no
// request.
// =============================================================================

import React from "react";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { WorkspaceLiveGateProvider } from "../../lib/workspaceLiveGate";
import { CreateProjectDocModal } from "./CreateProjectDocModal";

const gate = (live: boolean) => ({
  kind: live ? ("live" as const) : ("stopped" as const),
  canWrite: true,
  live,
  canStart: !live,
  reviewCopy: false,
  reviewIdentityKnown: true,
  owner: true,
  workspaceNoun: "the workspace",
});

function Host({ onClose }: { onClose: () => void }) {
  const [live, setLive] = React.useState(true);
  return (
    <WorkspaceLiveGateProvider value={gate(live)}>
      <button
        type="button"
        data-testid="stop-workspace"
        onClick={() => setLive(false)}
      >
        stop
      </button>
      <CreateProjectDocModal open workspaceId="ws-1" onClose={onClose} />
    </WorkspaceLiveGateProvider>
  );
}

describe("CreateProjectDocModal live gate", () => {
  it("disables Create Doc with the state's reason and Start once the workspace stops, keeping the draft and sending nothing", async () => {
    const created: unknown[] = [];
    server.use(
      http.post("/api/v1/workspaces/:workspaceId/docs", async ({ request }) => {
        created.push(await request.json());
        return HttpResponse.json({ data: {} }, { status: 500 });
      }),
    );
    const onClose = vi.fn();
    const user = userEvent.setup();
    renderApp(<Host onClose={onClose} />, { route: "/org/ws" });

    const nameInput = screen.getByPlaceholderText("e.g., auth-review");
    fireEvent.change(nameInput, { target: { value: "auth-review" } });
    await waitFor(() => expect(nameInput).toHaveValue("auth-review"));
    const submit = screen.getByTestId("create-doc-submit");
    expect(submit).toBeEnabled();
    expect(screen.queryByTestId("create-doc-start-hint")).toBeNull();

    await user.click(screen.getByTestId("stop-workspace"));

    await waitFor(() => expect(submit).toBeDisabled());
    expect(submit).toHaveAttribute(
      "title",
      "Start the workspace to create documents.",
    );
    expect(screen.queryByTestId("create-doc-start-hint")).toBeNull();
    fireEvent.click(screen.getByTestId("create-doc-start-hint-trigger"));
    expect(screen.getByTestId("create-doc-start-hint")).toHaveTextContent(
      "Start the workspace to create documents.",
    );
    expect(screen.getByTestId("create-doc-start-hint-start")).toBeEnabled();
    expect(nameInput).toHaveValue("auth-review");
    fireEvent.click(submit);
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(created).toHaveLength(0);
    expect(onClose).not.toHaveBeenCalled();
  });
});
