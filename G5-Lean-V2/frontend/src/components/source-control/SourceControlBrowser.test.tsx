import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { useAuth } from "../../hooks/useAuth";
import { http, HttpResponse } from "msw";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import {
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  SourceControlProvider,
  SourceControlMenu,
  SourceControlDiffWorkspace,
} from "./SourceControlBrowser";
import type {
  SourceMenu,
  SourceSnapshot,
} from "../../lib/sourceControlSelection";
const id = "source-workspace",
  base = `/api/v1/workspaces/${id}`;
const commit = (sha: string) => ({
  sha,
  summary: `Commit ${sha}`,
  authorName: "Real author",
  committedAt: 1_700_000_000,
  parents: ["base"],
});
let menu: SourceMenu,
  snapshot: SourceSnapshot,
  checks: number,
  loads: number,
  applies: unknown[],
  generation: number;
const originalIntersectionObserver = globalThis.IntersectionObserver;
function gate(running: boolean): WorkspaceLiveGate {
  return {
    kind: running ? "live" : "stopped",
    canWrite: true,
    live: running,
    canStart: !running,
    reviewCopy: false,
    reviewIdentityKnown: true,
    owner: true,
    workspaceNoun: "the workspace",
  };
}
beforeEach(() => {
  sessionStorage.clear();
  globalThis.IntersectionObserver =
    undefined as unknown as typeof IntersectionObserver;
  Element.prototype.scrollIntoView = vi.fn();
  checks = loads = generation = 0;
  applies = [];
  menu = {
    fingerprint: "initial",
    private: true,
    repos: ["Code", "Docs"].map((name) => ({
      repoId: name,
      name,
      base: "base",
      pushed: "pushed",
      commits: [commit("one"), commit("two")],
      truncated: false,
      error: null,
      live: { selectable: true },
    })),
  };
  snapshot = {
    fileCount: 2,
    private: true,
    patches: ["a.txt", "b.txt"].map((path) => ({
      kind: "code",
      repoId: "Code",
      repoName: "Code",
      label: "All changes vs workspace base",
      before: "base",
      after: null,
      file: {
        path,
        oldPath: null,
        change: "modified",
        diff: {
          status: "text",
          additions: 1,
          deletions: 0,
          hunks: [
            {
              startLine: 1,
              header: "@@ -0,0 +1 @@",
              lines: [
                { type: "addition", newLine: 1, content: `APPLIED ${path}` },
              ],
            },
          ],
        },
      },
    })),
  };
  server.use(
    http.get(`${base}/source-control/checkpoint`, () =>
      HttpResponse.json({ data: { generation } }),
    ),
    http.get(`${base}/review-flow`, () =>
      HttpResponse.json({
        data: {
          saveGeneration: generation,
          workspace: { ownerUserId: "user-alex", readOnly: false },
          heads: [],
          repos: [],
          requests: [],
          decisions: [],
        },
      }),
    ),
    http.get(`${base}/source-control/menu`, () => {
      loads++;
      return HttpResponse.json({ data: menu });
    }),
    http.post(`${base}/source-control/check`, async ({ request }) => {
      checks++;
      const old = (await request.json()) as { fingerprint: string };
      return HttpResponse.json({
        data:
          old.fingerprint === menu.fingerprint
            ? { changed: false }
            : { changed: true, menu },
      });
    }),
    http.post(`${base}/source-control/pushed`, () =>
      HttpResponse.json({ data: { ...snapshot, private: false } }),
    ),
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      const input = (await request.json()) as { fingerprint?: string };
      applies.push(input);
      return HttpResponse.json({
        data:
          input.fingerprint && input.fingerprint !== menu.fingerprint
            ? { refreshed: true, menu }
            : snapshot,
      });
    }),
  );
});
afterEach(() => {
  globalThis.IntersectionObserver = originalIntersectionObserver;
});
function App() {
  const { user } = useAuth();
  const [running, setRunning] = useState(true);
  const [runtimeKey, setRuntimeKey] = useState("first-runtime");
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <WorkspaceLiveGateProvider value={{ ...gate(running), runtimeKey }}>
      <SourceControlProvider workspaceId={id}>
        <output data-testid="source-control-test-actor">{user?.id}</output>
        <button
          onClick={() => {
            menu = {
              ...menu,
              private: false,
              repos: menu.repos.map((r) => ({ ...r, live: null })),
            };
            setRunning(false);
          }}
        >
          Stop fixture
        </button>
        <button onClick={() => setRuntimeKey("replacement-runtime")}>
          Replace runtime
        </button>
        <button onClick={() => navigate("/g5/workspace?view=source-control")}>
          Return to changes
        </button>
        <button
          onClick={() =>
            navigate("/g5/workspace?view=source-control", {
              state: { sourceControlSelection: "all" },
            })
          }
        >
          Open pushed comparison
        </button>
        <button
          onClick={() =>
            navigate("/g5/workspace?view=source-control", {
              state: { sourceControlSelection: "all" },
            })
          }
        >
          Open empty pushed comparison
        </button>
        <button
          onClick={() =>
            navigate("/g5/workspace?view=source-control", {
              state: { sourceControlSelection: "all" },
            })
          }
        >
          Open current pushed comparison
        </button>
        <output data-testid="entry-route" data-location-key={location.key}>
          {location.search}:{JSON.stringify(location.state)}
        </output>
        <button
          onClick={() =>
            navigate("/g5/workspace?view=source-control", {
              state: { sourceControlSelection: "code" },
            })
          }
        >
          Open Code comparison
        </button>
        <SourceControlMenu />
        <Routes>
          <Route
            path="/g5/workspace"
            element={<SourceControlDiffWorkspace />}
          />
          <Route
            path="/g5/workspace/file/*"
            element={<p>Ordinary file reader {location.search}</p>}
          />
        </Routes>
      </SourceControlProvider>
    </WorkspaceLiveGateProvider>
  );
}
async function mount() {
  const result = renderApp(<App />, { route: "/g5/workspace" });
  await waitFor(() =>
    expect(screen.getByTestId("source-control-test-actor")).toHaveTextContent(
      "user-alex",
    ),
  );
  await screen.findAllByText("Commit one");
  return result;
}
/** Save changes runs through the agent; the menu sees only its checkpoint event. */
async function saveCheckpoint(ws: Awaited<ReturnType<typeof mount>>["ws"]) {
  await act(async () => {
    ws.emit({
      channel: `workspace:${id}`,
      event: "workspace.source_control_saved",
      payload: { workspaceId: id, generation: ++generation },
    });
  });
}
it("applies personal unpushed work without inventing a fork baseline or commit count", async () => {
  menu.repos = [
    {
      ...menu.repos[0],
      repoId: "Personal",
      name: "Personal",
      base: null,
      unpushedOnly: true,
      commits: [],
    },
  ];
  snapshot.patches = snapshot.patches.slice(0, 1).map((patch) => ({
    ...patch,
    repoId: "Personal",
    repoName: "Personal",
    label: "Not yet pushed",
    before: "pushed",
  }));
  snapshot.fileCount = 1;
  renderApp(<App />, { route: "/g5/workspace" });
  expect(
    await screen.findByLabelText("Not yet pushed in Personal"),
  ).toBeChecked();
  expect(screen.queryByText("0 commits")).not.toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole("button", { name: "Show" }));
  expect(applies).toEqual([
    {
      fingerprint: menu.fingerprint,
      repos: [
        {
          repoId: "Personal",
          base: null,
          pushed: "pushed",
          commits: [],
          allPushed: false,
          includeUnpushed: true,
        },
      ],
    },
  ]);
  expect(
    await screen.findByRole("heading", { name: /^Not yet pushed/ }),
  ).toBeVisible();
  expect(
    screen.queryByText(/Pushed files →|Saved workspace base →/),
  ).toBeNull();
});

