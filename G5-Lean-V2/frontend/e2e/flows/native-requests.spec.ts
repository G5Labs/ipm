import { randomUUID } from "node:crypto";
import { expect, test, type BrowserContext, type Page } from "@playwright/test";
import { Pool } from "pg";
import {
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  expectSelectedAgentProvider,
  openWorkspacePath,
  readCurrentOrgContext,
  recordExpectedSubject,
  releaseChatTurn,
  runWorkspaceVmAction,
  selectAgentProviderViaSettings,
  waitForWorkspaceVmStatus,
  TEST_ORG,
  USING_OAUTH_STUB,
  type ChatTurnIdentity,
  type WorkspaceClone,
  expectAssistantReply,
} from "../helpers";
import { loginWithOAuthStub, SEEDED_FORK_MEMBER_IDENTITY } from "../oauth";

type NativeRequest = {
  id: string;
  attempt: number;
  kind: "question" | "permission";
};

async function startProbe(
  page: Page,
  kind: "QUESTION" | "PERMISSION" | "EXIT",
  token: string,
  onCreated: (turn: ChatTurnIdentity) => void,
): Promise<ChatTurnIdentity> {
  const input = page.getByTestId("chat-input").first();
  await expect(input).toBeVisible({ timeout: 30_000 });
  await input.fill(`G5_NATIVE_${kind}_E2E:${token}`);
  const send = page.waitForResponse((response) => {
    if (response.request().method() !== "POST") return false;
    const path = new URL(response.url()).pathname;
    return (
      /\/api\/v1\/sessions\/[^/]+\/messages$/.test(path) ||
      /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(path)
    );
  });
  await input.press("Enter");
  return captureChatTurn(page, await send, onCreated);
}

async function waitForNativeRequest(
  page: Page,
  taskId: string,
  kind: NativeRequest["kind"],
): Promise<NativeRequest> {
  let found: NativeRequest | undefined;
  await expect
    .poll(
      async () => {
        const response = await authedApiViaRequest(
          page,
          `/api/v1/tasks/${taskId}/timeline`,
        );
        expect(response.status, JSON.stringify(response.body)).toBe(200);
        const entries = (response.body?.data ?? response.body)?.entries ?? [];
        const opened = entries.find(
          (entry: { metadata?: Record<string, unknown> }) =>
            entry.metadata?.kind === "provider_request.opened" &&
            (entry.metadata?.payload as Record<string, unknown> | undefined)
              ?.request_kind === kind,
        );
        if (!opened) return false;
        const payload = opened.metadata.payload;
        found = {
          id: String(payload.provider_request_id),
          attempt: Number(opened.metadata.attempt),
          kind,
        };
        return true;
      },
      { timeout: 90_000, intervals: [500, 1_000] },
    )
    .toBe(true);
  expect(found?.id).toBeTruthy();
  expect(found?.attempt).toBeGreaterThan(0);
  return found!;
}

async function expectBypassQuestionPolicy(taskId: string, attempt: number) {
  const databaseUrl = process.env.E2E_DATABASE_URL;
  expect(
    databaseUrl,
    "candidate attempt policy must be inspected",
  ).toBeTruthy();
  const pool = new Pool({ connectionString: databaseUrl });
  try {
    const result = await pool.query<{ policy: Record<string, unknown> }>(
      `SELECT policy_snapshot_ref->'permission_policy' AS policy
       FROM task_attempts WHERE task_id = $1 AND attempt = $2`,
      [taskId, attempt],
    );
    expect(result.rows[0]?.policy).toMatchObject({
      questions: "enabled",
      tool_approval: "auto_allow",
      sandbox: "none",
    });
  } finally {
    await pool.end();
  }
}

