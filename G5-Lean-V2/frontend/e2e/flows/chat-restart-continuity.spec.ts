import { randomUUID } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { expect, test } from "@playwright/test";
import { Pool } from "pg";
import {
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  expectSelectedAgentProvider,
  openWorkspacePath,
  releaseChatTurn,
  runWorkspaceVmAction,
  selectAgentProviderViaSettings,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
  type ChatTurnIdentity,
  type WorkspaceClone,
  showWorkspaceTabs,
} from "../helpers";

const execFileAsync = promisify(execFile);

async function expectCodexAppServerAttempts(taskId: string) {
  const databaseUrl = process.env.E2E_DATABASE_URL;
  expect(
    databaseUrl,
    "inspect the candidate's frozen Codex transport",
  ).toBeTruthy();
  const pool = new Pool({ connectionString: databaseUrl });
  try {
    const result = await pool.query<{ policy: Record<string, unknown> }>(
      `SELECT policy_snapshot_ref AS policy FROM task_attempts
       WHERE task_id = $1 ORDER BY attempt`,
      [taskId],
    );
    expect(result.rows.length).toBeGreaterThan(0);
    for (const { policy } of result.rows) {
      expect(policy).toMatchObject({
        agent_module_version: "1.0.8",
        codex_transport: "app_server_stdio_v1",
      });
    }
  } finally {
    await pool.end();
  }
}

async function restartAssignedApi(page: import("@playwright/test").Page) {
  const container = process.env.E2E_API_CONTAINER;
  // The isolated provider-agent lane restarts its own API replica
  // (g5-api-pa) so the broad shards' g5-api is never touched; the shared-stack
  // rollback still targets g5-api. Either is a legitimate CI assignment.
  expect(container, "API restart requires an assigned CI container").toMatch(
    /^g5-api(-pa)?$/,
  );
  await execFileAsync("docker", ["restart", "--time", "10", container!], {
    timeout: 60_000,
  });
  await expect
    .poll(
      async () => {
        try {
          return (
            await page.request.get(`${process.env.E2E_API_URL}/healthz`, {
              timeout: 5_000,
            })
          ).ok();
        } catch {
          return false;
        }
      },
      { timeout: 90_000, intervals: [500, 1_000, 2_000] },
    )
    .toBe(true);
}