it("reopens a captured pushed scope after a checklist apply and clears pending warnings on Show", async () => {
  categoryFixture();
  let pushedReads = 0;
  server.use(
    http.post(`${base}/source-control/pushed`, () => {
      pushedReads++;
      return HttpResponse.json({ data: { ...snapshot, private: false } });
    }),
  );
  await mount();
  const user = userEvent.setup();
  await user.click(
    screen.getByRole("button", { name: "Open pushed comparison" }),
  );
  await screen.findByText(
    "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
  );
  await user.click(screen.getByRole("checkbox", { name: "Code" }));
  await screen.findByText(
    "Selection changed. Showing the previous selection until you click Show.",
  );
  await user.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText(
    "Ontology 1 · Docs 1 — 2 matching of 4 changed files",
  );
  expect(
    screen.queryByText(
      "Selection changed. Showing the previous selection until you click Show.",
    ),
  ).toBeNull();
  await user.click(
    screen.getByRole("button", { name: "Open pushed comparison" }),
  );
  await waitFor(() => expect(pushedReads).toBe(2));
  await screen.findByText(
    "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
  );
  const all = screen.getByRole("checkbox", { name: "All changes" });
  expect(all).toBePartiallyChecked();
  await user.click(all);
  expect(screen.getByRole("checkbox", { name: "Code" })).toBeChecked();
  await user.click(all);
  expect(screen.getByRole("button", { name: "Show" })).toBeDisabled();
});
it("selects repositories with visible unchecked children, mixed state and independent unpushed items", async () => {
  await mount();
  const user = userEvent.setup();
  const heading = screen.getByLabelText("All changes in Code");
  const global = screen.getByRole("checkbox", {
    name: "All changes",
  });
  expect(global).toBeChecked();
  expect(heading).toBeChecked();
  await user.click(heading);
  expect(global).toBePartiallyChecked();
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(screen.getByLabelText("one in Code")).toBeEnabled();
  expect(screen.getByLabelText("Not yet pushed in Docs")).toBeChecked();
  await user.click(screen.getByLabelText("two in Code"));
  expect(heading).toBePartiallyChecked();
  await user.click(heading);
  expect(heading).toBeChecked();
  await user.click(global);
  expect(global).not.toBeChecked();
  expect(screen.getByRole("button", { name: "Show" })).toBeDisabled();
  await user.click(screen.getByLabelText("Not yet pushed in Docs"));
  await user.click(screen.getByRole("checkbox", { name: "Docs" }));
  expect(screen.getByRole("button", { name: "Show" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Show" }));
  expect(applies[0]).toMatchObject({
    repos: [{ repoId: "Docs", includeUnpushed: true, commits: [] }],
  });
});
it("includes file types in All changes and applies both panes only on Show", async () => {
  categoryFixture();
  await mount();
  const user = userEvent.setup();
  const control = within(screen.getByTestId("source-control-menu"));
  const all = control.getByRole("checkbox", { name: "All changes" });
  const show = control.getByRole("button", { name: "Show" });
  const code = control.getByRole("checkbox", { name: "Code" });
  const expectFiles = (count: number) => {
    expect(screen.queryAllByTestId("source-control-file")).toHaveLength(count);
    expect(
      screen
        .getByTestId("source-control-diff-stream")
        .querySelectorAll("article"),
    ).toHaveLength(count);
  };
  await user.click(show);
  expectFiles(4);
  await user.click(code);
  expect(all).toBePartiallyChecked();
  expectFiles(4);
  await user.click(show);
  expectFiles(2);
  await user.click(all);
  expect(all).toBeChecked();
  expect(code).toBeChecked();
  expectFiles(2);
  await user.click(show);
  expectFiles(4);
  expect(applies).toHaveLength(3);

  for (const child of ["Code", "one in Code"]) {
    await user.click(all);
    for (const checkbox of control.getAllByRole("checkbox")) {
      expect(checkbox).not.toBeChecked();
      expect(checkbox).not.toBePartiallyChecked();
    }
    expect(show).toBeDisabled();
    expectFiles(4);
    await user.click(control.getByRole("checkbox", { name: child }));
    expect(all).toBePartiallyChecked();
    await user.click(all);
    for (const checkbox of control.getAllByRole("checkbox"))
      expect(checkbox).toBeChecked();
    expect(show).toBeEnabled();
  }
  expect(applies).toHaveLength(3);
});

it("keeps All changes usable for file types without available repository choices", async () => {
  menu = {
    ...menu,
    private: false,
    repos: menu.repos.map((repo) => ({ ...repo, commits: [], live: null })),
  };
  renderApp(<App />, { route: "/g5/workspace" });
  await screen.findByLabelText("Not yet pushed in Code");
  const user = userEvent.setup();
  const control = within(screen.getByTestId("source-control-menu"));
  const all = control.getByRole("checkbox", { name: "All changes" });
  expect(all).toBeEnabled();
  expect(all).toBeChecked();
  await user.click(all);
  expect(all).not.toBeChecked();
  expect(all).not.toBePartiallyChecked();
  await user.click(control.getByRole("checkbox", { name: "Docs" }));
  expect(all).toBePartiallyChecked();
  await user.click(all);
  for (const checkbox of within(
    control.getByRole("group", { name: "File types" }),
  ).getAllByRole("checkbox"))
    expect(checkbox).toBeChecked();
  expect(control.getByRole("button", { name: "Show" })).toBeDisabled();
  expect(
    control.getByRole("checkbox", { name: "Not yet pushed in Code" }),
  ).toBeDisabled();
});

it("applies explicitly and keeps all file cards present while jumping and resizing", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  expect(await screen.findByText("APPLIED a.txt")).toBeVisible();
  expect(screen.getByText("APPLIED b.txt")).toBeVisible();
  await userEvent.click(screen.getByLabelText("one in Code"));
  expect(screen.getByText("Selection changed · Click Show")).toBeVisible();
  expect(
    screen.getByText(
      "Selection changed. Showing the previous selection until you click Show.",
    ),
  ).toBeVisible();
  expect(applies).toHaveLength(1);
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  await userEvent.click(
    within(screen.getByRole("navigation", { name: "Changed files" })).getByRole(
      "button",
      { name: /b.txt/ },
    ),
  );
  expect(scroll).toHaveBeenCalled();
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  const separator = screen.getByRole("separator");
  Object.defineProperty(separator.parentElement!, "clientWidth", {
    value: 1000,
  });
  fireEvent.keyDown(separator, { key: "ArrowRight" });
  expect(separator).toHaveAttribute("aria-valuenow", "248");
});
it("keeps the net comparison heading when a full selection has no remaining file changes", async () => {
  snapshot = { ...snapshot, fileCount: 0, patches: [] };
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  expect(
    await screen.findByRole("heading", { name: "All changes" }),
  ).toBeVisible();
  expect(screen.queryByText(/Saved workspace base →/)).toBeNull();
  expect(
    screen.getByText("No net file changes in the selected comparison."),
  ).toBeVisible();
  expect(screen.queryByText("Patch totals")).toBeNull();
});
it("labels a selected commit's parent separately from unpushed work in a combined selection", async () => {
  snapshot.patches[0] = {
    ...snapshot.patches[0],
    label: "one · Commit one",
    after: "one",
  };
  snapshot.patches[1] = {
    ...snapshot.patches[1],
    label: "Not yet pushed",
    after: null,
  };
  await mount();
  await userEvent.click(screen.getByRole("checkbox", { name: "All changes" }));
  await userEvent.click(screen.getByRole("checkbox", { name: "Code" }));
  await userEvent.click(screen.getByLabelText("one in Code"));
  await userEvent.click(screen.getByLabelText("Not yet pushed in Code"));
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  const first = screen.getByText("APPLIED a.txt").closest("article")!;
  const second = screen.getByText("APPLIED b.txt").closest("article")!;
  expect(within(first).getByText("Parent")).toBeVisible();
  expect(within(first).getByText("Commit", { exact: true })).toBeVisible();
  expect(within(second).getByText("Before")).toBeVisible();
  expect(within(second).getByText("After")).toBeVisible();
  expect(within(second).queryByText("Commit", { exact: true })).toBeNull();
});
it("does not refresh on unrelated publication, and unchanged Save preserves custom checks and applied bytes", async () => {
  const { queryClient, ws } = await mount();
  expect(screen.queryByRole("button", { name: "Save changes" })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(screen.getByLabelText("one in Code"));
  const previousLoads = loads;
  await act(async () => {
    await queryClient.invalidateQueries({
      queryKey: ["workspace-review-flow", id],
    });
  });
  expect(checks).toBe(0);
  expect(loads).toBe(previousLoads);
  await saveCheckpoint(ws);
  await waitFor(() => expect(checks).toBe(1));
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(screen.queryByText(/checklist has been updated/)).toBeNull();
  expect(screen.getByText("Selection changed · Click Show")).toBeVisible();
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
});
it("updates pushed history on a chat Save checkpoint, preserves exclusions and the applied diff", async () => {
  const { ws } = await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(screen.getByLabelText("one in Code"));
  menu = {
    ...menu,
    fingerprint: "changed",
    repos: menu.repos.map((r) => ({
      ...r,
      commits: [commit("unrelated"), commit("local"), ...r.commits],
      live: { selectable: true },
    })),
  };
  generation++;
  await act(async () => {
    ws.emit({
      channel: `workspace:${id}`,
      event: "workspace.source_control_saved",
      payload: { workspaceId: id, generation },
    });
  });
  await screen.findByText(/checklist has been updated after Save changes/);
  expect(screen.getByLabelText("local in Code")).not.toBeChecked();
  expect(screen.getByLabelText("unrelat in Code")).not.toBeChecked();
  expect(screen.getByLabelText("unrelat in Docs")).toBeChecked();
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  expect(applies).toHaveLength(1);
});
it("retains the applied bytes after Save removes local changes until the empty comparison is applied", async () => {
  menu = {
    ...menu,
    repos: menu.repos.map((repo) => ({
      ...repo,
      base: repo.pushed,
      commits: [],
    })),
  };
  const { ws } = renderApp(<App />, { route: "/g5/workspace" });
  await screen.findByLabelText("Not yet pushed in Code");
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  menu = {
    ...menu,
    fingerprint: "clean-after-save",
    repos: menu.repos.map((repo) => ({ ...repo, live: null })),
  };
  snapshot = { patches: [], private: false, fileCount: 0 };
  await saveCheckpoint(ws);
  await screen.findByText(/checklist has been updated after Save changes/);
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  const show = screen.getByRole("button", { name: "Show" });
  expect(show).toBeEnabled();
  await userEvent.click(show);
  await screen.findByText("No net file changes in the selected comparison.");
  expect(screen.queryByText("APPLIED a.txt")).toBeNull();
  expect(applies).toHaveLength(2);
  expect(applies[1]).toMatchObject({
    repos: ["Code", "Docs"].map((repoId) => ({
      repoId,
      commits: [],
      includeUnpushed: false,
      allPushed: true,
    })),
  });
});
it("opens the ordinary file tab and restores diff scroll and selection on return", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  const stream = screen.getByTestId("source-control-diff-stream");
  stream.scrollTop = 123;
  fireEvent.scroll(stream);
  await userEvent.click(screen.getByLabelText("Actions for a.txt"));
  await userEvent.click(
    screen.getAllByRole("button", { name: "View file" })[0],
  );
  expect(await screen.findByText(/Ordinary file reader/)).toHaveTextContent(
    "branch=Code",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Return to changes" }),
  );
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  expect(screen.getByTestId("source-control-diff-stream").scrollTop).toBe(123);
  expect(applies).toHaveLength(2);
});
it("invalidates private applied diffs and checklists immediately on stop", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(screen.getByRole("button", { name: "Stop fixture" }));
  expect(screen.queryByText("APPLIED a.txt")).toBeNull();
  await screen.findAllByText("Commit one");
  expect(screen.getByLabelText("Not yet pushed in Code")).toBeDisabled();
  expect(screen.getByLabelText("Not yet pushed in Code")).not.toBeChecked();
});
it("ignores late private apply responses after a stop", async () => {
  let release!: () => void;
  let invoked = false;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  server.use(
    http.post(`${base}/source-control/apply`, async () => {
      invoked = true;
      await held;
      return HttpResponse.json({ data: snapshot });
    }),
  );
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await waitFor(() => expect(invoked).toBe(true));
  await userEvent.click(screen.getByRole("button", { name: "Stop fixture" }));
  await act(async () => {
    release();
    await held;
  });
  await screen.findAllByText("Commit one");
  expect(screen.queryByText("APPLIED a.txt")).toBeNull();
});
it("recovers a missed chat Save on reconnect, without checking on ordinary Git events", async () => {
  const { ws } = await mount();
  const prior = loads;
  await act(async () => {
    ws.emit({
      channel: `workspace:${id}`,
      event: "workspace.branch_updated",
      payload: { workspaceId: id, branch: "work", headCommit: "next" },
    });
  });
  expect(checks).toBe(0);
  expect(loads).toBe(prior);
  ws.simulateDisconnect();
  menu = { ...menu, fingerprint: "reconnected" };
  generation++;
  await act(async () => {
    ws.simulateReconnect();
  });
  await screen.findByText(/checklist has been updated after Save changes/);
  expect(checks).toBe(1);
});
it("explains an unavailable selected revision and retries the same pending selection without replacing the applied snapshot", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(screen.getByLabelText("one in Code"));
  const initialLoads = loads;
  const attempts: unknown[] = [];
  let fail = true;
  server.use(
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      attempts.push(await request.json());
      return fail
        ? HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "Selected revision is unavailable",
              },
            },
            { status: 409 },
          )
        : HttpResponse.json({ data: snapshot });
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findAllByText("Selected revision is unavailable");
  expect(screen.queryByRole("button", { name: "Refresh list" })).toBeNull();
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  expect(screen.getByText("APPLIED b.txt")).toBeVisible();
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(screen.getByLabelText("All changes in Code")).toBePartiallyChecked();
  expect(attempts[0]).toMatchObject({
    repos: [
      { repoId: "Code", commits: ["two"], includeUnpushed: true },
      { repoId: "Docs", commits: ["one", "two"], includeUnpushed: true },
    ],
  });
  snapshot = structuredClone(snapshot);
  for (const patch of snapshot.patches)
    for (const hunk of patch.file.diff.hunks)
      for (const line of hunk.lines)
        line.content = line.content.replace("APPLIED", "RETRIED");
  fail = false;
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  await screen.findByText("RETRIED a.txt");
  expect(attempts).toHaveLength(2);
  expect(attempts[1]).toEqual(attempts[0]);
  expect(screen.queryByText("APPLIED a.txt")).toBeNull();
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(checks).toBe(0);
  expect(loads).toBe(initialLoads);
});

