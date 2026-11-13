// =============================================================================
// E2E - Chat Timeline History Pagination (issues #2333 and #847)
// =============================================================================
// Verifies that a chat session with MORE timeline entries than the server's
// newest window (default 200) never presents a turn as complete without its
// tools (g5.chat.history-window): while only the newest window is loaded, the
// oldest turn shows a not-loaded marker instead of its saved reply, the marker
// loads the older backward-cursor page on its own because it is in view, and
// the turn then renders its tool evidence and narration together. HTTP is
// mocked at the browser boundary because the regression is client
// rendering/paging completeness, not provider execution or the server cursor
// contract.
// =============================================================================

import { expect, test, type Page, type Route } from "@playwright/test";

const ORG_SLUG = "org-g5labs";
const ORG_ID = "11111111-1111-4111-8111-111111111111";
const WORKSPACE_SLUG = "timeline-history";
const WORKSPACE_ID = "22222222-2222-4222-8222-222222222222";
const SESSION_ID = "44444444-4444-4444-8444-444444444444";
const OLD_USER_MESSAGE_ID = "55555555-5555-4555-8555-555555555555";
const OLD_ASSISTANT_MESSAGE_ID = "66666666-6666-4666-8666-666666666666";
const NEW_USER_MESSAGE_ID = "77777777-7777-4777-8777-777777777777";
const NEW_ASSISTANT_MESSAGE_ID = "88888888-8888-4888-8888-888888888888";
const TASK_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

// Distinctive marker only present on the OLD turn's early Bash tool call, which
// lives beyond the newest window. The newest window carries NO Bash tool, so
// the "Bash" chip is a unique signal that the older page was recovered.
const OLD_TURN_NARRATION = "Investigating the mermaid source leak.";
const NEW_TURN_NARRATION = "Done — the mermaid source leak is fixed.";

interface TimelineRow {
  id: string;
  entryType: string;
  content: string;
  metadata: Record<string, unknown>;
  sequence: number;
  createdAt: string;
}

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
      email: "e2e-chat-history@g5.dev",
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

// Build a chronologically ascending timeline that exceeds the newest window.
// The first turn (OLD_ASSISTANT_MESSAGE_ID) owns the early Bash tool call and a
// browser screenshot; ~210 later filler entries (thought/output on the recent
// turn, carrying no Bash tool) push the first turn's entries beyond the newest
// 200 so only backward pagination can recover them.
function buildTimelineRows(): TimelineRow[] {
  const rows: TimelineRow[] = [];
  const base = Date.parse("2026-08-31T16:53:00.000Z");
  const at = (offsetSeconds: number) =>
    new Date(base + offsetSeconds * 1000).toISOString();

  // --- Old turn (oldest entries) ---
  rows.push({
    id: "old-bash-started",
    entryType: "tool_call",
    content: "Claude Code used Bash",
    metadata: {
      source: "provided_agent",
      provider: "claude_code",
      providerEvent: "provided_agent.tool.started",
      phase: "tool_call",
      sessionId: SESSION_ID,
      messageId: OLD_ASSISTANT_MESSAGE_ID,
      toolName: "bash",
      args: { command: "rg -n 'mermaid' generation" },
    },
    sequence: 1,
    createdAt: at(1),
  });
  rows.push({
    id: "old-bash-completed",
    entryType: "tool_result",
    content: "match found in mermaid config",
    metadata: {
      source: "provided_agent",
      provider: "claude_code",
      providerEvent: "provided_agent.tool.completed",
      phase: "tool_result",
      sessionId: SESSION_ID,
      messageId: OLD_ASSISTANT_MESSAGE_ID,
      toolName: "bash",
      toolCallId: "old-tool-bash",
    },
    sequence: 2,
    createdAt: at(2),
  });
  rows.push({
    id: "old-screenshot",
    entryType: "tool_call",
    content: "Claude Code captured a browser screenshot",
    metadata: {
      source: "provided_agent",
      provider: "claude_code",
      providerEvent: "provided_agent.tool.started",
      phase: "tool_call",
      sessionId: SESSION_ID,
      messageId: OLD_ASSISTANT_MESSAGE_ID,
      toolName: "browser_take_screenshot",
      args: { reason: "RED reproduction evidence" },
    },
    sequence: 3,
    createdAt: at(3),
  });
  rows.push({
    id: "old-output-final",
    entryType: "output",
    content: OLD_TURN_NARRATION,
    metadata: {
      source: "provided_agent",
      provider: "claude_code",
      providerEvent: "provided_agent.output.final",
      phase: "output",
      sessionId: SESSION_ID,
      messageId: OLD_ASSISTANT_MESSAGE_ID,
      blockId: "old-block",
    },
    sequence: 4,
    createdAt: at(4),
  });

  // --- Recent turn filler: ~210 entries newer than the old turn, none of which
  // is a Bash tool, so the newest 200 window contains no "Bash" chip. ---
  for (let i = 0; i < 210; i += 1) {
    rows.push({
      id: `filler-${i}`,
      entryType: i % 2 === 0 ? "thought" : "output",
      content:
        i % 2 === 0
          ? `Background wake ${i}: re-checking status`
          : `Background wake ${i}: nothing to change`,
      metadata: {
        source: "provided_agent",
        provider: "claude_code",
        providerEvent:
          i % 2 === 0
            ? "provided_agent.thought"
            : "provided_agent.output.delta",
        phase: i % 2 === 0 ? "thought" : "output",
        sessionId: SESSION_ID,
        messageId: NEW_ASSISTANT_MESSAGE_ID,
        blockId: `filler-block-${i}`,
      },
      sequence: 100 + i,
      createdAt: at(60 + i),
    });
  }
  rows.push({
    id: "new-output-final",
    entryType: "output",
    content: NEW_TURN_NARRATION,
    metadata: {
      source: "provided_agent",
      provider: "claude_code",
      providerEvent: "provided_agent.output.final",
      phase: "output",
      sessionId: SESSION_ID,
      messageId: NEW_ASSISTANT_MESSAGE_ID,
      blockId: "new-block",
    },
    sequence: 400,
    createdAt: at(300),
  });

  return rows;
}

