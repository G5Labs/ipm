import { createHash } from "node:crypto";
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
 * Anchored comments (`g5.documents.comments`, `.identity`, `.replies`): a
 * comment written on unsaved text is anchored to the line the person chose,
 * keeps that line after Save and reload, and a reader of the saved version
 * sees it on the same line and can reply without a running VM.
 */
test.describe("Anchored comments", () => {
  test.describe.configure({ timeout: 360_000 });
  test.skip(
    !USING_OAUTH_STUB,
    "Needs the OAuth stub to log a second, non-owner identity in",
  );

  let workspace: WorkspaceClone | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    const { orgId } = await readCurrentOrgContext(page);
    const invited = await authedApi(
      page,
      `/api/v1/orgs/${orgId}/members`,
      "POST",
      { email: SEEDED_FORK_MEMBER_IDENTITY.email, role: "member" },
    );
    expect([200, 201], JSON.stringify(invited.body)).toContain(invited.status);
    workspace = await createWorkspaceClone(page, {
      namePrefix: `comments-${testInfo.title}`,
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

  function sha256(text: string): string {
    return createHash("sha256").update(text, "utf8").digest("hex");
  }

  /** The context a browser records around line `index` (0-based). */
  function anchorContext(lines: string[], index: number): string {
    return JSON.stringify({
      version: 1,
      before: lines.slice(Math.max(0, index - 2), index),
      line: lines[index],
      after: lines.slice(index + 1, index + 3),
      beforeTruncated: false,
      afterTruncated: false,
      lineTruncated: false,
    });
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
    const editor = page.getByTestId("center-panel").getByTestId("code-editor");
    await expect(editor).toHaveAttribute("data-file-path", fileName, {
      timeout: 30_000,
    });
    return editor;
  }

  async function showComments(editor: Locator): Promise<Locator> {
    const column = editor.getByTestId("comment-column");
    if (!(await column.isVisible())) {
      await editor.getByTestId("code-editor-toggle-comments-button").click();
    }
    await expect(column).toBeAttached();
    return column;
  }

  test("a comment on unsaved lines keeps its line after Save and reload, and a reader sees, answers and starts one", async ({
    page,
    browser,
  }) => {
    const id = workspace!.id;
    const detail = await authedApiViaRequest(page, `/api/v1/workspaces/${id}`);
    expect(detail.status).toBe(200);
    const workspaceDetail = detail.body?.data ?? detail.body;
    const codeBranch = workspaceDetail.branches.find(
      (branch: { repoId: string }) =>
        branch.repoId !== workspaceDetail.docsRepoId,
    );
    expect(codeBranch?.repoId).toBeTruthy();
    const repoId = String(codeBranch.repoId);

    const fileName = `e2e-comments-${Date.now()}.txt`;
    const original = "alpha line\nbeta line\ntarget line\nomega line\n";
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/files/${fileName}?repoId=${repoId}`,
      "PUT",
      { content: original, encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(created.body)).toContain(created.status);

    await openWorkspacePath(page, workspace!.path);
    const editor = await openFile(page, repoId, fileName);
    const lines = editor.locator(".monaco-editor .view-lines").first();
    await expect(lines).toBeVisible({ timeout: 15_000 });
    await expect(editor).toContainText("target line");

    // FILES opens files read-first; the owner edits after Edit.
    await page
      .getByTestId("center-panel")
      .getByTestId("file-view-edit")
      .click();
    // Unsaved: two lines inserted above; the target is now line 5.
    await lines.click();
    await page.keyboard.press("ControlOrMeta+Home");
    await page.keyboard.insertText("inserted one\ninserted two\n");
    await expect(
      editor.getByTestId("code-editor-dirty-indicator"),
    ).toBeVisible();
    await page.keyboard.press("ControlOrMeta+Home");
    for (let step = 0; step < 4; step += 1) {
      await page.keyboard.press("ArrowDown");
    }

    // Add Comment at the caret, from the menu of the comments margin (a
    // right-click in the text would also open Monaco's own menu).
    const column = await showComments(editor);
    await column.locator("..").click({ button: "right" });
    await page.getByTestId("code-editor-context-add-comment-button").click();
    const created201 = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/api/v1/workspaces/${id}/comments`),
    );
    await editor
      .getByLabel("Comment", { exact: true })
      .fill("Why keep the target line?");
    await editor.getByTestId("draft-comment-save").click();
    const createResponse = await created201;
    expect(createResponse.status()).toBe(201);
    const request = createResponse.request().postDataJSON();
    const unsaved = `inserted one\ninserted two\n${original}`;
    expect(request).toMatchObject({
      scope: "line",
      repoId,
      filePath: fileName,
      lineNumber: 5,
      lineHash: sha256("target line"),
      contentHash: sha256(unsaved),
    });
    expect(request).not.toHaveProperty("originBranch");
    expect(request).not.toHaveProperty("nodeId");
    const commentId = String((await createResponse.json())?.data?.id ?? "");
    expect(commentId).toBeTruthy();
    const thread = editor.getByTestId(`comment-thread-${commentId}`);
    await expect(thread.getByTestId("comment-thread-anchor")).toHaveText("L5");

    // Save, reload: the thread is still on the target line.
    const saved = page.waitForResponse(
      (response) =>
        response.request().method() === "PUT" &&
        response.url().includes(`/workspaces/${id}/files/${fileName}`),
    );
    await editor.getByTestId("code-editor-save-button").click();
    expect((await saved).status()).toBe(200);
    await page.reload({ waitUntil: "domcontentloaded" });
    const reloaded = await openFile(page, repoId, fileName);
    await showComments(reloaded);
    const ownerThread = reloaded.getByTestId(`comment-thread-${commentId}`);
    await expect(ownerThread.getByTestId("comment-thread-anchor")).toHaveText(
      "L5",
      { timeout: 30_000 },
    );

    // Unsaved again, a line deleted above: the thread follows it on screen,
    // and a second comment below is anchored to the text shown. The
    // reloaded page opens the file read-first again.
    await page
      .getByTestId("center-panel")
      .getByTestId("file-view-edit")
      .click();
    const reloadedLines = reloaded
      .locator(".monaco-editor .view-lines")
      .first();
    await reloadedLines.click();
    await page.keyboard.press("ControlOrMeta+Home");
    await page.keyboard.press("Shift+End");
    await page.keyboard.press("Delete");
    await page.keyboard.press("Delete");
    const trimmed = `inserted two\n${original}`;
    await expect(ownerThread.getByTestId("comment-thread-anchor")).toHaveText(
      "L4",
    );
    await page.keyboard.press("ControlOrMeta+Home");
    for (let step = 0; step < 4; step += 1) {
      await page.keyboard.press("ArrowDown");
    }
    await (await showComments(reloaded)).locator("..").click({
      button: "right",
    });
    await page.getByTestId("code-editor-context-add-comment-button").click();
    const secondCreated = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/api/v1/workspaces/${id}/comments`),
    );
    await reloaded
      .getByLabel("Comment", { exact: true })
      .fill("And the omega line?");
    await reloaded.getByTestId("draft-comment-save").click();
    const secondResponse = await secondCreated;
    expect(secondResponse.status()).toBe(201);
    expect(secondResponse.request().postDataJSON()).toMatchObject({
      lineNumber: 5,
      lineHash: sha256("omega line"),
      contentHash: sha256(trimmed),
    });
    const secondId = String((await secondResponse.json())?.data?.id ?? "");
    expect(secondId).toBeTruthy();

    // Save, reload: both threads are on their lines of the saved text.
    const savedTrimmed = page.waitForResponse(
      (response) =>
        response.request().method() === "PUT" &&
        response.url().includes(`/workspaces/${id}/files/${fileName}`),
    );
    await reloaded.getByTestId("code-editor-save-button").click();
    expect((await savedTrimmed).status()).toBe(200);
    await page.reload({ waitUntil: "domcontentloaded" });
    const afterTrim = await openFile(page, repoId, fileName);
    await showComments(afterTrim);
    await expect(
      afterTrim
        .getByTestId(`comment-thread-${commentId}`)
        .getByTestId("comment-thread-anchor"),
    ).toHaveText("L4", { timeout: 30_000 });
    await expect(
      afterTrim
        .getByTestId(`comment-thread-${secondId}`)
        .getByTestId("comment-thread-anchor"),
    ).toHaveText("L5");

    // A reader reads the saved version once the workspace is stopped, sees
    // the threads on the same lines, and replies and starts a thread without
    // a running VM.
    await runWorkspaceVmAction(page, id, "stop");
    await waitForWorkspaceVmStatus(page, id, "stopped");
    // Stop withdraws the private live tab. Reopen the authorized pushed file
    // before checking live comment updates in the stopped owner's reader.
    await expect(
      page.getByTestId("center-panel").getByTestId("code-editor"),
    ).toHaveCount(0);
    const stoppedOwnerReader = await openFile(page, repoId, fileName);
    await expect(stoppedOwnerReader).toContainText("inserted two");
    await showComments(stoppedOwnerReader);
    await expect(
      stoppedOwnerReader
        .getByTestId(`comment-thread-${commentId}`)
        .getByTestId("comment-thread-anchor"),
    ).toHaveText("L4");
    const memberContext = await browser.newContext({
      // The chromium project applies the seeded owner's storageState to every
      // context by default; the member must start signed out.
      storageState: { cookies: [], origins: [] },
    });
    try {
      const memberPage = await memberContext.newPage();
      await loginWithOAuthStub(memberPage, SEEDED_FORK_MEMBER_IDENTITY, {
        targetOrgSlug: TEST_ORG,
      });
      await recordExpectedSubject(memberPage);
      // A fresh context misses auth.setup's pin of the full sidebar
      // (g5.ui.sidebar-default); openFile clicks the stacked panel.
      await patchStoredUserSettings(memberPage, {
        sidebarDisplayMode: "expanded",
      });
      await openWorkspacePath(memberPage, workspace!.path);
      const readerEditor = await openFile(memberPage, repoId, fileName);
      await expect(readerEditor).toContainText("inserted two");
      await showComments(readerEditor);
      const readerThread = readerEditor.getByTestId(
        `comment-thread-${commentId}`,
      );
      await expect(
        readerThread.getByTestId("comment-thread-anchor"),
      ).toHaveText("L4", { timeout: 30_000 });
      // The comment made after a line was deleted above it is on its line
      // of the saved version the reader sees.
      await expect(
        readerEditor
          .getByTestId(`comment-thread-${secondId}`)
          .getByTestId("comment-thread-anchor"),
      ).toHaveText("L5");
      await readerThread.getByTestId("comment-thread-toggle-expand").click();
      await expect(
        readerThread.getByTestId("comment-thread-resolve"),
      ).toHaveCount(0);
      await expect(
        readerThread.getByTestId("comment-thread-delete"),
      ).toHaveCount(0);
      const replied = memberPage.waitForResponse(
        (response) =>
          response.request().method() === "POST" &&
          response.url().includes(`/api/v1/comments/${commentId}/replies`),
      );
      await readerThread.getByPlaceholder("Reply…").fill("It guards the retry");
      await readerThread.getByPlaceholder("Reply…").press("Enter");
      expect((await replied).status()).toBe(201);
      await expect(readerThread).toContainText("It guards the retry");

      // The reader starts a thread of their own from the comments margin's
      // menu (`g5.documents.comments.authority`), and the owner sees it.
      const readerColumn = await showComments(readerEditor);
      await readerColumn.locator("..").click({ button: "right" });
      await memberPage
        .getByTestId("code-editor-context-add-comment-button")
        .click();
      const readerCreated = memberPage.waitForResponse(
        (response) =>
          response.request().method() === "POST" &&
          response.url().includes(`/api/v1/workspaces/${id}/comments`),
      );
      await readerEditor
        .getByLabel("Comment", { exact: true })
        .fill("Is omega still needed?");
      await readerEditor.getByTestId("draft-comment-save").click();
      const readerCreateResponse = await readerCreated;
      expect(readerCreateResponse.status()).toBe(201);
      const readerThreadId = String(
        (await readerCreateResponse.json())?.data?.id ?? "",
      );
      expect(readerThreadId).toBeTruthy();
      const readerOwnThread = readerEditor.getByTestId(
        `comment-thread-${readerThreadId}`,
      );
      await expect(readerOwnThread).toContainText("Is omega still needed?");
      // Expanded, the reader's own thread offers a reply box but no
      // decisions: those render only when expanded, so collapsed would
      // prove nothing.
      await readerOwnThread.getByTestId("comment-thread-toggle-expand").click();
      await expect(readerOwnThread.getByPlaceholder("Reply…")).toBeVisible();
      await expect(
        readerOwnThread.getByTestId("comment-thread-resolve"),
      ).toHaveCount(0);
      await expect(
        readerOwnThread.getByTestId("comment-thread-delete"),
      ).toHaveCount(0);
      // The owner sees it live and decides it; the reader sees the decision.
      const ownerReaderThread = page
        .getByTestId("center-panel")
        .getByTestId(`comment-thread-${readerThreadId}`);
      await expect(ownerReaderThread).toContainText("Is omega still needed?", {
        timeout: 30_000,
      });
      if (
        (await ownerReaderThread
          .getByTestId("comment-thread-toggle-expand")
          .getAttribute("aria-expanded")) !== "true"
      ) {
        await ownerReaderThread
          .getByTestId("comment-thread-toggle-expand")
          .click();
      }
      await ownerReaderThread.getByTestId("comment-thread-resolve").click();
      await expect(readerOwnThread).toHaveAttribute("data-resolved", "true", {
        timeout: 30_000,
      });

      // Live, both ways, without reloading: the owner sees the reply, and
      // the reader sees the owner resolve the thread.
      const ownerLiveThread = page
        .getByTestId("center-panel")
        .getByTestId(`comment-thread-${commentId}`);
      if (
        (await ownerLiveThread
          .getByTestId("comment-thread-toggle-expand")
          .getAttribute("aria-expanded")) !== "true"
      ) {
        await ownerLiveThread
          .getByTestId("comment-thread-toggle-expand")
          .click();
      }
      await expect(ownerLiveThread).toContainText("It guards the retry", {
        timeout: 30_000,
      });
      await ownerLiveThread.getByTestId("comment-thread-resolve").click();
      await expect(readerThread).toHaveAttribute("data-resolved", "true", {
        timeout: 30_000,
      });
      // Reading, replying and resolving never started the owner's workspace.
      expect((await readWorkspaceVm(page, id)).status).toBe("stopped");
    } finally {
      await memberContext.close();
    }
  });
  test("a rendered Markdown file lists its threads by line and a label opens plain text", async ({
    page,
  }) => {
    const id = workspace!.id;
    const detail = await authedApiViaRequest(page, `/api/v1/workspaces/${id}`);
    expect(detail.status).toBe(200);
    const workspaceDetail = detail.body?.data ?? detail.body;
    const codeBranch = workspaceDetail.branches.find(
      (branch: { repoId: string }) =>
        branch.repoId !== workspaceDetail.docsRepoId,
    );
    const repoId = String(codeBranch.repoId);
    const fileName = `e2e-comments-preview-${Date.now()}.md`;
    const content =
      "# Guide\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n";
    const lines = content.split("\n");
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/files/${fileName}?repoId=${repoId}`,
      "PUT",
      { content, encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(created.body)).toContain(created.status);

    // The thread on the earlier line is started first, so the API, which
    // lists newest first, puts the one on line 7 above it.
    const startThread = async (lineNumber: number, text: string) => {
      const response = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${id}/comments`,
        "POST",
        {
          scope: "line",
          repoId,
          filePath: fileName,
          lineNumber,
          lineHash: sha256(lines[lineNumber - 1]),
          contentHash: sha256(content),
          anchorContext: anchorContext(lines, lineNumber - 1),
          content: text,
        },
      );
      expect(response.status, JSON.stringify(response.body)).toBe(201);
      const threadId = String(response.body?.data?.id ?? "");
      expect(threadId).toBeTruthy();
      return threadId;
    };
    const onLine3 = await startThread(3, "Why the first paragraph?");
    const onLine7 = await startThread(7, "And the third?");
    // A file without threads, for the panel's empty state.
    const emptyName = `e2e-comments-empty-${Date.now()}.md`;
    const emptyCreated = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/files/${emptyName}?repoId=${repoId}`,
      "PUT",
      {
        content: "# Empty\n\nNothing here.\n",
        encoding: "utf-8",
        expectedHash: null,
      },
    );
    expect([200, 201]).toContain(emptyCreated.status);

    await openWorkspacePath(page, workspace!.path);
    const center = page.getByTestId("center-panel");
    // Comments on a file without threads still shows the panel.
    const emptyEditor = await openFile(page, repoId, emptyName);
    await emptyEditor.getByTestId("code-editor-toggle-comments-button").click();
    await expect(
      emptyEditor
        .getByTestId("file-view-preview-threads")
        .getByTestId("side-threads-empty"),
    ).toHaveText("No comments on this file yet. Select text to comment.", {
      timeout: 30_000,
    });

    const editor = await openFile(page, repoId, fileName);
    // Markdown opens rendered; its threads are listed beside the rendering.
    await expect(center.getByTestId("file-view-switch-mode")).toHaveText(
      "Switch to plain text",
    );
    await expect(editor.getByTestId("markdown-preview")).toContainText(
      "Third paragraph.",
    );
    const panel = editor.getByTestId("file-view-preview-threads");
    if (!(await panel.isVisible())) {
      await editor.getByTestId("code-editor-toggle-comments-button").click();
    }
    await expect(panel.getByTestId("comment-thread-anchor")).toHaveText(
      ["L3", "L7"],
      { timeout: 30_000 },
    );
    await expect(panel.getByTestId(`comment-thread-${onLine3}`)).toBeVisible();

    // Selecting words in the rendered text offers Comment. The draft opens
    // in the panel, the page staying as it is, and the thread lands on the
    // selection's line with the words quoted.
    const paragraph = editor
      .getByTestId("markdown-preview")
      .getByText("Second paragraph.");
    const box = await paragraph.boundingBox();
    expect(box).toBeTruthy();
    await page.mouse.move(box!.x + 2, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.mouse.move(box!.x + box!.width * 0.6, box!.y + box!.height / 2, {
      steps: 8,
    });
    await page.mouse.up();
    await editor
      .getByTestId("document-preview-selection-comment-button")
      .click();
    const draft = panel.getByLabel("Comment", { exact: true });
    await expect(draft).toBeVisible();
    await expect(center.getByTestId("file-view-switch-mode")).toHaveText(
      "Switch to plain text",
    );
    const posted = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/api/v1/workspaces/${id}/comments`),
    );
    await draft.fill("Why the second paragraph?");
    await panel.getByTestId("draft-comment-save").click();
    const response = await posted;
    expect(response.status()).toBe(201);
    const body = response.request().postDataJSON();
    expect(body).toMatchObject({
      lineNumber: 5,
      lineHash: sha256("Second paragraph."),
    });
    expect(String(body.content)).toMatch(/^> Second/);
    await expect(panel.getByTestId("comment-thread-anchor")).toHaveText(
      ["L3", "L5", "L7"],
      { timeout: 30_000 },
    );

    // The label of the card on line 7 opens plain text at that line, where the
    // margin places the thread, and starts no draft.
    await panel
      .getByTestId(`comment-thread-${onLine7}`)
      .getByTestId("comment-thread-anchor")
      .click();
    await expect(center.getByTestId("file-view-switch-mode")).toHaveText(
      "Switch to rich text",
    );
    await expect(panel).toHaveCount(0);
    await expect(editor.getByLabel("Comment", { exact: true })).toHaveCount(0);
    await expect(
      editor
        .getByTestId(`comment-thread-${onLine7}`)
        .getByTestId("comment-thread-anchor"),
    ).toHaveText("L7", { timeout: 30_000 });
  });

  test("threads list from the top in line order, and an expanded card brings its line to the top", async ({
    page,
  }) => {
    const id = workspace!.id;
    const detail = await authedApiViaRequest(page, `/api/v1/workspaces/${id}`);
    expect(detail.status).toBe(200);
    const workspaceDetail = detail.body?.data ?? detail.body;
    const codeBranch = workspaceDetail.branches.find(
      (branch: { repoId: string }) =>
        branch.repoId !== workspaceDetail.docsRepoId,
    );
    const repoId = String(codeBranch.repoId);
    const fileName = `e2e-comments-long-${Date.now()}.txt`;
    const content =
      Array.from({ length: 150 }, (_, index) => `row ${index + 1}`).join("\n") +
      "\n";
    const created = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/files/${fileName}?repoId=${repoId}`,
      "PUT",
      { content, encoding: "utf-8", expectedHash: null },
    );
    expect([200, 201], JSON.stringify(created.body)).toContain(created.status);

    await openWorkspacePath(page, workspace!.path);
    const editor = await openFile(page, repoId, fileName);
    const lines = editor.locator(".monaco-editor .view-lines").first();
    await expect(lines).toBeVisible({ timeout: 30_000 });
    await expect(editor).toContainText("row 1");

    // A thread on the last row, started first.
    await lines.click();
    await page.keyboard.press("ControlOrMeta+End");
    await page.keyboard.press("ArrowUp");
    const column = await showComments(editor);
    await column.locator("..").click({ button: "right" });
    await page.getByTestId("code-editor-context-add-comment-button").click();
    const posted = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        response.url().includes(`/api/v1/workspaces/${id}/comments`),
    );
    await editor
      .getByLabel("Comment", { exact: true })
      .fill("Is the last row needed?");
    await editor.getByTestId("draft-comment-save").click();
    const response = await posted;
    expect(response.status()).toBe(201);
    expect(response.request().postDataJSON()).toMatchObject({
      lineNumber: 150,
      lineHash: sha256("row 150"),
    });
    const farId = String((await response.json())?.data?.id ?? "");
    const farThread = editor.getByTestId(`comment-thread-${farId}`);
    await expect(farThread.getByTestId("comment-thread-anchor")).toHaveText(
      "L150",
    );

    // A thread on row 20, started second: the list puts it first, by line.
    await lines.click();
    await page.keyboard.press("ControlOrMeta+Home");
    for (let row = 1; row < 20; row += 1) {
      await page.keyboard.press("ArrowDown");
    }
    await column.locator("..").click({ button: "right" });
    await page.getByTestId("code-editor-context-add-comment-button").click();
    const postedEarly = page.waitForResponse(
      (early) =>
        early.request().method() === "POST" &&
        early.url().includes(`/api/v1/workspaces/${id}/comments`),
    );
    await editor.getByLabel("Comment", { exact: true }).fill("Why row 20?");
    await editor.getByTestId("draft-comment-save").click();
    const early = await postedEarly;
    expect(early.status()).toBe(201);
    expect(early.request().postDataJSON()).toMatchObject({ lineNumber: 20 });
    const earlyId = String((await early.json())?.data?.id ?? "");
    const earlyThread = editor.getByTestId(`comment-thread-${earlyId}`);
    await expect(column.getByTestId("comment-thread-anchor")).toHaveText([
      "L20",
      "L150",
    ]);

    // The list stays put while the text scrolls: both cards remain in view
    // from the top of the file and from its end.
    await lines.click();
    await page.keyboard.press("ControlOrMeta+Home");
    await expect(earlyThread).toBeInViewport();
    await expect(farThread).toBeInViewport();
    await page.keyboard.press("ControlOrMeta+End");
    await expect(earlyThread).toBeInViewport();
    await expect(farThread).toBeInViewport();

    // Expanding a card makes its row the first line of the text. Monaco
    // draws a space as U+00A0, and a pattern is tested against the text as
    // drawn, so `\s` stands for the space.
    const viewport = editor.locator(".monaco-editor .overflow-guard").first();
    const rowAtTop = async (row: number) => {
      const line = editor
        .locator(".monaco-editor .view-line")
        .filter({ hasText: new RegExp(`^row\\s${row}$`) });
      await expect(line).toBeInViewport();
      const [lineBox, viewportBox] = await Promise.all([
        line.boundingBox(),
        viewport.boundingBox(),
      ]);
      expect(lineBox && viewportBox).toBeTruthy();
      expect(Math.abs(lineBox!.y - viewportBox!.y)).toBeLessThanOrEqual(24);
    };
    await earlyThread.getByTestId("comment-thread-toggle-expand").click();
    await rowAtTop(20);
    await expect(
      editor
        .locator(".monaco-editor .view-line")
        .filter({ hasText: /^row\s150$/ }),
    ).not.toBeInViewport();
    // The last row cannot be first (the text does not scroll past its end):
    // it comes into view and row 20 leaves.
    await farThread.getByTestId("comment-thread-toggle-expand").click();
    await expect(
      editor
        .locator(".monaco-editor .view-line")
        .filter({ hasText: /^row\s150$/ }),
    ).toBeInViewport();
    await expect(
      editor
        .locator(".monaco-editor .view-line")
        .filter({ hasText: /^row\s20$/ }),
    ).not.toBeInViewport();
  });
});