it("keeps a failed Save check retryable without marking the menu current", async () => {
  let fail = true;
  server.use(
    http.post(`${base}/source-control/check`, () => {
      checks++;
      return fail
        ? HttpResponse.json(
            { error: { message: "Check unavailable" } },
            { status: 503 },
          )
        : HttpResponse.json({ data: { changed: true, menu } });
    }),
  );
  const { ws } = await mount();
  await userEvent.click(screen.getByLabelText("one in Code"));
  menu = { ...menu, fingerprint: "newer" };
  await saveCheckpoint(ws);
  await screen.findAllByText(/Check unavailable/);
  expect(screen.queryByText(/checklist has been updated/)).toBeNull();
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  fail = false;
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  await screen.findByText(/checklist has been updated after Save changes/);
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
});

it("withdraws the previous live snapshot when a running VM is replaced", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(
    screen.getByRole("button", { name: "Replace runtime" }),
  );
  expect(screen.queryByText("APPLIED a.txt")).toBeNull();
});
it("withdraws private snapshots on an observed read revocation", async () => {
  const { ws } = await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  const denied = () =>
    HttpResponse.json(
      { error: { code: "FORBIDDEN", message: "Access revoked" } },
      { status: 403 },
    );
  server.use(
    http.post(`${base}/source-control/check`, denied),
    http.get(`${base}/source-control/menu`, denied),
  );
  await act(async () => {
    ws.emit({
      channel: `workspace:${id}`,
      event: "workspace.source_control_saved",
      payload: { workspaceId: id, generation: ++generation },
    });
  });
  await waitFor(() => expect(screen.queryByText("APPLIED a.txt")).toBeNull());
  expect(screen.queryByLabelText("Not yet pushed in Code")).toBeNull();
});
it("retries an incomplete checklist at Save even when refs are unchanged", async () => {
  menu.repos[0].error = "History temporarily unavailable";
  menu.repos[0].commits = [];
  const { ws } = await mount();
  await screen.findByText("History temporarily unavailable");
  menu = {
    ...menu,
    repos: menu.repos.map((repo) => ({ ...repo, error: null })),
  };
  await saveCheckpoint(ws);
  await screen.findByText(/checklist has been updated after Save changes/);
  expect(screen.queryByText("History temporarily unavailable")).toBeNull();
  expect(checks).toBe(1);
});
it("keeps a newer Save event when an older initial checkpoint and menu arrive late", async () => {
  const old = structuredClone({ ...menu, generation: 0 });
  let releaseCheckpoint!: () => void, releaseMenu!: () => void;
  let requested = false;
  const heldCheckpoint = new Promise<void>((resolve) => {
    releaseCheckpoint = resolve;
  });
  const heldMenu = new Promise<void>((resolve) => {
    releaseMenu = resolve;
  });
  server.use(
    http.get(`${base}/source-control/checkpoint`, async () => {
      requested = true;
      await heldCheckpoint;
      return HttpResponse.json({ data: { generation: 0 } });
    }),
    http.get(`${base}/source-control/menu`, async () => {
      loads++;
      await heldMenu;
      return HttpResponse.json({ data: old });
    }),
  );
  const { ws, queryClient } = renderApp(<App />, { route: "/g5/workspace" });
  await waitFor(() => {
    expect(requested).toBe(true);
    expect(loads).toBeGreaterThan(0);
  });
  menu = { ...menu, generation: 1, fingerprint: "after-save" };
  await act(async () => {
    ws.emit({
      channel: `workspace:${id}`,
      event: "workspace.source_control_saved",
      payload: { workspaceId: id, generation: 1 },
    });
  });
  await act(async () => {
    releaseCheckpoint();
    await heldCheckpoint;
  });
  await waitFor(() =>
    expect(
      queryClient.getQueryState(["source-control-checkpoint", id])?.fetchStatus,
    ).toBe("idle"),
  );
  await act(async () => {
    releaseMenu();
    await heldMenu;
  });
  await screen.findByText(/checklist has been updated after Save changes/);
  expect(checks).toBe(1);
});
it("opens Show from an ordinary file path when the tab bar is hidden", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(
    screen.getAllByRole("button", { name: "View file" })[0],
  );
  await screen.findByText(/Ordinary file reader/);
  const before = loads;
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await waitFor(() =>
    expect(screen.getByTestId("entry-route")).toHaveTextContent(
      "?view=source-control:{}",
    ),
  );
  expect(applies).toHaveLength(2);
  expect(loads).toBe(before);
  expect(screen.queryByText(/Ordinary file reader/)).toBeNull();
});
it("retains a failed menu apply and the previous snapshot when opening the reader", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(
    screen.getAllByRole("button", { name: "View file" })[0],
  );
  await screen.findByText(/Ordinary file reader/);
  const before = loads;
  server.use(
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      applies.push(await request.json());
      return HttpResponse.json(
        { error: { message: "Selected comparison is unavailable" } },
        { status: 409 },
      );
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findAllByText("Selected comparison is unavailable");
  await waitFor(() =>
    expect(screen.getByTestId("entry-route")).toHaveTextContent(
      "?view=source-control:{}",
    ),
  );
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  expect(applies).toHaveLength(2);
  expect(loads).toBe(before);
});
it("does not navigate over a newer count entry when an older menu apply returns", async () => {
  categoryFixture();
  await mount();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText(
    "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
  );
  await user.click(screen.getAllByRole("button", { name: "View file" })[0]);
  await screen.findByText(/Ordinary file reader/);
  let release!: () => void;
  let requested = false;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  server.use(
    http.post(`${base}/source-control/apply`, async () => {
      requested = true;
      await held;
      return HttpResponse.json({ data: snapshot });
    }),
  );
  await user.click(screen.getByRole("button", { name: "Show" }));
  await waitFor(() => expect(requested).toBe(true));
  await user.click(
    screen.getByRole("button", { name: "Open Code comparison" }),
  );
  await screen.findByText("Code 2 — 2 matching of 4 changed files");
  const route = screen
    .getByTestId("entry-route")
    .getAttribute("data-location-key");
  const responded = new Promise<void>((resolve) => {
    const received = ({ request }: { request: { url: string } }) => {
      if (!request.url.endsWith(`${base}/source-control/apply`)) return;
      server.events.removeListener("response:mocked", received);
      resolve();
    };
    server.events.on("response:mocked", received);
  });
  await act(async () => {
    release();
    await responded;
  });
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Show" })).toBeEnabled(),
  );
  expect(screen.getByTestId("entry-route")).toHaveAttribute(
    "data-location-key",
    route,
  );
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "Code 2 — 2 matching of 4 changed files",
  );
});

function categoryFixture() {
  const original = snapshot.patches[0];
  snapshot = {
    ...snapshot,
    fileCount: 4,
    patches: [
      ["Code", "readme.md", "code"],
      ["Code", "deleted.ts", "code"],
      ["Code", ".g5/ontology/rule.g5.md", "ontology"],
      ["Docs", "assets/diagram.txt", "docs"],
    ].map(([repoId, path, kind]) => ({
      ...original,
      kind: kind as "code" | "ontology" | "docs",
      repoId,
      repoName: repoId,
      fileKey: path,
      file: {
        ...original.file,
        path,
        change: path === "deleted.ts" ? "deleted" : "modified",
      },
    })),
  };
  snapshot.patches.push({
    ...snapshot.patches[2],
    file: { ...original.file, path: "old-rule.md" },
  });
}

it("filters both panes after reading current local work and preserves accessible help/file actions", async () => {
  categoryFixture();
  const user = userEvent.setup();
  await mount();
  const control = screen.getByTestId("source-control-menu");
  for (const name of ["Code", "Ontology", "Docs"])
    expect(within(control).getByRole("checkbox", { name })).toBeChecked();
  const show = within(control).getByRole("button", {
    name: "Show",
  });
  await user.click(show);
  const summary = screen.getByTestId("source-control-type-summary");
  expect(summary).toHaveTextContent(
    "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
  );
  const stream = screen.getByTestId("source-control-diff-stream");
  const bytes = stream.textContent;
  fireEvent.scroll(stream, { target: { scrollTop: 157 } });
  await user.click(within(control).getByRole("checkbox", { name: "Docs" }));
  expect(summary).toHaveTextContent("4 matching of 4 changed files");
  expect(stream.textContent).toBe(bytes);
  expect(stream.scrollTop).toBe(157);
  expect(applies).toHaveLength(1);
  await user.click(show);
  expect(summary).toHaveTextContent(
    "Code 2 · Ontology 1 — 3 matching of 4 changed files",
  );
  expect(applies).toHaveLength(2);
  expect(stream.scrollTop).toBe(157);
  const tree = screen.getByRole("navigation", { name: "Changed files" });
  expect(
    within(tree).queryByRole("button", { name: "diagram.txt" }),
  ).toBeNull();
  expect(stream.querySelectorAll("article")).toHaveLength(3);
  expect(stream).not.toHaveTextContent("assets/diagram.txt");
  expect(within(tree).getAllByTestId("source-control-file")).toHaveLength(3);
  expect(screen.getByText("3 files changed")).toBeVisible();
  const help = screen.getByLabelText("About changed file types");
  help.focus();
  await user.click(help);
  expect(help.closest("details")).toHaveAttribute("open");
  expect(
    screen.getByText(/Only files matching the applied types appear/),
  ).toBeVisible();
  await user.keyboard("{Escape}");
  expect(help.closest("details")).not.toHaveAttribute("open");
  expect(help).toHaveFocus();
  const card = document.getElementById("source-file-Code%00readme.md")!;
  await user.click(within(card).getByLabelText("Actions for readme.md"));
  await user.click(within(card).getByRole("button", { name: "View file" }));
  expect(
    await screen.findByText("Ordinary file reader ?branch=Code"),
  ).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Return to changes" }));
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "3 matching of 4 changed files",
  );
  expect(screen.getByTestId("source-control-diff-stream").scrollTop).toBe(157);
  await user.click(within(control).getByRole("checkbox", { name: "Docs" }));
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "3 matching of 4 changed files",
  );
  await user.click(show);
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "4 matching of 4 changed files",
  );
  expect(screen.getAllByTestId("source-control-file")).toHaveLength(4);
  expect(screen.getByTestId("source-control-diff-stream").textContent).toBe(
    bytes,
  );
  expect(applies).toHaveLength(4);
});

