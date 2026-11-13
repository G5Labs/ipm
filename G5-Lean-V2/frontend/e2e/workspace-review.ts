/** Real-stack acceptance fixtures for docs/review-flow.md (RV1–RV10).
 * No prototype imports or successful API stubs. New UI locators are the agreed
 * acceptance contract; missing production behavior must FAIL, never skip.
 */
import { createHash, randomUUID } from "node:crypto";
import {
  test as base,
  expect,
  type Page,
  type Locator,
  type Route,
  type Browser,
} from "@playwright/test";
import {
  APP_BASE_URL,
  authedApiViaRequest as api,
  activateSidebarSection,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
  readActiveSubject,
  readCurrentOrgContext,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  waitForLiveWorkspaceChanges,
  patchStoredUserSettings,
  TEST_ORG,
  USING_OAUTH_STUB,
  type WorkspaceClone,
} from "./helpers";
import { loginWithOAuthStub } from "./oauth";
import { readReviewFixtureFile } from "./review-fixture-read";
import { recordTiming, setTimingScope, timed } from "./timings";
export { expect, api };
export type Phase = "be" | "se" | "ie";
export type Vector = Record<string, string>;
export type Actor = { page: Page; id: string; name: string; email: string };
/** A distinct authenticated reader for approval integration regressions. */
export async function createReviewActor(
  page: Page,
  browser: Browser,
  orgId: string,
  role: string,
) {
  expect(USING_OAUTH_STUB, "Use the hermetic OAuth E2E stack").toBe(true);
  const token = randomUUID().replaceAll("-", "");
  const identity = {
    id: parseInt(token.slice(0, 12), 16),
    login: `rv-${token}`,
    email: `rv-${token}@g5.dev`,
    name: `${role} ${token.slice(0, 6)}`,
  };
  const invited = await api(page, `/api/v1/orgs/${orgId}/members`, "POST", {
    email: identity.email,
    role: "member",
  });
  expect([200, 201], JSON.stringify(invited.body)).toContain(invited.status);
  const context = await browser.newContext({
    storageState: { cookies: [], origins: [] },
  });
  try {
    const actorPage = await context.newPage();
    await loginWithOAuthStub(actorPage, identity, { targetOrgSlug: TEST_ORG });
    // A fresh context misses auth.setup's pin of the full sidebar
    // (g5.ui.sidebar-default); the hover rail's overlay would otherwise
    // sit over the center and intercept clicks after a rail hover.
    await patchStoredUserSettings(actorPage, {
      sidebarDisplayMode: "expanded",
    });
    return {
      context,
      actor: {
        page: actorPage,
        id: await readActiveSubject(actorPage),
        name: identity.name,
        email: identity.email,
      },
    };
  } catch (error) {
    await context.close();
    throw error;
  }
}
export type Definition = {
  id: string;
  name: string;
  latestVersion?: number;
  requireAllReviewers?: boolean;
  allowSelfApproval?: boolean;
};
export type Decision = {
  requestId: string;
  outcome: string;
  comment: string;
  actor: { userId: string };
  reviewedGitHeads: unknown;
};
export type Requirement = {
  state: string;
  definition: Definition;
  phase: Phase;
  requests: { requestId: string; actorUserId: string; state: string }[];
  decisionLog: Decision[];
};
export type ReviewFixture = {
  /** Completed transfer/fork evidence retains same-org member foreign keys. */
  retainedRuntimeParticipants: Set<string>;
  owner: Actor;
  reviewer: Actor;
  second: Actor;
  visitor: Actor;
  orgId: string;
  parent: WorkspaceClone;
  workspace: WorkspaceClone;
  security: Definition;
  business: Definition;
  base: Vector;
  pushed: Vector;
  codeRepo: string;
  docsRepo: string;
  codePath: string;
  docPath: string;
  before: string;
  after: string;
  /** Exact bytes of the pushed Project Doc at `docPath` (the merge/rebase
   * loops assert these on the parent and child without re-deriving them). */
  docBytes: string;
  task: string;
  implementation: string;
};

