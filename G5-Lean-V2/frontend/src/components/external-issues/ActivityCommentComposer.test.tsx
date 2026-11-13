/**
 * ActivityCommentComposer — comment on an issue as yourself, from a
 * workspace's Activity or from the issue page.
 *
 * Covered: review of the exact payload before posting, the request (body,
 * stable idempotency key, destination), the draft kept in this browser
 * across remounts and refusals, a comment GitHub did not confirm keeping
 * its draft and key so posting again is safe, blocked destinations that
 * still allow drafting, typed refusal messages (including detected secrets
 * without echoing them), and Connect GitHub returning to the current page.
 * A native G5 primary issue posts straight from the text box as "Post
 * comment", with no GitHub account, connection prompt or hidden line. On
 * the issue page the draft belongs to the person and issue, whichever
 * workspace they come from.
 */

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  CommentComposerDTO,
  CommentResultDTO,
  GithubCommentResultDTO,
} from "../../lib/types";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import {
  ActivityCommentComposer,
  IssuePageCommentComposer,
} from "./ActivityCommentComposer";

const WORKSPACE_ID = "ws-1";
const COMMENTS_URL = `/api/v1/workspaces/${WORKSPACE_ID}/activity/comments`;

function composer(
  overrides: Partial<CommentComposerDTO> = {},
): CommentComposerDTO {
  return {
    state: "ready",
    githubLogin: "ada-gh",
    destination: { trackerId: "tracker-1", externalIssueId: "9001" },
    displayKey: "acme/api#12",
    ...overrides,
  };
}

function publication(
  overrides: Partial<Record<keyof GithubCommentResultDTO, unknown>> = {},
): CommentResultDTO {
  return {
    id: "pub-1",
    provider: "github",
    state: "published",
    kind: "comment",
    origin: "person",
    destination: "issue",
    displayKey: "acme/api#12",
    githubLogin: "ada-gh",
    body: "Looks right.",
    payload: "Looks right.\n\n<!-- g5-comment pub-1 -->",
    externalCommentUrl: "https://github.com/acme/api/issues/12#issuecomment-1",
    error: null,
    createdAt: "2026-09-26T20:00:00Z",
    publishedAt: "2026-09-26T20:00:01Z",
    message: null,
    ...overrides,
  } as CommentResultDTO;
}

async function renderComposer(
  props: Partial<React.ComponentProps<typeof ActivityCommentComposer>> = {},
) {
  const view = renderApp(
    <ActivityCommentComposer
      workspaceId={WORKSPACE_ID}
      composer={composer()}
      {...props}
    />,
    { route: "/g5labs/current?view=activity", user: createMockUser() },
  );
  // The composer appears once the signed-in user is known.
  await screen.findByTestId("activity-comment-composer");
  return view;
}

beforeEach(() => {
  window.localStorage.clear();
});