async function expectQuestionPresentation(
  page: Page,
  taskId: string,
  state: "waiting" | "answered",
) {
  // Wait for the attempt to settle: checking while it still streams would
  // miss the persisted stopped status that caused the historical badge.
  await expect
    .poll(
      async () => {
        const task = await authedApiViaRequest(page, `/api/v1/tasks/${taskId}`);
        return (task.body?.data ?? task.body)?.status;
      },
      { timeout: 90_000 },
    )
    .toBe(state === "waiting" ? "waiting" : "succeeded");
  const card = page
    .getByTestId("assistant-tool-block")
    .filter({ hasText: "AskUserQuestion" })
    .last();
  await expect(card.locator(":scope > summary")).toContainText(
    state === "waiting" ? "Waiting for your answer" : "Answered",
  );
  await expect(card).not.toContainText("pending approval");
  await expect(
    page.locator(
      '[data-testid="assistant-status-block"][data-status-kind="stopped"]',
    ),
  ).toHaveCount(0);
}

async function loginMember(
  page: Page,
  browser: import("@playwright/test").Browser,
) {
  expect(USING_OAUTH_STUB, "reader qualification needs OAuth stub").toBe(true);
  const orgId = (await readCurrentOrgContext(page)).orgId;
  const invite = await authedApiViaRequest(
    page,
    `/api/v1/orgs/${orgId}/members`,
    "POST",
    { email: SEEDED_FORK_MEMBER_IDENTITY.email, role: "member" },
  );
  expect([200, 201], JSON.stringify(invite.body)).toContain(invite.status);
  const context = await browser.newContext({
    storageState: { cookies: [], origins: [] },
  });
  const memberPage = await context.newPage();
  await loginWithOAuthStub(memberPage, SEEDED_FORK_MEMBER_IDENTITY, {
    targetOrgSlug: TEST_ORG,
  });
  const memberId = await recordExpectedSubject(memberPage);
  return { context, memberPage, memberId };
}

