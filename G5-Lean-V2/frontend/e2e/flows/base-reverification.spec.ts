// =============================================================================
// Base re-verification from Home — the real stack end to end
// =============================================================================
// Covers g5.reverification (run record, scope, status, workspace) and
// g5.ui.home-reverification(.dialog) against the live API:
//
//   BR1  With the organization's verification switch on, Home shows Base's
//        status line and Re-run verification. The dialog starts a full run:
//        Base is forked, the browser lands on the new workspace's chat with
//        the run's prompt pre-typed under the bold notice and never sent.
//        Home then names the run in progress and greys the button, and a
//        second start is refused. Merging the run's pull request into Base
//        completes it; the next incremental run continues from its commits.
//   BR2  Deleting a run's workspace before it merges abandons the run and
//        frees the button.
//   BR3  With the switch off, Home shows none of it and a start is refused.
//
// Each test owns an organization: toggling its switch changes the effective
// agent launch revision and must not invalidate confirmations in other specs.
// =============================================================================

import { randomUUID } from "node:crypto";
import { test, expect, type Page } from "@playwright/test";
import {
  authedApi,
  deleteWorkspaceById,
  softVisible,
  switchAuthOrg,
} from "../helpers";
import { createOrganizationFor } from "../organization";

test.describe.configure({ mode: "serial", timeout: 300_000 });

const SWITCH = "verification_action_enabled";

interface RunHead {
  repoId: string;
  rootPath: string;
  commit: string;
}

interface Run {
  id: string;
  kind: "incremental" | "full";
  state: "in_progress" | "completed" | "abandoned";
  workspaceId: string;
  workspaceName: string | null;
  label: string;
  baseHeads: RunHead[];
  sinceHeads: RunHead[] | null;
}

interface Status {
  enabled: boolean;
  baseWorkspaceId?: string;
  state?: "never" | "current" | "stale" | "in_progress";
  canStart?: boolean;
  lastCompleted?: Run | null;
  inProgress?: Run | null;
  runs?: Run[];
}

async function openHome(
  page: Page,
): Promise<{ orgId: string; orgSlug: string }> {
  const me = await authedApi(page, "/api/v1/users/me");
  expect(me.status, JSON.stringify(me.body)).toBe(200);
  const email = (me.body.data ?? me.body).email;
  expect(email, "current administrator email").toBeTruthy();
  const org = await createOrganizationFor(
    email,
    `e2e-reverification-${randomUUID()}`,
  );
  await switchAuthOrg(page, org.id);
  await page.goto(`/${org.slug}/home`, { waitUntil: "domcontentloaded" });
  // A fresh e2e user meets the onboarding tour on Home, whose backdrop
  // blocks the page; skipping it is recorded, so later visits go straight in.
  const skip = page.getByRole("button", { name: "Skip tour" });
  if (await softVisible(skip, 5_000)) await skip.click();
  return { orgId: org.id, orgSlug: org.slug };
}

async function readStatus(page: Page, orgId: string): Promise<Status> {
  const result = await authedApi(
    page,
    `/api/v1/orgs/${orgId}/home/base-reverification`,
  );
  expect(result.status, JSON.stringify(result.body)).toBe(200);
  return (result.body.data ?? result.body) as Status;
}

/** Set the switch and return its earlier value, to put back afterwards. */
async function setSwitch(
  page: Page,
  orgId: string,
  on: boolean,
): Promise<boolean> {
  const read = await authedApi(page, `/api/v1/orgs/${orgId}/config`);
  expect(read.status, JSON.stringify(read.body)).toBe(200);
  const settings = (read.body.data ?? read.body).settings ?? {};
  const before = settings[SWITCH] === true;
  const write = await authedApi(page, `/api/v1/orgs/${orgId}/config`, "PUT", {
    settings: { [SWITCH]: on },
  });
  expect(write.status, JSON.stringify(write.body)).toBe(200);
  return before;
}

/** A run left in progress by an interrupted earlier attempt blocks starts. */
async function clearRunInProgress(page: Page, orgId: string): Promise<void> {
  const status = await readStatus(page, orgId);
  if (status.inProgress) {
    await deleteWorkspaceById(page, status.inProgress.workspaceId);
  }
}

async function startViaApi(
  page: Page,
  baseId: string,
  kind: "incremental" | "full",
) {
  return authedApi(page, `/api/v1/workspaces/${baseId}/clone`, "POST", {
    name: "base-reverification",
    idempotencyKey: `e2e-reverification-${randomUUID()}`,
    uniquifyName: true,
    reverification: { kind },
  });
}