export function vector(value: unknown): Vector {
  expect(
    Array.isArray(value),
    "server returns a complete repository vector",
  ).toBe(true);
  const rows = value as { repoId: string; headCommit: string }[];
  const result: Vector = {};
  for (const row of rows) {
    expect(row.repoId).toBeTruthy();
    expect(row.headCommit).toMatch(/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/);
    expect(result).not.toHaveProperty(row.repoId);
    result[row.repoId] = row.headCommit;
  }
  expect(rows.length).toBeGreaterThanOrEqual(2);
  return result;
}
/** Fail one mutation without letting an unrelated background GET consume it. */
export async function interceptNextPost(
  page: Page,
  url: string,
  effect: (route: Route) => Promise<void>,
) {
  let claimed = false;
  const handler = async (route: Route) => {
    if (route.request().method() !== "POST" || claimed) return route.continue();
    claimed = true;
    // Finish the intercepted request before removing interception. Changing
    // patterns while it is pending can release it before abort/fetch runs.
    try {
      await effect(route);
    } finally {
      await page.unroute(url, handler);
    }
  };
  await page.route(url, handler);
}
export async function heads(page: Page, id: string): Promise<Vector> {
  const response = await api(page, `/api/v1/workspaces/${id}/branches`);
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return vector(response.body.data);
}
export async function activateAlignment(page: Page, path: string) {
  // Fresh role contexts miss auth.setup's full-sidebar preference. These
  // review flows click its stacked sections; pin the same layout for every actor.
  await patchStoredUserSettings(page, {
    showTabBar: true,
    sidebarDisplayMode: "expanded",
  });
  await openWorkspacePath(page, `${path}?view=activity`);
}
export async function landing(actor: Actor, f: ReviewFixture) {
  await openWorkspacePath(actor.page, `${f.workspace.path}?view=activity`);
  await expect(
    actor.page.getByRole("heading", { name: "Activity", exact: true }),
  ).toBeVisible();
}
export async function approvals(actor: Actor, f: ReviewFixture) {
  await openWorkspacePath(actor.page, `${f.workspace.path}?view=approvals`);
  await expect(
    actor.page.getByRole("heading", {
      name: /^Approvals Required to merge into /,
    }),
  ).toBeVisible();
  await expect(actor.page.getByTestId("workspace-commit")).toHaveCount(0);
  const reviews = actor.page.getByTestId("approvals-reviews-toggle");
  if ((await reviews.getAttribute("aria-expanded")) === "false")
    await reviews.click();
  await expect(actor.page.getByTestId("workspace-commit")).toHaveCount(0);
}
export async function approvalTimeline(page: Page, name: string) {
  await expect(page.getByTestId("workspace-approvals-outgoing")).toBeVisible();
  const legacy = page.getByTestId("workspace-approvals-legacy");
  if (
    (await legacy
      .getByTestId("workspace-approvals-historical-requirement")
      .filter({ hasText: name })
      .count()) &&
    (await legacy.getAttribute("open")) === null
  )
    await legacy.getByTestId("workspace-approvals-legacy-toggle").click();
  const button = page.getByRole("button", {
    name: `Timeline for ${name}`,
    exact: true,
  });
  await expect(button).toBeVisible();
  if ((await button.getAttribute("aria-expanded")) === "false")
    await button.click();
  const timeline = page.getByRole("region", {
    name: `Timeline for ${name}`,
    exact: true,
  });
  await expect(timeline).toBeVisible();
  return timeline;
}
export async function setPhase(f: ReviewFixture, phase: Phase) {
  const r = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}`,
    "PATCH",
    { phase },
  );
  expect(r.status, JSON.stringify(r.body)).toBe(200);
}
export async function stop(f: ReviewFixture) {
  await runWorkspaceVmAction(f.owner.page, f.workspace.id, "stop");
  await waitForWorkspaceVmStatus(f.owner.page, f.workspace.id, "stopped");
}
export async function write(
  page: Page,
  ws: WorkspaceClone,
  repoId: string,
  path: string,
  content: string,
) {
  await timed("write", async () => {
    const read = await readReviewFixtureFile(() =>
      api(page, `/api/v1/workspaces/${ws.id}/files/${path}?repoId=${repoId}`),
    );
    expect([200, 404], JSON.stringify(read.body)).toContain(read.status);
    const expectedHash = read.status === 404 ? null : read.body.data.hash;
    if (read.status === 200) expect(expectedHash).toEqual(expect.any(String));
    const r = await api(
      page,
      `/api/v1/workspaces/${ws.id}/files/${path}?repoId=${repoId}`,
      "PUT",
      { content, encoding: "utf-8", expectedHash },
    );
    expect([200, 201], JSON.stringify(r.body)).toContain(r.status);
  });
}
const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
/** True when `page` already shows this workspace's terminal, connected. A
 * loop of commits (RV9) then keeps its session instead of reloading the view
 * per commit; any navigation, reconnect or other surface opens it afresh. */
async function connectedTerminalOpen(page: Page, ws: WorkspaceClone) {
  const url = new URL(page.url());
  // A fresh actor page must take the normal authenticated navigation path.
  if (url.protocol !== "http:" && url.protocol !== "https:") return false;
  if (
    url.pathname !== new URL(ws.path, url).pathname ||
    url.searchParams.get("view") !== "terminal"
  )
    return false;
  const status = page.getByText(/^Workspace terminal · /);
  return (
    (await status.count()) === 1 &&
    (await status.innerText()) === "Workspace terminal · Connected"
  );
}
export async function shell(page: Page, ws: WorkspaceClone, command: string) {
  const terminal = page.getByTestId("workspace-terminal");
  const status = page.getByText(/^Workspace terminal · /);
  await timed("shell.open-terminal", async () => {
    if (await connectedTerminalOpen(page, ws)) return;
    // The root intentionally lands on Activity, which may replace a restored
    // terminal a render after the shell appears. Address the desired surface
    // explicitly instead of treating that transient visibility as readiness.
    await openWorkspacePath(page, `${ws.path}?view=terminal`);
    await expect(page).toHaveURL(/view=terminal/);
    // Visiting the other three actors can exceed the dispatcher's 15-second
    // detached-PTY grace. Reopen through the real UI after that explicit expiry;
    // transport failures and reconnect failures must still fail this fixture.
    await expect(status).toHaveText(
      /^Workspace terminal · (Connected|Session ended)$/,
      { timeout: 30_000 },
    );
    if ((await status.innerText()).endsWith("Session ended")) {
      await page
        .getByRole("button", { name: "Open new terminal", exact: true })
        .click();
    }
    await expect(status).toHaveText("Workspace terminal · Connected", {
      timeout: 30_000,
    });
  });
  await timed("shell.command", async () => {
    const input = terminal.locator(".xterm-helper-textarea");
    await input.focus();
    await input.pressSequentially("stty -echo");
    await input.press("Enter");
    const token = randomUUID().replaceAll("-", "");
    await input.pressSequentially(
      `${command} && printf '%s_%s\\n' RV_DONE ${token}`,
    );
    await input.press("Enter");
    await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
      `RV_DONE_${token}`,
      { timeout: 30_000 },
    );
  });
}
export async function commit(
  page: Page,
  ws: WorkspaceClone,
  files: { repoId: string; path: string }[],
  message: string,
) {
  const live = await timed("commit.live-checkout", () =>
    waitForLiveWorkspaceChanges(page, ws.id),
  );
  const commands = files.map(({ repoId, path }) => {
    const member = live.members.find((m) => m.repo_id === repoId);
    expect(member, `VM checkout for ${repoId}`).toBeTruthy();
    const git = `git -C ${quote(member!.root_path)}`;
    return `${git} add -A -- ${quote(path)} && ${git} -c user.email=e2e@g5.dev -c user.name=Acceptance commit -qm ${quote(message)} -- ${quote(path)}`;
  });
  await shell(page, ws, commands.join(" && "));
}
export async function push(page: Page, ws: WorkspaceClone): Promise<Vector> {
  return timed("push", async () => {
    const r = await api(
      page,
      `/api/v1/workspaces/${ws.id}/changes/push`,
      "POST",
      {},
    );
    expect(r.status, JSON.stringify(r.body)).toBe(200);
    expect(r.body.data.status, JSON.stringify(r.body)).toBe("succeeded");
    return heads(page, ws.id);
  });
}
export async function revise(
  f: ReviewFixture,
  kind: "code" | "docs" | "both",
  label: string,
) {
  const files = [];
  if (kind !== "docs") {
    await write(
      f.owner.page,
      f.workspace,
      f.codeRepo,
      f.codePath,
      `${f.after}${label} code\n`,
    );
    files.push({ repoId: f.codeRepo, path: f.codePath });
  }
  if (kind !== "code") {
    await write(
      f.owner.page,
      f.workspace,
      f.docsRepo,
      f.docPath,
      `# Review document\n${label} docs\n`,
    );
    files.push({ repoId: f.docsRepo, path: f.docPath });
  }
  await commit(f.owner.page, f.workspace, files, label);
  return push(f.owner.page, f.workspace);
}
export async function submitPr(f: ReviewFixture) {
  const r = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}/pull-requests`,
    "POST",
    {
      targetWorkspaceId: f.parent.id,
      title: "Acceptance implementation",
      description: f.implementation,
    },
  );
  expect(r.status, JSON.stringify(r.body)).toBe(200);
  expect(vector(r.body.data.pullRequest.sourceHeads)).toEqual(f.pushed);
  return String(r.body.data.pullRequest.id);
}
export async function requirements(
  f: ReviewFixture,
  actor = f.owner,
): Promise<Requirement[]> {
  const r = await api(
    actor.page,
    `/api/v1/workspaces/${f.workspace.id}/approvals`,
  );
  expect(r.status, JSON.stringify(r.body)).toBe(200);
  return r.body.data.outgoing.requirements;
}
export async function request(
  f: ReviewFixture,
  definition = f.security,
  actors = [f.reviewer],
  phase: Phase = "se",
) {
  const r = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}/approvals/requests`,
    "POST",
    {
      definitionId: definition.id,
      definitionVersion: definition.latestVersion ?? 1,
      phase,
      actorUserIds: actors.map((a) => a.id),
    },
  );
  expect([200, 201], JSON.stringify(r.body)).toContain(r.status);
  expect(r.body.data.requests).toHaveLength(actors.length);
  const rows = await requirements(f);
  const requirement = rows.find((row) => row.definition.id === definition.id)!;
  expect(requirement).toBeTruthy();
  return actors.map((actor) => {
    const found = requirement.requests.find(
      (r) => r.actorUserId === actor.id && r.state === "pending",
    );
    expect(found, `addressed request for ${actor.name}`).toBeTruthy();
    return found!.requestId;
  });
}
/**
 * The one Request approvals dialog (g5.governance.approval.request-composer).
 * Its accessible name follows its step ("Request approvals", then "Review
 * request"), so address it by test id across both steps.
 */
