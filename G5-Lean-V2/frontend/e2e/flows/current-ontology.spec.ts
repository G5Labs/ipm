import { randomUUID } from "node:crypto";
import { expect, test } from "@playwright/test";
import {
  activateSidebarSection,
  expandRepositoryFileRoot,
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandOntologyTree,
  openWorkspacePath,
  openFileSource,
  openFilePreview,
  replaceFileSource,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
} from "../helpers";

test.describe.configure({ timeout: 600_000 });

// Real HTTP, running workspace, Monaco and persisted file bytes. No routes
// are intercepted; the checkout owns discovery and Save.
test("current ontology discovers live Markdown, preserves identity through editing, and navigates references", async ({
  page,
}) => {
  const workspace = await createWorkspaceClone(page, {
    namePrefix: "current-ontology",
  });
  try {
    await runWorkspaceVmAction(page, workspace.id, "resume");
    const vm = await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detail.status).toBe(200);
    const data = detail.body?.data ?? detail.body;
    const repoId = data.branches.find(
      (b: { repoId: string }) => b.repoId !== data.docsRepoId,
    )?.repoId;
    expect(repoId).toBeTruthy();
    const stem = `browser-${randomUUID()}`;
    const path = `.g5/ontology/${stem}.g5.md`;
    const source = `# Browser document\n\n[Reference](#authored-hidden-heading) [Next](next-${stem}.g5.md#target-heading) [Missing](gone-${stem}.g5.md#missing)\n\n## Authored hidden heading\n\n[I · browser.intent]\n\nOriginal browser content.\n\n## Metadata\n\n### browser.intent\n- source: human\n`;
    const fileUrl = `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`;
    const write = await authedApiViaRequest(page, fileUrl, "PUT", {
      content: source,
      encoding: "utf-8",
      expectedHash: null,
      expectedVmId: vm.vmId,
    });
    expect(write.status, JSON.stringify(write.body)).toBe(200);
    await openWorkspacePath(page, workspace.path);
    const tree = await expandOntologyTree(page);
    await expect(
      tree.getByText(/Source unavailable|Could not read/),
    ).toHaveCount(0);
    // File labels come from stems; no YAML/component identity is required.
    await tree.getByText(stem, { exact: true }).click();
    const editor = page.getByTestId("file-view");
    await expect(editor.getByTestId("semantic-record-id")).toHaveText(
      "browser.intent",
    );
    await expect(editor.getByTestId("semantic-record-id")).toBeHidden();
    await expect(
      editor.getByTestId("semantic-record-parent-control"),
    ).toHaveCount(0);
    await expect(
      editor.getByTestId("semantic-record-child-control"),
    ).toHaveCount(0);
    await editor.getByTestId("semantic-record-details-toggle").click();
    await expect(editor.getByTestId("semantic-record-id")).toBeVisible();
    await editor.getByTestId("semantic-record-details-toggle").click();
    await expect(editor.getByTestId("semantic-record-title")).toHaveCount(0);
    await expect(
      editor.getByText("Authored hidden heading", { exact: true }),
    ).toHaveCount(0);
    await replaceFileSource(
      page,
      source.replace(
        "Original browser content.",
        "Edited: Original browser content.",
      ),
    );
    await openFilePreview(page);
    // Files and Core Ontology share one repository/path tab and draft.
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
    await files.getByText(`${stem}.g5.md`, { exact: true }).click();
    await expect(page.getByTestId("file-view")).toBeVisible();
    await expect(page.getByTestId("file-view")).toContainText(
      "Edited: Original browser content.",
    );
    await expandOntologyTree(page);
    await tree.getByText(stem, { exact: true }).click();
    await expect(editor).toContainText("Edited: Original browser content.");
    const saveResponse = page.waitForResponse(
      (response) =>
        response.request().method() === "PUT" &&
        response.url().includes("/files/.g5/ontology/"),
    );
    await editor.getByTestId("code-editor-save-button").click();
    expect((await saveResponse).status()).toBe(200);
    // Settled: nothing is left unsaved. Saved from the reading view after
    // Done, the Save button itself goes with the draft.
    await expect(editor.getByTestId("code-editor-dirty-indicator")).toHaveCount(
      0,
    );
    const saved = await authedApiViaRequest(page, fileUrl);
    expect(saved.status).toBe(200);
    const savedText = (saved.body?.data ?? saved.body).content as string;
    expect(savedText).toContain("Edited: Original browser content.");
    expect(savedText).toContain(
      "## Authored hidden heading\n\n[I · browser.intent]",
    );
    expect(savedText.split("## Metadata")[1]).toBe(
      source.split("## Metadata")[1],
    );
    await editor.getByRole("link", { name: "Reference" }).click();
    const preview = page.getByTestId("ontology-reference");
    await expect(preview).toContainText("browser.intent");
    await preview.getByRole("button", { name: "Open document" }).click();
    await page.reload();
    await expect(page.getByTestId("semantic-record-id")).toHaveText(
      "browser.intent",
    );
    await expect(page.getByTestId("file-view")).toContainText(
      "Edited: Original browser content.",
    );
    // Comments and the agent composer retain the real repository/file identity.
    // Selecting the statement's text offers Comment
    // (`g5.documents.comments.identity`); a triple-click would also open Rich text.
    await page
      .locator('[data-record-span="browser.intent"] p')
      .first()
      .evaluate((element) => {
        const range = document.createRange();
        range.selectNodeContents(element);
        window.getSelection()?.removeAllRanges();
        window.getSelection()?.addRange(range);
        element.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
      });
    await page.getByTestId("document-preview-selection-comment-button").click();
    const commentSaved = page.waitForResponse(
      (r) =>
        r.request().method() === "POST" &&
        r.url().includes(`/workspaces/${workspace.id}/comments`),
    );
    await page
      .getByLabel("Comment", { exact: true })
      .fill("Ontology browser comment");
    await page.getByTestId("draft-comment-save").click();
    const comment = await commentSaved;
    expect(comment.status()).toBe(201);
    expect(comment.request().postDataJSON()).toMatchObject({
      repoId,
      filePath: path,
    });
    await expect(page.getByText(/Could not load comments/)).toHaveCount(0);
    await expandOntologyTree(page);
    await tree.getByText(stem, { exact: true }).hover();
    const fileRow = tree
      .locator(`[data-ontology-file]`)
      .filter({ hasText: stem })
      .filter({ has: page.getByTestId("ontology-file-explain-in-chat") })
      .last();
    await fileRow.getByTestId("ontology-file-explain-in-chat").click();
    await expect(page.getByTestId("chat-input")).toHaveValue(
      new RegExp(path.replaceAll(".", "\\.")),
    );

    await tree.getByText(stem, { exact: true }).click();
    await openFilePreview(page);

    // A newly authored file appears through the normal refresh without a job/reload.
    const nextPath = `.g5/ontology/next-${stem}.g5.md`;
    const nextSource =
      "# Target\n\n## Target heading\n\n[D · browser.target]\n\nTarget body.\n\n## Metadata\n\n### browser.target\n- source: agent\n";
    const nextUrl = `/api/v1/workspaces/${workspace.id}/files/${nextPath}?repoId=${repoId}`;
    const created = await authedApiViaRequest(page, nextUrl, "PUT", {
      content: nextSource,
      encoding: "utf-8",
      expectedHash: null,
      expectedVmId: vm.vmId,
    });
    expect(created.status).toBe(200);
    await expect(tree.getByText(`next-${stem}`, { exact: true })).toBeVisible({
      timeout: 30_000,
    });
    await editor.getByRole("link", { name: "Next", exact: true }).click();
    await expect(page.getByTestId("ontology-reference")).toContainText(
      "browser.target",
    );
    await page.keyboard.press("Escape");
    await editor.getByRole("link", { name: "Missing", exact: true }).click();
    await expect(
      page.getByTestId("ontology-reference").getByRole("alert"),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    // A move has a new path identity. The old tab/link fails rather than relocating.
    await editor.getByRole("link", { name: "Next", exact: true }).click();
    await page
      .getByTestId("ontology-reference")
      .getByRole("button", { name: "Open document" })
      .click();
    await expect(page.getByTestId("semantic-record-id")).toHaveText(
      "browser.target",
    );
    const movedPath = `.g5/ontology/moved-${stem}.g5.md`;
    const moved = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/files/rename`,
      "POST",
      {
        repoId,
        from: nextPath,
        to: movedPath,
        expectedHash: (created.body?.data ?? created.body).hash,
      },
    );
    expect(moved.status, JSON.stringify(moved.body)).toBe(200);
    await expect(tree.getByText(`moved-${stem}`, { exact: true })).toBeVisible({
      timeout: 30_000,
    });
    await expect(tree.getByText(`next-${stem}`, { exact: true })).toHaveCount(
      0,
    );
    await expect(
      page.getByText(
        "This file is unavailable in the current authorized source. It may have been deleted.",
        { exact: true },
      ),
    ).toBeVisible({
      timeout: 30_000,
    });
    await tree.getByText(stem, { exact: true }).click();
    await editor.getByRole("link", { name: "Next", exact: true }).click();
    await expect(
      page.getByTestId("ontology-reference").getByRole("alert"),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    const current = await authedApiViaRequest(page, fileUrl);
    const currentFile = current.body?.data ?? current.body;
    const updatedLinks = await authedApiViaRequest(page, fileUrl, "PUT", {
      content: currentFile.content.replace(
        `next-${stem}.g5.md`,
        `moved-${stem}.g5.md`,
      ),
      encoding: "utf-8",
      expectedHash: currentFile.hash,
      expectedVmId: vm.vmId,
    });
    expect(updatedLinks.status).toBe(200);
    await expect(
      editor.getByRole("link", { name: "Next", exact: true }),
    ).toHaveAttribute("href", `moved-${stem}.g5.md#target-heading`);
    await editor.getByRole("link", { name: "Next", exact: true }).click();
    await expect(page.getByTestId("ontology-reference")).toContainText(
      "browser.target",
    );
    await page.keyboard.press("Escape");
    await replaceFileSource(page, "");
    await openFilePreview(page);
    await expect(page.getByTestId("semantic-record")).toHaveCount(0);
    const native = await openFileSource(page);
    await native.locator(".monaco-editor .view-lines").click();
    await page.keyboard.press("ControlOrMeta+z");
    await openFilePreview(page);
    await expect(page.getByTestId("semantic-record-id")).toHaveText(
      "browser.intent",
    );
  } finally {
    await deleteWorkspaceById(page, workspace.id);
  }
});