/** Merge the run's (empty) pull request into Base as Base's administrator. */
async function mergeRunIntoBase(page: Page, run: Run, baseId: string) {
  const created = await authedApi(
    page,
    `/api/v1/workspaces/${run.workspaceId}/pull-requests`,
    "POST",
    {
      targetWorkspaceId: baseId,
      title: `Re-verify Base (${run.label})`,
      save: false,
    },
  );
  expect(created.status, JSON.stringify(created.body)).toBeLessThan(300);
  const pullRequest = (created.body.data ?? created.body).pullRequest;
  const merged = await authedApi(
    page,
    `/api/v1/workspaces/${baseId}/pull-requests/${pullRequest.id}/merge`,
    "POST",
    {},
  );
  expect(merged.status, JSON.stringify(merged.body)).toBe(200);
  expect((merged.body.data ?? merged.body).pullRequest.status).toBe("merged");
}

/** The dialog's row for one run; a deleted workspace's name can be reused. */
function runRow(page: Page, runId: string) {
  return page.locator(
    `[data-testid="home-reverification-run"][data-run-id="${runId}"]`,
  );
}

function formatHead(head: RunHead): string {
  return `${head.rootPath}@${head.commit}`;
}

test("BR1: a full run starts from Home, lands unsent, blocks a second start and completes on merge", async ({
  page,
}) => {
  const { orgId, orgSlug } = await openHome(page);
  const switchBefore = await setSwitch(page, orgId, true);
  const workspaces: string[] = [];
  try {
    await clearRunInProgress(page, orgId);
    const initial = await readStatus(page, orgId);
    expect(initial.enabled).toBe(true);
    expect(initial.canStart).toBe(true);
    const baseId = initial.baseWorkspaceId!;

    // Home's line matches the state the API reports.
    await page.reload({ waitUntil: "domcontentloaded" });
    const line = page.getByTestId("home-reverification-status");
    await expect(line).toBeVisible({ timeout: 30_000 });
    if (initial.state === "never") {
      await expect(line).toHaveText(/Base has never been re-verified/);
    } else if (initial.state === "current") {
      await expect(line).toHaveText(
        /Base was re-verified on .*no PRs merged since/,
      );
    } else {
      await expect(line).toHaveText(
        /Base has not been re-verified for \d+ merged PR/,
      );
    }
    const open = page.getByTestId("home-reverification-open");
    await expect(open).toBeEnabled();

    // The dialog explains the cost; incremental needs a completed run.
    await open.click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await expect(page.getByTestId("home-reverification-notice")).toContainText(
      /very expensive/i,
    );
    const incremental = page.getByTestId("home-reverification-incremental");
    if (initial.lastCompleted) {
      await expect(incremental).toBeEnabled();
    } else {
      await expect(incremental).toBeDisabled();
      await expect(
        page.getByTestId("home-reverification-incremental-reason"),
      ).toBeVisible();
    }

    // A full run forks Base and lands on its chat with the prompt unsent.
    await page.getByTestId("home-reverification-full").click();
    await page.waitForURL(/\?view=chat/, { timeout: 120_000 });
    const after = await readStatus(page, orgId);
    const run = after.inProgress!;
    expect(run, JSON.stringify(after)).toBeTruthy();
    workspaces.push(run.workspaceId);
    expect(run.kind).toBe("full");
    expect(after.state).toBe("in_progress");
    expect(decodeURIComponent(page.url())).toContain(`/${run.workspaceName}`);

    await expect(
      page.getByTestId("composer-seed-verification-notice"),
    ).toBeVisible({
      timeout: 60_000,
    });
    const composer = page.getByTestId("chat-input").first();
    await expect(composer).toHaveValue(/Base re-verification \(full\)/);
    await expect(composer).toHaveValue(
      new RegExp(`Run: ${run.label.replace(/[()]/g, "\\$&")}`),
    );
    await expect(composer).toHaveValue(
      new RegExp(formatHead(run.baseHeads[0])),
    );
    const sessions = await authedApi(
      page,
      `/api/v1/workspaces/${run.workspaceId}/sessions`,
    );
    expect(sessions.status).toBe(200);
    const sessionList = sessions.body.data ?? sessions.body;
    expect(
      Array.isArray(sessionList) ? sessionList : (sessionList.sessions ?? []),
      "landing never sends the prompt",
    ).toHaveLength(0);

    // Home names the run in progress and greys the button.
    await page.goto(`/${orgSlug}/home`, { waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-reverification-status")).toHaveText(
      new RegExp(`Verification in progress in workspace ${run.workspaceName}`),
      { timeout: 30_000 },
    );
    await expect(page.getByTestId("home-reverification-workspace")).toHaveText(
      run.workspaceName!,
    );
    await expect(page.getByTestId("home-reverification-open")).toBeDisabled();

    // A second start is refused while this one is in progress.
    const second = await startViaApi(page, baseId, "full");
    expect(second.status, JSON.stringify(second.body)).toBe(409);
    expect(JSON.stringify(second.body)).toContain(run.workspaceName!);

    // A direct merge is refused; the run lands only through its pull request.
    const direct = await authedApi(
      page,
      `/api/v1/workspaces/${run.workspaceId}/merge`,
      "POST",
      {},
    );
    // Refused either way: 403 when the caller does not own Base, otherwise 409.
    expect([403, 409], JSON.stringify(direct.body)).toContain(direct.status);
    expect((await readStatus(page, orgId)).inProgress?.id).toBe(run.id);

    // Merging its pull request into Base completes the run.
    await mergeRunIntoBase(page, run, baseId);
    const completed = await readStatus(page, orgId);
    expect(completed.lastCompleted?.id).toBe(run.id);
    expect(completed.lastCompleted?.state).toBe("completed");
    expect(completed.inProgress ?? null).toBeNull();
    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-reverification-status")).toHaveText(
      /Base was re-verified on|Base has not been re-verified for/,
      { timeout: 30_000 },
    );
    await expect(page.getByTestId("home-reverification-open")).toBeEnabled();

    // The dialog now offers incremental, whose run continues from the
    // completed run's commits.
    await page.getByTestId("home-reverification-open").click();
    await expect(
      runRow(page, run.id).getByTestId("home-reverification-run-state"),
    ).toHaveText(/completed/i);
    await expect(
      page.getByTestId("home-reverification-incremental"),
    ).toBeEnabled();
    await page.getByTestId("home-reverification-incremental").click();
    await page.waitForURL(/\?view=chat/, { timeout: 120_000 });
    const next = (await readStatus(page, orgId)).inProgress!;
    workspaces.push(next.workspaceId);
    expect(next.kind).toBe("incremental");
    expect(next.sinceHeads).toEqual(run.baseHeads);
    const nextComposer = page.getByTestId("chat-input").first();
    await expect(nextComposer).toHaveValue(
      /Base re-verification \(incremental\)/,
      {
        timeout: 60_000,
      },
    );
    await expect(nextComposer).toHaveValue(
      new RegExp(`from ${formatHead(run.baseHeads[0])}`),
    );
  } finally {
    for (const id of workspaces) await deleteWorkspaceById(page, id);
    await setSwitch(page, orgId, switchBefore);
  }
});