describe("ActivityCommentComposer", () => {
  it("shows the exact payload before posting and posts with a stable key", async () => {
    const requests: Array<Record<string, unknown>> = [];
    server.use(
      http.post(COMMENTS_URL, async ({ request }) => {
        requests.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({ data: publication() }, { status: 201 });
      }),
    );
    await renderComposer();
    const user = userEvent.setup();

    expect(
      screen.getByText("Comment on acme/api#12 as @ada-gh"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("activity-comment-review-button")).toBeDisabled();
    await user.type(
      screen.getByTestId("activity-comment-input"),
      "Looks right.",
    );
    await user.click(screen.getByTestId("activity-comment-review-button"));

    const review = screen.getByTestId("activity-comment-review");
    expect(review).toHaveTextContent(
      "Looks right. <!-- g5-comment (reference id) -->",
    );
    expect(review).toHaveTextContent("posted to acme/api#12 as @ada-gh");
    await user.click(screen.getByTestId("activity-comment-post"));

    expect(
      await screen.findByTestId("activity-comment-posted"),
    ).toHaveTextContent("Posted to GitHub.");
    expect(requests).toHaveLength(1);
    expect(requests[0].body).toBe("Looks right.");
    expect(requests[0].destination).toEqual({
      trackerId: "tracker-1",
      externalIssueId: "9001",
    });
    expect(typeof requests[0].idempotencyKey).toBe("string");
    // The draft is gone once the submission is recorded.
    expect(screen.getByTestId("activity-comment-input")).toHaveValue("");
    expect(
      Object.keys(window.localStorage).filter((key) =>
        key.startsWith("g5:issue-comment-draft:"),
      ),
    ).toEqual([]);
  });

  it("posts straight from the text box for a trusted domain", async () => {
    const requests: Array<Record<string, unknown>> = [];
    server.use(
      http.post(COMMENTS_URL, async ({ request }) => {
        requests.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({ data: publication() }, { status: 201 });
      }),
    );
    await renderComposer({ trusted: true });
    const user = userEvent.setup();

    expect(
      screen.queryByTestId("activity-comment-review-button"),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/adds a hidden reference line/)).toBeVisible();
    await user.type(screen.getByTestId("activity-comment-input"), "Ship it.");
    await user.click(screen.getByTestId("activity-comment-post"));

    expect(
      await screen.findByTestId("activity-comment-posted"),
    ).toBeInTheDocument();
    expect(requests[0].body).toBe("Ship it.");
    expect(
      screen.queryByTestId("activity-comment-review"),
    ).not.toBeInTheDocument();
  });

  it("keeps the draft and its key across a refusal and a remount", async () => {
    const keys: unknown[] = [];
    server.use(
      http.post(COMMENTS_URL, async ({ request }) => {
        keys.push(
          ((await request.json()) as Record<string, unknown>).idempotencyKey,
        );
        return HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "the workspace's primary issue changed",
              details: { code: "destination_changed" },
              requestId: "",
            },
          },
          { status: 409 },
        );
      }),
    );
    const first = await renderComposer();
    const user = userEvent.setup();
    await user.type(screen.getByTestId("activity-comment-input"), "Draft text");
    await user.click(screen.getByTestId("activity-comment-review-button"));
    await user.click(screen.getByTestId("activity-comment-post"));
    expect(
      await screen.findByTestId("activity-comment-error"),
    ).toHaveTextContent("primary issue changed");
    await user.click(screen.getByTestId("activity-comment-post"));
    await waitFor(() => expect(keys).toHaveLength(2));
    expect(keys[1]).toBe(keys[0]);
    first.unmount();

    await renderComposer();
    expect(screen.getByTestId("activity-comment-input")).toHaveValue(
      "Draft text",
    );
  });

  it("allows drafting but not posting to blocked destinations", async () => {
    for (const [state, text] of [
      ["public_destination", "Read-only integration: public destination"],
      ["comments_disabled", "G5 comments are not enabled"],
      ["visibility_unknown", "did not report this repository's visibility"],
      ["unavailable", "GitHub is unavailable"],
    ] as const) {
      const view = await renderComposer({
        composer: composer({ state }),
      });
      const user = userEvent.setup();
      await user.type(screen.getByTestId("activity-comment-input"), "Draft");
      expect(screen.getByTestId("activity-comment-blocked")).toHaveTextContent(
        text,
      );
      expect(
        screen.getByTestId("activity-comment-review-button"),
      ).toBeDisabled();
      view.unmount();
      window.localStorage.clear();
    }
  });

  it("explains a detected secret without echoing it", async () => {
    const secret = `ghp_${"a".repeat(36)}`;
    server.use(
      http.post(COMMENTS_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "Validation failed",
              details: {
                field: "body",
                code: "secret_detected",
                kinds: ["GitHub token"],
              },
              requestId: "",
            },
          },
          { status: 422 },
        ),
      ),
    );
    await renderComposer();
    const user = userEvent.setup();
    await user.type(
      screen.getByTestId("activity-comment-input"),
      `token ${secret}`,
    );
    await user.click(screen.getByTestId("activity-comment-review-button"));
    await user.click(screen.getByTestId("activity-comment-post"));

    const error = await screen.findByTestId("activity-comment-error");
    expect(error).toHaveTextContent(
      "appears to contain a secret (GitHub token)",
    );
    expect(error).not.toHaveTextContent(secret);
  });

  it("opens the shared account connection and preserves the current page", async () => {
    const redirect = vi.fn();
    await renderComposer({
      composer: composer({ state: "connection_required", githubLogin: null }),
      redirect,
    });
    await userEvent.click(screen.getByTestId("activity-comment-connect"));
    expect(redirect).toHaveBeenCalledWith(
      `/account/github?returnTo=${encodeURIComponent(window.location.pathname + window.location.search)}`,
    );
  });

  it("keeps the draft and its key when GitHub does not confirm the comment", async () => {
    const keys: unknown[] = [];
    server.use(
      http.post(COMMENTS_URL, async ({ request }) => {
        keys.push(
          ((await request.json()) as Record<string, unknown>).idempotencyKey,
        );
        return HttpResponse.json({
          data: publication(
            keys.length === 1
              ? {
                  state: "outcome_unknown",
                  externalCommentUrl: null,
                  publishedAt: null,
                  message: "GitHub did not respond within 5 seconds",
                }
              : {},
          ),
        });
      }),
    );
    const first = await renderComposer({ trusted: true });
    const user = userEvent.setup();
    await user.type(screen.getByTestId("activity-comment-input"), "Ship it.");
    await user.click(screen.getByTestId("activity-comment-post"));

    expect(
      await screen.findByTestId("activity-comment-unconfirmed"),
    ).toHaveTextContent("posting again is safe");
    expect(
      screen.queryByTestId("activity-comment-posted"),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("activity-comment-input")).toHaveValue(
      "Ship it.",
    );
    // The draft survives a remount with the same key.
    first.unmount();
    await renderComposer({ trusted: true });
    expect(screen.getByTestId("activity-comment-input")).toHaveValue(
      "Ship it.",
    );
    await user.click(screen.getByTestId("activity-comment-post"));
    expect(
      await screen.findByTestId("activity-comment-posted"),
    ).toHaveTextContent("Posted to GitHub.");
    expect(keys).toHaveLength(2);
    expect(keys[1]).toBe(keys[0]);
    expect(screen.getByTestId("activity-comment-input")).toHaveValue("");
  });
});

