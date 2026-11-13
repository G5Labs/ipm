import { expect, test } from "@playwright/test";
import {
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  openRailTerminal,
  openWorkspacePath,
  railTerminalButton,
  runWorkspaceVmAction,
  type WorkspaceClone,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
} from "../helpers";

test.describe("Workspace VM lifecycle", () => {
  let workspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `workspace-vm-${testInfo.title}`,
      start: false,
    });
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
  });

  test("preserves live edits and unpushed commits through stop and start", async ({
    page,
  }) => {
    await expect(page.getByTestId("workspace-vm-start")).toHaveAttribute(
      "title",
      /^VM: Stopped/,
    );
    // The owner keeps the terminal control while stopped, disabled with the
    // state's reason (`g5.workspace.ui.unavailable`).
    const terminalOpen = await railTerminalButton(page);
    await expect(terminalOpen).toBeVisible();
    await expect(terminalOpen).toBeDisabled();
    await expect(terminalOpen).toHaveAttribute(
      "title",
      "Start the workspace to open a terminal.",
    );

    await page.getByTestId("workspace-vm-start").click();
    const first = await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const changes = await waitForLiveWorkspaceChanges(page, workspace.id);
    expect(changes.members.length).toBeGreaterThan(0);
    const path = `e2e-vm-restored-${Date.now()}.txt`;
    const content = `workspace VM restoration ${workspace.id}\n`;
    const workspaceResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(
      workspaceResponse.status,
      JSON.stringify(workspaceResponse.body),
    ).toBe(200);
    const docsRepoId = String(
      (workspaceResponse.body?.data ?? workspaceResponse.body)?.docsRepoId ??
        "",
    );
    expect(docsRepoId).toBeTruthy();
    expect(
      changes.members.some((candidate) => candidate.repo_id === docsRepoId),
    ).toBe(true);
    const member = changes.members.find(
      (candidate) => candidate.repo_id !== docsRepoId,
    );
    expect(member).toBeTruthy();
    const docPath = `docs/${workspace.id}/e2e-vm-restored-${Date.now()}.md`;
    const docContent = `# VM restoration ${workspace.id}\n\nDirty Project Doc.\n`;

    const write = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${member!.repo_id}`,
      "PUT",
      { content, encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(write.body)).toContain(write.status);
    const docWrite = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/docs`,
      "POST",
      {
        name: docPath.split("/").at(-1),
        docType: "design",
        filePath: docPath,
        content: docContent,
      },
    );
    expect(docWrite.status, JSON.stringify(docWrite.body)).toBe(201);

    // Design v9 §5.5: a committed-but-unpushed commit on the shared branch
    // restores with the branch ahead of the internal ref, next to the dirty
    // edit above. The commit is made by the owner's shell inside the VM.
    const committedPath = `e2e-vm-committed-${Date.now()}.txt`;
    const repoPath = (member!.root_path || ".").replaceAll("'", "'\\''");
    await openRailTerminal(page);
    const terminal = page.getByTestId("workspace-terminal");
    await expect(terminal).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
      timeout: 30_000,
    });
    const input = terminal.locator(".xterm-helper-textarea");
    await input.focus();
    await input.pressSequentially("stty -echo");
    await input.press("Enter");
    await page.waitForTimeout(300);
    await input.pressSequentially(
      `cd -- '${repoPath}' && printf 'committed\\n' > '${committedPath}' && git add -- '${committedPath}' && git -c user.email=e2e@g5.dev -c user.name=e2e commit -qm 'e2e unpushed commit' -- '${committedPath}' && printf 'G5_%s %s\\n' COMMITTED "$(git rev-parse --short HEAD)"`,
    );
    await input.press("Enter");
    // xterm screen-reader mode mirrors the screen into this tree; the
    // panel keeps no separate transcript (see terminal.spec.ts).
    const transcript = terminal.locator(".xterm-accessibility-tree");
    await expect(transcript).toContainText(/G5_COMMITTED [0-9a-f]{7,}/, {
      timeout: 30_000,
    });
    await expect(transcript).not.toContainText(/fatal:|Permission denied/);
    const beforeStop = await waitForLiveWorkspaceChanges(page, workspace.id);
    const committedBefore = beforeStop.members.find(
      (candidate) => candidate.repo_id === member!.repo_id,
    );
    expect(committedBefore?.against_internal?.ahead).toBe(1);
    expect(
      beforeStop.members
        .find((candidate) => candidate.repo_id === docsRepoId)
        ?.dirty?.some((entry) => entry.path === docPath),
    ).toBe(true);
    const beforeVector = beforeStop.members
      .map(({ repo_id, shared_branch, head_oid }) => ({
        repo_id,
        shared_branch,
        head_oid,
      }))
      .sort((left, right) => left.repo_id.localeCompare(right.repo_id));
    expect(beforeVector.length).toBe(changes.members.length);
    for (const memberVector of beforeVector) {
      expect(
        memberVector.head_oid,
        `repo ${memberVector.repo_id} Git HEAD`,
      ).toMatch(/^([0-9a-f]{40}|[0-9a-f]{64})$/);
    }

    await runWorkspaceVmAction(page, workspace.id, "stop");
    const stopped = await waitForWorkspaceVmStatus(
      page,
      workspace.id,
      "stopped",
    );
    expect(stopped.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
    await runWorkspaceVmAction(page, workspace.id, "resume");
    const resumed = await waitForWorkspaceVmStatus(
      page,
      workspace.id,
      "running",
    );

    const restored = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${member!.repo_id}`,
    );
    expect(restored.status, JSON.stringify(restored.body)).toBe(200);
    expect(restored.body?.data?.content ?? restored.body?.content).toBe(
      content,
    );
    expect(restored.body?.data?.source ?? restored.body?.source).toBe("live");
    const restoredDoc = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${docPath}?repoId=${docsRepoId}`,
    );
    expect(restoredDoc.status, JSON.stringify(restoredDoc.body)).toBe(200);
    expect(restoredDoc.body?.data?.content ?? restoredDoc.body?.content).toBe(
      docContent,
    );
    expect(restoredDoc.body?.data?.source ?? restoredDoc.body?.source).toBe(
      "live",
    );
    const restoredCommit = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${committedPath}?repoId=${member!.repo_id}`,
    );
    expect(restoredCommit.status, JSON.stringify(restoredCommit.body)).toBe(
      200,
    );
    expect(
      restoredCommit.body?.data?.content ?? restoredCommit.body?.content,
    ).toBe("committed\n");
    const afterResume = await waitForLiveWorkspaceChanges(page, workspace.id);
    const committedAfter = afterResume.members.find(
      (candidate) => candidate.repo_id === member!.repo_id,
    );
    expect(
      committedAfter?.against_internal?.ahead,
      "the unpushed commit must restore with the branch ahead of the internal ref",
    ).toBe(1);
    expect(committedAfter?.dirty?.some((entry) => entry.path === path)).toBe(
      true,
    );
    expect(
      afterResume.members
        .find((candidate) => candidate.repo_id === docsRepoId)
        ?.dirty?.some((entry) => entry.path === docPath),
    ).toBe(true);
    expect(
      afterResume.members
        .map(({ repo_id, shared_branch, head_oid }) => ({
          repo_id,
          shared_branch,
          head_oid,
        }))
        .sort((left, right) => left.repo_id.localeCompare(right.repo_id)),
    ).toEqual(beforeVector);
    expect(resumed.lastFlushId).toBe(stopped.lastFlushId);

    expect(resumed.vmId).not.toBe(first.vmId);
    expect(first.epoch).toEqual(expect.any(Number));
    expect(resumed.epoch).toBeGreaterThan(first.epoch!);
    await expect(page.getByTestId("workspace-vm-restart")).toHaveCount(0);
  });

  // g5.lifecycle.stop.force: Stop always ends with the VM stopped. A file
  // over the 100 MiB capture limit makes the save fail in a way no retry
  // fixes, so Stop records the loss and kills the VM at once instead of
  // reopening it, and Start boots a fresh VM from the last saved state.
  test("stops a VM whose save cannot finish and starts it fresh", async ({
    page,
  }) => {
    await page.getByTestId("workspace-vm-start").click();
    const first = await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const changes = await waitForLiveWorkspaceChanges(page, workspace.id);
    const workspaceResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(
      workspaceResponse.status,
      JSON.stringify(workspaceResponse.body),
    ).toBe(200);
    const docsRepoId = String(
      (workspaceResponse.body?.data ?? workspaceResponse.body)?.docsRepoId ??
        "",
    );
    const member = changes.members.find(
      (candidate) => candidate.repo_id !== docsRepoId,
    );
    expect(member).toBeTruthy();

    const oversizedPath = `e2e-oversized-${Date.now()}.txt`;
    const repoPath = (member!.root_path || ".").replaceAll("'", "'\\''");
    await openRailTerminal(page);
    const terminal = page.getByTestId("workspace-terminal");
    await expect(terminal).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
      timeout: 30_000,
    });
    const input = terminal.locator(".xterm-helper-textarea");
    await input.focus();
    await input.pressSequentially("stty -echo");
    await input.press("Enter");
    await page.waitForTimeout(300);
    await input.pressSequentially(
      `cd -- '${repoPath}' && dd if=/dev/zero of='${oversizedPath}' bs=1048576 count=110 2>/dev/null && printf 'G5_%s %s\\n' WROTE "$(wc -c < '${oversizedPath}')"`,
    );
    await input.press("Enter");
    const transcript = terminal.locator(".xterm-accessibility-tree");
    await expect(transcript).toContainText(/G5_WROTE\s+115343360/, {
      timeout: 60_000,
    });

    const stopRequested = Date.now();
    await runWorkspaceVmAction(page, workspace.id, "stop");
    const stopped = await waitForWorkspaceVmStatus(
      page,
      workspace.id,
      "stopped",
      90_000,
    );
    expect(
      Date.now() - stopRequested,
      "a save that cannot finish does not hold the stop for its window",
    ).toBeLessThan(90_000);
    expect(stopped.unflushedLossAt, "the stop records the loss").toBeTruthy();
    await expect(page.getByTestId("workspace-vm-start")).toHaveAttribute(
      "title",
      /^VM: Stopped/,
    );

    await runWorkspaceVmAction(page, workspace.id, "resume");
    const resumed = await waitForWorkspaceVmStatus(
      page,
      workspace.id,
      "running",
    );
    expect(resumed.vmId).not.toBe(first.vmId);
    const oversized = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${oversizedPath}?repoId=${member!.repo_id}`,
    );
    expect(
      oversized.status,
      "the unsaved file is gone after the fresh start",
    ).toBe(404);
  });
});
