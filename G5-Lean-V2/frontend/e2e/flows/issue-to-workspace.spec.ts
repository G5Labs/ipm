// =============================================================================
// Issue to workspace — create a native issue, Create & Fork, close it
// =============================================================================
// Covers the Core "issue" stories the loops never exercised (audit
// 2026-10-30; g5.external-issues.creation, g5.issue.native.writes,
// g5.workbench.issues.create, g5.issue.primary, g5.external-issues.transition):
//
//   IS1  From Home a person creates a G5-native issue (no provider needed),
//        chooses Create & Fork Workspace, and the new workspace carries the
//        issue as its primary link. They then close the issue from its G5
//        page; its state becomes Closed and the workspace Activity shows it.
//
// Builds on the Home issues tile and the Start-workspace fork dialog. Uses the
// seeded org's native creation target; if the org's default creation domain
// were not the native tracker the destination assertion fails loudly rather
// than skipping.
// =============================================================================

import { randomUUID } from "node:crypto";
import { test, expect } from "@playwright/test";
import {
  TEST_ORG,
  authedApi,
  deleteWorkspaceById,
  openWorkspacePath,
  readCurrentOrgContext,
  resolveWorkspaceId,
} from "../helpers";

test.describe.configure({ timeout: 300_000 });

test("IS1: a native issue created with Create & Fork is the new workspace's primary issue and closes from its page", async ({
  page,
}) => {
  const { orgId } = await readCurrentOrgContext(page);
  const token = randomUUID().slice(0, 8);
  const title = `Native issue ${token}`;
  const body = `IS1 native issue body ${token}\n\nTrack the core issue path.`;
  const workspaceName = `issue-fork-${token}`;
  let createdWorkspaceId = "";

  try {
    // Home's Issues tile opens the Create dialog.
    await page.goto(`/${TEST_ORG}/home`, { waitUntil: "domcontentloaded" });
    const createButton = page.getByTestId("home-issues-create");
    await expect(createButton).toBeEnabled({ timeout: 30_000 });
    await createButton.click();

    const dialog = page.getByTestId("create-issue-dialog");
    await expect(dialog).toBeVisible({ timeout: 15_000 });
    // The destination is the native G5 tracker, so no GitHub account, hidden
    // reference line or review step applies (g5.issue.native.writes).
    await expect(dialog.getByTestId("create-issue-destination")).not.toContainText(
      " as @",
    );
    await dialog.getByTestId("create-issue-title").fill(title);
    await dialog.getByTestId("create-issue-body").fill(body);

    const createResponse = page.waitForResponse(
      (r) =>
        r.request().method() === "POST" &&
        new URL(r.url()).pathname === `/api/v1/orgs/${orgId}/issues`,
    );
    const forkButton = dialog.getByTestId("create-issue-submit-fork");
    await expect(forkButton).toBeEnabled();
    await forkButton.click();
    const creation = (await (await createResponse).json()).data as {
      state: string;
      issue: { number: number; displayKey: string; repositoryId: string } | null;
    };
    expect(creation.state, JSON.stringify(creation)).toBe("created");
    const created = creation.issue!;
    expect(created.number).toEqual(expect.any(Number));
    expect(created.repositoryId).toBe("g5");

    // Create & Fork opens the Start-workspace fork dialog with the issue
    // preselected as primary (g5.workbench.issues.create).
    const clone = page.getByTestId("clone-issue-field");
    await expect(clone).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId("clone-issue-ref")).toHaveValue(
      created.displayKey,
    );
    await expect(page.getByTestId("clone-issue-role")).toHaveValue("primary");
    // Fork without starting work: the primary link is what this loop proves.
    const startWork = page.getByTestId("clone-start-work-checkbox");
    if (await startWork.isChecked()) await startWork.uncheck();
    await page.locator("#clone-name").fill(workspaceName);
    const submit = page.getByTestId("clone-submit-button");
    await expect(submit).toBeEnabled({ timeout: 15_000 });
    await submit.click();

    // The fork lands on its workspace; it holds the issue as primary.
    await page.waitForURL(
      (url) =>
        url.pathname.startsWith(
          `/${TEST_ORG}/${encodeURIComponent(workspaceName)}`,
        ),
      { timeout: 60_000 },
    );
    createdWorkspaceId = await resolveWorkspaceId(page, TEST_ORG, workspaceName);
    const links = await authedApi(
      page,
      `/api/v1/workspaces/${createdWorkspaceId}/issue-links`,
    );
    expect(links.status, JSON.stringify(links.body)).toBe(200);
    const primary = (
      (links.body?.data ?? links.body).links as {
        role: string;
        displayKey: string;
        locator: { repositoryId: string; number: number };
      }[]
    ).find((link) => link.role === "primary");
    expect(primary, JSON.stringify(links.body)).toBeTruthy();
    expect(primary!.displayKey).toBe(created.displayKey);
    expect(primary!.locator).toMatchObject({ repositoryId: "g5", number: created.number });

    // Close the native issue from its own G5 page (a standalone page, not a
    // workspace shell, so navigate directly rather than via openWorkspacePath).
    await page.goto(`/${TEST_ORG}/issues/g5/${created.number}`, {
      waitUntil: "domcontentloaded",
    });
    await expect(page.getByTestId("external-issue-display-key")).toContainText(
      created.displayKey,
    );
    const stateButton = page.getByTestId("issue-page-state-button");
    await expect(stateButton).toHaveText(/Close issue/, { timeout: 15_000 });
    const closed = page.waitForResponse(
      (r) =>
        r.request().method() === "POST" &&
        /\/issues\/.*\/state$/.test(new URL(r.url()).pathname),
    );
    await stateButton.click();
    expect((await closed).status()).toBe(200);
    await expect(stateButton).toHaveText(/Reopen issue/, { timeout: 15_000 });

    // The issue state is Closed, in the data and on the page.
    const resolved = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/issues/resolve?ref=${encodeURIComponent(created.displayKey)}`,
    );
    expect(resolved.status, JSON.stringify(resolved.body)).toBe(200);
    expect((resolved.body?.data ?? resolved.body).state).toBe("closed");

    // Workspace Activity combines the primary issue's discussion with G5
    // activity (g5.issue.activity): the closed native issue surfaces in the
    // Activity feed itself, not only as the landing's primary-link label.
    await openWorkspacePath(
      page,
      `/${TEST_ORG}/${encodeURIComponent(workspaceName)}?view=activity`,
    );
    const activityEntries = page.getByTestId("workspace-activity-entries");
    await expect(activityEntries).toBeVisible({ timeout: 30_000 });
    await expect(
      activityEntries.getByText(created.displayKey).first(),
    ).toBeVisible({ timeout: 15_000 });
  } finally {
    if (createdWorkspaceId)
      await deleteWorkspaceById(page, createdWorkspaceId).catch(() => {});
  }
});
