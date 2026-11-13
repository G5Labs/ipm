import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useLayoutEffect, useState } from "react";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { createCommentAnchor } from "../../lib/commentAnchors";
import {
  WorkspaceReviewContent,
  WorkspaceReviewSidebar,
} from "./WorkspaceReviewContent";
import {
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import type {
  ReviewChanges,
  ReviewOverview,
} from "../../hooks/useWorkspaceReviewFlow";

const workspaceId = "workspace-review",
  base = `/api/v1/workspaces/${workspaceId}`;
const head = {
  workspaceId,
  repoId: "code",
  branchName: "review",
  headCommit: "a".repeat(40),
};
let overview: ReviewOverview,
  changes: ReviewChanges,
  posted: Record<string, unknown>[];
beforeEach(() => {
  sessionStorage.clear();
  posted = [];
  overview = {
    workspace: {
      ownerUserId: "another-owner",
      targetWorkspaceId: "parent",
      phase: "se",
      readOnly: false,
    },
    pr: null,
    heads: [head],
    baseHeads: [head],
    parentHeads: [head],
    pushedAt: {},
    repos: [{ id: "code", name: "Code", role: "code", rootPath: "code" }],
    requests: [],
    decisions: [],
  };
  changes = {
    beforeHeads: [head],
    afterHeads: [head],
    baselineAvailable: true,
    files: [],
    file: {
      repoId: "code",
      path: "review.txt",
      before: "before\n",
      after: "reviewed line\n",
      beforeCommit: "b".repeat(40),
      afterCommit: head.headCommit,
    },
  };
  server.use(
    http.get(`${base}/review-flow`, () =>
      HttpResponse.json({ data: overview }),
    ),
    http.get(`${base}/review-flow/changes`, () =>
      HttpResponse.json({ data: changes }),
    ),
    http.get(`${base}/comments`, () => HttpResponse.json({ data: [] })),
    http.post(`${base}/comments`, async ({ request }) => {
      posted.push((await request.json()) as Record<string, unknown>);
      return HttpResponse.json({ data: { id: "comment" } }, { status: 201 });
    }),
  );
});
afterEach(() => vi.unstubAllGlobals());
function mount(
  search = "?view=source-control&mode=base&reviewRepo=code&reviewPath=review.txt",
) {
  return renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={<WorkspaceReviewContent workspaceId={workspaceId} />}
      />
    </Routes>,
    { route: `/g5labs/review${search}` },
  );
}

it("lets a reader comment on exact pushed diff bytes over plain HTTP", async () => {
  const random = globalThis.crypto.getRandomValues.bind(globalThis.crypto);
  vi.stubGlobal("crypto", { getRandomValues: random });
  const anchor = await createCommentAnchor("reviewed line\n", 1);
  mount();
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Comment on added line 1" }),
  );
  await user.type(screen.getByLabelText("Comment"), "Explain this change");
  await user.click(screen.getByRole("button", { name: "Add comment" }));
  await waitFor(() => expect(posted).toHaveLength(1));
  expect(posted[0]).toMatchObject({
    ...anchor,
    repoId: "code",
    filePath: "review.txt",
    commitSha: head.headCommit,
    content: "Explain this change",
  });
  expect(posted[0].id).toMatch(/^[0-9a-f-]{36}$/);
});

it("retains a failed line comment and reuses its identity on retry", async () => {
  let fail = true;
  server.use(
    http.post(`${base}/comments`, async ({ request }) => {
      posted.push((await request.json()) as Record<string, unknown>);
      return fail
        ? HttpResponse.json(
            { error: { message: "Save failed" } },
            { status: 503 },
          )
        : HttpResponse.json({ data: { id: "comment" } }, { status: 201 });
    }),
  );
  mount();
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Comment on added line 1" }),
  );
  await user.type(screen.getByLabelText("Comment"), "Please clarify");
  await user.click(screen.getByRole("button", { name: "Add comment" }));
  expect(await screen.findByRole("alert")).toBeVisible();
  expect(screen.getByLabelText("Comment")).toHaveValue("Please clarify");
  fail = false;
  await user.click(screen.getByRole("button", { name: "Add comment" }));
  await waitFor(() => expect(posted).toHaveLength(2));
  expect(posted[0]).toEqual(posted[1]);
});

