import { expect, test } from "@playwright/test";

import {
  TEST_ORG,
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  navigateToWorkspace,
  openChatComposer,
  openRailTerminal,
  openWorkspacePath,
  railTerminalButton,
  readWorkspaceChanges,
  readWorkspaceVm,
  resolveWorkspaceId,
  runWorkspaceVmAction,
  type WorkspaceClone,
  waitForWorkspaceVmStatus,
} from "../helpers";

/**
 * Design v9 §5.2/§5.8 (Base never has a VM) and §9.1/§9.2 (Save never
 * ensures or boots compute; an unattached owner reads durable Git and sees
 * Resume; an explicit Resume admits the same Save).
 */
test.describe("Workspace attachment gates", () => {
  test("Base has no VM: start is refused and the page offers a fork instead", async ({
    page,
  }) => {
    const baseId = await resolveWorkspaceId(page, TEST_ORG, "Base");

    const resumed = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${baseId}/resume`,
      "POST",
      {},
    );
    expect(resumed.status, JSON.stringify(resumed.body)).toBe(409);
    expect(resumed.body?.error?.code).toBe("WORKSPACE_IS_BASE");

    const changes = await readWorkspaceChanges(page, baseId);
    expect(changes.source).toBe("durable");
    expect(changes.attached).toBe(false);

    await navigateToWorkspace(page, TEST_ORG, "Base");
    // A way to fork (the strip's "+" opens the picker), no owner controls.
    await expect(page.getByTestId("workspace-tab-add")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByTestId("workspace-vm-start")).toHaveCount(0);
    await expect(await railTerminalButton(page)).toHaveCount(0);
  });

  test.describe("stopped workspace", () => {
    let workspace: WorkspaceClone;

    test.beforeEach(async ({ page }, testInfo) => {
      workspace = await createWorkspaceClone(page, {
        namePrefix: `attachment-${testInfo.title}`,
        start: false,
      });
    });

    test.afterEach(async ({ page }) => {
      await deleteWorkspaceById(page, workspace?.id).catch(() => {});
    });

    test("Save and preview are refused without booting; reads are committed; Resume admits the same Save", async ({
      page,
    }) => {
      const id = workspace.id;
      const durable = await readWorkspaceChanges(page, id);
      expect(durable.source).toBe("durable");
      const codeMember =
        durable.members.find((member) => member.root_path !== "docs") ??
        durable.members[0];
      const repoId = codeMember.repo_id;
      expect((await readWorkspaceVm(page, id)).status).toBe("stopped");

      const path = `e2e-attachment-${Date.now()}.txt`;
      const refused = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${id}/files/${path}?repoId=${repoId}`,
        "PUT",
        { content: "cold\n", encoding: "utf-8", expectedHash: null },
      );
      expect(refused.status, JSON.stringify(refused.body)).toBe(409);
      expect(refused.body?.error?.code).toBe("WORKSPACE_NOT_RUNNING");

      const preview = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${id}/preview`,
        "POST",
        { repoId, idempotencyKey: `attachment-${Date.now()}` },
      );
      expect(preview.status, JSON.stringify(preview.body)).toBe(409);
      expect(preview.body?.error?.code).toBe("WORKSPACE_NOT_RUNNING");

      // Neither refusal booted compute.
      expect((await readWorkspaceVm(page, id)).status).toBe("stopped");

      const committed = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${id}/files/semlift.yaml?repoId=${repoId}`,
      );
      expect(committed.status, JSON.stringify(committed.body)).toBe(200);
      expect(["committed", "flush"]).toContain(
        committed.body?.data?.source ?? committed.body?.source,
      );

      await openWorkspacePath(page, workspace.path);
      await expect(page.getByTestId("workspace-vm-start")).toHaveAttribute(
        "title",
        /^VM: Stopped/,
      );
      await expect(page.getByTestId("workspace-vm-start")).toBeVisible();
      // The owner keeps the terminal control while stopped, disabled with
      // the state's reason (`g5.workspace.ui.unavailable`).
      const terminalOpen = await railTerminalButton(page);
      await expect(terminalOpen).toBeVisible();
      await expect(terminalOpen).toBeDisabled();
      await expect(terminalOpen).toHaveAttribute(
        "title",
        "Start the workspace to open a terminal.",
      );
      // §8.2: the owner's composer stays open while the VM is stopped and
      // offers Start and send, which starts the workspace with the message
      // (`g5.chat.start-and-send`); nothing is sent here, no LLM in the loop.
      await openChatComposer(page);
      const composer = page.getByTestId("chat-input").first();
      await expect(composer).toBeEnabled();
      await expect(
        page.getByTestId("start-and-send-note").first(),
      ).toContainText("Start and send");
      await expect(
        page
          .locator(
            '[data-testid="chat-panel-no-session-send"], [data-testid="chat-panel-send"]',
          )
          .first(),
      ).toHaveText(/Start and send/);

      // Explicit owner Resume admits the same Save; the read turns live.
      await runWorkspaceVmAction(page, id, "resume");
      await waitForWorkspaceVmStatus(page, id, "running");
      const admitted = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${id}/files/${path}?repoId=${repoId}`,
        "PUT",
        { content: "warm\n", encoding: "utf-8", expectedHash: null },
      );
      expect([200, 201], JSON.stringify(admitted.body)).toContain(
        admitted.status,
      );
      const live = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${id}/files/${path}?repoId=${repoId}`,
      );
      expect(live.status).toBe(200);
      expect(live.body?.data?.source ?? live.body?.source).toBe("live");
      expect(live.body?.data?.content ?? live.body?.content).toBe("warm\n");
    });
  });
});