describe("ActivityCommentComposer — native G5 issue", () => {
  const native = composer({
    provider: "g5",
    githubLogin: null,
    destination: { trackerId: "tracker-g5", externalIssueId: "712" },
    displayKey: "ACME-12",
  });

  it("posts straight from the text box without GitHub copy or a review step", async () => {
    const user = userEvent.setup();
    const requests: Array<Record<string, unknown>> = [];
    server.use(
      http.post(COMMENTS_URL, async ({ request }) => {
        requests.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(
          {
            data: publication({
              provider: "g5",
              id: "g5:41",
              displayKey: "ACME-12",
              githubLogin: null,
              payload: "Looks right.",
              externalCommentUrl:
                "http://localhost:3000/g5labs/issues/g5/12#comment-41",
            }),
          },
          { status: 201 },
        );
      }),
    );
    // Untrusted content would review a GitHub payload first; native never.
    await renderComposer({ composer: native, trusted: false });

    const composerSection = screen.getByTestId("activity-comment-composer");
    expect(composerSection).toHaveTextContent("Comment on ACME-12");
    expect(composerSection).not.toHaveTextContent("GitHub");
    expect(composerSection).not.toHaveTextContent("hidden reference");
    expect(
      screen.queryByTestId("activity-comment-review-button"),
    ).not.toBeInTheDocument();

    await user.type(
      screen.getByTestId("activity-comment-input"),
      "Looks right.",
    );
    const post = screen.getByTestId("activity-comment-post");
    expect(post).toHaveTextContent("Post comment");
    await user.click(post);

    await waitFor(() => expect(requests).toHaveLength(1));
    expect(requests[0]).toEqual({
      body: "Looks right.",
      idempotencyKey: expect.any(String),
      destination: { trackerId: "tracker-g5", externalIssueId: "712" },
    });
    expect(
      await screen.findByTestId("activity-comment-posted"),
    ).toHaveTextContent("Posted.");
    expect(screen.getByTestId("activity-comment-posted")).not.toHaveTextContent(
      "GitHub",
    );
  });

  it("needs no GitHub connection", async () => {
    // A native composer is ready without one; a stale state never prompts.
    await renderComposer({
      composer: { ...native, state: "connection_required" },
    });
    expect(
      screen.queryByTestId("activity-comment-connect"),
    ).not.toBeInTheDocument();
  });
});

