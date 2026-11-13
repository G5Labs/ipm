// Chat activity, personal inspection, header rename and per-chat prompt recall
// (issue #823; chat g5.chat.activity, g5.chat.activity.order,
// g5.chat.inspection, g5.chat.ui.header-title, g5.chat.ui.picker,
// g5.chat.ui.prompt-history). Runs against a real stack whose worker uses the
// deterministic fake Claude Code and Codex CLIs (G5_CLAUDE_CODE_FAKE=1,
// G5_CODEX_FAKE=1): a held turn waits for a gate file the spec writes through
// the files API, so a completion can land while another chat is shown.

import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  expectSelectedAgentProvider,
  openChatComposer,
  openWorkspacePath,
  releaseChatTurn,
  runWorkspaceVmAction,
  seedWorkspaceClaudeCredential,
  selectAgentProviderViaSettings,
  showWorkspaceTabs,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
  type ChatTurnIdentity,
  type WorkspaceClone,
} from "../helpers";

const hasLlm =
  !!process.env.ANTHROPIC_API_KEY ||
  !!process.env.OPENROUTER_API_KEY ||
  !!process.env.OPENAI_API_KEY ||
  !!process.env.LLM_ENABLED;

type Provider = "claude_code" | "codex";
const HELLO_REPLY: Record<Provider, string> = {
  claude_code: "hello from claude code",
  codex: "hello from codex",
};

interface Activity {
  activeTurnStartedAt: string | null;
  lastFinishedAt: string | null;
  latestResult: { taskId: string; attempt: number | null } | null;
  unseenResult: boolean;
}

async function sessionActivity(page: Page, sessionId: string) {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/sessions/${sessionId}`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  return (response.body?.data ?? response.body).activity as Activity;
}

async function listedOrder(page: Page, workspaceId: string) {
  const response = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}/sessions`,
  );
  expect(response.status, JSON.stringify(response.body)).toBe(200);
  const sessions = (response.body?.data ?? response.body) as Array<{
    id: string;
  }>;
  return sessions.map((session) => session.id);
}

function sendResponse(page: Page) {
  return page.waitForResponse((response) => {
    if (response.request().method() !== "POST") return false;
    const path = new URL(response.url()).pathname;
    return (
      /\/api\/v1\/sessions\/[^/]+\/messages$/.test(path) ||
      /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(path)
    );
  });
}

async function send(page: Page, text: string): Promise<ChatTurnIdentity> {
  const input = page.getByTestId("chat-input").first();
  await expect(input).toBeEnabled({ timeout: 30_000 });
  await input.fill(text);
  const response = sendResponse(page);
  await input.press("Enter");
  return captureChatTurn(page, await response);
}

/** The chat picker's trigger: the header title, or with no chat shown the
 * center view's "View all" / the panel's "History". */
function pickerTrigger(page: Page) {
  return page
    .getByTestId("chat-active-history")
    .or(page.getByTestId("chat-center-history"))
    .or(page.getByTestId("chat-history-header"))
    .first();
}

async function openPicker(page: Page) {
  const search = page.getByPlaceholder("Search sessions…");
  if (!(await search.isVisible().catch(() => false))) {
    await pickerTrigger(page).click();
  }
  await expect(search).toBeVisible();
}

async function closePicker(page: Page) {
  if (await page.getByPlaceholder("Search sessions…").isVisible()) {
    await pickerTrigger(page).click();
  }
}

function pickerRow(page: Page, text: string) {
  return page.getByTestId("chat-session-select").filter({ hasText: text });
}

async function expectRowState(page: Page, text: string, state: string) {
  await openPicker(page);
  await expect(
    pickerRow(page, text).getByTestId("chat-activity-indicator"),
  ).toHaveAttribute("data-state", state, { timeout: 60_000 });
}

async function selectChat(page: Page, text: string) {
  await openPicker(page);
  await pickerRow(page, text).click();
}

