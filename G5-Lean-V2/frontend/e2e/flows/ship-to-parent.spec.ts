// =============================================================================
// Ship to parent — the end of the product's main journey, through the UI
// =============================================================================
// Covers the two Core shipping stories the review loops stop short of
// (audit 2026-10-30; g5.workspace.merge, g5.git.multi-repo-merge,
// g5.governance.approval.requirements, g5.workspace.rebase,
// g5.git.live-rebase):
//
//   SH1  The owner creates the pull request from the workbench ribbon, the
//        parent's configured approval requirements block Ship to parent, a
//        reviewer approves them, the owner Ships, and the parent's branch
//        then holds the exact pushed code and Project Doc bytes for every
//        member repository while the pull request shows merged.
//
//   SH2  The parent advances, the running child rebases onto it from the UI,
//        and the child then holds both the parent's new bytes and its own
//        unpublished implementation across code and docs.
//
//   SH3  (same fixture, after SH2) With a pull request open and the child's
//        VM stopped, Prepare to Ship offers Start and ship with agent with
//        its note (g5.git.pr-auto-merge.with-agent, g5.chat.start-and-send).
//
// Reuses the RV real-stack fixtures (frontend/e2e/workspace-review.ts): the
// parent is a stopped, published workspace with Security (se) and Business
// (be) requirements attached; the child is a running clone that has pushed a
// code change and a Project Doc. Only the headline actions — Create PR,
// Approve, Ship, Rebase — are driven; the review API helpers set up the
// requirements and requests the product itself exposes.
// =============================================================================

import { randomUUID } from "node:crypto";
import {
  test,
  expect,
  api,
  decide,
  request,
  commit,
  push,
  write,
  type ReviewFixture,
} from "../workspace-review";
import {
  openWorkspacePath,
  readWorkspaceVm,
  runWorkspaceVmAction,
  waitForWorkspaceLoad,
  waitForWorkspaceVmStatus,
  type WorkspaceClone,
} from "../helpers";

test.describe.configure({ timeout: 600_000 });

/** Exact durable bytes of one repository file, as a person's reader sees. */
async function readFile(
  f: ReviewFixture,
  ws: WorkspaceClone,
  repoId: string,
  path: string,
): Promise<string> {
  const r = await api(
    f.owner.page,
    `/api/v1/workspaces/${ws.id}/files/${path}?repoId=${repoId}`,
  );
  expect(r.status, JSON.stringify(r.body)).toBe(200);
  return String((r.body?.data ?? r.body).content);
}

/** Open the child's workbench shell and select an engineering phase. */
async function openPhase(f: ReviewFixture, phase: "be" | "se" | "ie") {
  await openWorkspacePath(f.owner.page, f.workspace.path);
  await waitForWorkspaceLoad(f.owner.page);
  await f.owner.page.getByTestId(`phase-chevron-${phase}`).click();
}

test("SH1: approvals block the merge until a reviewer approves; Ship lands the exact bytes in the parent", async ({
  review: f,
}) => {
  // The owner creates the pull request from the Integration Engineering ribbon.
  await openPhase(f, "ie");
  await f.owner.page.getByTestId("ribbon-action-create-pr").click();
  await expect(
    f.owner.page.getByRole("heading", { name: /Prepare to Ship/ }),
  ).toBeVisible({ timeout: 15_000 });
  await f.owner.page.getByTestId("prepare-to-ship-create").click();
  await expect(
    f.owner.page.getByTestId("prepare-to-ship-result"),
  ).toContainText("PR created.", { timeout: 30_000 });
  // The PR captured the child's complete pushed vector.
  const prList = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.parent.id}/pull-requests?role=target&status=all`,
  );
  expect(prList.status, JSON.stringify(prList.body)).toBe(200);
  const openPr = (prList.body.data as { id: string; status: string }[]).find(
    (pr) => pr.status !== "merged" && pr.status !== "closed",
  );
  expect(openPr, JSON.stringify(prList.body)).toBeTruthy();
  const prId = String(openPr!.id);

  // Ship to parent is refused while the requirements are unmet.
  await expect(
    f.owner.page.getByTestId("prepare-to-ship-ship-button"),
  ).toBeDisabled();
  await expect(
    f.owner.page.getByTestId("prepare-to-ship-ship-note"),
  ).toContainText(/Approval\(s\) required before merging/);
  await f.owner.page.getByTestId("prepare-to-ship-close-button").click();

  // With the PR open, the parent's requirements each address the reviewer as
  // outgoing requests to settle (g5.governance.approval.request).
  await request(f, f.security, [f.reviewer], "se");
  await request(f, f.business, [f.reviewer], "be");

  // A reviewer approves both requirements through the approvals surface.
  await decide(
    f.reviewer,
    f,
    f.security,
    "se",
    "Approve",
    "SH1 security approved",
  );
  await decide(
    f.reviewer,
    f,
    f.business,
    "be",
    "Approve",
    "SH1 business approved",
  );

  // The owner reopens the ship surface: the merge is now admitted.
  await openPhase(f, "ie");
  await f.owner.page.getByTestId("ribbon-action-merge").click();
  await expect(
    f.owner.page.getByRole("heading", { name: /Prepare to Ship/ }),
  ).toBeVisible({ timeout: 15_000 });
  const ship = f.owner.page.getByTestId("prepare-to-ship-ship-button");
  await expect(ship).toBeEnabled({ timeout: 20_000 });
  await expect(
    f.owner.page.getByTestId("prepare-to-ship-ship-note"),
  ).not.toContainText(/Approval\(s\) required/);
  await ship.click();
  await expect(ship).toHaveText("Shipped", { timeout: 60_000 });
  await expect(
    f.owner.page.getByTestId("prepare-to-ship-ship-note"),
  ).toContainText("Merged into");

  // The parent's branch holds the exact pushed bytes for every member
  // repository (g5.git.multi-repo-merge), readable without starting its VM.
  expect(await readFile(f, f.parent, f.codeRepo, f.codePath)).toBe(f.after);
  expect(await readFile(f, f.parent, f.docsRepo, f.docPath)).toBe(f.docBytes);

  // The pull request shows merged exactly once.
  const merged = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.parent.id}/pull-requests?role=target&status=all`,
  );
  expect(merged.status, JSON.stringify(merged.body)).toBe(200);
  const mergedPr = (merged.body.data as { id: string; status: string }[]).find(
    (pr) => String(pr.id) === prId,
  );
  expect(mergedPr?.status, JSON.stringify(merged.body)).toBe("merged");
});

