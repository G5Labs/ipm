import { expect, test } from "@playwright/test";
import {
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandProjectDocs,
  expandRepositoryFileRoot,
  openFilePreview,
  openFileSource,
  openWorkspacePath,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
} from "../helpers";

test.describe.configure({ timeout: 360_000 });

// Real application routes, upload, Monaco and workspace persistence. The
// component browser regression runs the same editor without a worker stack.
test("uploaded empty and legacy Markdown stay editable through Project Docs, Files and deep links", async ({
  page,
}) => {
  const workspace = await createWorkspaceClone(page, {
    namePrefix: "project-doc-editor",
  });
  const obsolete: string[] = [];
  page.on("request", (request) => {
    if (/\/grounding\/review-(metadata|actions)/.test(request.url()))
      obsolete.push(request.url());
  });
  try {
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    const detail = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}`,
    );
    expect(detail.status).toBe(200);
    const repoId = String((detail.body?.data ?? detail.body).docsRepoId);
    await openWorkspacePath(page, workspace.path);

    for (const [name, initial] of [
      ["empty.md", ""],
      ["request.md", "[I1. Uploaded intent]\r\nKeep café 中文 😀.\r\n"],
      ["legacy.semlift.md", "[D1. Historical document]\nKeep these bytes.\n"],
    ]) {
      const path = `docs/${workspace.id}/${name}`;
      await page.getByTestId("ribbon-action-import").click();
      await page.getByTestId("ribbon-action-upload-file").click();
      await page.getByLabel("Upload a doc", { exact: true }).setInputFiles({
        name,
        mimeType: "text/markdown",
        buffer: Buffer.from(initial),
      });
      await page.getByTestId("create-doc-submit").click();
      const view = page.getByTestId("center-panel").getByTestId("file-view");
      await expect(view).toHaveAttribute("data-file-path", path);
      await expect(view.getByTestId("document-preview")).toBeVisible();
      const expectFilters = async () => {
        for (const id of ["toolbar-tag-human", "toolbar-tag-uncertainty"])
          await expect(view.getByTestId(id)).toHaveCount(initial ? 1 : 0);
      };
      await expectFilters();
      await expect(view.getByText(/Review metadata unavailable/)).toHaveCount(
        0,
      );
      await expect(view.getByTestId("toolbar-tag-unimplemented")).toHaveCount(
        0,
      );

      const docs = await expandProjectDocs(page);
      await docs.getByText(name, { exact: true }).click();
      await expect(view).toHaveAttribute("data-file-path", path);
      await expectFilters();
      await openFileSource(page);
      await page.keyboard.press("ControlOrMeta+End");
      const addition =
        "Typed **content** with  spaces and Unicode: café 中文 😀.";
      await page.keyboard.insertText(addition);
      await openFilePreview(page);
      await expect(view.getByTestId("document-preview")).toContainText(
        "Typed content",
      );
      await expectFilters();
      await view.getByTestId("code-editor-save-button").click();
      const read = () =>
        authedApiViaRequest(
          page,
          `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
        );
      await expect
        .poll(async () => {
          const response = await read();
          expect(response.status).toBe(200);
          return (response.body?.data ?? response.body).content;
        })
        .toBe(initial + addition);

      await expandRepositoryFileRoot(page, repoId);
      const files = page.getByTestId("sidebar-files-tab");
      for (const folder of ["docs", workspace.name]) {
        const row = files
          .locator('[role="treeitem"]:not([data-g5-node-id])')
          .filter({ has: page.getByText(folder, { exact: true }) });
        if ((await row.getAttribute("aria-expanded")) === "false")
          await row.click();
      }
      await files.getByText(name, { exact: true }).click();
      await expect(view).toHaveAttribute("data-file-path", path);
      await expect(view.getByTestId("document-preview")).toContainText(
        "Typed content",
      );
      await expectFilters();
      const deepLink = page.url();
      await page.reload();
      await expect(page).toHaveURL(deepLink);
      await expect(view.getByTestId("document-preview")).toContainText(
        "Typed content",
      );
      await expectFilters();
      const reloaded = await read();
      expect((reloaded.body?.data ?? reloaded.body).content).toBe(
        initial + addition,
      );
    }
    expect(obsolete).toEqual([]);
  } finally {
    await deleteWorkspaceById(page, workspace.id).catch(() => {});
  }
});