it("starts a fresh comment identity when another comparison has corrupt stored draft data", async () => {
  server.use(
    http.post(`${base}/comments`, async ({ request }) => {
      posted.push((await request.json()) as Record<string, unknown>);
      return HttpResponse.json(
        { error: { message: "Save failed" } },
        { status: 503 },
      );
    }),
  );
  function Harness() {
    const navigate = useNavigate();
    return (
      <>
        <button
          onClick={() =>
            navigate(
              "?view=source-control&mode=parent&reviewRepo=code&reviewPath=review.txt",
            )
          }
        >
          Compare parent
        </button>
        <WorkspaceReviewContent workspaceId={workspaceId} />
      </>
    );
  }
  renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<Harness />} />
    </Routes>,
    {
      route:
        "/g5labs/review?view=source-control&mode=base&reviewRepo=code&reviewPath=review.txt",
    },
  );
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Comment on added line 1" }),
  );
  await user.type(screen.getByLabelText("Comment"), "First comparison");
  await user.click(screen.getByRole("button", { name: "Add comment" }));
  await screen.findByText("Save failed");
  const key = Object.keys(sessionStorage).find((key) =>
    key.startsWith("g5:line-review:"),
  )!;
  expect(key).toBeTruthy();
  const corruptKey = key.replace("mode=base", "mode=parent");
  sessionStorage.setItem(corruptKey, "{");
  sessionStorage.setItem(corruptKey.replace(head.headCommit, "current"), "{");
  await user.click(screen.getByRole("button", { name: "Compare parent" }));
  await waitFor(() =>
    expect(screen.queryByLabelText("Comment")).not.toBeInTheDocument(),
  );
  await user.click(
    await screen.findByRole("button", { name: "Comment on added line 1" }),
  );
  await user.type(screen.getByLabelText("Comment"), "Second comparison");
  await user.click(screen.getByRole("button", { name: "Add comment" }));
  await waitFor(() => expect(posted).toHaveLength(2));
  expect(posted[1].id).not.toEqual(posted[0].id);
});

it("never exposes the previous draft to an immediate submit on a cached file comparison", async () => {
  server.use(
    http.get(`${base}/review-flow/changes`, ({ request }) =>
      HttpResponse.json({
        data:
          new URL(request.url).searchParams.get("path") === "other.txt"
            ? { ...changes, file: { ...changes.file, path: "other.txt" } }
            : changes,
      }),
    ),
  );
  let staleFormVisible = false;
  function Harness() {
    const navigate = useNavigate();
    const location = useLocation();
    // Observe the newly committed DOM before passive draft hydration. This
    // models submitting as soon as a cached comparison becomes available.
    useLayoutEffect(() => {
      if (!location.search.includes("other.txt")) return;
      const form = document.querySelector(
        "[data-testid=workspace-review-diff] form",
      );
      staleFormVisible = !!form;
      if (form) fireEvent.submit(form);
    }, [location.search]);
    return (
      <>
        <button
          onClick={() =>
            navigate(
              "?view=source-control&mode=base&reviewRepo=code&reviewPath=other.txt",
            )
          }
        >
          Compare another file
        </button>
        <WorkspaceReviewContent workspaceId={workspaceId} />
      </>
    );
  }
  const app = renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<Harness />} />
    </Routes>,
    {
      route:
        "/g5labs/review?view=source-control&mode=base&reviewRepo=code&reviewPath=review.txt",
    },
  );
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Comment on added line 1" }),
  );
  await user.type(
    screen.getByLabelText("Comment"),
    "This belongs to the first file",
  );
  const cached = app.queryClient
    .getQueryCache()
    .findAll({ queryKey: ["workspace-review-diff", workspaceId] })
    .find(
      (q) =>
        q.getObserversCount() > 0 &&
        q.state.data &&
        String(q.queryKey[2]).includes("path=review.txt"),
    )!;
  const nextKey = [...cached.queryKey];
  nextKey[2] = String(nextKey[2]).replace("path=review.txt", "path=other.txt");
  app.queryClient.setQueryData(nextKey, {
    ...changes,
    file: { ...changes.file, path: "other.txt" },
  });
  await user.click(
    screen.getByRole("button", { name: "Compare another file" }),
  );
  expect(
    await screen.findByRole("heading", { name: "other.txt" }),
  ).toBeVisible();
  expect(staleFormVisible).toBe(false);
  expect(posted).toHaveLength(0);
  expect(screen.queryByLabelText("Comment")).not.toBeInTheDocument();
});

