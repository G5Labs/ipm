import { randomUUID } from "node:crypto";
import { expect, test } from "@playwright/test";
import {
  activateSidebarSection,
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  expectSelectedAgentProvider,
  openWorkspacePath,
  readWorkspaceChanges,
  releaseChatTurn,
  runWorkspaceVmAction,
  selectAgentProviderViaSettings,
  showWorkspaceTabs,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
  type ChatTurnIdentity,
  type WorkspaceClone,
  expectAssistantReply,
} from "../helpers";

test.describe("Agent value outcome", () => {
  test.describe.configure({ timeout: 360_000 });
  let workspace: WorkspaceClone;
  let turn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    await showWorkspaceTabs(page);
    workspace = await createWorkspaceClone(page, {
      namePrefix: `agent-value-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
    await page.getByTestId("center-chat-tab").click();
  });

  test.afterEach(async ({ page }) => {
    if (turn) await releaseChatTurn(page, turn).catch(() => {});
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
    turn = undefined;
  });

  test("M1 one user request yields task evidence, code and Project Doc changes, Changes diff, and terminal result", async ({
    page,
  }) => {
    const detailResponse = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detailResponse.status).toBe(200);
    const detail = detailResponse.body?.data ?? detailResponse.body;
    const docsRepoId = String(detail.docsRepoId);
    const live = await waitForLiveWorkspaceChanges(page, workspace.id);
    const code = live.members.find((member) => member.repo_id !== docsRepoId);
    const docs = live.members.find((member) => member.repo_id === docsRepoId);
    expect(code?.root_path).toBeTruthy();
    expect(docs?.root_path).toBeTruthy();
    const token = randomUUID();
    const codePath = `e2e-agent-value-${token}.txt`;
    const docPath = `docs/${workspace.id}/e2e-agent-value-${token}.md`;
    const codeBytes = `agent code ${token}\n`;
    const docBytes = `# Agent Project Doc ${token}\n`;
    const writes = [
      {
        path: `${code!.root_path}/${codePath}`,
        contentBase64: Buffer.from(codeBytes).toString("base64"),
      },
      {
        path: `${docs!.root_path}/${docPath}`,
        contentBase64: Buffer.from(docBytes).toString("base64"),
      },
    ];
    const writeSet = Buffer.from(JSON.stringify(writes)).toString("base64");
    const request = `Create code and a Project Doc for ${token}. G5_FS_WRITE_SET_E2E:${writeSet}`;
    await selectAgentProviderViaSettings(page, "claude_code");
    await expectSelectedAgentProvider(page, "claude_code");
    const input = page.getByTestId("chat-input").first();
    await expect(input).toBeVisible({ timeout: 30_000 });
    await input.fill(request);
    const sendResponse = page.waitForResponse((response) => {
      if (response.request().method() !== "POST") return false;
      const path = new URL(response.url()).pathname;
      return (
        /\/api\/v1\/sessions\/[^/]+\/messages$/.test(path) ||
        /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(path)
      );
    });
    await input.press("Enter");
    turn = await captureChatTurn(page, await sendResponse, (created) => {
      turn = created;
    });
    expect(turn.workspaceId).toBe(workspace.id);
    expect(turn.taskId).toMatch(/^[0-9a-f-]{36}$/);
    await expectAssistantReply(
      page,
      turn!,
      "Fake workspace write completed: two workspace files",
      {
        timeout: 120_000,
      },
    );
    const timelineResponse = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}/timeline`,
    );
    expect(timelineResponse.status, JSON.stringify(timelineResponse.body)).toBe(
      200,
    );
    const timeline = timelineResponse.body?.data ?? timelineResponse.body;
    expect(timeline.entries.length).toBeGreaterThan(0);
    const taskResponse = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}`,
    );
    expect(taskResponse.status, JSON.stringify(taskResponse.body)).toBe(200);
    const task = taskResponse.body?.data ?? taskResponse.body;
    // A shared chat turn can remain waiting for a follow-up after its provider
    // reply. The fixture does not invoke `g5d task complete`.
    expect(["running", "waiting", "succeeded"]).toContain(task.status);
    await expect(page.getByTestId("chat-messages")).toContainText(token, {
      timeout: 30_000,
    });

    for (const { repoId, path, bytes } of [
      { repoId: code!.repo_id, path: codePath, bytes: codeBytes },
      { repoId: docsRepoId, path: docPath, bytes: docBytes },
    ]) {
      const fileResponse = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
      );
      expect(fileResponse.status, JSON.stringify(fileResponse.body)).toBe(200);
      const file = fileResponse.body?.data ?? fileResponse.body;
      expect(file.source, `${repoId}:${path}`).toBe("live");
      expect(file.content, `${repoId}:${path}`).toBe(bytes);
    }
    const changes = await readWorkspaceChanges(page, workspace.id);
    for (const [repoId, path] of [
      [code!.repo_id, codePath],
      [docsRepoId, docPath],
    ]) {
      expect(
        changes.members
          .find((member) => member.repo_id === repoId)
          ?.dirty?.some((entry) => entry.path === path),
        `${repoId}:${path} must appear in the real Git diff`,
      ).toBe(true);
    }
    // Agent filesystem writes do not emit the API's file-changed event. A
    // browser reload must still show the authoritative live Git diff.
    await page.reload({ waitUntil: "domcontentloaded" });
    await activateSidebarSection(page, "sidebar-source-control-tab");
    const sourceControl = page.getByTestId("source-control-menu");
    await expect(
      sourceControl.getByRole("checkbox", { name: /^Not yet pushed in / }),
    ).toHaveCount(2);
    const applied = page.waitForResponse(
      (r) =>
        r.url().endsWith("/source-control/apply") &&
        r.request().method() === "POST",
    );
    await sourceControl
      .getByRole("button", { name: "Show", exact: true })
      .click();
    const selected = await applied;
    expect(selected.status(), await selected.text()).toBe(200);
    const snapshot = (await selected.json()).data;
    expect(snapshot.fileCount).toBe(2);
    expect(
      snapshot.patches
        .map(
          (p: { repoId: string; file: { path: string } }) =>
            `${p.repoId}:${p.file.path}`,
        )
        .sort(),
    ).toEqual(
      [`${code!.repo_id}:${codePath}`, `${docsRepoId}:${docPath}`].sort(),
    );
    const changesView = page.getByTestId("source-control-diff-stream");
    await expect(changesView).toBeVisible();
    await expect(changesView).toContainText(codeBytes.trimEnd());
    await expect(changesView).toContainText(docBytes.trimEnd());

    await openWorkspacePath(page, `${workspace.path}?view=terminal`);
    const terminal = page.getByTestId("workspace-terminal");
    await expect(terminal).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
      timeout: 30_000,
    });
    const inputTerminal = terminal.locator(".xterm-helper-textarea");
    await inputTerminal.focus();
    // Verify the displayed patch bytes independently through the real Git
    // checkout in the browser terminal.
    await inputTerminal.pressSequentially("stty -echo");
    await inputTerminal.press("Enter");
    const shellQuote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
    await inputTerminal.pressSequentially(
      `GIT_PAGER=cat git -C ${shellQuote(code!.root_path)} -c color.ui=never diff --no-index -- /dev/null ${shellQuote(codePath)}`,
    );
    await inputTerminal.press("Enter");
    await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
      `+${codeBytes.trimEnd()}`,
      { timeout: 30_000 },
    );
    const suffix = token.replaceAll("-", "");
    const marker = `G5_M1_TERMINAL_${suffix}`;
    await inputTerminal.pressSequentially(
      `printf '%s_%s\\n' G5_M1_TERMINAL ${suffix}`,
    );
    await inputTerminal.press("Enter");
    await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
      marker,
      { timeout: 30_000 },
    );
  });
});