it.each(["Code", "Ontology", "Docs"])(
  "shows only %s filenames and diffs",
  async (name) => {
    categoryFixture();
    const user = userEvent.setup();
    await mount();
    const control = screen.getByTestId("source-control-menu");
    for (const other of ["Code", "Ontology", "Docs"].filter(
      (other) => other !== name,
    ))
      await user.click(within(control).getByRole("checkbox", { name: other }));
    await user.click(
      within(control).getByRole("button", {
        name: "Show",
      }),
    );
    const expected = name === "Code" ? 2 : 1;
    expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
      `${name} ${expected} — ${expected} matching of 4 changed files`,
    );
    expect(screen.getAllByTestId("source-control-file")).toHaveLength(expected);
    expect(
      screen
        .getByTestId("source-control-diff-stream")
        .querySelectorAll("article"),
    ).toHaveLength(expected);
  },
);

it.each(
  Array.from({ length: 8 }, (_, mask) => ({
    names: ["Code", "Ontology", "Docs"].filter(
      (_, index) => mask & (1 << index),
    ),
  })),
)(
  "renders the same fresh and reapplied type selection: $names",
  async ({ names }) => {
    categoryFixture();
    snapshot.patches = snapshot.patches.map((patch, index) => ({
      ...patch,
      file: {
        ...patch.file,
        diff: {
          ...patch.file.diff,
          hunks: [
            {
              startLine: 1,
              header: "@@ -0,0 +1 @@",
              lines: [
                {
                  type: "addition",
                  newLine: 1,
                  content: `PATCH ${index}: ${patch.file.path}`,
                },
              ],
            },
          ],
        },
      },
    }));
    const user = userEvent.setup();
    const setTypes = async () => {
      const control = within(screen.getByTestId("source-control-menu"));
      for (const name of ["Code", "Ontology", "Docs"])
        if (!names.includes(name))
          await user.click(control.getByRole("checkbox", { name }));
    };
    const show = () => user.click(screen.getByRole("button", { name: "Show" }));
    const capture = () => {
      const stream = screen.getByTestId("source-control-diff-stream");
      expect(screen.queryByText(/Selection changed/)).toBeNull();
      return {
        summary: screen.getByTestId("source-control-type-summary").textContent,
        files: screen
          .queryAllByTestId("source-control-file")
          .map((file) => file.title),
        cards: Array.from(stream.querySelectorAll("article"), (card) => ({
          id: card.id,
          text: card.textContent,
        })),
        lines: within(stream)
          .queryAllByTestId("diff-line-code")
          .map((line) => line.textContent)
          .sort(),
      };
    };

    const mounted = await mount();
    await show();
    const original = capture();
    await setTypes();
    expect(screen.getByTestId("source-control-type-summary").textContent).toBe(
      original.summary,
    );
    await show();
    const reapplied = capture();
    const reappliedReads = 2;
    expect(applies).toHaveLength(reappliedReads);
    const matching =
      Number(names.includes("Code")) * 2 +
      Number(names.includes("Ontology")) +
      Number(names.includes("Docs"));
    expect(reapplied.summary).toContain(
      `${matching} matching of 4 changed files`,
    );
    expect(reapplied.cards).toHaveLength(matching);
    expect(reapplied.files).toHaveLength(matching);
    const expectedPatches = snapshot.patches.filter((patch) =>
      names.some((name) => name.toLowerCase() === patch.kind),
    );
    expect(reapplied.lines).toEqual(
      expectedPatches
        .map((patch) => patch.file.diff.hunks[0].lines[0].content)
        .sort(),
    );
    const expectedTitles: Record<string, string> = {
      "readme.md": "readme.md modified",
      "deleted.ts": "deleted.ts deleted",
      ".g5/ontology/rule.g5.md":
        ".g5/ontology/rule.g5.md changed in multiple commits",
      "assets/diagram.txt": "assets/diagram.txt modified",
    };
    expect(reapplied.files.toSorted()).toEqual(
      [
        ...new Set(
          expectedPatches.map((patch) => expectedTitles[patch.fileKey!]),
        ),
      ].sort(),
    );
    expect(screen.getByText(`${matching} files changed`)).toBeVisible();
    expect(
      document.querySelector(
        ".source-control-heading .source-control-additions",
      ),
    ).toHaveTextContent(`+${expectedPatches.length}`);
    expect(
      document.querySelector(
        ".source-control-heading .source-control-deletions",
      ),
    ).toHaveTextContent("−0");

    mounted.unmount();
    sessionStorage.clear();
    await mount();
    await setTypes();
    await show();
    expect(capture()).toEqual(reapplied);
    expect(applies).toHaveLength(reappliedReads + 1);
  },
);

it("allows no types/custom changes while an empty repository selection still disables apply", async () => {
  categoryFixture();
  const user = userEvent.setup();
  await mount();
  const control = screen.getByTestId("source-control-menu");
  for (const name of ["Code", "Ontology", "Docs"])
    await user.click(within(control).getByRole("checkbox", { name }));
  const show = within(control).getByRole("button", {
    name: "Show",
  });
  expect(show).toBeEnabled();
  await user.click(show);
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    /^0 matching of 4 changed files$/,
  );
  expect(screen.queryAllByTestId("source-control-file")).toHaveLength(0);
  expect(screen.getByTestId("source-control-diff-stream")).toHaveTextContent(
    "No changed files match the selected file types.",
  );
  expect(screen.queryAllByTestId("diff-line-code")).toHaveLength(0);
  await user.click(
    within(control).getByRole("checkbox", { name: "one in Code" }),
  );
  await user.click(show);
  expect(applies).toHaveLength(2);
  expect(
    (
      applies[1] as { repos: { repoId: string; commits: string[] }[] }
    ).repos.find((repo) => repo.repoId === "Code")?.commits,
  ).toEqual(["two"]);
  const all = within(control).getByRole("checkbox", { name: "All changes" });
  await user.click(all);
  await user.click(all);
  expect(show).toBeDisabled();
  await user.click(within(control).getByRole("checkbox", { name: "Ontology" }));
  expect(show).toBeDisabled();
});

it("retains pending/applied types across Save and withdraws private category counts on Stop", async () => {
  categoryFixture();
  const user = userEvent.setup();
  const mounted = await mount();
  const control = screen.getByTestId("source-control-menu");
  const show = within(control).getByRole("button", {
    name: "Show",
  });
  await user.click(show);
  await user.click(within(control).getByRole("checkbox", { name: "Docs" }));
  menu = {
    ...menu,
    fingerprint: "new-save",
    repos: menu.repos.map((repo) => ({ ...repo, pushed: "new-pushed" })),
  };
  await saveCheckpoint(mounted.ws);
  await waitFor(() => expect(checks).toBe(1));
  expect(
    within(control).getByRole("checkbox", { name: "Docs" }),
  ).not.toBeChecked();
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "4 matching of 4 changed files",
  );
  await user.click(show);
  expect(applies).toHaveLength(2);
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "3 matching of 4 changed files",
  );
  await user.click(screen.getByRole("button", { name: "Stop fixture" }));
  expect(
    screen.queryByTestId("source-control-type-summary"),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByTestId("source-control-diff-stream"),
  ).not.toBeInTheDocument();
});

it("grows and shrinks both panes when commits and file types change", async () => {
  categoryFixture();
  snapshot.patches = snapshot.patches.map((patch, index) => ({
    ...patch,
    file: {
      ...patch.file,
      diff: {
        ...patch.file.diff,
        hunks: [
          {
            startLine: 1,
            header: "@@ -0,0 +1 @@",
            lines: [
              {
                type: "addition",
                newLine: 1,
                content: `PATCH ${index}: ${patch.file.path}`,
              },
            ],
          },
        ],
      },
    },
  }));
  const expanded = {
    ...snapshot,
    fileCount: 3,
    patches: snapshot.patches.filter((patch) => patch.repoId === "Code"),
  };
  const small = {
    ...snapshot,
    fileCount: 1,
    patches: snapshot.patches.slice(0, 1),
  };
  snapshot = small;
  const user = userEvent.setup();
  await mount();
  const menu = within(screen.getByTestId("source-control-menu"));
  const show = () => user.click(menu.getByRole("button", { name: "Show" }));
  const toggle = (name: string) =>
    user.click(menu.getByRole("checkbox", { name }));
  const expectVisible = (titles: string[], lines: string[]) => {
    expect(
      screen
        .queryAllByTestId("source-control-file")
        .map((row) => row.title)
        .sort(),
    ).toEqual(titles.toSorted());
    expect(
      screen
        .getByTestId("source-control-diff-stream")
        .querySelectorAll("article"),
    ).toHaveLength(titles.length);
    expect(
      screen
        .queryAllByTestId("diff-line-code")
        .map((line) => line.textContent)
        .sort(),
    ).toEqual(lines.toSorted());
    expect(screen.getByText(`${titles.length} files changed`)).toBeVisible();
  };
  const codeTitles = ["readme.md modified", "deleted.ts deleted"];
  const ontologyTitle = ".g5/ontology/rule.g5.md changed in multiple commits";
  const codeLines = ["PATCH 0: readme.md", "PATCH 1: deleted.ts"];
  const ontologyLines = [
    "PATCH 2: .g5/ontology/rule.g5.md",
    "PATCH 4: old-rule.md",
  ];
  await toggle("All changes");
  await toggle("one in Code");
  await toggle("Code");
  await show();
  expectVisible(codeTitles.slice(0, 1), codeLines.slice(0, 1));

  snapshot = expanded;
  await toggle("two in Code");
  expectVisible(codeTitles.slice(0, 1), codeLines.slice(0, 1));
  await show();
  expectVisible(codeTitles, codeLines);
  expect(applies).toHaveLength(2);

  await toggle("Ontology");
  expectVisible(codeTitles, codeLines);
  await show();
  expectVisible(
    [...codeTitles, ontologyTitle],
    [...codeLines, ...ontologyLines],
  );
  expect(applies).toHaveLength(3);
  const tree = within(
    screen.getByRole("navigation", { name: "Changed files" }),
  );
  await user.click(tree.getByRole("button", { name: "readme.md" }));
  expect(tree.getByRole("button", { name: "readme.md" })).toHaveAttribute(
    "aria-current",
    "true",
  );
  await toggle("Code");
  await show();
  expectVisible([ontologyTitle], ontologyLines);
  expect(
    screen
      .getByRole("navigation", { name: "Changed files" })
      .querySelector('[aria-current="true"]'),
  ).toBeNull();

  snapshot = small;
  await toggle("two in Code");
  expectVisible([ontologyTitle], ontologyLines);
  await show();
  expectVisible([], []);
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "0 matching of 1 changed files",
  );
  expect(applies).toHaveLength(5);

  snapshot = expanded;
  await toggle("two in Code");
  await show();
  expectVisible([ontologyTitle], ontologyLines);
  await toggle("Code");
  await show();
  expectVisible(
    [...codeTitles, ontologyTitle],
    [...codeLines, ...ontologyLines],
  );
  expect(tree.getByRole("button", { name: "readme.md" })).not.toHaveAttribute(
    "aria-current",
  );
  expect(applies).toHaveLength(7);
  expect(
    (applies[1] as { repos: { commits: string[] }[] }).repos[0].commits,
  ).toEqual(["one", "two"]);
  expect(
    (applies[4] as { repos: { commits: string[] }[] }).repos[0].commits,
  ).toEqual(["one"]);
});

