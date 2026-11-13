// =============================================================================
// E2E - Chat Timeline No-Refresh Recovery
// =============================================================================
// Verifies that an active right-panel chat turn recovers from missed websocket
// finalization by re-reading server-authoritative session/message/timeline
// state. The test mocks HTTP at the browser boundary because the regression is
// client cache recovery, not provider execution.
// =============================================================================

import { expect, test, type Page, type Route } from "@playwright/test";

const ORG_SLUG = "org-g5labs";
const ORG_ID = "11111111-1111-4111-8111-111111111111";
const WORKSPACE_SLUG = "timeline-recovery";
const WORKSPACE_ID = "22222222-2222-4222-8222-222222222222";
const SESSION_ID = "44444444-4444-4444-8444-444444444444";
const USER_MESSAGE_ID = "55555555-5555-4555-8555-555555555555";
const ASSISTANT_MESSAGE_ID = "66666666-6666-4666-8666-666666666666";
const TASK_ID = "88888888-8888-4888-8888-888888888888";
const FINAL_TEXT = "The chat turn finished without a page refresh.";

function apiJson(data: unknown): {
  status: number;
  contentType: string;
  body: string;
} {
  return {
    status: 200,
    contentType: "application/json",
    body: JSON.stringify({ data }),
  };
}

function createJwt(): string {
  const now = Math.floor(Date.now() / 1000);
  const encode = (value: unknown) =>
    Buffer.from(JSON.stringify(value)).toString("base64url");
  return [
    encode({ alg: "HS256", typ: "JWT" }),
    encode({
      sub: "e2e-user",
      email: "e2e-chat-recovery@g5.dev",
      orgId: ORG_ID,
      role: "admin",
      scopes: [
        "workspaces:read",
        "workspaces:write",
        "sessions:write",
        "tasks:execute",
        "workers:admin",
      ],
      iat: now,
      exp: now + 3600,
    }),
    "test-signature",
  ].join(".");
}