test.describe("Native request owner flow", () => {
  test.describe.configure({ timeout: 600_000 });
  // Every test clones its own workspace VM; parallel-safe across workers
  // (deployment g5.quality.delivery.ci.provider-agent-parallel).
  test.describe.configure({ mode: "parallel" });
  let workspace: WorkspaceClone;
  let turn: ChatTurnIdentity | undefined;
  let memberContext: BrowserContext | undefined;
  let cleanupPage: Page | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `native-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
    await selectAgentProviderViaSettings(page, "claude_code");
    await expectSelectedAgentProvider(page, "claude_code");
  });

  test.afterEach(async ({ page }) => {
    if (turn) await releaseChatTurn(page, turn).catch(() => {});
    await deleteWorkspaceById(cleanupPage ?? page, workspace?.id).catch(
      () => {},
    );
    await memberContext?.close().catch(() => {});
    turn = undefined;
    memberContext = undefined;
    cleanupPage = undefined;
  });

  test("M7 reader cannot answer a native question; owner UI resumes Claude once", async ({
    page,
    browser,
  }) => {
    const member = await loginMember(page, browser);
    memberContext = member.context;
    const memberPage = member.memberPage;

    const token = randomUUID();
    turn = await startProbe(page, "QUESTION", token, (created) => {
      turn = created;
    });
    const request = await waitForNativeRequest(page, turn.taskId, "question");
    await expectBypassQuestionPolicy(turn.taskId, request.attempt);
    await expect(page.getByTestId("agent-question-form")).toBeVisible({
      timeout: 30_000,
    });
    await expectQuestionPresentation(page, turn.taskId, "waiting");
    await page.reload();
    await expectQuestionPresentation(page, turn.taskId, "waiting");
    const denied = await authedApiViaRequest(
      memberPage,
      `/api/v1/tasks/${turn.taskId}/provider-requests/${request.id}/reply`,
      "POST",
      {
        attempt: request.attempt,
        requestId: randomUUID(),
        validatedResponse: {
          g5AnswersByIndex: { "0": { selectedOptionIndexes: [0] } },
        },
      },
    );
    expect(denied.status, JSON.stringify(denied.body)).toBe(403);
    await page
      .getByTestId("agent-question-option")
      .filter({ hasText: "Blue" })
      .click();
    await page.getByTestId("agent-question-submit").click();
    await expectAssistantReply(
      page,
      turn!,
      `Native question answered Blue for ${token}.`,
      {
        timeout: 90_000,
      },
    );
    await expectQuestionPresentation(page, turn.taskId, "answered");
    await page.reload();
    await expectQuestionPresentation(page, turn.taskId, "answered");
    const duplicate = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}/provider-requests/${request.id}/reply`,
      "POST",
      {
        attempt: request.attempt,
        requestId: randomUUID(),
        validatedResponse: {
          g5AnswersByIndex: { "0": { selectedOptionIndexes: [0] } },
        },
      },
    );
    expect(duplicate.status, JSON.stringify(duplicate.body)).toBe(409);
  });

  test("Bypass accepts free text on fresh and resumed native sessions", async ({
    page,
  }) => {
    let sessionId: string | undefined;
    let providerSessionId: string | undefined;
    for (const answer of [
      "Custom blue 🟦",
      "Red and Blue, with my own explanation",
    ]) {
      const token = randomUUID();
      turn = await startProbe(page, "QUESTION", token, (created) => {
        turn = created;
      });
      if (sessionId) expect(turn.sessionId).toBe(sessionId);
      sessionId = turn.sessionId;
      const request = await waitForNativeRequest(page, turn.taskId, "question");
      await expectBypassQuestionPolicy(turn.taskId, request.attempt);
      await page.getByTestId("agent-question-other").fill(answer);
      await page.getByTestId("agent-question-submit").click();
      await expectAssistantReply(
        page,
        turn!,
        `Native question answered ${answer} for ${token}.`,
        {
          timeout: 90_000,
        },
      );
      await expect
        .poll(
          async () => {
            const task = await authedApiViaRequest(
              page,
              `/api/v1/tasks/${turn!.taskId}`,
            );
            return (task.body?.data ?? task.body)?.status;
          },
          { timeout: 90_000 },
        )
        .toBe("succeeded");
      const timeline = await authedApiViaRequest(
        page,
        `/api/v1/tasks/${turn.taskId}/timeline`,
      );
      expect(timeline.status, JSON.stringify(timeline.body)).toBe(200);
      const entries = (timeline.body?.data ?? timeline.body)?.entries ?? [];
      const native = entries.find(
        (entry: { metadata?: { kind?: string } }) =>
          entry.metadata?.kind === "provider.session",
      );
      const nativeId = native?.metadata?.payload?.provider_session_id;
      expect(nativeId).toBeTruthy();
      if (providerSessionId) {
        expect(native?.metadata?.payload?.phase).toBe("resumed");
        expect(nativeId).toBe(providerSessionId);
      }
      providerSessionId = nativeId;
    }
  });

  test("M7 owner approves a native permission and Claude observes delivery", async ({
    page,
    browser,
  }) => {
    const member = await loginMember(page, browser);
    memberContext = member.context;
    const token = randomUUID();
    turn = await startProbe(page, "PERMISSION", token, (created) => {
      turn = created;
    });
    const request = await waitForNativeRequest(page, turn.taskId, "permission");
    await expect(page.getByTestId("agent-approval-approve")).toBeVisible({
      timeout: 30_000,
    });
    await expect(
      page.getByTestId("assistant-tool-block").last().locator(":scope > summary"),
    ).toContainText("pending approval");
    const denied = await authedApiViaRequest(
      member.memberPage,
      `/api/v1/tasks/${turn.taskId}/provider-requests/${request.id}/reply`,
      "POST",
      {
        attempt: request.attempt,
        requestId: randomUUID(),
        validatedResponse: { behavior: "allow" },
      },
    );
    expect(denied.status, JSON.stringify(denied.body)).toBe(403);
    await page.getByTestId("agent-approval-approve").click();
    await expectAssistantReply(
      page,
      turn!,
      `Native permission allow for ${token}.`,
      {
        timeout: 90_000,
      },
    );
  });

  test("M8 cancellation fences a pending native answer", async ({ page }) => {
    const token = randomUUID();
    turn = await startProbe(page, "QUESTION", token, (created) => {
      turn = created;
    });
    const request = await waitForNativeRequest(page, turn.taskId, "question");
    await expectQuestionPresentation(page, turn.taskId, "waiting");
    const cancel = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}/cancel`,
      "POST",
      {},
    );
    expect(cancel.status, JSON.stringify(cancel.body)).toBe(200);
    await expect
      .poll(
        async () => {
          const task = await authedApiViaRequest(
            page,
            `/api/v1/tasks/${turn!.taskId}`,
          );
          return (task.body?.data ?? task.body)?.status;
        },
        { timeout: 30_000, intervals: [500, 1_000] },
      )
      .toBe("cancelled");
    // This out-of-page API cancellation of a held task does not invalidate
    // the browser's query cache. Reload to read the authoritative cancellation.
    await page.reload();
    await expect(
      page.locator(
        '[data-testid="assistant-status-block"][data-status-kind="stopped"]',
      ),
    ).toBeVisible();
    await expect(
      page.getByTestId("assistant-tool-block").last().locator(":scope > summary"),
    ).toContainText("Stopped");
    await expect(page.getByTestId("agent-question-form")).toHaveCount(0);
    const stale = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}/provider-requests/${request.id}/reply`,
      "POST",
      {
        attempt: request.attempt,
        requestId: randomUUID(),
        validatedResponse: {
          g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
        },
      },
    );
    expect(stale.status, JSON.stringify(stale.body)).toBe(409);
  });

  test("M8 questions remain answerable beyond the permission deadline", async ({
    page,
  }) => {
    const databaseUrl = process.env.E2E_DATABASE_URL;
    expect(
      databaseUrl,
      "permission deadline setup requires the CI test database",
    ).toBeTruthy();
    const token = randomUUID();
    const timeoutMs = 8_000;

    // The stopped VM keeps the public-API-created task queued while this test
    // sets its policy. Only configuration is written directly; dispatch,
    // question lifetime, task state, and native answer delivery use the production path.
    await runWorkspaceVmAction(page, workspace.id, "stop");
    await waitForWorkspaceVmStatus(page, workspace.id, "stopped");
    const start = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/sessions/start`,
      "POST",
      {
        content: `G5_NATIVE_QUESTION_E2E:${token}`,
        agentProvider: "claude_code",
        permissionMode: "bypassPermissions",
      },
    );
    expect(start.status, JSON.stringify(start.body)).toBe(201);
    const started = start.body?.data ?? start.body;
    turn = {
      workspaceId: workspace.id,
      sessionId: String(started.session?.id),
      inputId: String(started.assistantMessageId),
      taskId: String(started.taskId),
    };
    expect(turn.sessionId).toMatch(/^[0-9a-f-]{36}$/);
    expect(turn.taskId).toMatch(/^[0-9a-f-]{36}$/);

    const pool = new Pool({ connectionString: databaseUrl });
    try {
      const configured = await pool.query<{ native_timeout_ms: string }>(
        `UPDATE tasks
         SET spec = jsonb_set(
           spec, '{agent,limits,nativeResponseTimeoutMs}', to_jsonb($3::bigint), true
         )
         WHERE id = $1 AND workspace_id = $2 AND kind = 'agent' AND status = 'queued'
         RETURNING spec #>> '{agent,limits,nativeResponseTimeoutMs}' AS native_timeout_ms`,
        [turn.taskId, workspace.id, timeoutMs],
      );
      expect(configured.rowCount, "one undispatched task was configured").toBe(
        1,
      );
      expect(Number(configured.rows[0]?.native_timeout_ms)).toBe(timeoutMs);
    } finally {
      await pool.end();
    }

    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const request = await waitForNativeRequest(page, turn.taskId, "question");
    const timeline = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}/timeline`,
    );
    expect(timeline.status, JSON.stringify(timeline.body)).toBe(200);
    const opened = (timeline.body?.data ?? timeline.body)?.entries.find(
      (entry: { metadata?: Record<string, unknown> }) =>
        entry.metadata?.kind === "provider_request.opened" &&
        (entry.metadata?.payload as Record<string, unknown> | undefined)
          ?.provider_request_id === request.id,
    );
    expect(opened, "the candidate VM opened the native question").toBeTruthy();
    expect(opened.metadata.payload.deadline_unix_ms).toBeNull();
    // This session was created through the API, so select it explicitly before
    // exercising the question card across the permission deadline and reload.
    await openWorkspacePath(
      page,
      `${workspace.path}?session=${turn.sessionId}`,
    );
    await expect(page.getByTestId("agent-question-form")).toBeVisible({
      timeout: 30_000,
    });
    await page.waitForTimeout(timeoutMs + 1_000);
    await page.reload();
    await expect(page.getByTestId("agent-question-form")).toBeVisible({
      timeout: 30_000,
    });
    await page
      .getByTestId("agent-question-option")
      .filter({ hasText: "Blue" })
      .click();
    await page.getByTestId("agent-question-submit").click();
    await expectAssistantReply(
      page,
      turn!,
      `Native question answered Blue for ${token}.`,
      {
        timeout: 90_000,
      },
    );
  });

  test("M8 Stop and Resume preserve the same unanswered native question", async ({
    page,
  }) => {
    const token = randomUUID();
    turn = await startProbe(page, "QUESTION", token, (created) => {
      turn = created;
    });
    const request = await waitForNativeRequest(page, turn.taskId, "question");
    await expect
      .poll(
        async () => {
          const response = await authedApiViaRequest(
            page,
            `/api/v1/tasks/${turn!.taskId}`,
          );
          return (response.body?.data ?? response.body)?.status;
        },
        { timeout: 90_000 },
      )
      .toBe("waiting");
    const before = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/vm`,
    );
    expect(before.status, JSON.stringify(before.body)).toBe(200);
    const oldVmId = (before.body?.data ?? before.body)?.vmId;
    expect(oldVmId, JSON.stringify(before.body)).toBeTruthy();
    await runWorkspaceVmAction(page, workspace.id, "stop");
    await waitForWorkspaceVmStatus(page, workspace.id, "stopped");
    await runWorkspaceVmAction(page, workspace.id, "resume");
    const resumed = await waitForWorkspaceVmStatus(
      page,
      workspace.id,
      "running",
    );
    expect(resumed.vmId).toBeTruthy();
    expect(resumed.vmId).not.toBe(oldVmId);
    await page.reload();
    const restored = await waitForNativeRequest(page, turn.taskId, "question");
    expect(restored).toEqual(request);
    await expect(page.getByTestId("agent-question-form")).toBeVisible({
      timeout: 30_000,
    });
    await expectQuestionPresentation(page, turn.taskId, "waiting");
    await page
      .getByTestId("agent-question-option")
      .filter({ hasText: "Blue" })
      .click();
    await page.getByTestId("agent-question-submit").click();
    await expectAssistantReply(
      page,
      turn!,
      `Native question answered Blue for ${token}.`,
      {
        timeout: 90_000,
      },
    );
    await expectQuestionPresentation(page, turn.taskId, "answered");
    await page.reload();
    await expectQuestionPresentation(page, turn.taskId, "answered");
    const timeline = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}/timeline`,
    );
    const entries = (timeline.body?.data ?? timeline.body)?.entries ?? [];
    const delivered = entries.find(
      (entry: {
        metadata?: { kind?: string; payload?: Record<string, unknown> };
      }) =>
        entry.metadata?.kind === "provider_request.replied" &&
        entry.metadata.payload?.provider_request_id === request.id,
    );
    expect(delivered?.metadata.payload.source_attempt).toBe(request.attempt);
    expect(delivered?.metadata.attempt).toBeGreaterThan(request.attempt);
  });

  test("M8 provider exit invalidates its unanswered native request", async ({
    page,
  }) => {
    const token = randomUUID();
    turn = await startProbe(page, "EXIT", token, (created) => {
      turn = created;
    });
    const request = await waitForNativeRequest(page, turn.taskId, "question");
    await expect
      .poll(
        async () => {
          const timeline = await authedApiViaRequest(
            page,
            `/api/v1/tasks/${turn!.taskId}/timeline`,
          );
          expect(timeline.status, JSON.stringify(timeline.body)).toBe(200);
          return (timeline.body?.data ?? timeline.body)?.entries.some(
            (entry: { metadata?: Record<string, unknown> }) =>
              entry.metadata?.kind === "provider_request.invalidated" &&
              (entry.metadata?.payload as Record<string, unknown> | undefined)
                ?.provider_request_id === request.id,
          );
        },
        { timeout: 60_000, intervals: [500, 1_000] },
      )
      .toBe(true);
    const stale = await authedApiViaRequest(
      page,
      `/api/v1/tasks/${turn.taskId}/provider-requests/${request.id}/reply`,
      "POST",
      {
        attempt: request.attempt,
        requestId: randomUUID(),
        validatedResponse: {
          g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
        },
      },
    );
    expect(stale.status, JSON.stringify(stale.body)).toBe(409);
  });

  test("M7/M8 transfer lets the current owner answer the preserved native question", async ({
    page,
    browser,
  }) => {
    const member = await loginMember(page, browser);
    memberContext = member.context;
    const token = randomUUID();
    turn = await startProbe(page, "QUESTION", token, (created) => {
      turn = created;
    });
    const request = await waitForNativeRequest(page, turn.taskId, "question");
    const transferKey = randomUUID();
    await expect
      .poll(
        async () => {
          try {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/transfer`,
              "POST",
              { toUserId: member.memberId, idempotencyKey: transferKey },
            );
            expect(response.status, JSON.stringify(response.body)).toBe(200);
            return (response.body?.data ?? response.body)?.status;
          } catch (error) {
            if (
              error instanceof Error &&
              /timeout|ECONNRESET/.test(error.message)
            ) {
              return "pending";
            }
            throw error;
          }
        },
        { timeout: 360_000, intervals: [1_000, 2_000] },
      )
      .toBe("completed");
    cleanupPage = member.memberPage;
    await waitForWorkspaceVmStatus(member.memberPage, workspace.id, "stopped");
    const answer = {
      attempt: request.attempt,
      requestId: randomUUID(),
      validatedResponse: {
        g5AnswersByIndex: { "0": { selectedOptionIndexes: [1] } },
      },
    };
    const route = `/api/v1/tasks/${turn.taskId}/provider-requests/${request.id}/reply`;
    const oldOwner = await authedApiViaRequest(page, route, "POST", answer);
    expect(oldOwner.status, JSON.stringify(oldOwner.body)).toBe(403);
    const newOwner = await authedApiViaRequest(
      member.memberPage,
      route,
      "POST",
      { ...answer, requestId: randomUUID() },
    );
    expect(newOwner.status, JSON.stringify(newOwner.body)).toBe(409);
    await runWorkspaceVmAction(member.memberPage, workspace.id, "resume");
    await waitForWorkspaceVmStatus(member.memberPage, workspace.id, "running");
    await openWorkspacePath(
      member.memberPage,
      `${workspace.path}?session=${turn.sessionId}`,
    );
    await expect(
      member.memberPage.getByTestId("agent-question-form"),
    ).toBeVisible({ timeout: 30_000 });
    const accepted = await authedApiViaRequest(
      member.memberPage,
      route,
      "POST",
      answer,
    );
    expect(accepted.status, JSON.stringify(accepted.body)).toBe(200);
    await expect(
      member.memberPage.getByTestId("assistant-message").last(),
    ).toContainText(`Native question answered Blue for ${token}.`, {
      timeout: 90_000,
    });
    const timeline = await authedApiViaRequest(
      member.memberPage,
      `/api/v1/tasks/${turn.taskId}/timeline`,
    );
    expect(timeline.status, JSON.stringify(timeline.body)).toBe(200);
    expect(
      (timeline.body?.data ?? timeline.body)?.entries.some(
        (entry: { metadata?: Record<string, unknown> }) =>
          entry.metadata?.kind === "provider_request.opened" &&
          (entry.metadata?.payload as Record<string, unknown> | undefined)
            ?.provider_request_id === request.id,
      ),
    ).toBe(true);
  });
});
