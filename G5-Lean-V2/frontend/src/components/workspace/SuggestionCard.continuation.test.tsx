import React from "react";
import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HttpResponse, http } from "msw";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { SuggestionCard } from "./ReviewsPanel";
import { ComposerOwner } from "../comments/ComposerOwner";
import { expandReviewSuggestionEdits } from "../../lib/reviewSuggestionEdits";
import { reviewEditFixture } from "../../test/reviewEdits";
import { getApiToken, setApiToken, tokenPrincipal } from "../../lib/api";
import type { ReviewSuggestion } from "../../lib/types";

const user = {
  id: "user-owner",
  email: "owner@example.test",
  displayName: "Owner",
  avatarUrl: null,
  orgs: [
    {
      orgId: "org-g5labs",
      orgName: "G5Labs",
      orgSlug: "g5labs",
      role: "admin" as const,
    },
  ],
  scopes: ["*"],
};

function child(parent: string): ReviewSuggestion {
  return expandReviewSuggestionEdits([
    {
      id: parent,
      publicationId: parent,
      findingId: null,
      repoId: "repo",
      path: "doc.md",
      kind: "hunk",
      beforeLines: "old",
      afterLines: "new",
      anchorStart: 1,
      contentHash: null,
      anchorContext: null,
      authorKind: "human",
      authorLabel: "reviewer",
      note: "",
      commentId: "parent-thread",
      status: "open",
      statusBy: null,
      statusAt: null,
      supersedes: null,
      adoptedFrom: null,
      createdAt: "2026-10-01T00:00:00Z",
      edits: [
        reviewEditFixture("shared-edit", "old", 0, 3, "new", {
          commentId: "child-thread",
        }),
      ],
    },
  ])[0];
}

function mountCards() {
  let setParents!: (parents: string[]) => void;
  function Host() {
    const [parents, set] = React.useState(["a"]);
    React.useEffect(() => {
      setParents = set;
    }, [set]);
    return (
      <ComposerOwner scope="ws-source-1">
        {parents.map((parent) => (
          <SuggestionCard
            key={parent}
            workspaceId="ws-source-1"
            suggestion={child(parent)}
            viewerOwnsWorkspace
            viewerIsReviewer={false}
            ownerVmRunning
            sourceWritable
          />
        ))}
      </ComposerOwner>
    );
  }
  const view = renderApp(<Host />, { user });
  return Object.assign(
    async (parents: string[]) => {
      await act(async () => setParents(parents));
    },
    { queryClient: view.queryClient },
  );
}

async function openReply(parent: string) {
  fireEvent.click(
    screen.getByTestId(
      `review-suggestion-replies-toggle-${parent}:shared-edit`,
    ),
  );
  return screen.findByTestId(
    `review-suggestion-reply-input-${parent}:shared-edit`,
  );
}

function holdReply(fail = false) {
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const requests: Array<{ parent: string; body: unknown }> = [];
  server.use(
    http.get(
      "*/api/v1/workspaces/ws-source-1/review/suggestions/:parent/replies",
      () => HttpResponse.json({ data: { replies: [] } }),
    ),
    http.post(
      "*/api/v1/workspaces/ws-source-1/review/suggestions/:parent/replies",
      async ({ params, request }) => {
        requests.push({
          parent: String(params.parent),
          body: await request.json(),
        });
        await held;
        return fail
          ? HttpResponse.json(
              {
                error: { code: "CONFLICT", message: "The reply was refused." },
              },
              { status: 409 },
            )
          : HttpResponse.json({ data: { replies: [] } });
      },
    ),
  );
  return { requests, release: () => release() };
}

async function sendFirst(input: HTMLElement, requests: unknown[]) {
  await waitFor(() =>
    expect(tokenPrincipal(getApiToken())).toContain("user-owner"),
  );
  fireEvent.change(input, { target: { value: "first reply" } });
  fireEvent.click(
    screen.getByTestId("review-suggestion-reply-send-a:shared-edit"),
  );
  await waitFor(() => expect(requests).toHaveLength(1));
  expect(
    screen.getByTestId("review-suggestion-reply-send-a:shared-edit"),
  ).toBeDisabled();
}

