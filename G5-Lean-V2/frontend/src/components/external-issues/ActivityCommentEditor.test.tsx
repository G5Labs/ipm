/**
 * ActivityCommentEditor — edit your own comment on the primary issue.
 *
 * Covered: the editor opens on the text without G5's reference line, a
 * trusted domain saves directly while an untrusted one reviews first, the
 * request (comment id, body, key, destination, expected update time), a
 * comment changed on GitHub, and a save GitHub did not confirm, which
 * stays open and saves again. A native G5 comment saves through the native
 * route without a review step.
 */

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { describe, expect, it, vi } from "vitest";

import type {
  CommentComposerDTO,
  CommentResultDTO,
  GithubCommentResultDTO,
} from "../../lib/types";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { ActivityCommentEditor } from "./ActivityCommentEditor";

const WORKSPACE_ID = "ws-1";
const EDIT_URL = `/api/v1/workspaces/${WORKSPACE_ID}/activity/comments/github/7001`;
const REFERENCE = "<!-- g5-comment cbc40edc-c5ed-4f83-ba43-85069252d6b6 -->";

const composer: CommentComposerDTO = {
  state: "ready",
  githubLogin: "ada-gh",
  destination: { trackerId: "tracker-1", externalIssueId: "9001" },
  displayKey: "acme/api#12",
};

function saved(
  overrides: Partial<Record<keyof GithubCommentResultDTO, unknown>> = {},
): CommentResultDTO {
  return {
    id: "pub-e",
    provider: "github",
    state: "published",
    kind: "edit",
    origin: "person",
    destination: "issue",
    displayKey: "acme/api#12",
    githubLogin: "ada-gh",
    body: "New text",
    payload: `New text\n\n${REFERENCE}`,
    externalCommentUrl:
      "https://github.com/acme/api/issues/12#issuecomment-7001",
    error: null,
    createdAt: "2026-09-26T21:00:00Z",
    publishedAt: "2026-09-26T21:00:01Z",
    message: null,
    ...overrides,
  } as CommentResultDTO;
}

function renderEditor(trusted: boolean, onDone = vi.fn()) {
  renderApp(
    <ActivityCommentEditor
      workspaceId={WORKSPACE_ID}
      composer={composer}
      commentId="7001"
      body={`Old text\n\n${REFERENCE}`}
      updatedAt="2026-09-26T20:00:00Z"
      trusted={trusted}
      onDone={onDone}
    />,
    { user: createMockUser() },
  );
  return { onDone };
}

describe("ActivityCommentEditor", () => {
  it("saves a trusted domain's comment directly", async () => {
    const requests: Array<Record<string, unknown>> = [];
    server.use(
      http.patch(EDIT_URL, async ({ request }) => {
        requests.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({ data: saved() }, { status: 201 });
      }),
    );
    const { onDone } = renderEditor(true);
    const user = userEvent.setup();

    const input = screen.getByTestId("activity-comment-edit-input");
    expect(input).toHaveValue("Old text");
    await user.clear(input);
    await user.type(input, "New text");
    await user.click(screen.getByTestId("activity-comment-edit-save"));

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(requests).toHaveLength(1);
    expect(requests[0]).toMatchObject({
      body: "New text",
      destination: composer.destination,
      expectedUpdatedAt: "2026-09-26T20:00:00Z",
    });
    expect(typeof requests[0].idempotencyKey).toBe("string");
  });

  it("reviews the change first for an untrusted domain", async () => {
    server.use(
      http.patch(EDIT_URL, () =>
        HttpResponse.json({ data: saved() }, { status: 201 }),
      ),
    );
    const { onDone } = renderEditor(false);
    const user = userEvent.setup();

    expect(
      screen.queryByTestId("activity-comment-edit-save"),
    ).not.toBeInTheDocument();
    await user.click(screen.getByTestId("activity-comment-edit-review-button"));
    expect(
      screen.getByTestId("activity-comment-edit-review"),
    ).toHaveTextContent("keeps its hidden reference line");
    await user.click(screen.getByTestId("activity-comment-edit-save"));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("does not overwrite a comment changed on GitHub", async () => {
    server.use(
      http.patch(EDIT_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "the comment changed on GitHub",
              details: { code: "comment_changed" },
              requestId: "",
            },
          },
          { status: 409 },
        ),
      ),
    );
    const { onDone } = renderEditor(true);
    const user = userEvent.setup();

    await user.click(screen.getByTestId("activity-comment-edit-save"));
    expect(
      await screen.findByTestId("activity-comment-edit-error"),
    ).toHaveTextContent("changed on GitHub since you opened the editor");
    expect(onDone).not.toHaveBeenCalled();
  });

  it("stays open when GitHub does not confirm, and saves again", async () => {
    let calls = 0;
    server.use(
      http.patch(EDIT_URL, () => {
        calls += 1;
        return HttpResponse.json({
          data:
            calls === 1
              ? saved({ state: "outcome_unknown", publishedAt: null })
              : saved(),
        });
      }),
    );
    const { onDone } = renderEditor(true);
    const user = userEvent.setup();

    await user.click(screen.getByTestId("activity-comment-edit-save"));
    expect(
      await screen.findByTestId("activity-comment-edit-error"),
    ).toHaveTextContent(
      "GitHub did not confirm the change; saving again is safe.",
    );
    expect(onDone).not.toHaveBeenCalled();
    await user.click(screen.getByTestId("activity-comment-edit-save"));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(calls).toBe(2);
  });
});

describe("ActivityCommentEditor — native G5 comment", () => {
  it("saves through the native route without a review step", async () => {
    const user = userEvent.setup();
    const requests: Array<Record<string, unknown>> = [];
    server.use(
      http.patch(
        `/api/v1/workspaces/${WORKSPACE_ID}/activity/comments/g5/41`,
        async ({ request }) => {
          requests.push((await request.json()) as Record<string, unknown>);
          return HttpResponse.json({
            data: saved({
              provider: "g5",
              id: "g5:41",
              displayKey: "ACME-12",
              githubLogin: null,
              payload: "New text",
            }),
          });
        },
      ),
    );
    const onDone = vi.fn();
    renderApp(
      <ActivityCommentEditor
        workspaceId={WORKSPACE_ID}
        composer={{
          provider: "g5",
          state: "ready",
          githubLogin: null,
          destination: { trackerId: "tracker-g5", externalIssueId: "712" },
          displayKey: "ACME-12",
        }}
        commentId="41"
        body="Old text"
        updatedAt="2026-10-19T13:00:00.123456Z"
        trusted={false}
        onDone={onDone}
      />,
      { user: createMockUser() },
    );

    const input = screen.getByTestId("activity-comment-edit-input");
    await user.clear(input);
    await user.type(input, "New text");
    const save = screen.getByTestId("activity-comment-edit-save");
    expect(save).toHaveTextContent("Save");
    expect(save).not.toHaveTextContent("GitHub");
    await user.click(save);

    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(requests).toEqual([
      {
        body: "New text",
        idempotencyKey: expect.any(String),
        destination: { trackerId: "tracker-g5", externalIssueId: "712" },
        expectedUpdatedAt: "2026-10-19T13:00:00.123456Z",
      },
    ]);
  });
});