it("updates commit metadata without rerendering unchanged diff lines", async () => {
  const renderer = vi.spyOn(
    await import("../editor/DiffHunksView"),
    "DiffHunksView",
  );
  try {
    snapshot.patches = snapshot.patches.map((patch) => ({
      ...patch,
      after: "one",
    }));
    const { ws } = await mount();
    const user = userEvent.setup();
    await user.click(screen.getByRole("checkbox", { name: "All changes" }));
    await user.click(screen.getByRole("checkbox", { name: "Code" }));
    await user.click(screen.getByRole("checkbox", { name: "one in Code" }));
    await user.click(screen.getByRole("button", { name: "Show" }));
    const stream = screen.getByTestId("source-control-diff-stream");
    expect(within(stream).getAllByText("Commit one")).toHaveLength(2);
    const calls = renderer.mock.calls.length;
    menu = {
      ...menu,
      fingerprint: "updated-commit-metadata",
      repos: menu.repos.map((repo) => ({
        ...repo,
        commits: repo.commits.map((commit) =>
          commit.sha === "one"
            ? {
                ...commit,
                summary: "Refined commit title",
                committedAt: commit.committedAt + 3600,
                parents: ["base", "other"],
              }
            : commit,
        ),
      })),
    };
    await saveCheckpoint(ws);
    await waitFor(() => expect(checks).toBe(1));
    expect(within(stream).getAllByText("Commit one")).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "Show" }));
    expect(within(stream).getAllByText("Refined commit title")).toHaveLength(2);
    expect(within(stream).queryByText("Commit one")).toBeNull();
    expect(
      within(stream).getAllByText(/Merge, first-parent diff/),
    ).toHaveLength(2);
    expect(renderer).toHaveBeenCalledTimes(calls);
    expect(applies).toHaveLength(2);
  } finally {
    renderer.mockRestore();
  }
});

it("reuses surviving patch bodies while revalidating local work on Show", async () => {
  const grouping = vi.spyOn(
    await import("../../lib/sourceControlFiles"),
    "groupSourcePatches",
  );
  const renderer = vi.spyOn(
    await import("../editor/DiffHunksView"),
    "DiffHunksView",
  );
  try {
    const original = snapshot.patches[0];
    snapshot = {
      ...snapshot,
      fileCount: 250,
      patches: Array.from({ length: 500 }, (_, i) => ({
        ...original,
        fileKey: `large/${Math.floor(i / 2)}.txt`,
        kind: ["code", "ontology", "docs"][Math.floor(i / 2) % 3] as
          | "code"
          | "ontology"
          | "docs",
        file: { ...original.file, path: `patch-${i}.txt` },
      })),
    };
    const user = userEvent.setup();
    await mount();
    const control = screen.getByTestId("source-control-menu");
    const show = within(control).getByRole("button", {
      name: "Show",
    });
    await user.click(show);
    const rendered = renderer.mock.calls.length;
    expect(rendered).toBeGreaterThanOrEqual(500);
    const grouped = grouping.mock.calls.length;
    expect(grouped).toBeGreaterThan(0);
    const originalRows = new Map(
      screen
        .getAllByTestId("source-control-file")
        .map((row) => [(row as HTMLButtonElement).title, row]),
    );
    const reads = { loads, checks };
    fireEvent.keyDown(screen.getByRole("separator"), { key: "ArrowRight" });
    await user.click(within(control).getByRole("checkbox", { name: "Docs" }));
    await user.click(
      within(control).getByRole("checkbox", { name: "one in Code" }),
    );
    await user.click(
      within(control).getByRole("checkbox", { name: "one in Code" }),
    );
    expect(renderer).toHaveBeenCalledTimes(rendered);
    expect(grouping).toHaveBeenCalledTimes(grouped);
    for (const row of screen.getAllByTestId("source-control-file"))
      expect(row).toBe(originalRows.get((row as HTMLButtonElement).title));
    expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
      "250 matching of 250 changed files",
    );
    await user.click(show);
    expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
      "167 matching of 250 changed files",
    );
    expect(renderer).toHaveBeenCalledTimes(rendered);
    expect(screen.getAllByTestId("source-control-file")).toHaveLength(167);
    for (const row of screen.getAllByTestId("source-control-file"))
      expect(row).toBe(originalRows.get((row as HTMLButtonElement).title));
    await user.click(within(control).getByRole("checkbox", { name: "Docs" }));
    await user.click(show);
    expect(screen.getAllByTestId("source-control-file")).toHaveLength(250);
    expect(renderer).toHaveBeenCalledTimes(rendered + 166);
    expect(applies).toHaveLength(3);
    expect({ loads, checks }).toEqual(reads);
    expect(grouping).toHaveBeenCalledTimes(grouped + 2);
  } finally {
    grouping.mockRestore();
    renderer.mockRestore();
  }
});

it("keeps a historical file and a later directory at the same path independently accessible", async () => {
  const original = snapshot.patches[0];
  snapshot = {
    ...snapshot,
    fileCount: 2,
    patches: ["guide", "guide/new.md"].map((path, i) => ({
      ...original,
      repoId: "Docs",
      repoName: "Docs",
      kind: "docs",
      fileKey: path,
      file: { ...original.file, path, change: i ? "added" : "deleted" },
    })),
  };
  const user = userEvent.setup();
  await mount();
  const control = screen.getByTestId("source-control-menu");
  const show = within(control).getByRole("button", { name: "Show" });
  await user.click(show);
  expect(
    screen
      .getAllByTestId("source-control-file")
      .map((row) => row.getAttribute("aria-label"))
      .sort(),
  ).toEqual(["guide", "new.md"]);
  for (const row of screen.getAllByTestId("source-control-file"))
    expect(row).toBeEnabled();
  await user.click(within(control).getByRole("checkbox", { name: "Docs" }));
  await user.click(show);
  expect(screen.queryAllByTestId("source-control-file")).toHaveLength(0);
  expect(screen.queryAllByTestId("diff-line-code")).toHaveLength(0);
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "0 matching of 2 changed files",
  );
});

it.each(["captured", "current"] as const)(
  "acknowledges type-only edits of an empty %s comparison without requesting an empty selection",
  async (scope) => {
    menu = {
      ...menu,
      private: false,
      repos: menu.repos.map((repo) => ({
        ...repo,
        base: repo.pushed,
        commits: [],
        live: null,
      })),
    };
    snapshot = { patches: [], fileCount: 0, private: false };
    let pushedReads = 0;
    server.use(
      http.post(`${base}/source-control/pushed`, () => {
        pushedReads++;
        return HttpResponse.json({ data: snapshot });
      }),
    );
    renderApp(<App />, { route: "/g5/workspace" });
    const user = userEvent.setup();
    await screen.findByLabelText("Not yet pushed in Code");
    await user.click(
      screen.getByRole("button", {
        name:
          scope === "captured"
            ? "Open pushed comparison"
            : "Open current pushed comparison",
      }),
    );
    await screen.findByText(
      "Code 0 · Ontology 0 · Docs 0 — 0 matching of 0 changed files",
    );
    const show = screen.getByRole("button", {
      name: "Show",
    });
    expect(show).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: "Code" }));
    expect(show).toBeEnabled();
    await user.click(show);
    await screen.findByText(
      "Ontology 0 · Docs 0 — 0 matching of 0 changed files",
    );
    expect(screen.queryByText(/Selection changed/)).toBeNull();
    expect(applies).toHaveLength(0);
    expect(pushedReads).toBe(1);
    const unpushed = screen.getByLabelText("Not yet pushed in Code");
    expect(unpushed).toBeDisabled();
    expect(unpushed).not.toBeChecked();
    expect(unpushed).toHaveAccessibleDescription(
      "Available only to the workspace owner while the VM is running.",
    );
    expect(unpushed.closest("label")).not.toHaveTextContent("0 files");
    const help = screen.getByLabelText("About Source Control selection");
    await user.click(help);
    expect(
      within(help.closest("details")!).getByText(/When the VM is stopped/),
    ).toBeVisible();
    await user.keyboard("{Escape}");
    expect(help).toHaveFocus();
    expect(help.closest("details")).not.toHaveAttribute("open");
  },
);

