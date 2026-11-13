import { expect, test } from "@playwright/test";
import {
  activateSidebarSection,
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  openWorkspacePath,
  readWorkspaceChanges,
  runWorkspaceVmAction,
  type WorkspaceClone,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
} from "../helpers";

test.describe("Workspace branch association", () => {
  let workspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `association-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
  });

  test("a terminal checkout blocks Save until the owner switches back", async ({
    page,
  }) => {
    const live = await waitForLiveWorkspaceChanges(page, workspace.id);
    const member = live.members[0];
    const repoPath = (member.root_path || ".").replaceAll("'", "'\\''");
    const foreignBranch = `e2e-mismatch-${Date.now()}`;

    await openWorkspacePath(page, `${workspace.path}?view=terminal`);
    const terminal = page.getByTestId("workspace-terminal");
    await expect(terminal).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
      timeout: 30_000,
    });
    const input = terminal.locator(".xterm-helper-textarea");
    await input.focus();
    // Stop the terminal echoing the typed line: the marker must come from the
    // command's own output, otherwise a failed checkout still "matches".
    await input.pressSequentially("stty -echo");
    await input.press("Enter");
    await page.waitForTimeout(300);
    await input.pressSequentially(
      `cd -- '${repoPath}' && git checkout -b '${foreignBranch}' && printf 'G5_ASSOCIATION_%s %s\\n' CHANGED "$(git rev-parse --abbrev-ref HEAD)"`,
    );
    await input.press("Enter");
    // xterm screen-reader mode mirrors the screen into this tree; the
    // panel keeps no separate transcript (see terminal.spec.ts).
    const transcript = terminal.locator(".xterm-accessibility-tree");
    await expect(transcript).toContainText(
      `G5_ASSOCIATION_CHANGED ${foreignBranch}`,
      { timeout: 30_000 },
    );
    await expect(transcript).not.toContainText(/fatal:|Permission denied/);

    await expect
      .poll(
        async () => {
          const report = await readWorkspaceChanges(page, workspace.id);
          return report.members[0]?.errors ?? [];
        },
        { timeout: 30_000 },
      )
      .toContain("BRANCH_ASSOCIATION_MISMATCH");

    const blockedPath = `e2e-association-${Date.now()}.txt`;
    const blocked = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${blockedPath}?repoId=${member.repo_id}`,
      "PUT",
      { content: "blocked\n", encoding: "utf-8", expectedHash: null },
    );
    expect(blocked.status).toBe(409);
    expect(blocked.body?.error?.code).toBe("WORKSPACE_BRANCH_MISMATCH");

    await activateSidebarSection(page, "sidebar-source-control-tab");
    // The metadata-only checklist defers local checkout validation to Show.
    const comparison = page.waitForResponse((response) =>
      response.url().endsWith("/source-control/apply"),
    );
    await page
      .getByTestId("source-control-menu")
      .getByRole("button", { name: "Show", exact: true })
      .click();
    const refusedComparison = await comparison;
    expect(refusedComparison.status()).toBeGreaterThanOrEqual(400);
    expect((await refusedComparison.json()).data?.patches).toBeUndefined();
    await expect(page.getByTestId("source-control-menu")).toContainText(
      /Live comparison unavailable|(?:branch|repository).*?(?:association|mismatch)/i,
    );
    const checkpoint = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/source-control/checkpoint`,
    );
    expect(checkpoint.status).toBe(200);
    const saveFailure = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/source-control/save`,
      "POST",
      {},
    );
    expect(saveFailure.status).toBeGreaterThanOrEqual(400);
    const afterFailure = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/source-control/checkpoint`,
    );
    expect(afterFailure.body.data).toEqual(checkpoint.body.data);
    await expect(page.getByTestId("source-control-menu")).not.toContainText(
      "checklist has been updated after Save changes",
    );
    await openWorkspacePath(page, `${workspace.path}?view=changes`);
    const view = page.getByTestId("workspace-changes-view");
    await expect(
      view.getByTestId("workspace-association-mismatch"),
    ).toBeVisible();
    const switchResponse = page.waitForResponse((response) =>
      response
        .url()
        .includes(`/workspaces/${workspace.id}/changes/switch-back`),
    );
    await view.getByTestId("workspace-changes-switch-back").click();
    expect((await switchResponse).status()).toBe(200);
    await expect(
      view.getByTestId("workspace-association-mismatch"),
    ).toHaveCount(0, {
      timeout: 30_000,
    });

    const saved = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${blockedPath}?repoId=${member.repo_id}`,
      "PUT",
      { content: "saved\n", encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(saved.body)).toContain(saved.status);
  });
});