async function expectRecall(page: Page, expected: string) {
  const input = page.getByTestId("chat-input").first();
  await expect(input).toBeEnabled({ timeout: 30_000 });
  await input.fill("");
  await input.press("ArrowUp");
  await expect(input).toHaveValue(expected);
  await input.press("ArrowDown");
  await expect(input).toHaveValue("");
}

for (const provider of ["claude_code", "codex"] as const) {
  test.describe(`Chat activity and per-chat recall (${provider})`, () => {
    test.describe.configure({ timeout: 600_000 });
    let workspace: WorkspaceClone | undefined;
    const turns: ChatTurnIdentity[] = [];

    test.beforeEach(async ({ page }, testInfo) => {
      test.skip(!hasLlm, "Requires the fake providers (set LLM_ENABLED)");
      await showWorkspaceTabs(page);
      workspace = await createWorkspaceClone(page, {
        namePrefix: `chat-activity-${provider}-${testInfo.retry}`,
      });
      // The fake ignores credentials, but launch resolution requires one.
      if (provider === "claude_code") {
        await seedWorkspaceClaudeCredential(page, workspace.id);
      }
      await runWorkspaceVmAction(page, workspace.id, "resume");
      await waitForWorkspaceVmStatus(page, workspace.id, "running");
      await openWorkspacePath(page, workspace.path);
    });

    test.afterEach(async ({ page }) => {
      for (const turn of turns.splice(0)) {
        await releaseChatTurn(page, turn).catch(() => {});
      }
      if (workspace)
        await deleteWorkspaceById(page, workspace.id).catch(() => {});
      workspace = undefined;
    });

    test("background completion, inspection, ordering, header rename and recall", async ({
      page,
      context,
    }) => {
      const ws = workspace!;
      const token = randomUUID().slice(0, 8);
      const renamedA = "Renamed first chat";
      const rowB = `Hold ${token}`;
      const detail = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${ws.id}`,
      );
      const docsRepoId = String((detail.body?.data ?? detail.body)?.docsRepoId);
      const changes = await waitForLiveWorkspaceChanges(page, ws.id);
      const code = changes.members.find(
        (member) => member.repo_id !== docsRepoId,
      );
      const docs = changes.members.find(
        (member) => member.repo_id === docsRepoId,
      );
      expect(code?.root_path).toBeTruthy();
      expect(docs?.root_path).toBeTruthy();

      await selectAgentProviderViaSettings(page, provider);
      await expectSelectedAgentProvider(page, provider);

      // Chat A: a first send creates the chat; seeing its reply inspects it.
      const promptA = "hello";
      const turnA = await send(page, promptA);
      turns.push(turnA);
      await expect(page.getByTestId("chat-messages")).toContainText(
        HELLO_REPLY[provider],
        { timeout: 120_000 },
      );
      await expect
        .poll(
          async () =>
            (await sessionActivity(page, turnA.sessionId)).unseenResult,
          {
            timeout: 30_000,
          },
        )
        .toBe(false);
      await expectRowState(page, promptA, "idle");
      await closePicker(page);

      // Chat B: a held turn, active while chat A is shown.
      await page.getByTestId("new-session-header").click();
      const gatePath = `e2e-823-gate-${token}.txt`;
      const writes = Buffer.from(
        JSON.stringify([
          {
            path: `${code!.root_path}/e2e-823-${token}.txt`,
            contentBase64: Buffer.from(`823 ${token}\n`).toString("base64"),
          },
          {
            path: `${docs!.root_path}/docs/${ws.id}/e2e-823-${token}.md`,
            contentBase64: Buffer.from(`# 823 ${token}\n`).toString("base64"),
          },
        ]),
      ).toString("base64");
      const promptB = `Hold ${token} until released. G5_FS_WRITE_SET_E2E:${writes} G5_WAIT_FOR_FILE_E2E:${code!.root_path}/${gatePath}`;
      const turnB = await send(page, promptB);
      turns.push(turnB);
      expect(turnB.sessionId).not.toBe(turnA.sessionId);
      await expectRowState(page, rowB, "active");
      expect((await listedOrder(page, ws.id))[0]).toBe(turnB.sessionId);

      // Show chat A, then let B finish in the background.
      await selectChat(page, promptA);
      await expect(page.getByTestId("chat-messages")).toContainText(
        HELLO_REPLY[provider],
      );
      // A second tab of the same person shows chat A too.
      const second = await context.newPage();
      await openWorkspacePath(second, ws.path);
      await openChatComposer(second);
      await selectChat(second, promptA);

      const release = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${ws.id}/files/${gatePath}?repoId=${code!.repo_id}`,
        "PUT",
        { content: "release\n", encoding: "utf-8", expectedHash: null },
      );
      expect([200, 201], JSON.stringify(release.body)).toContain(
        release.status,
      );

      // B finished while A was shown: a solid circle, first by last finish.
      await expectRowState(page, rowB, "unseen");
      await expectRowState(second, rowB, "unseen");
      expect(
        (await sessionActivity(page, turnB.sessionId)).activeTurnStartedAt,
      ).toBeNull();
      expect(await listedOrder(page, ws.id)).toEqual([
        turnB.sessionId,
        turnA.sessionId,
      ]);

      // Showing B in an unfocused page does not acknowledge it.
      await page.evaluate(() => {
        Object.defineProperty(document, "hasFocus", {
          configurable: true,
          value: () => false,
        });
        window.dispatchEvent(new Event("blur"));
      });
      await selectChat(page, rowB);
      await expect(page.getByTestId("chat-messages")).toContainText(
        "Fake workspace write completed",
        { timeout: 60_000 },
      );
      await page.waitForTimeout(3_000);
      expect((await sessionActivity(page, turnB.sessionId)).unseenResult).toBe(
        true,
      );

      // Focus returns: the displayed result is acknowledged, and the other
      // tab converges.
      await page.evaluate(() => {
        Object.defineProperty(document, "hasFocus", {
          configurable: true,
          value: () => true,
        });
        window.dispatchEvent(new Event("focus"));
      });
      await expect
        .poll(
          async () =>
            (await sessionActivity(page, turnB.sessionId)).unseenResult,
          {
            timeout: 30_000,
          },
        )
        .toBe(false);
      await expectRowState(second, rowB, "idle");
      await second.close();
      expect(await listedOrder(page, ws.id)).toEqual([
        turnB.sessionId,
        turnA.sessionId,
      ]);

      // Renaming A from the header keeps the order.
      await closePicker(page);
      await selectChat(page, promptA);
      await page.getByTestId("chat-header-rename").click();
      const title = page.getByTestId("chat-header-title-input");
      await title.fill(renamedA);
      await title.press("Enter");
      await expect(page.getByTestId("chat-active-history")).toContainText(
        renamedA,
      );
      expect(await listedOrder(page, ws.id)).toEqual([
        turnB.sessionId,
        turnA.sessionId,
      ]);
      await openPicker(page);
      await expect(page.getByTestId(/^rename-session-/)).toHaveCount(0);
      await closePicker(page);

      // Up/Down recall only the shown chat's prompts, across a reload.
      await expectRecall(page, promptA);
      await selectChat(page, rowB);
      await expectRecall(page, promptB);
      await page.reload({ waitUntil: "domcontentloaded" });
      await openChatComposer(page);
      await selectChat(page, rowB);
      await expectRecall(page, promptB);
      await selectChat(page, renamedA);
      await expectRecall(page, promptA);

      // History and recall survive a VM stop and start.
      if (provider === "claude_code") {
        await runWorkspaceVmAction(page, ws.id, "stop");
        await waitForWorkspaceVmStatus(page, ws.id, "stopped");
        await runWorkspaceVmAction(page, ws.id, "resume");
        await waitForWorkspaceVmStatus(page, ws.id, "running");
        await page.reload({ waitUntil: "domcontentloaded" });
        await openChatComposer(page);
        await selectChat(page, renamedA);
        await expect(page.getByTestId("chat-messages")).toContainText(
          HELLO_REPLY[provider],
          { timeout: 60_000 },
        );
        await expectRecall(page, promptA);
        await selectChat(page, rowB);
        await expectRecall(page, promptB);
      }
    });
  });
}