it("adds Reply context to a saved destination draft without losing its text or duplicating the quote", async () => {
  const quote =
    "> [Jordan commented](<https://github.com/acme/api/issues/12#issuecomment-7>)\n>\n> Please check pending values.\n\n";
  let view = await renderComposer();
  await userEvent.type(
    screen.getByTestId("activity-comment-input"),
    "Existing unsent text.",
  );
  view.unmount();
  view = await renderComposer({ initialBody: quote, focusInput: true });
  const input = screen.getByTestId(
    "activity-comment-input",
  ) as HTMLTextAreaElement;
  expect(input.value).toBe("Existing unsent text.\n\n" + quote);
  await waitFor(() => expect(input).toHaveFocus());
  const draftKey = Object.keys(localStorage).find((key) =>
    key.startsWith("g5:issue-comment-draft:"),
  )!;
  const saved = localStorage.getItem(draftKey);
  view.unmount();
  await renderComposer({ initialBody: quote });
  expect(screen.getByTestId("activity-comment-input")).toHaveValue(input.value);
  expect(localStorage.getItem(draftKey)).toBe(saved);
  // GitHub's existing untrusted-destination review step remains mandatory.
  expect(screen.getByTestId("activity-comment-review-button")).toBeEnabled();
  expect(screen.queryByTestId("activity-comment-post")).toBeNull();
});

describe("IssuePageCommentComposer", () => {
  const PAGE_COMMENTS_URL =
    "/api/v1/orgs/org-g5labs/issues/github/11/12/comments";

  async function renderPageComposer(
    props: Partial<React.ComponentProps<typeof IssuePageCommentComposer>> = {},
  ) {
    const view = renderApp(
      <IssuePageCommentComposer
        orgId="org-g5labs"
        locator={{ repositoryId: "11", number: 12 }}
        composer={composer({ provider: "github" })}
        {...props}
      />,
      { route: "/g5labs/issues/github/11/12", user: createMockUser() },
    );
    await screen.findByTestId("activity-comment-composer");
    return view;
  }

  it("reviews the exact payload and posts body and key only", async () => {
    const requests: Array<Record<string, unknown>> = [];
    server.use(
      http.post(PAGE_COMMENTS_URL, async ({ request }) => {
        requests.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({ data: publication() }, { status: 201 });
      }),
    );
    await renderPageComposer();
    const user = userEvent.setup();
    await user.type(
      screen.getByTestId("activity-comment-input"),
      "Looks right.",
    );
    await user.click(screen.getByTestId("activity-comment-review-button"));
    expect(screen.getByTestId("activity-comment-review")).toHaveTextContent(
      "Looks right. <!-- g5-comment (reference id) -->",
    );
    await user.click(screen.getByTestId("activity-comment-post"));

    expect(
      await screen.findByTestId("activity-comment-posted"),
    ).toHaveTextContent("Posted to GitHub.");
    expect(requests).toEqual([
      { body: "Looks right.", idempotencyKey: expect.any(String) },
    ]);
  });

  it("keeps one draft per person and issue, apart from workspace drafts", async () => {
    const page = await renderPageComposer();
    await userEvent.type(
      screen.getByTestId("activity-comment-input"),
      "From the page",
    );
    page.unmount();

    const keys = Object.keys(window.localStorage).filter((key) =>
      key.startsWith("g5:issue-comment-draft:"),
    );
    expect(keys).toEqual([
      "g5:issue-comment-draft:user-alex:issue-page:tracker-1:9001",
    ]);
    // The workspace's Activity composer for the same issue has its own draft.
    const workspace = await renderComposer();
    expect(screen.getByTestId("activity-comment-input")).toHaveValue("");
    workspace.unmount();
    await renderPageComposer();
    expect(screen.getByTestId("activity-comment-input")).toHaveValue(
      "From the page",
    );
  });
});