async function installChatRecoveryRoutes(
  page: Page,
  provider: "claude_code" | "codex" = "claude_code",
): Promise<{
  completeTurn: () => void;
  counts: Record<string, number>;
}> {
  const jwt = createJwt();
  let turnComplete = false;
  const counts: Record<string, number> = {
    messages: 0,
    timeline: 0,
    session: 0,
    sessions: 0,
    tasks: 0,
  };

  const now = new Date("2026-06-04T12:00:00.000Z").toISOString();
  const session = () => ({
    id: SESSION_ID,
    workspaceId: WORKSPACE_ID,
    sessionType: "chat",
    taskId: null,
    scopeType: "workspace",
    scopeRef: WORKSPACE_ID,
    model: null,
    agentProvider: provider,
    providerState: null,
    title: "Timeline recovery turn",
    summary: null,
    tags: [],
    status: "active",
    messageCount: 2,
    totalInputTokens: 0,
    totalOutputTokens: turnComplete ? 12 : 0,
    totalCostUsd: 0,
    forkedFromId: null,
    branch: "main",
    branchMode: "workspace_head",
    linkedCommits: [],
    createdAt: now,
    updatedAt: now,
  });
  const messages = () => ({
    messages: [
      {
        id: USER_MESSAGE_ID,
        sessionId: SESSION_ID,
        role: "user",
        content: "Please verify the timeline recovery path.",
        artifacts: [],
        citations: [],
        inputTokens: null,
        outputTokens: null,
        status: "complete",
        messageIndex: 0,
        createdAt: now,
      },
      {
        id: ASSISTANT_MESSAGE_ID,
        sessionId: SESSION_ID,
        role: "assistant",
        content: turnComplete ? FINAL_TEXT : "",
        artifacts: [],
        citations: [],
        inputTokens: null,
        outputTokens: turnComplete ? 12 : null,
        status: turnComplete ? "complete" : "pending",
        messageIndex: 1,
        createdAt: now,
      },
    ],
    cursor: null,
    hasMore: false,
  });
  const timeline = () => ({
    entries: turnComplete
      ? [
          {
            id: "timeline-thinking-complete",
            entryType: "output",
            content: FINAL_TEXT,
            metadata: {
              sessionId: SESSION_ID,
              messageId: ASSISTANT_MESSAGE_ID,
              phase: "output",
              providerEvent: "provided_agent.output.final",
            },
            sequence: 2,
            createdAt: now,
          },
        ]
      : [
          {
            id: "timeline-thinking-started",
            entryType: "thought",
            content: "Thinking",
            metadata: {
              sessionId: SESSION_ID,
              messageId: ASSISTANT_MESSAGE_ID,
              phase: "thought",
              providerEvent: "provided_agent.thinking.started",
            },
            sequence: 1,
            createdAt: now,
          },
        ],
    cursor: null,
    hasMore: false,
  });
  const task = () => ({
    id: TASK_ID,
    name: "Chat turn",
    taskType: "agent",
    status: turnComplete ? "succeeded" : "running",
    workspaceId: WORKSPACE_ID,
    workspaceName: WORKSPACE_SLUG,
    specContext: null,
    dependsOn: [],
    branchName: "main",
    filesChanged: [],
    costUsd: 0,
    sessionId: SESSION_ID,
    chatBranchMode: "workspace_head",
    outputMode: "workspace_head",
    chatSessionId: SESSION_ID,
    latestAssistantState: turnComplete ? "complete" : "pending",
    draftSummary: null,
    createdAt: now,
    updatedAt: now,
    assignedAt: now,
    completedAt: turnComplete ? now : null,
  });

  await page.addInitScript((token) => {
    window.localStorage.setItem("g5_auth_token", token);
  }, jwt);

  await page.route("**/api/v1/**", async (route: Route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;

    if (path === "/api/v1/auth/refresh") {
      await route.fulfill(apiJson({ jwt }));
      return;
    }
    if (path === "/api/v1/auth/ws-ticket") {
      await route.fulfill(apiJson({ ticket: "e2e-ws-ticket" }));
      return;
    }
    if (path === "/api/v1/instance/info") {
      await route.fulfill(
        apiJson({
          version: "0.1.0-e2e",
          features: ["crdt", "chat"],
          authProviders: [],
          maxUploadSizeMb: 50,
          selfHosted: false,
          devLoginEnabled: true,
        }),
      );
      return;
    }
    if (path === "/api/v1/users/me") {
      await route.fulfill(
        apiJson({
          id: "e2e-user",
          email: "e2e-chat-recovery@g5.dev",
          displayName: "E2E Chat Recovery",
          avatarUrl: null,
          oauthProvider: null,
          activeOrgId: ORG_ID,
          orgs: [
            {
              orgId: ORG_ID,
              orgName: "G5 Labs",
              orgSlug: ORG_SLUG,
              role: "admin",
            },
          ],
          scmConnections: [],
          createdAt: now,
        }),
      );
      return;
    }
    if (path === "/api/v1/resolve") {
      await route.fulfill(
        apiJson({ orgId: ORG_ID, workspaceId: WORKSPACE_ID }),
      );
      return;
    }
    if (path === `/api/v1/orgs/${ORG_ID}/workspaces`) {
      await route.fulfill(
        apiJson([
          {
            id: WORKSPACE_ID,
            name: WORKSPACE_SLUG,
            description: "Chat recovery E2E workspace",
            isBase: false,
            readOnly: false,
            ownerUserId: "e2e-user",
            clonedFromId: null,
            branches: [],
            tasksCompleted: turnComplete ? 1 : 0,
            totalTasks: 1,
            runningTime: null,
            cost: 0,
            concurrentTasks: turnComplete ? 0 : 1,
            lastUpdated: now,
            createdAt: now,
          },
        ]),
      );
      return;
    }
    if (path === `/api/v1/workspaces/${WORKSPACE_ID}`) {
      await route.fulfill(
        apiJson({
          id: WORKSPACE_ID,
          orgId: ORG_ID,
          name: WORKSPACE_SLUG,
          isBase: false,
          readOnly: false,
          ownerUserId: "e2e-user",
          description: "Chat recovery E2E workspace",
          clonedFromId: null,
          docsRepoId: null,
          docsBranchName: null,
          docsBranchId: null,
          createdBy: "e2e-user",
          createdAt: now,
          updatedAt: now,
          branches: [
            {
              workspaceId: WORKSPACE_ID,
              repoId: "44444444-4444-4444-8444-444444444444",
              repoName: "code",
              branchName: "main",
              trackingMode: "tracking",
              headCommit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              divergence: 0,
              behindBase: 0,
              hasDraft: false,
              dirtyFileCount: 0,
            },
          ],
        }),
      );
      return;
    }
    if (path === `/api/v1/workspaces/${WORKSPACE_ID}/vm`) {
      await route.fulfill(
        apiJson({
          orgId: ORG_ID,
          workspaceId: WORKSPACE_ID,
          ownerUserId: "e2e-user",
          status: "running",
          hostId: "host-e2e",
          vmId: "vm-e2e",
          epoch: 1,
        }),
      );
      return;
    }
    if (path === `/api/v1/workspaces/${WORKSPACE_ID}/sessions`) {
      counts.sessions += 1;
      await route.fulfill(apiJson([session()]));
      return;
    }
    if (path === `/api/v1/sessions/${SESSION_ID}`) {
      counts.session += 1;
      await route.fulfill(apiJson(session()));
      return;
    }
    if (path === `/api/v1/sessions/${SESSION_ID}/messages`) {
      counts.messages += 1;
      await route.fulfill(apiJson(messages()));
      return;
    }
    if (path === `/api/v1/sessions/${SESSION_ID}/timeline`) {
      counts.timeline += 1;
      await route.fulfill(apiJson(timeline()));
      return;
    }
    if (path === `/api/v1/workspaces/${WORKSPACE_ID}/tasks`) {
      counts.tasks += 1;
      await route.fulfill(
        apiJson({ tasks: [task()], cursor: null, hasMore: false }),
      );
      return;
    }
    if (path === `/api/v1/tasks/${TASK_ID}`) {
      await route.fulfill(apiJson(task()));
      return;
    }
    if (
      path ===
      `/api/v1/workspaces/${WORKSPACE_ID}/repos/44444444-4444-4444-8444-444444444444/files`
    ) {
      await route.fulfill(apiJson([]));
      return;
    }
    if (path === `/api/v1/workspaces/${WORKSPACE_ID}/changes`) {
      await route.fulfill(
        apiJson({
          workspace_id: WORKSPACE_ID,
          source: "live",
          attached: true,
          members: [],
        }),
      );
      return;
    }

    await route.fulfill(apiJson([]));
  });

  return {
    completeTurn: () => {
      turnComplete = true;
    },
    counts,
  };
}