test.describe("Chat restart and reload continuity", () => {
  test.describe.configure({ timeout: 600_000 });
  // Every test clones and boots its own workspace VM, so the suite is
  // parallel-safe across workers; the single API-restart test is run alone by
  // scripts/ci-run-playwright-selector.sh because a restart would interrupt
  // the other tests in flight (deployment
  // g5.quality.delivery.ci.provider-agent-parallel).
  test.describe.configure({ mode: "parallel" });
  let workspace: WorkspaceClone;
  let turn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    await showWorkspaceTabs(page);
    workspace = await createWorkspaceClone(page, {
      namePrefix: `chat-reload-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    if (turn) await releaseChatTurn(page, turn).catch(() => {});
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
    turn = undefined;
  });

  test("M6/M14 API restart and reload recover one held turn, Continue, and Project Docs", async ({
    page,
  }) => {
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detail.status, JSON.stringify(detail.body)).toBe(200);
    const docsRepoId = String((detail.body?.data ?? detail.body)?.docsRepoId);
    const changes = await waitForLiveWorkspaceChanges(page, workspace.id);
    const code = changes.members.find(
      (member) => member.repo_id !== docsRepoId,
    );
    const docs = changes.members.find(
      (member) => member.repo_id === docsRepoId,
    );
    expect(code?.root_path).toBeTruthy();
    expect(docs?.root_path).toBeTruthy();

    const token = randomUUID();
    const codePath = `e2e-m6-reload-${token}.txt`;
    const docsPath = `docs/${workspace.id}/e2e-m6-reload-${token}.md`;
    const gatePath = `e2e-m6-reload-gate-${token}.txt`;
    const codeBytes = `M6 reload code ${token}\n`;
    const docsBytes = `# M6 reload Project Doc ${token}\n`;
    const writes = [
      {
        path: `${code!.root_path}/${codePath}`,
        contentBase64: Buffer.from(codeBytes).toString("base64"),
      },
      {
        path: `${docs!.root_path}/${docsPath}`,
        contentBase64: Buffer.from(docsBytes).toString("base64"),
      },
    ];
    const directive = Buffer.from(JSON.stringify(writes)).toString("base64");
    // This journey exercises a queued follow-up across restart. Claude now
    // steers active turns; Codex retains the queue contract under test.
    await selectAgentProviderViaSettings(page, "codex");
    await expectSelectedAgentProvider(page, "codex");
    const input = page.getByTestId("chat-input").first();
    await expect(input).toBeVisible({ timeout: 30_000 });
    await input.fill(
      `M6 ${token}: write code and Project Doc, then wait. G5_FS_WRITE_SET_E2E:${directive} G5_WAIT_FOR_FILE_E2E:${code!.root_path}/${gatePath} G5_REQUIRE_CODEX_APP_SERVER_E2E`,
    );
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

    for (const { repoId, path, bytes } of [
      { repoId: code!.repo_id, path: codePath, bytes: codeBytes },
      { repoId: docsRepoId, path: docsPath, bytes: docsBytes },
    ]) {
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
            );
            return response.status === 200
              ? (response.body?.data ?? response.body)?.content
              : null;
          },
          { timeout: 90_000, intervals: [500, 1_000] },
        )
        .toBe(bytes);
    }
    const taskId = turn.taskId;
    const taskPath = `/api/v1/tasks/${taskId}`;
    const taskBefore = await authedApiViaRequest(page, taskPath);
    expect(taskBefore.status, JSON.stringify(taskBefore.body)).toBe(200);
    const beforeTask = taskBefore.body?.data ?? taskBefore.body;
    expect(beforeTask.workspaceId).toBeTruthy();
    expect(beforeTask.sessionId).toBeTruthy();
    expect(Array.isArray(beforeTask?.dependsOn)).toBe(true);
    const timelinePath = `/api/v1/tasks/${taskId}/timeline`;
    const before = await authedApiViaRequest(page, timelinePath);
    expect(before.status, JSON.stringify(before.body)).toBe(200);
    const beforeEntries = (before.body?.data ?? before.body)?.entries ?? [];

    await expectCodexAppServerAttempts(taskId);
    await restartAssignedApi(page);
    await page.reload({ waitUntil: "domcontentloaded" });
    await expect(page.getByTestId("chat-messages")).toContainText(token, {
      timeout: 30_000,
    });
    const resumedTask = await authedApiViaRequest(
      page,
      `/api/v1/sessions/${turn.sessionId}/task`,
    );
    expect(resumedTask.status, JSON.stringify(resumedTask.body)).toBe(200);
    expect((resumedTask.body?.data ?? resumedTask.body)?.id).toBe(taskId);
    const taskAfter = await authedApiViaRequest(page, taskPath);
    expect(taskAfter.status, JSON.stringify(taskAfter.body)).toBe(200);
    const afterTask = taskAfter.body?.data ?? taskAfter.body;
    expect(afterTask?.workspaceId).toBe(beforeTask.workspaceId);
    expect(afterTask?.sessionId).toBe(beforeTask.sessionId);
    expect(afterTask?.dependsOn).toEqual(beforeTask.dependsOn);
    // Healthz can recover before the host's process-local tunnel reconnects.
    // Read the durable bytes through the live VM before queuing Continue.
    await expect
      .poll(
        async () => {
          const response = await authedApiViaRequest(
            page,
            `/api/v1/workspaces/${workspace.id}/files/${codePath}?repoId=${code!.repo_id}`,
          );
          if (response.status === 503) return null;
          expect(response.status, JSON.stringify(response.body)).toBe(200);
          return (response.body?.data ?? response.body)?.content;
        },
        { timeout: 120_000, intervals: [500, 1_000] },
      )
      .toBe(codeBytes);
    await expect
      .poll(
        async () => {
          const response = await authedApiViaRequest(
            page,
            `/api/v1/workspaces/${workspace.id}/files/${docsPath}?repoId=${docsRepoId}`,
          );
          if (response.status === 503) return null;
          expect(response.status, JSON.stringify(response.body)).toBe(200);
          return (response.body?.data ?? response.body)?.content;
        },
        { timeout: 120_000, intervals: [500, 1_000] },
      )
      .toBe(docsBytes);
    const afterReload = await authedApiViaRequest(page, timelinePath);
    expect(afterReload.status, JSON.stringify(afterReload.body)).toBe(200);
    const afterEntries =
      (afterReload.body?.data ?? afterReload.body)?.entries ?? [];
    const afterIds = afterEntries.map((entry: { id: string }) => entry.id);
    expect(new Set(afterIds).size).toBe(afterIds.length);
    for (const entry of beforeEntries) {
      expect(afterIds).toContain(entry.id);
    }

    const nextCodePath = `e2e-m6-continue-${token}.txt`;
    const nextDocsPath = `docs/${workspace.id}/e2e-m6-continue-${token}.md`;
    const nextCodeBytes = `M6 continue code ${token}\n`;
    const nextDocsBytes = `# M6 continue Project Doc ${token}\n`;
    const nextWrites = Buffer.from(
      JSON.stringify([
        {
          path: `${code!.root_path}/${nextCodePath}`,
          contentBase64: Buffer.from(nextCodeBytes).toString("base64"),
        },
        {
          path: `${docs!.root_path}/${nextDocsPath}`,
          contentBase64: Buffer.from(nextDocsBytes).toString("base64"),
        },
      ]),
    ).toString("base64");
    // Continue while the streamed turn is held. The active session task owns
    // the queued input; after it settles, a new send would create a new task.
    const continueInput = page.getByTestId("chat-input").first();
    await expect(continueInput).toBeVisible();
    await continueInput.fill(
      `Continue M6 ${token} in this chat. G5_FS_WRITE_SET_E2E:${nextWrites}`,
    );
    const continueResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname ===
          `/api/v1/sessions/${turn!.sessionId}/messages`,
    );
    await continueInput.press("Enter");
    const continued = await captureChatTurn(page, await continueResponse);
    expect(continued.sessionId).toBe(turn.sessionId);
    expect(continued.taskId).toBe(taskId);
    turn = continued;
    const queuedFile = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${nextCodePath}?repoId=${code!.repo_id}`,
    );
    expect(queuedFile.status, JSON.stringify(queuedFile.body)).toBe(404);

    const release = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/${gatePath}?repoId=${code!.repo_id}`,
      "PUT",
      { content: "release\n", encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(release.body)).toContain(release.status);
    await expect(
      page.getByTestId("assistant-message").filter({
        hasText: "Fake workspace write completed: two workspace files",
      }),
    ).toHaveCount(2, { timeout: 120_000 });
    for (const { repoId, path, bytes } of [
      { repoId: code!.repo_id, path: nextCodePath, bytes: nextCodeBytes },
      { repoId: docsRepoId, path: nextDocsPath, bytes: nextDocsBytes },
    ]) {
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
            );
            return response.status === 200
              ? (response.body?.data ?? response.body)?.content
              : null;
          },
          { timeout: 90_000, intervals: [500, 1_000] },
        )
        .toBe(bytes);
    }
    await expectCodexAppServerAttempts(taskId);
    const finalTimeline = await authedApiViaRequest(page, timelinePath);
    expect(finalTimeline.status, JSON.stringify(finalTimeline.body)).toBe(200);
    const entries =
      (finalTimeline.body?.data ?? finalTimeline.body)?.entries ?? [];
    const positions = entries.map(
      (entry: { createdAt: string; id: string }) => [entry.createdAt, entry.id],
    );
    expect(entries.length).toBeGreaterThan(afterEntries.length);
    expect(new Set(entries.map((entry: { id: string }) => entry.id)).size).toBe(
      entries.length,
    );
    expect(positions).toEqual(
      [...positions].sort(
        (left, right) =>
          left[0].localeCompare(right[0]) || left[1].localeCompare(right[1]),
      ),
    );
    const finalTask = await authedApiViaRequest(page, taskPath);
    expect(finalTask.status, JSON.stringify(finalTask.body)).toBe(200);
    expect((finalTask.body?.data ?? finalTask.body)?.workspaceId).toBe(
      beforeTask.workspaceId,
    );
    expect((finalTask.body?.data ?? finalTask.body)?.dependsOn).toEqual(
      beforeTask.dependsOn,
    );
    const attempts = await authedApiViaRequest(page, `${taskPath}/attempts`);
    expect(attempts.status, JSON.stringify(attempts.body)).toBe(200);
    expect((attempts.body?.data ?? attempts.body)?.attempts).toHaveLength(1);
    const messages = await authedApiViaRequest(
      page,
      `/api/v1/sessions/${turn.sessionId}/messages`,
    );
    expect(messages.status, JSON.stringify(messages.body)).toBe(200);
    expect(
      (messages.body?.data ?? messages.body)?.messages?.filter(
        (message: { role: string }) => message.role === "user",
      ),
    ).toHaveLength(2);

    // The agent-authored Project Docs must survive the next lifecycle cut as
    // well as the API restart and browser reload exercised above.
    await expect
      .poll(
        async () => {
          const response = await authedApiViaRequest(page, taskPath);
          expect(response.status, JSON.stringify(response.body)).toBe(200);
          return (response.body?.data ?? response.body)?.status;
        },
        { timeout: 90_000, intervals: [500, 1_000] },
      )
      .toBe("succeeded");
    const beforeStop = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/vm`,
    );
    expect(beforeStop.status, JSON.stringify(beforeStop.body)).toBe(200);
    const oldVmId = (beforeStop.body?.data ?? beforeStop.body)?.vmId;
    expect(oldVmId).toBeTruthy();
    await runWorkspaceVmAction(page, workspace.id, "stop");
    const stopped = await waitForWorkspaceVmStatus(
      page,
      workspace.id,
      "stopped",
    );
    expect(stopped.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
    const docFiles = [
      { path: docsPath, bytes: docsBytes },
      { path: nextDocsPath, bytes: nextDocsBytes },
    ];
    // These agent-authored files have not been pushed. The stop receipt
    // preserves them for resume; it does not publish them for browsing.
    for (const { path } of docFiles) {
      const response = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${docsRepoId}`,
      );
      expect(response.status, JSON.stringify(response.body)).toBe(404);
    }
    await runWorkspaceVmAction(page, workspace.id, "resume");
    const restored = await waitForWorkspaceVmStatus(
      page,
      workspace.id,
      "running",
    );
    expect(restored.vmId).toBeTruthy();
    expect(restored.vmId).not.toBe(oldVmId);
    expect(restored.lastFlushId).toBe(stopped.lastFlushId);
    await page.reload({ waitUntil: "domcontentloaded" });
    for (const { path, bytes } of docFiles) {
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${docsRepoId}`,
            );
            expect(response.status, JSON.stringify(response.body)).toBe(200);
            const file = response.body?.data ?? response.body;
            expect(file.source).toBe("live");
            return file.content;
          },
          { timeout: 90_000, intervals: [500, 1_000] },
        )
        .toBe(bytes);
    }
    await expect(page.getByTestId("chat-messages")).toContainText(token, {
      timeout: 30_000,
    });
    const docsSection = page.getByTestId("sidebar-docs-tab");
    await expect(docsSection).toBeVisible();
    const docsHeader = docsSection.getByRole("button", {
      name: /^(project docs|docs)$/i,
    });
    if ((await docsHeader.getAttribute("aria-expanded")) === "false") {
      await docsHeader.click();
    }
    for (const { path, bytes } of docFiles) {
      const name = path.split("/").at(-1)!;
      const row = docsSection.getByTestId("doc-item").filter({ hasText: name });
      await expect(row).toBeVisible({ timeout: 30_000 });
      await row.click();
      await expect(page.getByTestId("file-view")).toContainText(
        bytes.replace(/^# /, "").trim(),
        { timeout: 30_000 },
      );
    }
    const sessionList = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/sessions`,
    );
    expect(sessionList.status, JSON.stringify(sessionList.body)).toBe(200);
    const retainedTitle = (sessionList.body?.data ?? sessionList.body)?.find(
      (session: { id: string }) => session.id === turn!.sessionId,
    )?.title;
    expect(retainedTitle).toBeTruthy();
    await page.getByTestId("center-chat-tab").click();
    await expect(page.getByTestId("chat-active-history")).toBeVisible();
    await page.getByTestId("chat-active-history").click();
    await page.getByPlaceholder("Search sessions…").fill(retainedTitle);
    const retainedSession = page
      .getByTestId("chat-session-select")
      .filter({ hasText: retainedTitle });
    await expect(retainedSession).toHaveCount(1);
    await retainedSession.click();
    await expect(page.getByTestId("chat-messages")).toContainText(token);
  });

  type SessionTimelineEntry = {
    metadata?: {
      kind?: string;
      payload?: {
        provider_id?: string;
        provider_session_id?: string;
        phase?: string;
        code?: string;
        level?: string;
        reason?: string;
        message?: string;
      };
    };
  };

  async function settledTaskTimeline(
    page: import("@playwright/test").Page,
    taskId: string,
    expectedStatus = "succeeded",
  ): Promise<SessionTimelineEntry[]> {
    await expect
      .poll(
        async () => {
          const task = await authedApiViaRequest(
            page,
            `/api/v1/tasks/${taskId}`,
          );
          expect(task.status, JSON.stringify(task.body)).toBe(200);
          return (task.body?.data ?? task.body)?.status;
        },
        { timeout: 120_000, intervals: [500, 1_000] },
      )
      .toBe(expectedStatus);
    const timeline = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${taskId}/timeline?limit=200`,
    );
    expect(timeline.status, JSON.stringify(timeline.body)).toBe(200);
    return (timeline.body?.data ?? timeline.body)
      ?.entries as SessionTimelineEntry[];
  }

  async function sendAndSettleChatFollowUp(
    page: import("@playwright/test").Page,
    sessionId: string,
    prompt: string,
    expectedStatus = "succeeded",
  ): Promise<{
    taskId: string;
    messageId: string;
    events: SessionTimelineEntry[];
  }> {
    const input = page.getByTestId("chat-input").first();
    await input.fill(prompt);
    const response = page.waitForResponse(
      (candidate) =>
        candidate.request().method() === "POST" &&
        new URL(candidate.url()).pathname ===
          `/api/v1/sessions/${sessionId}/messages`,
    );
    await input.press("Enter");
    const sent = await response;
    expect(sent.ok()).toBe(true);
    const body = await sent.json();
    const taskId = String((body?.data ?? body)?.taskId ?? "");
    const messageId = String((body?.data ?? body)?.messageId ?? "");
    expect(taskId).not.toBe("");
    return {
      taskId,
      messageId,
      events: await settledTaskTimeline(page, taskId, expectedStatus),
    };
  }

  async function recordReplaceVmRecall(
    page: import("@playwright/test").Page,
    provider: "codex" | "claude_code",
    mode:
      | "resume"
      | "warm_recall"
      | "absent"
      | "failed_save"
      | "failed_save_same_vm"
      | "refused_resume"
      | "stopped"
      | "stopped_failed_save"
      | "stopped_flush_save"
      | "chat_stop_next_turn",
  ) {
    const chatStopNextTurn = mode === "chat_stop_next_turn";
    const persist =
      mode === "resume" ||
      mode === "warm_recall" ||
      mode === "stopped" ||
      mode === "stopped_flush_save" ||
      chatStopNextTurn;
    const stopping =
      mode === "stopped" ||
      mode === "stopped_failed_save" ||
      mode === "stopped_flush_save" ||
      chatStopNextTurn;
    const deferStopSave = mode === "stopped_flush_save" || chatStopNextTurn;
    let readyFile:
      | { path: string; repoId: string; absolutePath: string }
      | undefined;
    if (stopping) {
      const detail = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}`,
      );
      expect(detail.status, JSON.stringify(detail.body)).toBe(200);
      const docsRepoId = String((detail.body?.data ?? detail.body)?.docsRepoId);
      const changes = await waitForLiveWorkspaceChanges(page, workspace.id);
      const code = changes.members.find(
        (member) => member.repo_id !== docsRepoId,
      );
      expect(code?.root_path).toBeTruthy();
      const path = `e2e-m14-native-ready-${randomUUID()}.txt`;
      readyFile = {
        path,
        repoId: code!.repo_id,
        absolutePath: `${code!.root_path}/${path}`,
      };
    }
    await selectAgentProviderViaSettings(page, provider);
    await expectSelectedAgentProvider(page, provider);
    const beforeVm = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/vm`,
    );
    expect(beforeVm.status, JSON.stringify(beforeVm.body)).toBe(200);
    const oldVmId = (beforeVm.body?.data ?? beforeVm.body)?.vmId;
    const firstInput = page.getByTestId("chat-input").first();
    await expect(firstInput).toBeVisible();
    await firstInput.fill(
      `Record a fact in tool output only. G5_TOOL_ONLY_FACT_E2E:record${mode === "absent" ? " G5_TOOL_ONLY_FACT_E2E:skip-persistence" : ""}${stopping ? ` G5_TOOL_ONLY_FACT_E2E:block-after-save G5_TOOL_ONLY_FACT_E2E:ready:${readyFile!.absolutePath}` : ""}${mode === "stopped_failed_save" ? " G5_TOOL_ONLY_FACT_E2E:fail-save" : ""}${deferStopSave ? " G5_TOOL_ONLY_FACT_E2E:defer-stop-save" : ""}`,
    );
    const firstResponse = page.waitForResponse((response) => {
      if (response.request().method() !== "POST") return false;
      return /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(
        new URL(response.url()).pathname,
      );
    });
    await firstInput.press("Enter");
    turn = await captureChatTurn(page, await firstResponse);
    const firstTaskId = turn.taskId;
    if (stopping) {
      await expect
        .poll(
          async () => {
            const timeline = await authedApiViaRequest(
              page,
              `/api/v1/tasks/${firstTaskId}/timeline?limit=200`,
            );
            if (timeline.status !== 200) return false;
            const entries =
              (timeline.body?.data ?? timeline.body)?.entries ?? [];
            return entries.some(
              (entry: { metadata?: { kind?: string; payload?: unknown } }) =>
                entry.metadata?.kind === "tool.completed" &&
                /tool-only-e2e-[0-9a-f-]{36}/.test(
                  JSON.stringify(entry.metadata.payload ?? {}),
                ),
            );
          },
          { timeout: 120_000, intervals: [500, 1_000] },
        )
        .toBe(true);
      // The fake writes this through the workspace only after its native
      // transcript or rollout is durable, then blocks until Stop reaps it.
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/files/${readyFile!.path}?repoId=${readyFile!.repoId}`,
            );
            return response.status === 200
              ? (response.body?.data ?? response.body)?.content
              : null;
          },
          { timeout: 90_000, intervals: [500, 1_000] },
        )
        .toMatch(/^[0-9a-f-]{36}$/);
      const active = await authedApiViaRequest(
        page,
        `/api/v1/tasks/${firstTaskId}`,
      );
      expect((active.body?.data ?? active.body)?.status).toBe("running");
      if (chatStopNextTurn) {
        const stopped = await authedApiViaRequest(
          page,
          `/api/v1/sessions/${turn.sessionId}/messages/${turn.inputId}/stop`,
          "POST",
        );
        expect(stopped.status, JSON.stringify(stopped.body)).toBe(200);
      } else {
        await runWorkspaceVmAction(page, workspace.id, "stop");
        await waitForWorkspaceVmStatus(page, workspace.id, "stopped");
      }
    } else {
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/tasks/${firstTaskId}`,
            );
            expect(response.status, JSON.stringify(response.body)).toBe(200);
            return (response.body?.data ?? response.body)?.status;
          },
          { timeout: 120_000, intervals: [500, 1_000] },
        )
        .toBe("succeeded");
    }
    if (stopping) {
      await expect
        .poll(
          async () => {
            const task = await authedApiViaRequest(
              page,
              `/api/v1/tasks/${firstTaskId}`,
            );
            expect(task.status, JSON.stringify(task.body)).toBe(200);
            return (task.body?.data ?? task.body)?.status;
          },
          { timeout: 120_000, intervals: [500, 1_000] },
        )
        .toBe(chatStopNextTurn ? "cancelled" : "waiting");
    }
    const firstTimeline = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${firstTaskId}/timeline?limit=200`,
    );
    expect(firstTimeline.status, JSON.stringify(firstTimeline.body)).toBe(200);
    const firstEvents = (firstTimeline.body?.data ?? firstTimeline.body)
      ?.entries as SessionTimelineEntry[];
    const providerSession = firstEvents.find(
      (entry) => entry.metadata?.kind === "provider.session",
    )?.metadata?.payload?.provider_session_id;
    expect(providerSession).toMatch(/^[0-9a-f-]{36}$/);
    expect(
      firstEvents.find((entry) => entry.metadata?.kind === "provider.session")
        ?.metadata?.payload?.provider_id,
    ).toBe(provider);
    if (mode === "stopped_failed_save") {
      expect(
        firstEvents.some(
          (entry) =>
            entry.metadata?.kind === "notice" &&
            entry.metadata.payload?.code === "session_files_persist_failed" &&
            entry.metadata.payload.reason === "symlink",
        ),
        JSON.stringify(firstEvents),
      ).toBe(true);
    }
    if (deferStopSave) {
      expect(
        firstEvents.some(
          (entry) =>
            entry.metadata?.kind === "notice" &&
            entry.metadata.payload?.code === "session_files_persist_failed" &&
            entry.metadata.payload.reason === "stopped_before_save",
        ),
        "the first turn must leave its native generation pending",
      ).toBe(true);
    }
    const facts = new Set(
      firstEvents
        .filter((entry) => entry.metadata?.kind === "tool.completed")
        .flatMap(
          (entry) =>
            JSON.stringify(entry.metadata?.payload ?? {}).match(
              /tool-only-e2e-[0-9a-f-]{36}/g,
            ) ?? [],
        ),
    );
    expect([...facts]).toHaveLength(1);
    const [fact] = [...facts];
    const beforeMessages = await authedApiViaRequest(
      page,
      `/api/v1/sessions/${turn.sessionId}/messages`,
    );
    expect(beforeMessages.status, JSON.stringify(beforeMessages.body)).toBe(
      200,
    );
    expect(JSON.stringify(beforeMessages.body)).not.toContain(fact);

    if (mode === "resume" || mode === "warm_recall") {
      const warm = await sendAndSettleChatFollowUp(
        page,
        turn.sessionId,
        mode === "warm_recall"
          ? "Recall the earlier tool-only fact. G5_TOOL_ONLY_FACT_E2E:recall"
          : "Continue while retaining the earlier native context.",
      );
      turn = {
        ...turn,
        taskId: warm.taskId,
        inputId: warm.messageId,
      };
      const native = warm.events.find(
        (entry) => entry.metadata?.kind === "provider.session",
      );
      expect(native?.metadata?.payload?.provider_id).toBe(provider);
      expect(
        native?.metadata?.payload?.phase,
        JSON.stringify(warm.events),
      ).toBe("resumed");
      expect(native?.metadata?.payload?.provider_session_id).toBe(
        providerSession,
      );
      expect(
        warm.events.some(
          (entry) =>
            entry.metadata?.kind === "notice" &&
            String(entry.metadata.payload?.code).startsWith("conversation_"),
        ),
        "same-VM verified resume must be silent",
      ).toBe(false);
      const warmMessages = await authedApiViaRequest(
        page,
        `/api/v1/sessions/${turn.sessionId}/messages`,
      );
      if (mode === "warm_recall")
        expect(JSON.stringify(warmMessages.body)).toContain(fact);
      else expect(JSON.stringify(warmMessages.body)).not.toContain(fact);
    }

    if (mode === "failed_save" || mode === "failed_save_same_vm") {
      const saved = await sendAndSettleChatFollowUp(
        page,
        turn.sessionId,
        "Continue, then make the native save fail. G5_TOOL_ONLY_FACT_E2E:fail-save",
      );
      turn = {
        ...turn,
        taskId: saved.taskId,
        inputId: saved.messageId,
      };
      expect(
        saved.events.some(
          (entry) =>
            entry.metadata?.kind === "notice" &&
            entry.metadata.payload?.code === "session_files_persist_failed" &&
            entry.metadata.payload.reason === "symlink" &&
            entry.metadata.payload.message?.includes(
              "The next turn will rebuild",
            ),
        ),
        JSON.stringify(saved.events),
      ).toBe(true);
    }
    if (mode === "failed_save_same_vm") {
      const rebuilt = await sendAndSettleChatFollowUp(
        page,
        turn.sessionId,
        "Recall the earlier tool-only fact. G5_TOOL_ONLY_FACT_E2E:recall",
      );
      turn = {
        ...turn,
        taskId: rebuilt.taskId,
        inputId: rebuilt.messageId,
      };
      const native = rebuilt.events.find(
        (entry) => entry.metadata?.kind === "provider.session",
      );
      expect(
        native?.metadata?.payload?.phase,
        JSON.stringify(rebuilt.events),
      ).toBe("started");
      expect(native?.metadata?.payload?.provider_session_id).not.toBe(
        providerSession,
      );
      expect(
        rebuilt.events.some(
          (entry) =>
            entry.metadata?.kind === "notice" &&
            entry.metadata.payload?.code === "conversation_reconstructed",
        ),
      ).toBe(true);
      const sameVm = await waitForWorkspaceVmStatus(
        page,
        workspace.id,
        "running",
      );
      expect(sameVm.vmId).toBe(oldVmId);
      const messages = await authedApiViaRequest(
        page,
        `/api/v1/sessions/${turn.sessionId}/messages`,
      );
      expect(JSON.stringify(messages.body)).not.toContain(fact);
      const replies = (
        (messages.body?.data ?? messages.body)?.messages ?? []
      ).filter((message: { role: string }) => message.role === "assistant");
      expect(replies.at(-1)?.content).toContain("tool-only fact unavailable");
      return;
    }
    if (mode === "refused_resume") {
      const refused = await sendAndSettleChatFollowUp(
        page,
        turn.sessionId,
        "Try the saved session. G5_TOOL_ONLY_FACT_E2E:refuse-resume",
        "failed",
      );
      turn = {
        ...turn,
        taskId: refused.taskId,
        inputId: refused.messageId,
      };
      expect(
        refused.events.some(
          (entry) =>
            entry.metadata?.kind === "notice" &&
            entry.metadata.payload?.code === "session_files_persist_failed" &&
            entry.metadata.payload.reason === "resume_not_reported",
        ),
        "a refused resume must not republish the rejected native set",
      ).toBe(true);
      expect(
        refused.events.some(
          (entry) => entry.metadata?.kind === "provider.session",
        ),
      ).toBe(false);
    }

    if (!stopping) {
      await runWorkspaceVmAction(page, workspace.id, "stop");
      await waitForWorkspaceVmStatus(page, workspace.id, "stopped");
    }
    if (!chatStopNextTurn) {
      await runWorkspaceVmAction(page, workspace.id, "resume");
      const replacement = await waitForWorkspaceVmStatus(
        page,
        workspace.id,
        "running",
      );
      expect(replacement.vmId).not.toBe(oldVmId);
    } else {
      const sameVm = await waitForWorkspaceVmStatus(
        page,
        workspace.id,
        "running",
      );
      expect(sameVm.vmId).toBe(oldVmId);
    }
    await page.reload({ waitUntil: "domcontentloaded" });
    const sessions = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/sessions`,
    );
    expect(sessions.status, JSON.stringify(sessions.body)).toBe(200);
    const title = (sessions.body?.data ?? sessions.body)?.find(
      (session: { id: string }) => session.id === turn!.sessionId,
    )?.title;
    expect(title).toBeTruthy();
    await page.getByTestId("center-chat-tab").click();
    await page.getByTestId("chat-active-history").click();
    await page.getByPlaceholder("Search sessions…").fill(title);
    const retained = page
      .getByTestId("chat-session-select")
      .filter({ hasText: title });
    await expect(retained).toHaveCount(1);
    await retained.click();
    // Start returns the turn a workspace Stop held to the queue
    // (`g5.lifecycle.start.held-work`), so it continues with no Continue.
    let continuationEvents: SessionTimelineEntry[] | undefined;
    if (stopping && !chatStopNextTurn) {
      continuationEvents = await settledTaskTimeline(page, firstTaskId);
    }
    const recalled = await sendAndSettleChatFollowUp(
      page,
      turn.sessionId,
      "Recall the earlier tool-only fact. G5_TOOL_ONLY_FACT_E2E:recall",
    );
    if (provider === "codex") {
      await expectCodexAppServerAttempts(firstTaskId);
      await expectCodexAppServerAttempts(recalled.taskId);
    }
    if (stopping) expect(recalled.taskId).not.toBe(firstTaskId);
    turn = {
      ...turn,
      taskId: recalled.taskId,
      inputId: recalled.messageId,
    };
    // After a workspace Stop, Start continues the held turn before the next
    // human turn. Chat Stop cancels its turn, so the next human turn must
    // complete the pending save and select native resume itself.
    const nextEvents = continuationEvents ?? recalled.events;
    const conversationNotices = nextEvents.filter(
      (entry) =>
        entry.metadata?.kind === "notice" &&
        String(entry.metadata.payload?.code).startsWith("conversation_"),
    );
    expect(
      conversationNotices,
      "a verified native resume is silent",
    ).toHaveLength(persist ? 0 : 1);
    if (!persist)
      expect(conversationNotices[0]?.metadata?.payload?.code).toBe(
        "conversation_reconstructed",
      );
    if (mode === "absent") {
      expect(conversationNotices[0]?.metadata?.payload?.level).toBe("warning");
      expect(conversationNotices[0]?.metadata?.payload?.message).toContain(
        "prior native session could not resume",
      );
    }
    // Continue reuses the interrupted logical task. Chat Stop's next turn is
    // a distinct task and has only its own provider.session event.
    const nativeEvents = nextEvents.filter(
      (entry) => entry.metadata?.kind === "provider.session",
    );
    if (stopping && !chatStopNextTurn)
      expect(nativeEvents.length).toBeGreaterThan(1);
    if (chatStopNextTurn) expect(nativeEvents).toHaveLength(1);
    const native = nativeEvents.at(-1);
    expect(native?.metadata?.payload?.provider_id).toBe(provider);
    expect(native?.metadata?.payload?.phase).toBe(
      persist ? "resumed" : "started",
    );
    if (persist)
      expect(native?.metadata?.payload?.provider_session_id).toBe(
        providerSession,
      );
    else
      expect(native?.metadata?.payload?.provider_session_id).not.toBe(
        providerSession,
      );
    if (stopping && !chatStopNextTurn) {
      const recallNative = recalled.events.find(
        (entry) => entry.metadata?.kind === "provider.session",
      );
      expect(recallNative?.metadata?.payload?.provider_id).toBe(provider);
      expect(recallNative?.metadata?.payload?.phase).toBe("resumed");
      expect(recallNative?.metadata?.payload?.provider_session_id).toBe(
        native?.metadata?.payload?.provider_session_id,
      );
    }
    const afterMessages = await authedApiViaRequest(
      page,
      `/api/v1/sessions/${turn.sessionId}/messages`,
    );
    expect(afterMessages.status, JSON.stringify(afterMessages.body)).toBe(200);
    const replies = (
      (afterMessages.body?.data ?? afterMessages.body)?.messages ?? []
    ).filter((message: { role: string }) => message.role === "assistant");
    expect(replies.at(-1)?.content).toContain(
      persist ? fact : "tool-only fact unavailable",
    );
  }

  for (const provider of ["claude_code", "codex"] as const) {
    for (const persist of [true, false]) {
      test(`M14 ${provider} ${persist ? "recalls a restored tool result" : "rebuilds when no complete bundle exists"}`, async ({
        page,
      }) => {
        await recordReplaceVmRecall(
          page,
          provider,
          persist ? "resume" : "absent",
        );
      });
    }
    test(`M14 ${provider} recalls a tool-only fact on the same VM`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "warm_recall");
    });
  }
  for (const provider of ["claude_code", "codex"] as const) {
    test(`M14 ${provider} rebuilds after a complete generation is followed by a failed save`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "failed_save");
    });
    test(`M14 ${provider} rebuilds after failed save on the same VM`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "failed_save_same_vm");
    });
    test(`M14 ${provider} rebuilds after the provider refuses native resume`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "refused_resume");
    });
    test(`M14 ${provider} resumes files saved after Stop interrupted a running provider`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "stopped");
    });
    test(`M14 ${provider} rebuilds when Stop cannot save native files`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "stopped_failed_save");
    });
    test(`M14 ${provider} shutdown flush completes a pending native save`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "stopped_flush_save");
    });
    test(`M14 ${provider} chat Stop then next turn completes a pending native save on the same VM`, async ({
      page,
    }) => {
      await recordReplaceVmRecall(page, provider, "chat_stop_next_turn");
    });
  }
});