export function requestDialog(page: Page) {
  return page.getByTestId("approval-request-dialog");
}
/** Page-level Request: a fresh dialog with no mode selected. */
export async function openRequestDialog(actor: Actor, f: ReviewFixture) {
  await approvals(actor, f);
  await actor.page.getByTestId("workspace-request-review").click();
  const dialog = requestDialog(actor.page);
  await expect(dialog).toBeVisible();
  // A page-level open with an unfinished failed batch reopens at its retry
  // step; start a new selection from there.
  const fresh = dialog.getByTestId("approval-request-new-selection");
  if (await fresh.count()) await fresh.click();
  await expect(
    dialog.getByRole("heading", { name: "Request approvals", exact: true }),
  ).toBeVisible();
  return dialog;
}
/** The Pick group of one requirement: its recipients' checkboxes. */
export function pickGroup(dialog: Locator, definition: Definition) {
  return dialog.getByRole("group", { name: definition.name, exact: true });
}
export async function requestComposer(
  actor: Actor,
  f: ReviewFixture,
  definition = f.security,
) {
  const dialog = await openRequestDialog(actor, f);
  await chooseRequestMode(dialog, "pick");
  return { dialog, group: pickGroup(dialog, definition) };
}
/** The Request mode ▾ trigger; it reads "Select" until a mode is chosen. */
export function requestModeTrigger(dialog: Locator) {
  return dialog.getByTestId("approval-request-mode");
}
export async function chooseRequestMode(
  dialog: Locator,
  mode: "all" | "agents" | "pick",
) {
  await requestModeTrigger(dialog).click();
  await dialog.getByTestId(`approval-request-mode-option-${mode}`).click();
}
export async function selectHuman(group: Locator, name: string) {
  await group.getByRole("checkbox", { name, exact: true }).check();
}
/** The `Selected approvals` pairs, as `<requirement> → <recipient>`. */
export async function selectedPairs(dialog: Locator) {
  return (
    await dialog.getByTestId("approval-request-pair").allTextContents()
  ).map((text) => text.replace(/\s+/g, " ").trim());
}
export async function previewRequests(dialog: Locator) {
  await dialog.getByTestId("approval-request-preview").click();
  await expect(
    dialog.getByRole("heading", { name: "Review request", exact: true }),
  ).toBeVisible();
}
/**
 * Submit the reviewed request. Full success closes the dialog after the
 * roster refresh; failures keep it open with a per-recipient retry.
 */