test.describe("Chat timeline recovery", () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test("clears stale Thinking after missed chat.done without page refresh", async ({
    page,
  }) => {
    // Allow background polling and UI reconciliation under CI load.
    test.setTimeout(60_000);
    const fixture = await installChatRecoveryRoutes(page);

    await page.goto(`/${ORG_SLUG}/${WORKSPACE_SLUG}?session=${SESSION_ID}`, {
      waitUntil: "domcontentloaded",
    });

    const rightPanel = page.getByTestId("right-panel");
    await expect(rightPanel).toBeVisible({ timeout: 15_000 });
    await expect(rightPanel).toContainText(
      "Please verify the timeline recovery path.",
      { timeout: 15_000 },
    );
    await expect(page.getByTestId("provider-thinking-indicator")).toBeVisible({
      timeout: 15_000,
    });

    const before = { ...fixture.counts };
    fixture.completeTurn();

    await expect(rightPanel).toContainText(FINAL_TEXT, { timeout: 15_000 });

    // Task polling recovers a missed completion event.
    await expect
      .poll(() => fixture.counts.tasks, { timeout: 20_000 })
      .toBeGreaterThan(before.tasks);

    await expect(page.getByTestId("provider-thinking-indicator")).toBeHidden({
      timeout: 15_000,
    });
    await expect(page.getByTestId("streaming-indicator")).toHaveCount(0);

    await expect
      .poll(() => fixture.counts.messages, { timeout: 15_000 })
      .toBeGreaterThan(before.messages);
    await expect
      .poll(() => fixture.counts.timeline, { timeout: 15_000 })
      .toBeGreaterThan(before.timeline);
    await expect
      .poll(() => fixture.counts.session, { timeout: 15_000 })
      .toBeGreaterThan(before.session);
    await expect
      .poll(() => fixture.counts.sessions, { timeout: 15_000 })
      .toBeGreaterThan(before.sessions);
  });
});

