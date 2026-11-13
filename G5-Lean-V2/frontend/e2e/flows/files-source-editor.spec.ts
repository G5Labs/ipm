import { expect, test, type Locator, type Page } from "@playwright/test";

import {
  authedApi,
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandRepositoryFileRoot,
  openWorkspacePath,
  patchStoredUserSettings,
  readCurrentOrgContext,
  readWorkspaceVm,
  recordExpectedSubject,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  TEST_ORG,
  USING_OAUTH_STUB,
  type WorkspaceClone,
} from "../helpers";
import { loginWithOAuthStub, SEEDED_FORK_MEMBER_IDENTITY } from "../oauth";

/**
 * FILES opens files like GitHub (`g5.documents.files-view`): Markdown in a
 * rendered Preview, other text in read-only Monaco, editing as an explicit
 * step. The owner saves (commit and push); a reader suggests changes that
 * the owner accepts (`g5.governance.file-suggestions`). Monaco comes from
 * G5's own bundle (`g5.documents.editor.assets`), and unsaved text outlives
 * view, tab and page changes (`g5.documents.files-view.drafts`).
 */
test.describe("FILES source editor", () => {
  test.describe.configure({ timeout: 360_000 });
  let workspace: WorkspaceClone | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `files-view-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
  });

  test.afterEach(async ({ page }) => {
    if (workspace) {
      await deleteWorkspaceById(page, workspace.id).catch(() => {});
      workspace = undefined;
    }
  });

  async function codeRepoId(page: Page, id: string): Promise<string> {
    const detail = await authedApiViaRequest(page, `/api/v1/workspaces/${id}`);
    expect(detail.status).toBe(200);
    const data = detail.body?.data ?? detail.body;
    const branch = data.branches.find(
      (candidate: { repoId: string }) => candidate.repoId !== data.docsRepoId,
    );
    expect(branch?.repoId).toBeTruthy();
    return String(branch.repoId);
  }

  /** Writes, commits and pushes a file, so readers see it too. */
  async function commitFile(
    page: Page,
    id: string,
    repoId: string,
    path: string,
    content: string,
    encoding: "utf-8" | "base64" = "utf-8",
  ): Promise<void> {
    const vm = await readWorkspaceVm(page, id);
    const written = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/files/${path}?repoId=${repoId}`,
      "PUT",
      {
        content,
        encoding,
        expectedHash: null,
        expectedVmId: vm.vmId,
        commitAndPush: true,
      },
    );
    expect([200, 201], JSON.stringify(written.body)).toContain(written.status);
  }

  async function readFile(
    page: Page,
    id: string,
    repoId: string,
    path: string,
  ): Promise<string> {
    const read = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/files/${path}?repoId=${repoId}`,
    );
    expect(read.status).toBe(200);
    return String((read.body?.data ?? read.body).content);
  }

  async function openFile(
    page: Page,
    repoId: string,
    fileName: string,
  ): Promise<Locator> {
    await expandRepositoryFileRoot(page, repoId);
    const entry = page
      .getByTestId("sidebar-files-tab")
      .getByTestId("file-tree-entry")
      .filter({ hasText: fileName })
      .first();
    await expect(entry).toBeVisible({ timeout: 30_000 });
    await entry.click();
    // The text view, or the card a file that is not text gets instead.
    const view = page
      .getByTestId("center-panel")
      .locator(
        `[data-testid="file-view"][data-file-path="${fileName}"], ` +
          `[data-testid="file-view-unavailable"][data-file-path="${fileName}"]`,
      );
    await expect(view).toBeVisible({ timeout: 30_000 });
    return view;
  }

  async function branchHead(
    page: Page,
    id: string,
    repoId: string,
  ): Promise<string> {
    const response = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/branches`,
    );
    expect(response.status, JSON.stringify(response.body)).toBe(200);
    const body = response.body?.data ?? response.body;
    const branches = Array.isArray(body) ? body : (body?.branches ?? []);
    const branch = branches.find(
      (row: { repoId?: string; repo_id?: string }) =>
        row.repoId === repoId || row.repo_id === repoId,
    );
    return String(branch?.headCommit ?? branch?.head_commit ?? "");
  }

  /** Leaves the workspace page inside the app (no reload) and comes back. */
  async function visitHomeAndReturn(page: Page): Promise<void> {
    const back = new URL(page.url()).pathname;
    await page.evaluate((org) => {
      window.history.pushState({}, "", `/${org}/home`);
      window.dispatchEvent(new PopStateEvent("popstate"));
    }, TEST_ORG);
    await expect(page.getByTestId("center-panel")).toHaveCount(0, {
      timeout: 30_000,
    });
    await page.evaluate((path) => {
      window.history.pushState({}, "", path);
      window.dispatchEvent(new PopStateEvent("popstate"));
    }, back);
    await expect(page.getByTestId("center-panel")).toBeVisible({
      timeout: 30_000,
    });
  }

  async function typeAtEnd(
    page: Page,
    editor: Locator,
    text: string,
  ): Promise<void> {
    const lines = editor.locator(".monaco-editor .view-lines").first();
    await expect(lines).toBeVisible({ timeout: 15_000 });
    await lines.click();
    await page.keyboard.press("ControlOrMeta+End");
    await page.keyboard.insertText(text);
  }

  test("the owner reads first, edits in place, keeps unsaved text, and Save commits it", async ({
    page,
  }) => {
    // Monaco must come from G5's own bundle.
    const cdnRequests: string[] = [];
    await page.route("**/cdn.jsdelivr.net/**", (route) => {
      cdnRequests.push(route.request().url());
      return route.abort();
    });
    const id = workspace!.id;
    const repoId = await codeRepoId(page, id);
    const stamp = Date.now();
    const mdName = `e2e-files-view-${stamp}.md`;
    const tsName = `e2e-files-view-${stamp}.ts`;
    const binName = `e2e-files-view-${stamp}.bin`;
    await commitFile(
      page,
      id,
      repoId,
      mdName,
      "# Read first\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> [!NOTE]\n> A GitHub alert.\n\n- [x] done\n",
    );
    await commitFile(page, id, repoId, tsName, "export const answer = 42;\n");
    await commitFile(
      page,
      id,
      repoId,
      binName,
      Buffer.from([0, 1, 2, 255, 0, 66]).toString("base64"),
      "base64",
    );

    await openWorkspacePath(page, workspace!.path);
    const view = await openFile(page, repoId, mdName);

    // Markdown opens rendered, like GitHub.
    const preview = view.getByTestId("markdown-preview");
    await expect(preview.locator("h1")).toHaveText("Read first");
    await expect(preview.locator(".markdown-alert-note")).toContainText(
      "A GitHub alert.",
    );
    await expect(preview.locator("table td").first()).toHaveText("1");

    // Plain text is read-only Monaco, and the owner starts a thread from its
    // glyph margin.
    await view.getByTestId("file-view-switch-mode").click();
    const editor = view.getByTestId("code-editor");
    const firstLine = editor.locator(".monaco-editor .view-line").first();
    await expect(firstLine).toBeVisible({ timeout: 30_000 });
    // Monaco can expose its empty first row before the model and hover
    // handlers settle. Target the actual text, then prove its glyph is ready.
    await expect(firstLine).toContainText("# Read first");
    const commentGlyph = editor.locator(".g5-monaco-add-comment").first();
    await expect(async () => {
      await firstLine.hover({ position: { x: 4, y: 10 } });
      await expect(commentGlyph).toBeVisible({ timeout: 1_000 });
    }).toPass({ timeout: 15_000, intervals: [250] });
    await commentGlyph.click();
    const commented = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/api/v1/workspaces/${id}/comments`),
    );
    await editor
      .getByLabel("Comment", { exact: true })
      .fill("Is this title final?");
    await editor.getByTestId("draft-comment-save").click();
    const comment = await commented;
    expect(comment.status()).toBe(201);
    expect(comment.request().postDataJSON()).toMatchObject({
      repoId,
      filePath: mdName,
      lineNumber: 1,
    });
    const threadId = String((await comment.json())?.data?.id ?? "");
    await expect(
      editor
        .getByTestId(`comment-thread-${threadId}`)
        .getByTestId("comment-thread-anchor"),
    ).toHaveText("L1");

    // Edit turns the same view editable; Done shows the unsaved text in the
    // reading view, and the draft stays until Save or Discard.
    await view.getByTestId("file-view-edit").click();
    await typeAtEnd(page, editor, "\nAn unsaved line\n");
    await expect(
      editor.getByTestId("code-editor-dirty-indicator"),
    ).toBeVisible();
    await view.getByTestId("file-view-done").click();
    await expect(view.getByTestId("markdown-preview")).toContainText(
      "An unsaved line",
    );
    // Back to plain text editing: the switch while reading, then Edit.
    await view.getByTestId("file-view-switch-mode").click();
    await view.getByTestId("file-view-edit").click();

    // Another tab, then the page Home and back: the unsaved text stays.
    await openFile(page, repoId, tsName);
    await expect(
      page.getByTestId("center-panel").getByTestId("code-editor"),
    ).toContainText("answer");
    await visitHomeAndReturn(page);
    const reopened = await openFile(page, repoId, mdName);
    await expect(reopened.getByTestId("file-view-done")).toBeVisible();
    await expect(reopened.getByTestId("code-editor")).toContainText(
      "An unsaved line",
    );
    // Closing or reloading the tab would ask first.
    expect(
      await page.evaluate(() => {
        const event = new Event("beforeunload", { cancelable: true });
        window.dispatchEvent(event);
        return event.defaultPrevented;
      }),
    ).toBe(true);

    // Save commits and pushes the file, as before.
    const headBeforeSave = await branchHead(page, id, repoId);
    const saved = page.waitForResponse(
      (response) =>
        response.request().method() === "PUT" &&
        response.url().includes(`/workspaces/${id}/files/${mdName}`),
    );
    await reopened
      .getByTestId("code-editor")
      .getByTestId("code-editor-save-button")
      .click();
    expect((await saved).status()).toBe(200);
    await expect(reopened.getByTestId("file-view-done")).toBeVisible();
    expect(await readFile(page, id, repoId, mdName)).toContain(
      "An unsaved line",
    );
    await expect
      .poll(() => branchHead(page, id, repoId), { timeout: 30_000 })
      .not.toBe(headBeforeSave);

    // A binary file shows a card with Download, never the editor.
    const binary = await openFile(page, repoId, binName);
    await expect(binary).toHaveAttribute(
      "data-testid",
      "file-view-unavailable",
    );
    await expect(binary).toContainText("Binary file");
    await expect(binary.getByTestId("code-editor")).toHaveCount(0);
    const download = page.waitForEvent("download");
    await binary.getByTestId("file-view-download").click();
    expect((await download).suggestedFilename()).toBe(binName);

    expect(cdnRequests).toEqual([]);
  });

  test("a reader suggests a change with the owner's VM stopped, and the owner accepts it", async ({
    page,
    browser,
  }) => {
    test.skip(
      !USING_OAUTH_STUB,
      "Needs the OAuth stub to log a second, non-owner identity in",
    );
    const id = workspace!.id;
    const { orgId } = await readCurrentOrgContext(page);
    const invited = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/members`,
      "POST",
      {
        email: SEEDED_FORK_MEMBER_IDENTITY.email,
        role: "member",
      },
    );
    expect([200, 201], JSON.stringify(invited.body)).toContain(invited.status);
    const repoId = await codeRepoId(page, id);
    const mdName = `e2e-files-suggest-${Date.now()}.md`;
    await commitFile(
      page,
      id,
      repoId,
      mdName,
      "# Guide\n\nThe old sentence.\n\nKeep this line.\n",
    );
    await runWorkspaceVmAction(page, id, "stop");
    await waitForWorkspaceVmStatus(page, id, "stopped");

    const readerContext = await browser.newContext({
      storageState: { cookies: [], origins: [] },
    });
    try {
      const reader = await readerContext.newPage();
      await loginWithOAuthStub(reader, SEEDED_FORK_MEMBER_IDENTITY, {
        targetOrgSlug: TEST_ORG,
      });
      await recordExpectedSubject(reader);
      // A fresh context misses auth.setup's pin of the full sidebar
      // (g5.ui.sidebar-default); openFile clicks the stacked panel.
      await patchStoredUserSettings(reader, {
        sidebarDisplayMode: "expanded",
      });
      await openWorkspacePath(reader, workspace!.path);
      const view = await openFile(reader, repoId, mdName);
      await expect(view.getByTestId("markdown-preview")).toContainText(
        "The old sentence.",
      );
      // A reader suggests; there is no Edit and no Start.
      await expect(view.getByTestId("file-view-edit")).toHaveCount(0);
      await expect(view.getByTestId("file-view-start-to-edit")).toHaveCount(0);
      await view.getByTestId("file-view-suggest").click();
      // Suggest changes opens Rich text; this reader types plain text.
      await view.getByTestId("file-view-switch-mode").click();
      const editor = view.getByTestId("code-editor");
      const lines = editor.locator(".monaco-editor .view-lines").first();
      await expect(lines).toBeVisible({ timeout: 30_000 });
      await lines.click();
      await reader.keyboard.press("ControlOrMeta+End");
      await reader.keyboard.insertText("A suggested line.\n");
      await view.getByTestId("file-view-push-review").click();
      await view
        .getByTestId("file-view-push-note")
        .fill("Adds the missing line");
      const pushed = reader.waitForResponse(
        (response) =>
          response.request().method() === "POST" &&
          response.url().includes(`/workspaces/${id}/review/file-suggestions`),
      );
      await view.getByTestId("file-view-push-submit").click();
      expect((await pushed).status()).toBe(201);
      await expect(view.getByTestId("file-view-push-done")).toBeVisible();
    } finally {
      await readerContext.close();
    }

    // The owner starts the workspace and accepts the suggestion in plain text.
    await runWorkspaceVmAction(page, id, "resume");
    await waitForWorkspaceVmStatus(page, id, "running");
    await openWorkspacePath(page, workspace!.path);
    const ownerView = await openFile(page, repoId, mdName);
    await ownerView.getByTestId("file-view-switch-mode").click();
    const accept = ownerView
      .locator('[data-testid^="review-suggestion-accept-"]')
      .first();
    await expect(accept).toBeEnabled({ timeout: 60_000 });
    const accepted = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        /\/review\/suggestions\/[^/]+\/accept/.test(response.url()),
    );
    await accept.click();
    expect((await accepted).status()).toBe(200);
    await expect
      .poll(() => readFile(page, id, repoId, mdName), { timeout: 30_000 })
      .toContain("A suggested line.");
  });
});
