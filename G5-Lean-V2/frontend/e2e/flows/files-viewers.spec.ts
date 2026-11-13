import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";

import {
  authedApiViaRequest,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandRepositoryFileRoot,
  openWorkspacePath,
  readWorkspaceVm,
  runWorkspaceVmAction,
  waitForWorkspaceVmStatus,
  type WorkspaceClone,
} from "../helpers";

const fixtures = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "fixtures",
  "viewers",
);

/**
 * FILES opens images and PDFs in read-only viewers in the file tab
 * (`g5.documents.viewers`): the image through the browser's decoder
 * (`g5.documents.viewers.images`), the PDF through PDF.js with its worker
 * and assets served by the edge (`g5.documents.viewers.pdf`). The files are
 * written through the authorized file API, so the viewers read what the
 * text editor reads.
 */
test.describe("FILES image and PDF viewers", () => {
  test.describe.configure({ timeout: 360_000 });
  let workspace: WorkspaceClone | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    workspace = await createWorkspaceClone(page, {
      namePrefix: `files-viewers-${testInfo.title}`,
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

  /** Writes, commits and pushes a binary file from the fixtures. */
  async function commitFixture(
    page: Page,
    id: string,
    repoId: string,
    filePath: string,
    fixture: string,
  ): Promise<void> {
    const vm = await readWorkspaceVm(page, id);
    const written = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${id}/files/${filePath}?repoId=${repoId}`,
      "PUT",
      {
        content: readFileSync(path.join(fixtures, fixture)).toString("base64"),
        encoding: "base64",
        expectedHash: null,
        expectedVmId: vm.vmId,
        commitAndPush: true,
      },
    );
    expect([200, 201], JSON.stringify(written.body)).toContain(written.status);
  }

  async function clickFile(page: Page, repoId: string, fileName: string) {
    await expandRepositoryFileRoot(page, repoId);
    const entry = page
      .getByTestId("sidebar-files-tab")
      .getByTestId("file-tree-entry")
      .filter({ hasText: fileName })
      .first();
    await expect(entry).toBeVisible({ timeout: 30_000 });
    await entry.click();
  }

  test("clicking a PNG and a PDF in FILES opens their viewers", async ({
    page,
  }) => {
    const id = workspace!.id;
    const repoId = await codeRepoId(page, id);
    const stamp = Date.now();
    const pngName = `e2e-viewer-${stamp}.png`;
    const pdfName = `e2e-viewer-${stamp}.pdf`;
    await commitFixture(page, id, repoId, pngName, "transparent.png");
    await commitFixture(page, id, repoId, pdfName, "text.pdf");
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));

    // The clone was created through the API; open its page before FILES.
    await openWorkspacePath(page, workspace!.path);
    await clickFile(page, repoId, pngName);
    const imageView = page
      .getByTestId("center-panel")
      .locator(`[data-testid="image-view"][data-file-path="${pngName}"]`);
    await expect(imageView).toBeVisible({ timeout: 30_000 });
    await expect(imageView.getByTestId("image-view-image")).toHaveAttribute(
      "data-status",
      "loaded",
      { timeout: 30_000 },
    );
    await expect(imageView.getByTestId("image-view-dimensions")).toHaveText(
      /64 × 48/,
    );
    await expect(page.getByTestId("file-view-edit")).toHaveCount(0);
    await expect(page.getByTestId("file-view-unavailable")).toHaveCount(0);

    await clickFile(page, repoId, pdfName);
    const pdfView = page
      .getByTestId("center-panel")
      .locator(`[data-testid="pdf-view"][data-file-path="${pdfName}"]`);
    await expect(pdfView).toBeVisible({ timeout: 30_000 });
    // The worker, character maps and standard fonts come from the edge.
    await expect(pdfView.getByTestId("pdf-view-page-count")).toHaveText("/ 3", {
      timeout: 60_000,
    });
    const first = pdfView.locator(
      '[data-testid="pdf-page"][data-page-number="1"]',
    );
    await expect(first).toHaveAttribute("data-rendered", "true", {
      timeout: 60_000,
    });
    await expect(first.locator(".textLayer span").first()).toHaveText(
      "Hello G5 page 1 of 3",
    );
    await pdfView.getByTestId("pdf-view-search").fill("quarterly");
    await expect(pdfView.getByTestId("pdf-view-search-count")).toHaveText(
      "1 of 3",
      { timeout: 30_000 },
    );
    expect(errors).toEqual([]);
  });
});