it("retries a captured scope when authority settles and ignores the earlier response", async () => {
  categoryFixture();
  let reads = 0;
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  server.use(
    http.post(`${base}/source-control/pushed`, async () => {
      const request = ++reads;
      if (request === 1) await held;
      return HttpResponse.json({
        data: {
          ...snapshot,
          private: false,
          fileCount: request === 1 ? 99 : 4,
        },
      });
    }),
  );
  await mount();
  const user = userEvent.setup();
  try {
    await user.click(
      screen.getByRole("button", { name: "Open pushed comparison" }),
    );
    await waitFor(() => expect(reads).toBe(1));
    await user.click(screen.getByRole("button", { name: "Replace runtime" }));
    await screen.findByText(
      "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
    );
    expect(reads).toBe(2);
  } finally {
    await act(async () => {
      release();
      await held;
    });
  }
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "4 matching of 4 changed files",
  );
  expect(screen.queryByText(/99 matching/)).toBeNull();
});

it.each(["pushed", "custom"] as const)(
  "finishes restoring a %s comparison when authority settles during reload",
  async (comparison) => {
    categoryFixture();
    const mounted = await mount();
    const user = userEvent.setup();
    if (comparison === "pushed") {
      await user.click(
        screen.getByRole("button", { name: "Open Code comparison" }),
      );
    } else {
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Show" })).toBeEnabled(),
      );
      await user.click(screen.getByLabelText("one in Code"));
      expect(screen.getByLabelText("one in Code")).not.toBeChecked();
      await user.click(screen.getByRole("button", { name: "Show" }));
    }
    await screen.findByTestId("source-control-type-summary");
    const expected = screen.getByTestId(
      "source-control-type-summary",
    ).textContent;
    const applied = applies.length;
    mounted.unmount();

    let release!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    let settling = true;
    const initialLoads = loads;
    server.use(
      http.get(`${base}/source-control/menu`, async () => {
        loads++;
        if (settling) await held;
        return HttpResponse.json({ data: menu });
      }),
    );
    renderApp(<App />, { route: "/g5/workspace?view=source-control" });
    try {
      await waitFor(() => expect(loads).toBeGreaterThan(initialLoads));
      settling = false;
      await user.click(screen.getByRole("button", { name: "Replace runtime" }));
      await waitFor(() =>
        expect(
          screen.getByTestId("source-control-type-summary").textContent,
        ).toBe(expected),
      );
      if (comparison === "custom") {
        expect(applies).toHaveLength(applied + 1);
        expect(screen.getByLabelText("one in Code")).not.toBeChecked();
      }
    } finally {
      await act(async () => {
        release();
        await held;
      });
    }
    expect(screen.getByTestId("source-control-type-summary").textContent).toBe(
      expected,
    );
  },
);

it("keeps edits made during apply pending until the next Show", async () => {
  categoryFixture();
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  server.use(
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      applies.push(await request.json());
      await held;
      return HttpResponse.json({ data: snapshot });
    }),
  );
  await mount();
  const user = userEvent.setup();
  const show = screen.getByRole("button", {
    name: "Show",
  });
  await user.click(screen.getByRole("checkbox", { name: "Code" }));
  try {
    await user.click(show);
    await waitFor(() => expect(applies).toHaveLength(1));
    await user.click(screen.getByRole("checkbox", { name: "Docs" }));
  } finally {
    await act(async () => {
      release();
      await held;
    });
  }
  await screen.findByText(
    "Ontology 1 · Docs 1 — 2 matching of 4 changed files",
  );
  expect(
    screen.getByText(
      "Selection changed. Showing the previous selection until you click Show.",
    ),
  ).toBeVisible();
  await user.click(show);
  await screen.findByText("Ontology 1 — 1 matching of 4 changed files");
  expect(screen.queryByText(/Selection changed/)).toBeNull();
  expect(applies).toHaveLength(2);
});

it("retains the applied bytes, counts and pending warning when a new comparison fails", async () => {
  categoryFixture();
  await mount();
  const user = userEvent.setup();
  const show = screen.getByRole("button", {
    name: "Show",
  });
  await user.click(show);
  await screen.findByText(
    "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
  );
  const bytes = screen.getByTestId("source-control-diff-stream").textContent;
  await user.click(screen.getByRole("checkbox", { name: "Code" }));
  await user.click(screen.getByRole("checkbox", { name: "one in Code" }));
  server.use(
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      applies.push(await request.json());
      return HttpResponse.json(
        { error: { message: "Comparison read unavailable" } },
        { status: 503 },
      );
    }),
  );
  await user.click(show);
  await screen.findAllByText("Comparison read unavailable");
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "4 matching of 4 changed files",
  );
  expect(screen.getByTestId("source-control-diff-stream").textContent).toBe(
    bytes,
  );
  expect(
    screen.getByText(
      "Selection changed. Showing the previous selection until you click Show.",
    ),
  ).toBeVisible();
  expect(applies).toHaveLength(2);
});

it("keeps the latest captured scope when an older comparison returns late", async () => {
  categoryFixture();
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let reads = 0;
  server.use(
    http.post(`${base}/source-control/pushed`, async ({ request }) => {
      reads++;
      const { heads } = (await request.json()) as {
        heads: Record<string, string>;
      };
      if (heads.Code === "pushed") {
        await held;
        return HttpResponse.json({ data: { ...snapshot, private: false } });
      }
      return HttpResponse.json({
        data: { patches: [], private: false, fileCount: 0 },
      });
    }),
  );
  await mount();
  const user = userEvent.setup();
  try {
    await user.click(
      screen.getByRole("button", { name: "Open pushed comparison" }),
    );
    await waitFor(() => expect(reads).toBe(1));
    menu = {
      ...menu,
      repos: menu.repos.map((repo) => ({
        ...repo,
        pushed: "empty-head",
        base: "empty-head",
        commits: [],
      })),
    };
    await user.click(
      screen.getByRole("button", { name: "Open empty pushed comparison" }),
    );
    await screen.findByText(
      "Code 0 · Ontology 0 · Docs 0 — 0 matching of 0 changed files",
    );
  } finally {
    await act(async () => {
      release();
      await held;
    });
  }
  expect(reads).toBe(2);
  expect(screen.getByTestId("source-control-type-summary")).toHaveTextContent(
    "0 matching of 0 changed files",
  );
});

it.each(["failed", "truncated"] as const)(
  "does not invent an empty comparison from a %s menu",
  async (kind) => {
    menu = {
      ...menu,
      private: false,
      repos: menu.repos.map((repo) => ({
        ...repo,
        commits: [],
        live: null,
        error: kind === "failed" ? "History read failed" : null,
        truncated: kind === "truncated",
      })),
    };
    server.use(
      http.post(`${base}/source-control/pushed`, () =>
        HttpResponse.json(
          { error: { message: "Comparison unavailable" } },
          { status: 409 },
        ),
      ),
    );
    renderApp(<App />, { route: "/g5/workspace" });
    const user = userEvent.setup();
    await screen.findByLabelText("Not yet pushed in Code");
    await user.click(
      screen.getByRole("button", { name: "Open current pushed comparison" }),
    );
    await user.click(screen.getByRole("checkbox", { name: "Code" }));
    expect(screen.queryByTestId("source-control-type-summary")).toBeNull();
    expect(screen.getByRole("button", { name: "Show" })).toBeDisabled();
    expect(applies).toHaveLength(0);
  },
);

it.each(["captured", "current"] as const)(
  "keeps private work unchecked when the menu arrives before the %s pushed comparison",
  async (scope) => {
    categoryFixture();
    snapshot.private = false;
    let release!: () => void;
    let reads = 0;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const response = async () => {
      reads++;
      await held;
      return HttpResponse.json({ data: snapshot });
    };
    server.use(
      http.post(`${base}/source-control/pushed`, response),
      http.post(`${base}/source-control/apply`, response),
    );
    renderApp(<App />, {
      route: "/g5/workspace?view=source-control",
      routeState: { sourceControlSelection: "all" },
    });
    const user = userEvent.setup();
    try {
      await waitFor(() =>
        expect(
          screen.getByTestId("source-control-test-actor"),
        ).toHaveTextContent("user-alex"),
      );
      await screen.findByLabelText("Not yet pushed in Code");
      await waitFor(() => expect(reads).toBeGreaterThanOrEqual(1));
      expect(screen.getByRole("button", { name: "Show" })).toBeDisabled();
      expect(screen.getByLabelText("Not yet pushed in Code")).not.toBeChecked();
      expect(screen.getByLabelText("Not yet pushed in Docs")).not.toBeChecked();
      await user.click(screen.getByRole("checkbox", { name: "two in Code" }));
    } finally {
      await act(async () => {
        release();
        await held;
      });
    }
    await screen.findByText(
      "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
    );
    expect(reads).toBeLessThanOrEqual(2);
    expect(screen.getByLabelText("two in Code")).not.toBeChecked();
    expect(screen.getByLabelText("Not yet pushed in Code")).not.toBeChecked();
    expect(
      screen.getByText(
        "Selection changed. Showing the previous selection until you click Show.",
      ),
    ).toBeVisible();
  },
);

it("count links replace earlier types and pushed choices, then reload retains applied edits", async () => {
  categoryFixture();
  let mounted = await mount();
  const user = userEvent.setup();
  await user.click(
    screen.getByRole("button", { name: "Open pushed comparison" }),
  );
  await screen.findByText(
    "Code 2 · Ontology 1 · Docs 1 — 4 matching of 4 changed files",
  );
  await user.click(
    screen.getByRole("button", { name: "Open Code comparison" }),
  );
  await screen.findByText("Code 2 — 2 matching of 4 changed files");
  expect(screen.getByRole("checkbox", { name: "Code" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Ontology" })).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Docs" })).not.toBeChecked();
  expect(screen.getByLabelText("Not yet pushed in Code")).not.toBeChecked();
  expect(screen.getByTestId("entry-route")).not.toHaveTextContent(
    "sourceControlSelection",
  );
  expect(screen.getByTestId("entry-route")).not.toHaveTextContent("scope=");
  await user.click(screen.getByRole("checkbox", { name: "Ontology" }));
  await user.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText(
    "Code 2 · Ontology 1 — 3 matching of 4 changed files",
  );
  const before = loads;
  mounted.unmount();
  menu = {
    ...menu,
    fingerprint: "new-head",
    repos: menu.repos.map((r) => ({
      ...r,
      pushed: "new-head",
      commits: [commit("new"), ...r.commits],
    })),
  };
  mounted = renderApp(<App />, {
    route: "/g5/workspace?view=source-control",
    routeState: { sourceControlFromMenu: true },
  });
  await screen.findByText(
    "Code 2 · Ontology 1 — 3 matching of 4 changed files",
  );
  expect(loads).toBeGreaterThan(before);
  expect(screen.getByLabelText("new in Code")).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Docs" })).not.toBeChecked();
  await waitFor(() =>
    expect(screen.getByTestId("entry-route")).not.toHaveTextContent(
      "sourceControlFromMenu",
    ),
  );
  mounted.unmount();
});