it("preserves a replacement draft when an earlier send finishes after navigation", async () => {
  let refreshReads = 0;
  server.use(
    http.get(`${base}/review-flow`, () => {
      refreshReads += 1;
      return HttpResponse.json({ data: overview });
    }),
  );
  let finish!: () => void;
  server.use(
    http.post(`${base}/comments`, async ({ request }) => {
      posted.push((await request.json()) as Record<string, unknown>);
      await new Promise<void>((resolve) => {
        finish = resolve;
      });
      return HttpResponse.json({ data: { id: "comment" } }, { status: 201 });
    }),
  );
  function Harness() {
    const navigate = useNavigate();
    return (
      <>
        <button
          onClick={() =>
            navigate(
              "?view=source-control&mode=parent&reviewRepo=code&reviewPath=review.txt",
            )
          }
        >
          Compare parent
        </button>
        <button
          onClick={() =>
            navigate(
              "?view=source-control&mode=base&reviewRepo=code&reviewPath=review.txt",
            )
          }
        >
          Compare base
        </button>
        <WorkspaceReviewContent workspaceId={workspaceId} />
      </>
    );
  }
  const app = renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<Harness />} />
    </Routes>,
    {
      route:
        "/g5labs/review?view=source-control&mode=base&reviewRepo=code&reviewPath=review.txt",
    },
  );
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Comment on added line 1" }),
  );
  await user.type(screen.getByLabelText("Comment"), "Earlier thread");
  await user.click(screen.getByRole("button", { name: "Add comment" }));
  await waitFor(() => expect(posted).toHaveLength(1));
  await user.click(screen.getByRole("button", { name: "Compare parent" }));
  await waitFor(() =>
    expect(screen.queryByLabelText("Comment")).not.toBeInTheDocument(),
  );
  await user.click(screen.getByRole("button", { name: "Compare base" }));
  await screen.findByLabelText("Comment");
  await user.click(
    screen.getByRole("button", { name: "Comment on added line 1" }),
  );
  await user.clear(screen.getByLabelText("Comment"));
  await user.type(screen.getByLabelText("Comment"), "Replacement thread");
  const key = Object.keys(sessionStorage).find(
    (key) => key.startsWith("g5:line-review:") && key.includes("mode=base"),
  )!;
  const replacement = sessionStorage.getItem(key);
  expect(JSON.parse(replacement!).id).not.toEqual(posted[0].id);
  const beforeFinish = refreshReads;
  finish();
  await waitFor(() => expect(refreshReads).toBeGreaterThan(beforeFinish));
  await waitFor(() => expect(app.queryClient.isFetching()).toBe(0));
  expect(screen.getByLabelText("Comment")).toHaveValue("Replacement thread");
  expect(screen.getByRole("button", { name: "Add comment" })).toBeEnabled();
  expect(sessionStorage.getItem(key)).toEqual(replacement);
});

it("explains binary content without fabricating a deleted text file", async () => {
  changes.file = { ...changes.file!, binary: true, after: null };
  mount();
  expect(await screen.findByRole("status")).toHaveTextContent(/Loading|Binary/);
  expect(
    await screen.findByText(/A text diff and line comments are unavailable/),
  ).toBeVisible();
  expect(screen.queryByText("File deleted")).toBeNull();
  expect(screen.queryByRole("button", { name: /Comment on/ })).toBeNull();
});