export async function submitRequests(dialog: Locator) {
  await dialog.getByTestId("review-request-submit").click();
}

export async function reviewDialog(
  actor: Actor,
  f: ReviewFixture,
  definition = f.security,
  phase: Phase = "se",
) {
  await approvals(actor, f);
  await actor.page
    .getByTestId("workspace-approvals-requirement")
    .filter({ hasText: definition.name })
    .getByRole("button", { name: "Approve", exact: true })
    .click();
  const dialog = actor.page.getByRole("dialog", {
    name: "Submit review",
    exact: true,
  });
  await dialog
    .getByLabel("Approval type")
    .selectOption({ label: definition.name });
  return dialog;
}
export async function decide(
  actor: Actor,
  f: ReviewFixture,
  definition: Definition,
  phase: Phase,
  outcome: "Approve" | "Request changes",
  comment: string,
) {
  const dialog = await reviewDialog(actor, f, definition, phase);
  await dialog.getByRole("radio", { name: new RegExp(`^${outcome}`) }).check();
  await dialog.getByLabel("Review comment").fill(comment);
  const response = actor.page.waitForResponse(
    (r) =>
      r.request().method() === "POST" &&
      r.url().endsWith(`/workspaces/${f.workspace.id}/approvals/decisions`),
  );
  await dialog
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  expect((await response).status()).toBe(200);
  await expect(dialog).toHaveCount(0);
}
export async function assertDecision(
  f: ReviewFixture,
  requestId: string,
  actor: Actor,
  outcome: string,
  expected: Vector,
  comment: string,
) {
  const rows = await requirements(f);
  const decisions = rows
    .flatMap((r) => r.decisionLog)
    .filter((d) => d.requestId === requestId);
  expect(decisions).toHaveLength(1);
  expect(decisions[0]).toMatchObject({
    outcome,
    actor: { userId: actor.id },
    comment,
  });
  expect(vector(decisions[0].reviewedGitHeads)).toEqual(expected);
}
export async function vectorTable(scope: Locator | Page, expected: Vector) {
  // One ROW is one complete workspace revision; repositories are COLUMNS.
  const table = scope.getByRole("table", {
    name: "Workspace commit vectors",
    exact: true,
  });
  await expect(table).toBeVisible();
  await expect(table.getByRole("columnheader")).toHaveCount(
    Object.keys(expected).length + 1,
  );
  let row = table.getByRole("row");
  for (const oid of Object.values(expected))
    row = row.filter({
      has: table.page().getByLabel(`Commit ${oid}`, { exact: true }),
    });
  await expect(row).toHaveCount(1);
  for (const [repoId, oid] of Object.entries(expected)) {
    await expect(
      row
        .locator(`[data-repo-id="${repoId}"]`)
        .getByLabel(`Commit ${oid}`, { exact: true }),
    ).toBeVisible();
  }
  return row;
}
/** The landing's repository rows: one per repository, each with its commit. */
export async function repositoryRows(scope: Locator | Page, expected: Vector) {
  const rows = scope.getByTestId("landing-repository-rows").first();
  await expect(rows).toBeVisible();
  await expect(rows.getByTestId("landing-repository-row")).toHaveCount(
    Object.keys(expected).length,
  );
  for (const [repoId, oid] of Object.entries(expected)) {
    await expect(
      rows
        .locator(`[data-repo-id="${repoId}"]`)
        .getByLabel(`Commit ${oid}`, { exact: true }),
    ).toBeVisible();
  }
}
export async function contentView(
  actor: Actor,
  f: ReviewFixture,
  view: "source-control" | "docs" | "files" | "ontology",
  mode: string,
) {
  await openWorkspacePath(actor.page, f.workspace.path);
  await activateSidebarSection(actor.page, `sidebar-${view}-tab`);
  if (mode !== "All files") {
    const modes: Record<string, string> = {
      "Pushed vs workspace base": "base",
      "Pushed vs parent": "parent",
      "Since my last review": "last_review",
      "All unpushed changes": "unpushed",
      "Uncommitted changes": "uncommitted",
    };
    const query = new URLSearchParams({
      view: "source-control",
      mode: modes[mode],
      reviewPath: "",
    });
    if (modes[mode] === "last_review") {
      query.set("definitionId", f.security.id);
      query.set("phase", "se");
    }
    await openWorkspacePath(actor.page, `${f.workspace.path}?${query}`);
  }
}
export async function openDiff(
  actor: Actor,
  f: ReviewFixture,
  view: "source-control" | "docs" | "files",
  path: string,
) {
  // Existing discussion links resolve an immutable comparison directly. The
  // ordinary Files/Docs menus no longer carry a comparison picker.
  const query = new URLSearchParams({
    view: view === "docs" ? "source-control" : view,
    mode: "base",
    reviewRepo: path === f.docPath ? f.docsRepo : f.codeRepo,
    reviewPath: path,
  });
  await openWorkspacePath(actor.page, `${f.workspace.path}?${query}`);
  const diff = actor.page.getByTestId("workspace-review-diff");
  await expect(diff).toBeVisible();
  return diff;
}
export const timeline = (page: Page) =>
  page.getByRole("region", { name: "Workspace timeline", exact: true });
