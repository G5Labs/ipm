import { afterEach, beforeEach, expect, it } from "vitest";
import { act } from "@testing-library/react";
import { useLocation } from "react-router-dom";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import {
  WorkspaceFeedbackButton,
  WorkspaceThread,
} from "./WorkspaceDiscussion";

beforeEach(() => sessionStorage.clear());
const originalObserver = window.IntersectionObserver;
afterEach(() => {
  window.IntersectionObserver = originalObserver;
});

it.each(["feedback", "reply"])(
  "retains %s while disabling submission after a policy change",
  async (kind) => {
    let readOnly = false;
    let posted = false;
    const workspaceId = "workspace";
    server.use(
      http.get(`/api/v1/workspaces/${workspaceId}/review-flow`, () =>
        HttpResponse.json({ data: { workspace: { readOnly } } }),
      ),
      http.get("/api/v1/comments/thread/replies", () =>
        HttpResponse.json({ data: [] }),
      ),
      http.post("/api/v1/comments/thread/replies", () => {
        posted = true;
        return HttpResponse.json({ data: {} });
      }),
      http.post(`/api/v1/workspaces/${workspaceId}/comments`, () => {
        posted = true;
        return HttpResponse.json({ data: {} });
      }),
    );
    const app = renderApp(
      kind === "feedback" ? (
        <WorkspaceFeedbackButton workspaceId={workspaceId} />
      ) : (
        <WorkspaceThread
          comment={{
            id: "thread",
            workspaceId,
            authorId: "reviewer",
            authorName: "Reviewer",
            scope: "workspace",
            content: "Please clarify",
            createdAt: "2026-10-12T10:00:00Z",
            resolved: false,
          }}
        />
      ),
    );
    const user = userEvent.setup();
    const opener = await screen.findByRole("button", {
      name: kind === "feedback" ? "Add workspace comment" : "Reply",
    });
    await waitFor(() => expect(opener).toBeEnabled());
    await user.click(opener);
    const draft = screen.getByLabelText(
      kind === "feedback" ? "Comment" : "Write a reply",
    );
    await user.type(draft, "Preserve my words");
    readOnly = true;
    await app.queryClient.invalidateQueries({
      queryKey: ["workspace-review-flow", workspaceId],
    });
    const submit = screen.getByRole("button", {
      name: kind === "feedback" ? "Post comment" : "Post reply",
    });
    await waitFor(() => expect(submit).toBeDisabled());
    expect(draft).toHaveValue("Preserve my words");
    await user.click(submit);
    expect(posted).toBe(false);
  },
);

it("loads replies when a thread enters view and opens current-work anchors without claiming a pushed revision", async () => {
  let observe!: (entries: { isIntersecting: boolean }[]) => void;
  window.IntersectionObserver = class {
    constructor(callback: typeof observe) {
      observe = callback;
    }
    observe() {}
    disconnect() {}
  } as unknown as typeof IntersectionObserver;
  let reads = 0;
  server.use(
    http.get("/api/v1/workspaces/workspace/review-flow", () =>
      HttpResponse.json({ data: { workspace: { readOnly: false } } }),
    ),
    http.get("/api/v1/comments/current-thread/replies", () => {
      reads++;
      return HttpResponse.json({
        data: [
          { id: "reply", content: "Persisted response", authorName: "Owner" },
        ],
      });
    }),
  );
  function Surface() {
    const location = useLocation();
    return (
      <>
        <p data-testid="discussion-route">{location.search}</p>
        <WorkspaceThread
          comment={{
            id: "current-thread",
            workspaceId: "workspace",
            authorId: "owner",
            authorName: "Owner",
            scope: "line",
            content: "Current work",
            repoId: "code",
            filePath: "src/current.ts",
            lineNumber: 2,
            commitSha: null,
            createdAt: "2026-10-12T10:00:00Z",
            resolved: false,
          }}
        />
      </>
    );
  }
  renderApp(<Surface />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Reply" })).toBeEnabled(),
  );
  expect(reads).toBe(0);
  act(() => observe([{ isIntersecting: true }]));
  expect(await screen.findByText("Persisted response")).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: /View current file/ }),
  );
  expect(screen.getByTestId("discussion-route")).toHaveTextContent("mode=all");
  expect(screen.getByTestId("discussion-route")).not.toHaveTextContent(
    "comment=",
  );
});