it("loads history only on expansion, pages it without changing the diff, and clears opt-in rows with All changes", async () => {
  menu.repos = menu.repos.slice(0, 1).map((repo) => ({
    ...repo,
    workspaceBase: "fork",
    parent: "parent",
    comparison: {
      mergeBase: "base",
      mergeBaseCommit: commit("base"),
      parentMerges: [],
      workspaceBase: commit("fork"),
      workspaceBaseInHistory: true,
      sinceWorkspaceBaseCount: 2,
    },
  }));
  let historyReads = 0;
  server.use(
    http.post(`${base}/source-control/history`, async ({ request }) => {
      historyReads++;
      const input = (await request.json()) as { cursor: string | null };
      return HttpResponse.json({
        data: {
          commits: [commit(input.cursor ? "older-two" : "older-one")],
          nextCursor: input.cursor ? null : "older-one",
        },
      });
    }),
  );
  await mount();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  const applied = applies.length;
  const before = screen.getByTestId("source-control-diff-stream").textContent;
  expect(historyReads).toBe(0);
  await user.click(screen.getByText("Since workspace base · 2 commits"));
  await screen.findByLabelText("older-o in Code");
  await user.click(screen.getByRole("button", { name: "Show more" }));
  await screen.findByLabelText("older-t in Code");
  expect(historyReads).toBe(2);
  expect(applies.length).toBe(applied);
  expect(screen.getByTestId("source-control-diff-stream").textContent).toBe(
    before,
  );
  await user.click(screen.getByLabelText("older-o in Code"));
  expect(
    screen.getByRole("checkbox", { name: "All changes" }),
  ).toBePartiallyChecked();
  await user.click(screen.getByRole("checkbox", { name: "All changes" }));
  expect(screen.getByLabelText("older-o in Code")).not.toBeChecked();
  expect(screen.getByLabelText("older-t in Code")).not.toBeChecked();
});

it.each([false, true])(
  "shows the real merge-base commit with opt-in selection (combined bases: %s)",
  async (combined) => {
    menu.repos = [
      {
        ...menu.repos[0],
        workspaceBase: combined ? "base" : "fork",
        parent: "parent",
        comparison: {
          mergeBase: "base",
          mergeBaseCommit: {
            ...commit("base"),
            summary: "Original shared change",
          },
          parentMerges: [],
          workspaceBase: commit(combined ? "base" : "fork"),
          workspaceBaseInHistory: true,
          sinceWorkspaceBaseCount: combined ? 0 : 2,
        },
      },
    ];
    await mount();
    const user = userEvent.setup();
    const checkbox = screen.getByRole("checkbox", { name: "base in Code" });
    const row = within(checkbox.closest("label")!);
    expect(
      row.getByText(combined ? "Merge base · Workspace base" : "Merge base"),
    ).toBeVisible();
    expect(row.getByText("Original shared change")).toBeVisible();
    expect(row.getByText("base", { selector: "code" })).toBeVisible();
    expect(row.getByText(/Real author · Committed/)).toBeVisible();
    expect(checkbox).not.toBeChecked();
    expect(screen.queryByText("Workspace base", { exact: true }) !== null).toBe(
      !combined,
    );
    expect(
      screen.queryByText("Since workspace base · 2 commits") !== null,
    ).toBe(!combined);
    expect(
      screen.getByText("Earlier history").closest("details"),
    ).not.toHaveAttribute("open");
    await user.click(screen.getByRole("button", { name: "Show" }));
    await screen.findByText("APPLIED a.txt");
    const originalInput = applies.at(-1);
    const originalDiff = screen.getByTestId(
      "source-control-diff-stream",
    ).textContent;
    expect(originalInput).toMatchObject({
      repos: [
        {
          base: "base",
          commits: ["one", "two"],
          allPushed: true,
          includeUnpushed: true,
        },
      ],
    });

    await user.click(checkbox);
    expect(checkbox).toBeChecked();
    expect(
      screen.queryByText(/Selected revision base is unavailable/),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("checkbox", { name: "All changes" }),
    ).toBePartiallyChecked();
    expect(screen.getByTestId("source-control-diff-stream").textContent).toBe(
      originalDiff,
    );
    await user.click(screen.getByRole("button", { name: "Show" }));
    await waitFor(() =>
      expect(applies.at(-1)).toMatchObject({
        repos: [
          { base: "base", commits: ["one", "two", "base"], allPushed: false },
        ],
      }),
    );

    await user.click(screen.getByRole("checkbox", { name: "All changes" }));
    expect(checkbox).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Show" }));
    await waitFor(() => expect(applies.at(-1)).toEqual(originalInput));
    expect(screen.getByTestId("source-control-diff-stream").textContent).toBe(
      originalDiff,
    );
    expect(
      screen.getAllByRole("checkbox", { name: "base in Code" }),
    ).toHaveLength(1);
  },
);

it("keeps Base workspace commits in collapsed history without base markers", async () => {
  menu.repos = [
    {
      ...menu.repos[0],
      base: null,
      workspaceBase: null,
      unpushedOnly: true,
      commits: [],
      live: null,
    },
  ];
  server.use(
    http.post(`${base}/source-control/history`, () =>
      HttpResponse.json({
        data: {
          commits: [commit("pushed"), commit("oldest")],
          nextCursor: null,
        },
      }),
    ),
  );
  renderApp(<App />, { route: "/g5/workspace" });
  expect(await screen.findByLabelText("Not yet pushed in Code")).toBeDisabled();
  expect(screen.getByText("No outgoing comparison")).toBeVisible();
  expect(
    screen.queryByText(/Merge base|Workspace base/),
  ).not.toBeInTheDocument();
  const history = screen.getByText("Show history");
  expect(history.closest("details")).not.toHaveAttribute("open");
  expect(screen.queryByText("Commit pushed")).not.toBeInTheDocument();
  await userEvent.setup().click(history);
  expect(await screen.findByLabelText("pushed in Code")).not.toBeChecked();
  expect(screen.getByLabelText("oldest in Code")).not.toBeChecked();
  expect(screen.getByText("Commit pushed")).toBeVisible();
  expect(screen.getByText("Commit oldest")).toBeVisible();
});

function staleListResponse() {
  return HttpResponse.json(
    {
      error: {
        code: "CONFLICT",
        message: "The workspace changed.",
        details: { kind: "source_control_stale_list" },
      },
    },
    { status: 409 },
  );
}

it("offers inline read-only refresh after a stale Show, preserving exclusions and the prior comparison until another Show", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  await userEvent.click(screen.getByLabelText("one in Code"));
  const controls = within(screen.getByTestId("source-control-menu"));
  await userEvent.click(controls.getByRole("checkbox", { name: "Docs" }));
  const read = vi.fn();
  let stale = true;
  server.use(
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      applies.push(await request.json());
      return stale
        ? staleListResponse()
        : HttpResponse.json({ data: snapshot });
    }),
    http.post(`${base}/source-control/check`, async ({ request }) => {
      read(await request.json());
      return HttpResponse.json({
        data: {
          changed: true,
          menu: {
            ...menu,
            fingerprint: "fresh",
            repos: menu.repos.map((repo) => ({
              ...repo,
              commits: [commit("new"), ...repo.commits],
            })),
          },
        },
      });
    }),
  );
  expect(screen.queryByRole("button", { name: "Refresh list" })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findAllByRole("button", { name: "Refresh list" });
  expect(screen.getByText(/Previous comparison ·/)).toBeVisible();
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  await userEvent.click(controls.getByRole("button", { name: "Refresh list" }));
  await screen.findByText("Checklist updated · Click Show");
  expect(read).toHaveBeenCalledWith(
    expect.objectContaining({
      fingerprint: "",
      selected: { Code: ["two"], Docs: ["one", "two"] },
    }),
  );
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(screen.getByLabelText("two in Code")).toBeChecked();
  expect(screen.getByLabelText("new in Code")).not.toBeChecked();
  expect(controls.getByRole("checkbox", { name: "Docs" })).not.toBeChecked();
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
  expect(applies).toHaveLength(2);
  expect(screen.queryByRole("button", { name: "Refresh list" })).toBeNull();
  stale = false;
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await waitFor(() =>
    expect(screen.queryByText(/Previous comparison ·/)).toBeNull(),
  );
  expect(applies).toHaveLength(3);
});