test("BR2: deleting a run's workspace abandons the run and frees the button", async ({
  page,
}) => {
  const { orgId } = await openHome(page);
  const switchBefore = await setSwitch(page, orgId, true);
  let workspaceId = "";
  try {
    await clearRunInProgress(page, orgId);
    const initial = await readStatus(page, orgId);
    const started = await startViaApi(page, initial.baseWorkspaceId!, "full");
    expect(started.status, JSON.stringify(started.body)).toBe(201);
    const run = (started.body.data ?? started.body).reverificationRun as Run;
    workspaceId = run.workspaceId;

    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-reverification-open")).toBeDisabled({
      timeout: 30_000,
    });

    await deleteWorkspaceById(page, workspaceId);
    workspaceId = "";
    const after = await readStatus(page, orgId);
    expect(after.inProgress ?? null).toBeNull();
    expect(after.runs?.find((r) => r.id === run.id)?.state).toBe("abandoned");

    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-reverification-open")).toBeEnabled({
      timeout: 30_000,
    });
    await page.getByTestId("home-reverification-open").click();
    await expect(
      runRow(page, run.id).getByTestId("home-reverification-run-state"),
    ).toHaveText(/abandoned/i);
  } finally {
    await deleteWorkspaceById(page, workspaceId);
    await setSwitch(page, orgId, switchBefore);
  }
});

test("BR3: with the switch off Home shows nothing and a start is refused", async ({
  page,
}) => {
  const { orgId } = await openHome(page);
  const switchBefore = await setSwitch(page, orgId, true);
  const baseId = (await readStatus(page, orgId)).baseWorkspaceId!;
  await setSwitch(page, orgId, false);
  try {
    expect(await readStatus(page, orgId)).toEqual({ enabled: false });
    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("home-needs-attention")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByTestId("home-reverification")).toHaveCount(0);

    const refused = await startViaApi(page, baseId, "full");
    expect(refused.status, JSON.stringify(refused.body)).toBe(409);
    expect(JSON.stringify(refused.body)).toContain("turned off");
  } finally {
    await setSwitch(page, orgId, switchBefore);
  }
});
