import { expect, test } from "@playwright/test";
import {
  activateSidebarSection,
  authedApiViaRequest,
  cloneWorkspaceById,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandOntologyTree,
  expandRepositoryFileRoot,
  openWorkspacePath,
  openFileSource,
  openFilePreview,
  replaceFileSource,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  type WorkspaceClone,
} from "../helpers";

test("committed ontology supports Core, Compare, All Changes and Files without starting a workspace", async ({
  page,
}) => {
  const workspace = await createWorkspaceClone(page, {
    namePrefix: "ontology-committed",
  });
  try {
    await openWorkspacePath(page, workspace.path);
    const tree = await expandOntologyTree(page);
    await tree.getByText("root", { exact: true }).click();
    const editor = page.getByTestId("file-view");
    await expect(editor).toContainText(
      "Evaluate arithmetic expressions with deterministic results.",
    );
    await expect(
      editor.getByRole("button", { name: "Compare", exact: true }),
    ).toBeVisible();
    // The shared browsing seed has bare I/D/Q records without
    // authored anchors. Compare must explain that limitation, not claim zero.
    await expect(
      editor.getByTestId("ontology-comparison-status"),
    ).toContainText("Statement is missing its authored anchor or heading");
    await page.reload();
    await expect(editor).toBeVisible();
    await (await expandOntologyTree(page))
      .getByRole("button", { name: "View all changes" })
      .click();
    const overview = page.getByTestId("ontology-changes-view");
    await expect(overview).toContainText("No statement changes.");
    await page.reload();
    await expect(overview).toContainText("No statement changes.");
    await activateSidebarSection(page, "sidebar-files-tab");
    await expandRepositoryFileRoot(page);
    const files = page.getByTestId("sidebar-files-tab");
    for (const folder of [".g5", "ontology"]) {
      const row = files
        .getByRole("treeitem")
        .filter({ has: page.getByText(folder, { exact: true }) });
      if ((await row.getAttribute("aria-expanded")) === "false")
        await row.click();
    }
    await files.getByText("root.g5.md", { exact: true }).click();
    await expect(page.getByTestId("file-view")).toContainText(
      "Evaluate arithmetic expressions with deterministic results.",
    );
    await expect(page).not.toHaveURL(/editor=ontology/);
  } finally {
    await deleteWorkspaceById(page, workspace.id);
  }
});

