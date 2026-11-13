import { test, expect, type Page } from "@playwright/test";
import {
  authedApi,
  clickSidebarSectionAction,
  createWorkspace,
  deleteWorkspaceById,
  navigateToWorkspace,
  softVisible,
  TEST_ORG,
} from "../helpers";


async function expandProjectDocs(page: Page) {
  const docsSection = page.getByTestId("sidebar-docs-tab");
  await expect(docsSection).toBeVisible({ timeout: 10_000 });
  const docsHeader = docsSection.getByRole("button", {
    name: /^(project docs|docs)$/i,
  });
  if (await softVisible(docsHeader, 3_000)) {
    const expanded = await docsHeader.getAttribute("aria-expanded");
    if (expanded === "false") {
      await docsHeader.click();
    }
  }
  return docsSection;
}

test("isolated workspace can create its first project doc", async ({
  page,
}) => {
  const workspace = await createWorkspace(page, {
    orgSlug: TEST_ORG,
    namePrefix: "first-project-doc",
    description: "Isolated workspace for first project-doc coverage",
  });
  const workspaceId = workspace.id;
  const docName = `member-base-doc-${Date.now()}`;
  let docId = "";

  try {
    await navigateToWorkspace(page, TEST_ORG, workspace.name);

    const docsSection = await expandProjectDocs(page);
    await expect(docsSection.getByText(/no project docs yet/i)).toBeVisible({
      timeout: 10_000,
    });
    await clickSidebarSectionAction(
      page,
      "sidebar-docs-tab",
      "Create project doc",
    );
    const modal = page.getByRole("dialog");
    await expect(modal).toBeVisible({ timeout: 5_000 });

    await modal
      .getByRole("button", { name: /plain doc/i })
      .first()
      .click();
    await modal
      .getByRole("textbox")
      .or(modal.locator('input[type="text"]'))
      .first()
      .fill(docName);

    await modal.getByRole("button", { name: /^create doc$/i }).click();
    await expect(modal).not.toBeVisible({ timeout: 15_000 });

    const createdDoc = docsSection.getByText(`${docName}.md`).first();
    await expect(createdDoc).toBeVisible({ timeout: 20_000 });

    const listResult = await authedApi(
      page,
      `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
    );
    expect(listResult.status).toBe(200);
    const listData = listResult.body?.data ?? listResult.body ?? {};
    const docs = Array.isArray(listData)
      ? listData
      : Array.isArray(listData.docs)
        ? listData.docs
        : [];
    const expectedPath = `docs/${workspaceId}/${docName}.md`;
    const doc = docs.find((item: Record<string, unknown>) => {
      return item.filePath === expectedPath;
    });
    expect(doc).toBeTruthy();
    docId = String(doc.id ?? "");
    expect(docId).toMatch(/^overlay:/);
  } finally {
    try {
      if (docId) {
        const deleteResult = await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs/project-docs/${docId}`,
          "DELETE",
        );
        expect([204, 404]).toContain(deleteResult.status);
      }
    } finally {
      await deleteWorkspaceById(page, workspaceId).catch(() => {});
    }
  }
});