test.describe("Conversation activity presentation", () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  for (const provider of ["claude_code", "codex"] as const) {
    test(`${provider}: compact journal activity, details, reload, and unchanged composer draft`, async ({
      page,
    }) => {
      const fixture = await installChatRecoveryRoutes(page, provider);
      fixture.completeTurn();
      let seq = 0;
      const entry = (kind: string, payload: Record<string, unknown>) => {
        seq += 1;
        return {
          id: `activity-${seq}`,
          entryType:
            kind === "message.final"
              ? "agent_message"
              : kind === "message.delta"
                ? "output"
                : kind === "tool.started"
                  ? "tool_call"
                  : "tool_result",
          content: String(payload.content ?? payload.text_delta ?? kind),
          createdAt: new Date(Date.UTC(2026, 8, 25, 12, 0, seq)).toISOString(),
          metadata: {
            schema_version: "g5.task-event.v1",
            kind,
            payload,
            taskId: TASK_ID,
            attempt: 1,
            sessionId: SESSION_ID,
            messageId: ASSISTANT_MESSAGE_ID,
          },
        };
      };
      const rows = [
        entry("message.delta", {
          block_id: "native-1:0",
          text_delta: "Checking",
        }),
        entry("message.final", {
          block_id: "native-1:0",
          content: "Checking the documentation history.",
        }),
      ];
      for (let i = 1; i <= 6; i += 1) {
        rows.push(
          entry("tool.started", {
            call_id: `call-${i}`,
            name: provider === "claude_code" ? "Bash" : "command_execution",
            input: {
              command: `cat docs/report-${i}.md`,
              ...(provider === "claude_code"
                ? { description: `Inspect report ${i}` }
                : {}),
            },
          }),
        );
        rows.push(
          entry("tool.completed", {
            call_id: `call-${i}`,
            name: provider === "claude_code" ? "Bash" : "command_execution",
            status: i === 6 ? "error" : "ok",
            output: i === 6 ? "Permission denied" : `Report ${i} contents`,
          }),
        );
      }
      rows.push(
        entry("message.final", {
          block_id: "native-2:0",
          content: "Five reports are available; one needs access.",
        }),
      );
      rows.push(
        entry("message.final", {
          block_id: "native-2:0",
          content: "Five reports are available; one needs access.",
        }),
      );
      await page.route(`**/api/v1/sessions/${SESSION_ID}/timeline*`, (route) =>
        route.fulfill(apiJson({ entries: rows, cursor: null, hasMore: false })),
      );
      await page.goto(`/${ORG_SLUG}/${WORKSPACE_SLUG}?session=${SESSION_ID}`, {
        waitUntil: "domcontentloaded",
      });
      const panel = page.getByTestId("right-panel");
      await expect(
        panel.getByText("Checking the documentation history.", { exact: true }),
      ).toBeVisible();
      await expect(
        panel.getByText("Five reports are available; one needs access.", {
          exact: true,
        }),
      ).toBeVisible();
      const group = panel.getByTestId("chat-activity-group");
      const summary = group.locator("details > summary").first();
      await expect(summary).toContainText("Ran 6 commands");
      await expect(summary).not.toContainText("failed");
      await expect(group.locator("details").first()).not.toHaveAttribute(
        "open",
      );
      const input = panel.getByTestId("chat-input");
      await input.fill("Keep this draft while I inspect the output");
      const inputClass = await input.getAttribute("class");
      await summary.click();
      const tools = group.getByTestId("assistant-tool-block");
      await expect(tools).toHaveCount(6);
      await expect(tools.first().locator("summary")).toHaveText(
        provider === "claude_code"
          ? "Inspect report 1"
          : "cat docs/report-1.md",
      );
      await tools.first().locator("summary").click();
      await expect(
        tools.first().getByText("Report 1 contents", { exact: true }),
      ).toBeVisible();
      await tools.last().locator("summary").click();
      await expect(
        tools.last().getByText("Permission denied", { exact: true }),
      ).toBeVisible();
      await expect(input).toHaveValue(
        "Keep this draft while I inspect the output",
      );
      expect(await input.getAttribute("class")).toBe(inputClass);
      await expect(group.locator("details").first()).toHaveAttribute("open");
      await test.info().attach(`${provider}-timeline`, {
        body: await panel.screenshot({
          path: test.info().outputPath(`${provider}-timeline.png`),
        }),
        contentType: "image/png",
      });
      await page.reload({ waitUntil: "domcontentloaded" });
      await expect(
        panel.getByText("Checking the documentation history.", { exact: true }),
      ).toBeVisible();
      await expect(summary).toContainText("Ran 6 commands");
      await expect(summary).not.toContainText("failed");
      await summary.click();
      await expect(group.getByTestId("assistant-tool-block")).toHaveCount(6);
    });
  }
});