test("SH2: the parent advances and the running child rebases onto it, keeping both sides' bytes; SH3: the stopped child offers Start and ship with agent", async ({
  review: f,
}) => {
  // The parent advances with a new, non-conflicting file.
  const parentFile = `parent-advance-${randomUUID()}.txt`;
  const parentBytes = `parent advanced ${parentFile}\n`;
  await runWorkspaceVmAction(f.owner.page, f.parent.id, "resume");
  await waitForWorkspaceVmStatus(f.owner.page, f.parent.id, "running");
  await write(f.owner.page, f.parent, f.codeRepo, parentFile, parentBytes);
  await commit(
    f.owner.page,
    f.parent,
    [{ repoId: f.codeRepo, path: parentFile }],
    "Parent advance",
  );
  await push(f.owner.page, f.parent);
  await runWorkspaceVmAction(f.owner.page, f.parent.id, "stop");
  await waitForWorkspaceVmStatus(f.owner.page, f.parent.id, "stopped");

  // The running child rebases onto the parent from the workbench ribbon.
  await openPhase(f, "se");
  await f.owner.page.getByTestId("ribbon-action-rebase").click();
  await expect(
    f.owner.page.getByRole("heading", { name: /Rebase Workspace/ }),
  ).toBeVisible({ timeout: 15_000 });
  const start = f.owner.page.getByTestId("workspace-merge-confirm-start");
  await expect(start).toBeEnabled({ timeout: 20_000 });
  // Confirm runs a preflight; with changes to pull it opens a clean-rebase
  // review step whose "Rebase Workspace" approval starts the rebase.
  await start.click();
  const approve = f.owner.page.getByTestId("workspace-rebase-approve");
  await expect(approve).toBeVisible({ timeout: 30_000 });
  const rebased = f.owner.page.waitForResponse(
    (r) =>
      r.request().method() === "POST" &&
      r.url().includes(`/api/v1/workspaces/${f.workspace.id}/rebase-merge`) &&
      !r.url().endsWith("/preflight"),
  );
  await approve.click();
  const response = await rebased;
  expect(response.status(), await response.text()).toBe(200);
  // A clean rebase the VM applied — not a conflict resolution task.
  expect(
    (await response.json()).data.rebase,
    await response.text(),
  ).toBeTruthy();

  // The child now holds the parent's new bytes and its own unpublished
  // implementation across code and docs (g5.git.rebase-history).
  await expect
    .poll(() => readFile(f, f.workspace, f.codeRepo, parentFile), {
      timeout: 60_000,
    })
    .toBe(parentBytes);
  expect(await readFile(f, f.workspace, f.codeRepo, f.codePath)).toBe(f.after);
  expect(await readFile(f, f.workspace, f.docsRepo, f.docPath)).toBe(
    f.docBytes,
  );

  // SH3: with a pull request open and the child's VM stopped, Prepare to
  // Ship's agent control is the compound Start and ship with agent
  // (g5.git.pr-auto-merge.with-agent, g5.chat.start-and-send): offered,
  // labelled as a Start, and explained; nothing is clicked here, so no VM
  // starts and no agent runs.
  const created = await api(
    f.owner.page,
    `/api/v1/workspaces/${f.workspace.id}/pull-requests`,
    "POST",
    { targetWorkspaceId: f.parent.id },
  );
  expect(created.status, JSON.stringify(created.body)).toBe(200);
  await runWorkspaceVmAction(f.owner.page, f.workspace.id, "stop");
  await waitForWorkspaceVmStatus(f.owner.page, f.workspace.id, "stopped");
  await openPhase(f, "ie");
  await f.owner.page.getByTestId("ribbon-action-merge").click();
  await expect(
    f.owner.page.getByRole("heading", { name: /Prepare to Ship/ }),
  ).toBeVisible({ timeout: 15_000 });
  const agent = f.owner.page.getByTestId("prepare-to-ship-ship-with-agent");
  await expect(agent).toHaveText(/Start and ship with agent/);
  await expect(agent).toBeEnabled({ timeout: 20_000 });
  await expect(
    f.owner.page.getByTestId("prepare-to-ship-start-and-ship-note"),
  ).toContainText("starts it, turns on Merge when ready");
  await f.owner.page.getByTestId("prepare-to-ship-close-button").click();
  expect((await readWorkspaceVm(f.owner.page, f.workspace.id)).status).toBe(
    "stopped",
  );
});