const WINDOW_LIMIT = 200;

async function installTimelineHistoryRoutes(page: Page): Promise<{
  counts: { timelineNewest: number; timelineOlder: number };
  releaseOlder: () => void;
}> {
  const jwt = createJwt();
  const now = new Date("2026-08-31T16:53:00.000Z").toISOString();
  const allRows = buildTimelineRows();
  const counts = { timelineNewest: 0, timelineOlder: 0 };
  // Older pages wait until the test releases them, so the not-loaded state is
  // observable before the automatic load completes.
  let releaseOlder: () => void = () => undefined;
  const olderReleased = new Promise<void>((resolve) => {
    releaseOlder = resolve;
  });

  const session = () => ({
    id: SESSION_ID,
    workspaceId: WORKSPACE_ID,
    sessionType: "chat",
    taskId: null,
    scopeType: "workspace",
    scopeRef: WORKSPACE_ID,
    model: null,
    agentProvider: "claude_code",
    providerState: null,
    title: "Mermaid source leak investigation",
    summary: null,
    tags: [],
    status: "active",
    messageCount: 4,
    totalInputTokens: 0,
    totalOutputTokens: 24,
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
        id: OLD_USER_MESSAGE_ID,
        sessionId: SESSION_ID,
        role: "user",
        content: "Investigate the mermaid source leak.",
        artifacts: [],
        citations: [],
        inputTokens: null,
        outputTokens: null,
        status: "complete",
        messageIndex: 0,
        createdAt: now,
      },
      {
        id: OLD_ASSISTANT_MESSAGE_ID,
        sessionId: SESSION_ID,
        role: "assistant",
        content: OLD_TURN_NARRATION,
        artifacts: [],
        citations: [],
        inputTokens: null,
        outputTokens: 12,
        status: "complete",
        messageIndex: 1,
        createdAt: now,
      },
      {
        id: NEW_USER_MESSAGE_ID,
        sessionId: SESSION_ID,
        role: "user",
        content: "Great, confirm the fix.",
        artifacts: [],
        citations: [],
        inputTokens: null,
        outputTokens: null,
        status: "complete",
        messageIndex: 2,
        createdAt: now,
      },
      {
        id: NEW_ASSISTANT_MESSAGE_ID,
        sessionId: SESSION_ID,
        role: "assistant",
        content: NEW_TURN_NARRATION,
        artifacts: [],
        citations: [],
        inputTokens: null,
        outputTokens: 12,
        status: "complete",
        messageIndex: 3,
        createdAt: now,
      },
    ],
    cursor: null,
    hasMore: false,
  });

  // Newest-first backward-cursor paging, mirroring
  // ChatService::get_session_timeline: no cursor → newest window (ascending),
  // cursor → the older remainder.
  const timeline = (cursor: string | null) => {
    if (cursor == null) {
      counts.timelineNewest += 1;
      const windowRows = allRows.slice(-WINDOW_LIMIT);
      const hasMore = allRows.length > WINDOW_LIMIT;
      const oldest = windowRows[0];
      return {
        entries: windowRows,
        cursor: hasMore ? `${oldest.createdAt}|${oldest.id}` : null,
        hasMore,
      };
    }
    counts.timelineOlder += 1;
    const olderRows = allRows.slice(
      0,
      Math.max(0, allRows.length - WINDOW_LIMIT),
    );
    return { entries: olderRows, cursor: null, hasMore: false };
  };

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
          email: "e2e-chat-history@g5.dev",
          displayName: "E2E Chat History",
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
            description: "Chat history E2E workspace",
            isBase: false,
            readOnly: false,
            ownerUserId: "e2e-user",
            clonedFromId: null,
            branches: [],
            tasksCompleted: 1,
            totalTasks: 1,
            runningTime: null,
            cost: 0,
            concurrentTasks: 0,
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
          description: "Chat history E2E workspace",
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
      await route.fulfill(apiJson([session()]));
      return;
    }
    if (path === `/api/v1/sessions/${SESSION_ID}`) {
      await route.fulfill(apiJson(session()));
      return;
    }
    if (path === `/api/v1/sessions/${SESSION_ID}/messages`) {
      await route.fulfill(apiJson(messages()));
      return;
    }
    if (path === `/api/v1/sessions/${SESSION_ID}/timeline`) {
      const cursor = url.searchParams.get("cursor");
      const body = timeline(cursor);
      if (cursor != null) await olderReleased;
      await route.fulfill(apiJson(body));
      return;
    }
    if (path === `/api/v1/workspaces/${WORKSPACE_ID}/tasks`) {
      await route.fulfill(apiJson({ tasks: [], cursor: null, hasMore: false }));
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

  return { counts, releaseOlder };
}