describe("SuggestionCard carried replies at actual mutation completion", () => {
  it.each([true, false])(
    "old completion=%s preserves newer continuation words for the next publication",
    async (completeOld) => {
      const held = holdReply();
      const setParents = mountCards();
      const oldInput = await openReply("a");
      await sendFirst(oldInput, held.requests);
      await setParents(["a", "b"]);
      const newerInput = await openReply("b");
      expect(newerInput).toHaveValue("first reply");
      expect(
        screen.getByTestId("review-suggestion-reply-send-b:shared-edit"),
      ).toBeEnabled();
      fireEvent.change(newerInput, { target: { value: "newer unsent draft" } });
      try {
        if (completeOld) {
          await act(async () => held.release());
          await waitFor(() => expect(oldInput).toHaveValue(""));
        }
        expect(newerInput).toHaveValue("newer unsent draft");
        await setParents(["c"]);
        const continued = await openReply("c");
        expect(continued).toHaveValue("newer unsent draft");
        expect(
          screen.getByTestId("review-suggestion-reply-send-c:shared-edit"),
        ).toBeEnabled();
        expect(held.requests).toEqual([
          {
            parent: "a",
            body: { content: "first reply", editId: "shared-edit" },
          },
        ]);
      } finally {
        await act(async () => held.release());
        await waitFor(() =>
          expect(
            setParents.queryClient.getMutationCache().getAll()[0].state.status,
          ).toBe("success"),
        );
      }
    },
  );

  it("keeps newer typing in the same display after successful completion and carries it onward", async () => {
    const held = holdReply();
    const setParents = mountCards();
    const input = await openReply("a");
    await sendFirst(input, held.requests);
    fireEvent.change(input, { target: { value: "same display newer words" } });
    await act(async () => held.release());
    await waitFor(() =>
      expect(
        screen.getByTestId("review-suggestion-reply-send-a:shared-edit"),
      ).toBeEnabled(),
    );
    expect(input).toHaveValue("same display newer words");
    await setParents(["c"]);
    expect(await openReply("c")).toHaveValue("same display newer words");
  });

  it("clears exactly the sent words from the display and the next continuation", async () => {
    const held = holdReply();
    const setParents = mountCards();
    const input = await openReply("a");
    await sendFirst(input, held.requests);
    await act(async () => held.release());
    await waitFor(() => expect(input).toHaveValue(""));
    await setParents(["c"]);
    expect(await openReply("c")).toHaveValue("");
  });

  it("retains failed words and a display-local error while newer continuation words carry onward", async () => {
    const held = holdReply(true);
    const setParents = mountCards();
    const oldInput = await openReply("a");
    await sendFirst(oldInput, held.requests);
    await setParents(["a", "b"]);
    const input = await openReply("b");
    fireEvent.change(input, { target: { value: "later unsent words" } });
    await act(async () => held.release());
    expect(
      await within(
        screen.getByTestId("review-suggestion-a:shared-edit"),
      ).findByText("The reply was refused."),
    ).toBeInTheDocument();
    expect(oldInput).toHaveValue("first reply");
    expect(
      screen.getByTestId("review-suggestion-b:shared-edit"),
    ).not.toHaveTextContent("The reply was refused.");
    await setParents(["c"]);
    expect(await openReply("c")).toHaveValue("later unsent words");
    expect(
      screen.getByTestId("review-suggestion-c:shared-edit"),
    ).not.toHaveTextContent("The reply was refused.");
  });

  it.each([
    { sub: "another-owner", org: "org-g5labs" },
    { sub: "user-owner", org: "another-org" },
  ])(
    "resets carried words for principal $sub/$org and isolates the old completion",
    async ({ sub, org }) => {
      const held = holdReply();
      const setParents = mountCards();
      const oldInput = await openReply("a");
      await sendFirst(oldInput, held.requests);
      const token = getApiToken();
      // Principal reset clears the cache; the admitted old mutation still settles.
      const oldMutation = setParents.queryClient.getMutationCache().getAll()[0];
      const encode = (value: object) =>
        btoa(JSON.stringify(value))
          .replace(/\+/g, "-")
          .replace(/\//g, "_")
          .replace(/=+$/, "");
      try {
        await act(async () =>
          setApiToken(
            `${encode({ alg: "HS256" })}.${encode({ sub, org, scopes: [] })}.changed`,
          ),
        );
        await setParents(["b"]);
        const next = await openReply("b");
        expect(next).toHaveValue("");
        fireEvent.change(next, { target: { value: "new principal words" } });
        await act(async () => held.release());
        await waitFor(() => expect(oldMutation.state.status).toBe("success"));
        await setParents(["c"]);
        expect(await openReply("c")).toHaveValue("new principal words");
      } finally {
        await act(async () => held.release());
        await act(async () => setApiToken(token));
      }
    },
  );
});