test.describe(() => {
  test.describe.configure({ timeout: 600_000 });
  // Real API/Git/worker/file persistence, in disposable parent and child workspaces.
  test("statement comparison preserves editing, Save and Undo against the parent baseline", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    const parent = await createWorkspaceClone(page, {
      namePrefix: "comparison-parent",
    });
    let child: WorkspaceClone | undefined;
    const path = ".g5/ontology/comparison-browser.g5.md";
    const source = `# Comparison browser\n\nReading context remains visible.\n\n<a id="browser.a"></a>\n[I · browser.a]\n\nOriginal statement. [Self](#browser.a)\n\n<a id="browser.b"></a>\n[D · browser.b]\n\nUnchanged statement.\n\n## Metadata\n\n### browser.a\n\n- Original evidence. [Self](#browser.a)\n\n### browser.b\n\n- Other evidence\n`;
    try {
      await runWorkspaceVmAction(page, parent.id, "resume");
      const parentVm = await waitForWorkspaceVmStatus(
        page,
        parent.id,
        "running",
      );
      const detail = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${parent.id}`,
      );
      const data = detail.body?.data ?? detail.body;
      const repoId = data.branches.find(
        (b: { repoId: string }) => b.repoId !== data.docsRepoId,
      )?.repoId;
      expect(repoId).toBeTruthy();
      const created = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${parent.id}/files/${path}?repoId=${repoId}`,
        "PUT",
        {
          content: source,
          encoding: "utf-8",
          expectedHash: null,
          expectedVmId: parentVm.vmId,
          commitAndPush: true,
        },
      );
      expect(created.status, JSON.stringify(created.body)).toBe(200);
      // The baseline is committed. Release its runtime before starting the child;
      // comparison reads durable Git and does not require two live workspaces.
      await runWorkspaceVmAction(page, parent.id, "stop");
      await waitForWorkspaceVmStatus(page, parent.id, "stopped");
      child = await cloneWorkspaceById(page, parent.id, {
        namePrefix: "comparison-child",
        source: "durable",
      });
      // Retain the child identity for cleanup even if runtime startup fails.
      await runWorkspaceVmAction(page, child.id, "resume");
      await waitForWorkspaceVmStatus(page, child.id, "running");
      await openWorkspacePath(page, child.path);
      const tree = await expandOntologyTree(page);
      await tree.getByText("comparison-browser", { exact: true }).click();
      const editor = page.getByTestId("file-view");
      const toggle = editor.getByRole("button", {
        name: "Compare",
        exact: true,
      });
      await expect(toggle).toHaveAttribute("aria-pressed", "true");
      const status = editor.getByTestId("ontology-comparison-status");
      await expect(status).toContainText("0 statement changes");
      await expect(
        editor
          .getByTestId("file-view-preview")
          .getByText("Reading context remains visible."),
      ).toBeVisible();
      await replaceFileSource(
        page,
        source.replace("Original statement.", "Edited: Original statement."),
      );
      await openFilePreview(page);
      const before = editor.getByRole("region", {
        name: "Before statement browser.a",
        exact: true,
      });
      // The edit only adds text, so the pair hides Before until asked, with
      // one Show before / Hide before control on the Modified row
      // (g5.documents.ontology.change-highlights). A pair that kept the
      // reader's choice still shows Before; either way Hide before proves it
      // is the row the reader opened, not a whole-row fallback.
      const modified = editor.getByRole("region", {
        name: "Modified statement browser.a",
        exact: true,
      });
      const show = modified.getByRole("button", { name: /^Show before/ });
      const hide = modified.getByRole("button", { name: /^Hide before/ });
      const showBefore = async () => {
        await expect(show.or(hide)).toBeVisible();
        if (await show.isVisible()) await show.click();
        await expect(hide).toBeVisible();
        await expect(before).toBeVisible();
      };
      await showBefore();
      await expect(before).toContainText("Original statement.");
      await expect(before).not.toContainText("Edited:");
      await expect(
        before.locator('[contenteditable="true"], [data-block-index], a[href]'),
      ).toHaveCount(0);
      await before
        .getByRole("button", { name: "Statement details for browser.a" })
        .click();
      await expect(before).toContainText("Original evidence");
      const save = editor.getByTestId("code-editor-save-button");
      const savedResponse = page.waitForResponse(
        (r) =>
          r.request().method() === "PUT" &&
          r.url().includes("/files/.g5/ontology/"),
      );
      await save.click();
      expect((await savedResponse).status()).toBe(200);
      // Settled: nothing is left unsaved. Saved from the reading view after
      // Done, the Save button itself goes with the draft.
      await expect(
        editor.getByTestId("code-editor-dirty-indicator"),
      ).toHaveCount(0);
      const fileUrl = `/api/v1/workspaces/${child.id}/files/${path}?repoId=${repoId}`;
      const saved = await authedApiViaRequest(page, fileUrl);
      const savedText = (saved.body?.data ?? saved.body).content as string;
      expect(savedText).toContain("Edited:");
      expect(savedText.match(/Original statement\./g)).toHaveLength(1);
      expect(savedText).not.toContain("Before");
      // The overview has its own reloadable route and opens this exact file.
      await tree.getByRole("button", { name: "View all changes" }).click();
      const overview = page.getByTestId("ontology-changes-view");
      await expect(overview.getByLabel(path, { exact: true })).toContainText(
        "Edited:",
      );
      await page.reload();
      await expect(overview).toBeVisible();
      await overview
        .getByLabel(path, { exact: true })
        .getByTestId("ontology-changes-open-file")
        .click();
      await expect(editor).toBeVisible();
      await expect(page).toHaveURL(
        (url) =>
          decodeURIComponent(url.pathname).endsWith(`/file/${path}`) &&
          url.searchParams.get("branch") === repoId &&
          !url.searchParams.has("editor"),
      );
      await page.reload();
      await (await expandOntologyTree(page))
        .getByText("comparison-browser", { exact: true })
        .click();
      await expect(toggle).toHaveAttribute("aria-pressed", "true");
      await showBefore();
      await expect(before).toContainText("Original statement.");
      // Reading renders Markdown; authored metadata links still navigate normally.
      await editor
        .locator('[data-record-span="browser.a"]')
        .getByRole("button", { name: "Statement details for browser.a" })
        .click();
      await editor
        .locator('[data-record-span="browser.a"]')
        .getByTestId("semantic-record-metadata")
        .getByRole("link", { name: "Self" })
        .click();
      await expect(page.getByTestId("ontology-reference")).toContainText(
        "Edited:",
      );
      await page.getByRole("button", { name: "Close reference" }).click();
      await replaceFileSource(page, "");
      await openFilePreview(page);
      await expect(
        editor.getByRole("region", {
          name: "Deleted statement browser.a",
          exact: true,
        }),
      ).toBeVisible();
      const native = await openFileSource(page);
      await native.locator(".monaco-editor .view-lines").click();
      await page.keyboard.press("ControlOrMeta+z");
      await openFilePreview(page);
      await showBefore();
      await expect(
        editor.locator('[data-record-span="browser.a"]'),
      ).toContainText("Edited:");
      await toggle.click();
      await expect(before).toHaveCount(0);
      await toggle.click();
      await showBefore();
      await page.screenshot({
        path: "/tmp/g5-statement-comparison.png",
        fullPage: false,
      });
      // The current statement's contextual comment retains its repository/file identity.
      // Selecting the statement's text offers Comment
      // (`g5.documents.comments.identity`); a triple-click would also open Rich text.
      await page
        .locator('[data-record-span="browser.a"] p')
        .first()
        .evaluate((element) => {
          const range = document.createRange();
          range.selectNodeContents(element);
          window.getSelection()?.removeAllRanges();
          window.getSelection()?.addRange(range);
          element.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
        });
      await page
        .getByTestId("document-preview-selection-comment-button")
        .click();
      await page
        .getByLabel("Comment", { exact: true })
        .fill("Comparison current statement check");
      const commentResponse = page.waitForResponse(
        (r) =>
          r.request().method() === "POST" &&
          r.url().includes(`/workspaces/${child!.id}/comments`),
      );
      await page.getByTestId("draft-comment-save").click();
      const comment = await commentResponse;
      expect(comment.status()).toBe(201);
      expect(comment.request().postDataJSON()).toMatchObject({
        repoId,
        filePath: path,
      });
      // A competing agent-style file write cannot replace the browser draft.
      await replaceFileSource(
        page,
        savedText.replace(
          "Unchanged statement.",
          "Unchanged statement. Human draft.",
        ),
      );
      await openFilePreview(page);
      const latest = await authedApiViaRequest(page, fileUrl);
      const currentFile = latest.body?.data ?? latest.body;
      const agentWrite = await authedApiViaRequest(page, fileUrl, "PUT", {
        content: currentFile.content.replace(
          "Unchanged statement.",
          "Agent changed statement.",
        ),
        encoding: "utf-8",
        expectedHash: currentFile.hash,
      });
      expect(agentWrite.status, JSON.stringify(agentWrite.body)).toBe(200);
      await expect(
        editor.locator('[data-record-span="browser.b"]'),
      ).toContainText("Human draft.");
      const conflictResponse = page.waitForResponse(
        (r) =>
          r.request().method() === "PUT" &&
          r.url().includes("/files/.g5/ontology/"),
      );
      await save.click();
      expect((await conflictResponse).status()).toBe(409);
      await expect(editor).toContainText("Human draft.");
      await expect(
        editor.getByTestId("code-editor-conflict-dialog"),
      ).toBeVisible();
      await editor.getByTestId("code-editor-conflict-reload").click();
      await expect(editor).toContainText("Agent changed statement.");
      await expect(editor).not.toContainText("Human draft.");
      // The reloaded file is still compared; whether Before shows depends on
      // whether the pair remounted, which this journey does not decide.
      await expect(
        editor.getByRole("region", {
          name: "Modified statement browser.a",
          exact: true,
        }),
      ).toBeVisible();
    } finally {
      if (child) await deleteWorkspaceById(page, child.id);
      await deleteWorkspaceById(page, parent.id);
    }
  });
});