test.describe("Chat timeline history pagination", () => {
  test.use({ storageState: { cookies: [], origins: [] } });

  test("marks the unloaded oldest turn and loads its tool evidence as it comes into view", async ({
    page,
  }, testInfo) => {
    test.setTimeout(60_000);
    const fixture = await installTimelineHistoryRoutes(page);

    await page.goto(`/${ORG_SLUG}/${WORKSPACE_SLUG}?session=${SESSION_ID}`, {
      waitUntil: "domcontentloaded",
    });

    const rightPanel = page.getByTestId("right-panel");
    await expect(rightPanel).toBeVisible({ timeout: 15_000 });
    const transcript = page.getByTestId("chat-messages");
    const oldTurn = transcript.locator(
      `[data-message-id="${OLD_ASSISTANT_MESSAGE_ID}"]`,
    );
    const newTurn = transcript.locator(
      `[data-message-id="${NEW_ASSISTANT_MESSAGE_ID}"]`,
    );
    await expect(newTurn).toContainText("Background wake 209", {
      timeout: 15_000,
    });

    // Not-loaded state: the oldest turn shows its marker, not its saved reply,
    // and no Bash tool is rendered. Its marker is far above the reading
    // position, so nothing older is requested yet.
    const oldMarker = oldTurn.getByTestId("chat-earlier-activity");
    await expect(oldMarker).toHaveAttribute("data-state", "idle");
    await expect(oldTurn).not.toContainText(OLD_TURN_NARRATION);
    await expect(transcript.getByText("Bash", { exact: true })).toHaveCount(0);
    await page.waitForTimeout(500);
    expect(fixture.counts.timelineOlder).toBe(0);

    // Scrolling up to it requests the older page (held by the route until
    // released).
    await oldMarker.scrollIntoViewIfNeeded();
    await expect.poll(() => fixture.counts.timelineOlder).toBe(1);
    await expect(oldTurn.getByTestId("chat-earlier-activity")).toHaveAttribute(
      "data-state",
      "loading",
    );

    fixture.releaseOlder();

    // Loaded: the oldest turn renders its narration with its tool evidence,
    // and no marker or load-earlier affordance remains.
    await expect(oldTurn).toContainText(OLD_TURN_NARRATION, {
      timeout: 15_000,
    });
    await expect(transcript.getByTestId("chat-earlier-activity")).toHaveCount(
      0,
    );
    await expect(
      page.getByTestId("chat-load-earlier-history-button"),
    ).toHaveCount(0);
    const recoveredActivity = oldTurn
      .getByTestId("chat-activity-group")
      .first();
    await recoveredActivity.locator("summary").first().click();
    const recoveredTool = oldTurn.getByTestId("assistant-tool-block").first();
    await expect(recoveredTool.locator("summary")).toBeVisible({
      timeout: 15_000,
    });
    await recoveredTool.locator("summary").click();
    await expect(
      recoveredTool.getByText("Bash", { exact: true }),
    ).toBeVisible();
    expect(fixture.counts.timelineOlder).toBe(1);

    // Loaded older history survives later refreshes without re-reading it.
    await page.waitForTimeout(1_000);
    await expect(
      transcript.getByText("Bash", { exact: true }).first(),
    ).toBeVisible();
    expect(fixture.counts.timelineOlder).toBe(1);

    // Evidence artifact: recovered history with the oldest turn's tool call.
    await transcript
      .getByText("Bash", { exact: true })
      .first()
      .scrollIntoViewIfNeeded();
    const shot = await page.screenshot({ fullPage: false });
    await testInfo.attach("recovered-chat-history", {
      body: shot,
      contentType: "image/png",
    });
  });
});