export async function timelineEntry(
  actor: Actor,
  f: ReviewFixture,
  text: string,
) {
  await landing(actor, f);
  const entry = timeline(actor.page)
    .getByRole("article")
    .filter({ hasText: text });
  await expect(entry).toHaveCount(1);
  // The landing begins at the task summary; replies load when their thread
  // enters view. Read the thread as a person would, rather than asserting on
  // an offscreen node that has not requested its discussion yet.
  await entry.scrollIntoViewIfNeeded();
  await expect(entry).toBeInViewport();
  return entry;
}

export const test = base.extend<{ review: ReviewFixture }>({
  review: async ({ page, browser }, provide, info) => {
    const cleanupErrors: unknown[] = [];
    const retainedRuntimeParticipants = new Set<string>();
    // Required acceptance prerequisites fail closed. Feature absence is never a skip.
    expect(USING_OAUTH_STUB, "Use the hermetic OAuth E2E stack").toBe(true);
    expect(
      [
        "localhost",
        "127.0.0.1",
        "[::1]",
        ...(process.env.E2E_DISPOSABLE_STACK_HOST
          ? [process.env.E2E_DISPOSABLE_STACK_HOST]
          : []),
        ...(process.env.G5_CI_COMPOSE_PROJECT
          ? [`${process.env.G5_CI_COMPOSE_PROJECT}-client`]
          : []),
        "client",
        "host.docker.internal",
      ],
      "Acceptance fixtures require the disposable local/CI stack, never QA",
    ).toContain(new URL(APP_BASE_URL).hostname);
    const orgId = (await readCurrentOrgContext(page)).orgId;
    const owner: Actor = {
      page,
      id: await readActiveSubject(page),
      name: "Workspace owner",
      email: "",
    };
    const contexts = [];
    const workspaces: WorkspaceClone[] = [];
    const actors: Actor[] = [];
    // Phase timings (g5.quality.delivery.ci.timing-evidence) measure what a
    // shared starting point could save; they change nothing about the setup.
    const setupStarted = Date.now();
    let lapStarted = setupStarted;
    const lap = (phase: string) => {
      const now = Date.now();
      recordTiming(phase, now - lapStarted);
      lapStarted = now;
    };
    let bodyStarted: number | undefined;
    setTimingScope("setup");
    try {
      for (const role of ["Reviewer", "Second", "Visitor"]) {
        const { context, actor } = await createReviewActor(
          page,
          browser,
          orgId,
          role,
        );
        contexts.push(context);
        actors.push(actor);
      }
      expect(new Set([owner, ...actors].map((a) => a.id)).size).toBe(4);
      lap("actors");
      const task = `Task ${randomUUID()}: retain workspace context.\n\n${"Acceptance detail. ".repeat(200)}\nEND OF TASK`;
      const parent = await createWorkspaceClone(page, {
        namePrefix: "rv-parent",
        start: true,
      });
      workspaces.push(parent);
      const live = await waitForLiveWorkspaceChanges(page, parent.id);
      const detail = await api(page, `/api/v1/workspaces/${parent.id}`);
      expect(detail.status).toBe(200);
      const docsRepo = String(detail.body.data.docsRepoId);
      const codeRepo = live.members.find(
        (m) => m.repo_id !== docsRepo,
      )!.repo_id;
      const codePath = `rv-${randomUUID()}.txt`;
      lap("parent-boot");
      const before = "unchanged context\nbefore review\nanchor line\n";
      const after = "unchanged context\nafter review\nanchor line\n";
      await write(page, parent, codeRepo, codePath, before);
      await commit(
        page,
        parent,
        [{ repoId: codeRepo, path: codePath }],
        "Review base",
      );
      const savedBase = await push(page, parent);
      lap("parent-publish");
      await runWorkspaceVmAction(page, parent.id, "stop");
      await waitForWorkspaceVmStatus(page, parent.id, "stopped");
      lap("parent-stop");
      const definition = async (
        name: string,
        members: Actor[],
        phase: Phase,
      ) => {
        const r = await api(
          page,
          `/api/v1/orgs/${orgId}/approval-definitions`,
          "POST",
          {
            name: `${name} ${parent.id.slice(0, 8)}`,
            humanUserIds: members.map((a) => a.id),
            phases: [phase],
          },
        );
        expect(r.status, JSON.stringify(r.body)).toBe(201);
        const attached = await api(
          page,
          `/api/v1/workspaces/${parent.id}/approvals/requirements`,
          "POST",
          { definitionId: r.body.data.id },
        );
        expect(attached.status, JSON.stringify(attached.body)).toBe(200);
        return { id: String(r.body.data.id), name: String(r.body.data.name) };
      };
      const security = await definition(
        "Security",
        [actors[0], actors[1], actors[2], owner],
        "se",
      );
      const business = await definition("Business", [actors[0]], "be");
      lap("approval-definitions");
      const workspace = await cloneWorkspaceById(page, parent.id, {
        namePrefix: "rv-workspace",
        description: task,
        start: true,
      });
      workspaces.push(workspace);
      await waitForWorkspaceVmStatus(page, workspace.id, "running");
      expect(await heads(page, workspace.id)).toEqual(savedBase);
      lap("child-boot");
      const docPath = `docs/${workspace.id}/review.md`;
      const docBytes = "# Review document\nfirst pushed docs\n";
      const doc = await api(
        page,
        `/api/v1/workspaces/${workspace.id}/docs`,
        "POST",
        {
          name: "Review document",
          docType: "design",
          filePath: docPath,
          content: docBytes,
        },
      );
      expect(doc.status, JSON.stringify(doc.body)).toBe(201);
      await write(page, workspace, codeRepo, codePath, after);
      await commit(
        page,
        workspace,
        [
          { repoId: codeRepo, path: codePath },
          { repoId: docsRepo, path: docPath },
        ],
        "Review implementation",
      );
      const pushed = await push(page, workspace);
      lap("child-publish");
      for (const actor of [owner, ...actors])
        await activateAlignment(actor.page, workspace.path);
      lap("page-setup");
      setTimingScope(undefined);
      recordTiming("fixture.setup", Date.now() - setupStarted);
      setTimingScope("body");
      bodyStarted = Date.now();
      await provide({
        retainedRuntimeParticipants,
        owner,
        reviewer: actors[0],
        second: actors[1],
        visitor: actors[2],
        orgId,
        parent,
        workspace,
        security,
        business,
        base: savedBase,
        pushed,
        codeRepo,
        docsRepo,
        codePath,
        docPath,
        before,
        after,
        docBytes,
        task,
        implementation: `Implementation ${workspace.id}: preserve explicit revision evidence.\n\n${"Decision detail. ".repeat(200)}\nEND OF IMPLEMENTATION`,
      });
    } finally {
      const cleanupStarted = Date.now();
      setTimingScope(undefined);
      if (bodyStarted !== undefined)
        recordTiming("fixture.body", cleanupStarted - bodyStarted);
      setTimingScope("cleanup");
      for (const ws of workspaces.reverse()) {
        try {
          await deleteWorkspaceById(page, ws.id);
        } catch (error) {
          cleanupErrors.push(error);
        }
      }
      for (const actor of actors) {
        try {
          const removed = await api(
            page,
            `/api/v1/orgs/${orgId}/members/${actor.id}`,
            "DELETE",
          );
          if (removed.status === 422) {
            // The route's member-ID admission runs before remove_member's
            // not-found response. Revocation tests already removed this actor.
            expect(removed.body.error?.details).toMatchObject({
              field: "userId",
              details: { org_id: orgId, missing_user_ids: [actor.id] },
            });
          } else if (
            removed.status === 409 &&
            retainedRuntimeParticipants.has(actor.id)
          ) {
            // Completed transfers and review forks retain same-org membership
            // FKs after deletion. Keep only this known historical reference
            // until the disposable stack is destroyed; never waive an active
            // owner conflict or a failure for an unrelated fixture member.
            expect(removed.body.error).toMatchObject({
              code: "CONFLICT",
              message:
                "Conflict: Cannot remove member: the member is referenced by active workspace ownership or a durable runtime operation; transfer/delete the workspace or finish the operation first",
            });
          } else {
            expect([200, 204, 404], JSON.stringify(removed.body)).toContain(
              removed.status,
            );
          }
        } catch (error) {
          cleanupErrors.push(error);
        }
      }
      for (const context of contexts) {
        try {
          await context.close();
        } catch (error) {
          cleanupErrors.push(error);
        }
      }
      setTimingScope(undefined);
      recordTiming("fixture.cleanup", Date.now() - cleanupStarted);
      if (cleanupErrors.length) {
        await info.attach("review-fixture-cleanup", {
          body: cleanupErrors.map(String).join("\n"),
          contentType: "text/plain",
        });
      }
    }
    if (cleanupErrors.length && info.status === info.expectedStatus)
      throw new AggregateError(cleanupErrors, "Review fixture cleanup failed");
  },
});