it.each([true, false])(
  "removes only confirmed missing selections without substitution (remaining choices: %s)",
  async (remaining) => {
    await mount();
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    await screen.findByText("APPLIED a.txt");
    await userEvent.click(screen.getByLabelText("one in Code"));
    if (!remaining) {
      await userEvent.click(screen.getByLabelText("Not yet pushed in Code"));
      await userEvent.click(
        screen.getByRole("checkbox", { name: "All changes in Docs" }),
      );
    }
    let stale = true;
    server.use(
      http.post(`${base}/source-control/apply`, async ({ request }) => {
        applies.push(await request.json());
        return stale
          ? staleListResponse()
          : HttpResponse.json({ data: snapshot });
      }),
      http.post(`${base}/source-control/check`, () =>
        HttpResponse.json({
          data: {
            changed: true,
            menu: {
              ...menu,
              fingerprint: "rewritten",
              repos: menu.repos.map((repo) =>
                repo.repoId === "Code"
                  ? { ...repo, commits: [commit("new"), commit("one")] }
                  : repo,
              ),
            },
            missingCommits: { Code: ["two"] },
          },
        }),
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    const controls = within(screen.getByTestId("source-control-menu"));
    await userEvent.click(
      await controls.findByRole("button", { name: "Refresh list" }),
    );
    const notice = await controls.findByText(/Removed commit/);
    expect(notice).toHaveTextContent(
      "Removed commit two in Code from selection",
    );
    expect(
      notice.compareDocumentPosition(
        screen.getByRole("checkbox", { name: "All changes in Code" }),
      ) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.queryByLabelText("two in Code")).toBeNull();
    expect(screen.getByLabelText("new in Code")).not.toBeChecked();
    expect(screen.getByLabelText("one in Code")).not.toBeChecked();
    expect(screen.getByText("APPLIED a.txt")).toBeVisible();
    expect(screen.getByText(/Previous comparison ·/)).toBeVisible();
    if (!remaining) {
      expect(screen.getByRole("button", { name: "Show" })).toBeDisabled();
      expect(applies).toHaveLength(2);
      return;
    }
    stale = false;
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    await waitFor(() =>
      expect(screen.queryByText(/Removed commit/)).toBeNull(),
    );
    expect(applies.at(-1)).toMatchObject({
      repos: [
        { repoId: "Code", commits: [], includeUnpushed: true },
        { repoId: "Docs" },
      ],
    });
  },
);

it.each(["refresh", "comparison"])(
  "retains the previous diff and manual recovery when another edit races with %s",
  async (race) => {
    await mount();
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    await screen.findByText("APPLIED a.txt");
    const refreshRead = vi.fn();
    server.use(
      http.post(`${base}/source-control/apply`, () => staleListResponse()),
      http.post(`${base}/source-control/check`, () => {
        refreshRead();
        return race === "refresh"
          ? staleListResponse()
          : HttpResponse.json({ data: { changed: true, menu } });
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Show" }));
    const controls = within(screen.getByTestId("source-control-menu"));
    await userEvent.click(
      await controls.findByRole("button", { name: "Refresh list" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Show" })).toBeEnabled(),
    );
    if (race === "comparison")
      await userEvent.click(screen.getByRole("button", { name: "Show" }));
    await controls.findByRole("button", { name: "Refresh list" });
    expect(screen.getByText("APPLIED a.txt")).toBeVisible();
    expect(screen.getByText(/Previous comparison ·/)).toBeVisible();
    expect(screen.getByLabelText("one in Code")).toBeChecked();
    expect(refreshRead).toHaveBeenCalledTimes(1);
  },
);

it("does not claim a previous comparison exists when the first Show is stale", async () => {
  server.use(
    http.post(`${base}/source-control/apply`, () => staleListResponse()),
  );
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findAllByRole("button", { name: "Refresh list" });
  expect(screen.queryByText(/diff below still shows/)).toBeNull();
  expect(screen.queryByText(/Previous comparison ·/)).toBeNull();
});

it("preserves edits during refresh and withdraws the response after Stop", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let started = false;
  server.use(
    http.post(`${base}/source-control/apply`, () => staleListResponse()),
    http.post(`${base}/source-control/check`, async () => {
      started = true;
      await held;
      return HttpResponse.json({
        data: { changed: true, menu: { ...menu, private: true } },
      });
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  const controls = within(screen.getByTestId("source-control-menu"));
  await userEvent.click(
    await controls.findByRole("button", { name: "Refresh list" }),
  );
  await waitFor(() => expect(started).toBe(true));
  await userEvent.click(screen.getByLabelText("one in Code"));
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  await userEvent.click(screen.getByRole("button", { name: "Stop fixture" }));
  await act(async () => {
    release();
    await held;
  });
  await screen.findAllByText("Commit one");
  expect(screen.queryByText("APPLIED a.txt")).toBeNull();
  expect(screen.getByLabelText("Not yet pushed in Code")).toBeDisabled();
});

it("keeps checkbox and type edits made while refresh is in flight", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  server.use(
    http.post(`${base}/source-control/apply`, () => staleListResponse()),
    http.post(`${base}/source-control/check`, async () => {
      await held;
      return HttpResponse.json({
        data: { changed: true, menu: { ...menu, fingerprint: "fresh" } },
      });
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  const controls = within(screen.getByTestId("source-control-menu"));
  await userEvent.click(
    await controls.findByRole("button", { name: "Refresh list" }),
  );
  await userEvent.click(screen.getByLabelText("one in Code"));
  await userEvent.click(controls.getByRole("checkbox", { name: "Docs" }));
  await act(async () => {
    release();
    await held;
  });
  await screen.findByText("Checklist updated · Click Show");
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(controls.getByRole("checkbox", { name: "Docs" })).not.toBeChecked();
  expect(screen.getByText("APPLIED a.txt")).toBeVisible();
});

it("does not let a late manual refresh replace a newer count-entry checklist or clear its valid choices", async () => {
  await mount();
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  await screen.findByText("APPLIED a.txt");
  const oldMenu = structuredClone(menu);
  let release!: () => void;
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  let pushedReads = 0;
  server.use(
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      applies.push(await request.json());
      return staleListResponse();
    }),
    http.post(`${base}/source-control/pushed`, () => {
      pushedReads++;
      return HttpResponse.json({ data: { ...snapshot, private: false, menu } });
    }),
    http.post(`${base}/source-control/check`, async () => {
      await held;
      return HttpResponse.json({
        data: {
          changed: true,
          menu: oldMenu,
          missingCommits: { Code: ["one"] },
        },
      });
    }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Show" }));
  const controls = within(screen.getByTestId("source-control-menu"));
  await userEvent.click(
    await controls.findByRole("button", { name: "Refresh list" }),
  );
  menu = {
    ...menu,
    fingerprint: "newer-entry",
    repos: menu.repos.map((repo) => ({
      ...repo,
      commits: [commit("new"), ...repo.commits],
    })),
  };
  await userEvent.click(
    screen.getByRole("button", { name: "Open Code comparison" }),
  );
  await waitFor(() => expect(pushedReads).toBe(1));
  await waitFor(() =>
    expect(screen.getByLabelText("new in Code")).toBeChecked(),
  );
  await act(async () => {
    release();
    await held;
  });
  expect(screen.getByLabelText("new in Code")).toBeChecked();
  expect(screen.getByLabelText("one in Code")).toBeChecked();
  expect(controls.getByRole("checkbox", { name: "Docs" })).not.toBeChecked();
  expect(screen.queryByText(/Removed commit/)).toBeNull();
  expect(screen.queryByText(/Previous comparison ·/)).toBeNull();
});

it("loads one pushed checklist without enumerating local work", async () => {
  menu.repos = menu.repos.map((repo) => ({
    ...repo,
    live: { selectable: true },
  }));
  await mount();
  expect(loads).toBe(1);
  expect(screen.getByLabelText("Not yet pushed in Code")).toBeChecked();
  expect(screen.queryByText(/local commit|private.txt|0 files/)).toBeNull();
});

it("refreshes advanced history once on Show and preserves exclusions and the previous comparison", async () => {
  await mount();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Show" }));
  const stream = screen.getByTestId("source-control-diff-stream");
  stream.scrollTop = 123;
  const old = stream.textContent;
  await user.click(screen.getByLabelText("one in Code"));
  await user.click(
    within(screen.getByRole("group", { name: "File types" })).getByLabelText(
      "Docs",
    ),
  );
  menu = {
    ...menu,
    fingerprint: "advanced",
    repos: menu.repos.map((repo) =>
      repo.repoId === "Code"
        ? { ...repo, pushed: "new", commits: [commit("new"), ...repo.commits] }
        : repo,
    ),
  };
  await user.click(screen.getByRole("button", { name: "Show" }));
  expect(applies).toHaveLength(2);
  expect(checks).toBe(0);
  expect(loads).toBe(1);
  expect(screen.getAllByText(/Code has changed since this list/)).toHaveLength(
    2,
  );
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(screen.getByLabelText("new in Code")).not.toBeChecked();
  expect(
    within(screen.getByRole("group", { name: "File types" })).getByLabelText(
      "Docs",
    ),
  ).not.toBeChecked();
  expect(stream.textContent).toBe(old);
  expect(stream.scrollTop).toBe(123);
  expect(screen.queryByTestId("source-control-refresh")).toBeNull();
  expect(screen.getByText(/Previous comparison ·/)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Show" }));
  expect(applies).toHaveLength(3);
  expect(screen.queryByText(/has changed since this list/)).toBeNull();
});

it("reads current local work on each Show without refreshing unchanged pushed history", async () => {
  await mount();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Show" }));
  snapshot = {
    ...snapshot,
    patches: snapshot.patches.map((patch) => ({
      ...patch,
      file: {
        ...patch.file,
        diff: {
          ...patch.file.diff,
          hunks: [
            {
              startLine: 1,
              header: "@@ -0,0 +1 @@",
              lines: [
                { type: "addition", newLine: 1, content: "LATEST LOCAL WORK" },
              ],
            },
          ],
        },
      },
    })),
  };
  await user.click(screen.getByRole("button", { name: "Show" }));
  expect(await screen.findAllByText("LATEST LOCAL WORK")).toHaveLength(2);
  expect({ loads, checks }).toEqual({ loads: 1, checks: 0 });
  expect(applies).toHaveLength(2);
  expect(screen.queryByText(/list has been updated/)).toBeNull();
});

it("waits for known VM state even when the runtime identity stays the same", async () => {
  function SettlingWorkspace() {
    const { user } = useAuth();
    const [known, setKnown] = useState(false);
    return <WorkspaceLiveGateProvider value={{ ...gate(false), kind: known ? "stopped" : "unknown", runtimeKey: "same-runtime" }}>
      <output>{user?.id}</output><button onClick={() => setKnown(true)}>Settle workspace</button>
      <SourceControlProvider workspaceId={id}><SourceControlMenu /><SourceControlDiffWorkspace /></SourceControlProvider>
    </WorkspaceLiveGateProvider>;
  }
  menu.private = false;
  menu.repos = menu.repos.map((repo) => ({ ...repo, live: null }));
  renderApp(<SettlingWorkspace />);
  await screen.findByText("user-alex");
  expect(loads).toBe(0);
  await userEvent.click(screen.getByRole("button", { name: "Settle workspace" }));
  await screen.findAllByText("Commit one");
  expect(loads).toBe(1);
});


it("automatically removes a missing restored choice after the list has already captured current tips", async () => {
  sessionStorage.setItem(
    `g5:source-control:${id}:user-alex`,
    JSON.stringify({
      selection: {
        Code: { checked: ["gone"], followAll: false },
        Docs: { checked: [], followAll: false },
      },
      types: ["code"],
      comparison: "patches",
    }),
  );
  server.use(
    http.post(`${base}/source-control/apply`, async ({ request }) => {
      applies.push(await request.json());
      return HttpResponse.json({
        data: { refreshed: true, menu, missingCommits: { Code: ["gone"] } },
      });
    }),
  );
  renderApp(<App />, { route: "/g5/workspace?view=source-control" });
  await screen.findAllByText(/because it is no longer available/);
  expect(applies).toHaveLength(1);
  expect({ loads, checks }).toEqual({ loads: 1, checks: 0 });
  expect(screen.getByLabelText("one in Code")).not.toBeChecked();
  expect(screen.queryByText(/Selected revision gone/)).toBeNull();
  expect(screen.queryByTestId("source-control-refresh")).toBeNull();
  expect(screen.getByRole("button", { name: "Show" })).toBeDisabled();
  expect(screen.queryByText(/Previous comparison/)).toBeNull();
});