it("keeps ordinary document browsing independent of comparison URL scopes", () => {
  renderApp(
    <WorkspaceReviewSidebar workspaceId={workspaceId} section="docs">
      <button>Managed document</button>
    </WorkspaceReviewSidebar>,
    { route: "/g5labs/review?mode=base" },
  );
  expect(
    screen.getByRole("button", { name: "Managed document" }),
  ).toBeVisible();
  expect(screen.queryByRole("combobox", { name: "File view" })).toBeNull();
});

it("keeps a draft unsent when writable policy changes", async () => {
  const app = mount();
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Comment on added line 1" }),
  );
  await user.type(screen.getByLabelText("Comment"), "Retain this draft");
  overview.workspace.readOnly = true;
  await app.queryClient.invalidateQueries({
    queryKey: ["workspace-review-flow", workspaceId],
  });
  expect(screen.getByLabelText("Comment")).toHaveValue("Retain this draft");
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Add comment" })).toBeDisabled(),
  );
  expect(posted).toHaveLength(0);
});

function reviewGate(running: boolean, owner = true): WorkspaceLiveGate {
  return {
    kind: !owner ? "reader" : running ? "live" : "stopped",
    canWrite: owner,
    live: running && owner,
    canStart: owner && !running,
    reviewCopy: false,
    reviewIdentityKnown: true,
    owner,
    workspaceNoun: "the workspace",
  };
}

it.each([
  ["running owner", true, true],
  ["stopped writable owner", false, true],
  ["reader of a running workspace", true, false],
] as const)(
  "shows the durable and permitted current views for a %s",
  async (_, running, owner) => {
    overview.workspace.ownerUserId = owner ? "user-alex" : "another-owner";
    renderApp(
      <WorkspaceLiveGateProvider value={reviewGate(running, owner)}>
        <WorkspaceReviewSidebar workspaceId={workspaceId} section="files">
          <button>Full authorized file tree</button>
        </WorkspaceReviewSidebar>
      </WorkspaceLiveGateProvider>,
    );
    expect(
      screen.getByRole("button", { name: "Full authorized file tree" }),
    ).toBeVisible();
    expect(screen.queryByRole("combobox", { name: "File view" })).toBeNull();
  },
);

it.each(["all", "unpushed"])(
  "replaces cached current content with pushed bytes when stopping in %s",
  async (mode) => {
    overview.workspace.ownerUserId = "user-alex";
    let running = true;
    const requestedModes: string[] = [];
    server.use(
      http.get(`${base}/review-flow/changes`, ({ request }) => {
        requestedModes.push(new URL(request.url).searchParams.get("mode")!);
        return HttpResponse.json({
          data: {
            ...changes,
            file: {
              ...changes.file,
              after: running
                ? "PRIVATE DIRTY CONTENT\n"
                : "PUBLISHED CONTENT\n",
            },
          },
        });
      }),
    );
    function StopWorkspace() {
      const [live, setLive] = useState(true);
      return (
        <WorkspaceLiveGateProvider value={reviewGate(live)}>
          <button
            onClick={() => {
              running = false;
              setLive(false);
            }}
          >
            Stop fixture
          </button>
          <WorkspaceReviewSidebar workspaceId={workspaceId} section="files" />
          <WorkspaceReviewContent workspaceId={workspaceId} />
        </WorkspaceLiveGateProvider>
      );
    }
    renderApp(<StopWorkspace />, {
      route: `/g5labs/review?view=source-control&mode=${mode}&reviewRepo=code&reviewPath=review.txt`,
    });
    expect(await screen.findByText("PRIVATE DIRTY CONTENT")).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Stop fixture" }));
    expect(await screen.findByText("PUBLISHED CONTENT")).toBeVisible();
    expect(screen.queryByText("PRIVATE DIRTY CONTENT")).toBeNull();
    expect(screen.queryByRole("combobox", { name: "File view" })).toBeNull();
    expect(requestedModes.at(-1)).toBe("all");
  },
);
