import { randomUUID } from "node:crypto";
import { expect, test, type Page } from "@playwright/test";
import {
  authedApi,
  authedApiViaRequest,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  docIdForPath,
  expectAssistantReply,
  expectSelectedAgentProvider,
  openChatComposer,
  openWorkspacePath,
  releaseChatTurn,
  runWorkspaceVmAction,
  seedWorkspaceClaudeCredential,
  seedWorkspaceCodexCredential,
  selectAgentProviderViaSettings,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceVmStatus,
  type ChatTurnIdentity,
  type WorkspaceClone,
} from "../helpers";

// =============================================================================
// Agents cite workspace files as g5://file links (issue #875)
// =============================================================================
// A chat reply's `g5://file` links open the cited file in a workspace tab: a
// Project Doc in the docs repository, a code file in the code repository and,
// with `#L<n>`, at that line in Source (g5.chat.file-citations,
// g5.documents.file-links, g5.documents.file-links.line). The fake providers
// echo the prompt, so the reply carries exactly the links the prompt names
// through the real dispatch, adapters and shared turn renderer. Whether a
// real model follows the rule is the owner's recorded live check.
// =============================================================================

const LINE = 150;

interface Repos {
  docs: string;
  code: string;
}

async function workspaceRepos(page: Page, workspaceId: string): Promise<Repos> {
  const detail = await authedApiViaRequest(
    page,
    `/api/v1/workspaces/${workspaceId}`,
  );
  expect(detail.status).toBe(200);
  const docs = String((detail.body?.data ?? detail.body).docsRepoId);
  const live = await waitForLiveWorkspaceChanges(page, workspaceId);
  const code = live.members.find((member) => member.repo_id !== docs);
  expect(code?.repo_id, "the workspace has a code repository").toBeTruthy();
  return { docs, code: code!.repo_id };
}

async function writeFile(
  page: Page,
  workspaceId: string,
  repoId: string,
  path: string,
  content: string,
): Promise<void> {
  const written = await authedApi(
    page,
    `/api/v1/workspaces/${workspaceId}/files/${path}?repoId=${repoId}`,
    "PUT",
    { content, encoding: "utf-8", expectedHash: null },
  );
  expect(written.status, JSON.stringify(written.body)).toBe(200);
}

/** A code-repository Markdown file whose lines name themselves. */
function numberedLines(token: string): string {
  return Array.from(
    { length: 200 },
    (_, index) => `line ${index + 1} ${token}`,
  ).join("\n");
}

/** The file open in the center panel shows `LINE` in plain text, scrolled to it. */
async function expectSourceAtLine(page: Page, token: string): Promise<void> {
  const view = page.getByTestId("center-panel").getByTestId("file-view");
  // A Markdown file at a line shows plain text, with rich text one switch away.
  await expect(view.getByTestId("file-view-switch-mode")).toHaveText(
    "Switch to rich text",
    { timeout: 30_000 },
  );
  const lines = view.locator(".monaco-editor .view-lines");
  await expect(lines).toContainText(`line ${LINE} ${token}`, {
    timeout: 30_000,
  });
  // Monaco renders only lines near the viewport: the top of the file is not
  // shown, so the editor scrolled to the cited line.
  await expect(lines).not.toContainText(`line 2 ${token}`);
}

test.describe("Agent file citations", () => {
  test.describe.configure({ timeout: 480_000 });
  let workspace: WorkspaceClone | null = null;
  let turn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `file-links-${testInfo.title.slice(0, 20)}`,
    });
    await seedWorkspaceClaudeCredential(page, workspace.id);
    await seedWorkspaceCodexCredential(page, workspace.id);
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    if (turn) await releaseChatTurn(page, turn).catch(() => {});
    turn = undefined;
    if (workspace)
      await deleteWorkspaceById(page, workspace.id).catch(() => {});
    workspace = null;
  });

  for (const provider of ["claude_code", "codex"] as const) {
    test(`${provider} reply links open the Project Doc and the cited code line`, async ({
      page,
    }) => {
      const ws = workspace!;
      const repos = await workspaceRepos(page, ws.id);
      const token = randomUUID().slice(0, 8);
      const docName = `links-${token}.md`;
      const docPath = `docs/${ws.id}/${docName}`;
      const codeName = `notes-${token}.md`;
      await writeFile(
        page,
        ws.id,
        repos.docs,
        docPath,
        `# Links doc ${token}\n`,
      );
      await writeFile(page, ws.id, repos.code, codeName, numberedLines(token));

      await selectAgentProviderViaSettings(page, provider);
      await expectSelectedAgentProvider(page, provider);
      const input = page.getByTestId("chat-input").first();
      await expect(input).toBeVisible({ timeout: 30_000 });
      // A one-code-repository workspace: neither link names a repository.
      await input.fill(
        `See [${docName}](g5://file/${docPath}) and [${codeName}](g5://file/${codeName}#L${LINE}).`,
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
      await expectAssistantReply(page, turn!, codeName, { timeout: 180_000 });
      const reply = page
        .locator(`[data-testid="assistant-message"]:has([data-g5-file-link])`)
        .last();

      // The Project Doc opens in the docs repository, in Preview.
      await reply.getByRole("link", { name: docName }).click();
      const view = page.getByTestId("center-panel").getByTestId("file-view");
      await expect(view.getByTestId("file-view-preview")).toContainText(
        `Links doc ${token}`,
        { timeout: 30_000 },
      );

      // Opening a file can move Chat out of view; bring it back for the
      // second link. The code link opens the code repository's file at the
      // cited line.
      await openChatComposer(page);
      await reply.getByRole("link", { name: codeName }).click();
      await expectSourceAtLine(page, token);
    });
  }

  test("an explainer link opens a qualified file at its line; an unknown repository is refused", async ({
    page,
  }) => {
    const ws = workspace!;
    const repos = await workspaceRepos(page, ws.id);
    const token = randomUUID().slice(0, 8);
    const codeName = `notes-${token}.md`;
    await writeFile(page, ws.id, repos.code, codeName, numberedLines(token));
    const htmlPath = `docs/${ws.id}/explainers/links-${token}.html`;
    await writeFile(
      page,
      ws.id,
      repos.docs,
      htmlPath,
      `<!doctype html><html><body>
<h1>Links explainer ${token}</h1>
<p><a id="unknown" href="g5://file/${codeName}?repo=no-such-repo#L${LINE}">unknown repository</a></p>
<p><a id="qualified" href="g5://file/${codeName}?repo=${repos.code}#L${LINE}">qualified line</a></p>
</body></html>`,
    );
    const docId = await docIdForPath(page, ws.id, htmlPath);
    await page.goto(`${ws.path}/doc/${encodeURIComponent(docId)}`);
    const frame = page.frameLocator("iframe[sandbox]").first();
    await expect(frame.locator("h1")).toContainText(
      `Links explainer ${token}`,
      {
        timeout: 60_000,
      },
    );

    // An unknown repository fails closed and says which one it named.
    await frame.locator("#unknown").click();
    await expect(page.getByText("Cannot open file link")).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText(/"no-such-repo"/)).toBeVisible();

    // Past the frame's one-proposal-per-750ms bound, the qualified link
    // opens the code file at the cited line.
    await page.waitForTimeout(1_000);
    await frame.locator("#qualified").click();
    await expectSourceAtLine(page, token);
  });
});
