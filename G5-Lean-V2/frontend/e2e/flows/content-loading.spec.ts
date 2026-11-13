import { test, expect, type Locator, type Page } from "@playwright/test";
import {
  activateSidebarSection,
  authedApiViaRequest,
  clickSidebarSectionAction,
  createWorkspaceClone,
  deleteWorkspaceById,
  expandProjectDocs,
  expandRepositoryFileRoot,
  navigateToWorkspace,
  openFileSource,
  TEST_ORG,
  softVisible,
  type WorkspaceClone,
} from "../helpers";

function looksLikeLoadedContent(content: string | null): boolean {
  const normalized = (content ?? "").trim().toLowerCase();
  return (
    normalized.length > 0 &&
    !normalized.includes("loading...") &&
    !normalized.includes("placeholder") &&
    !normalized.includes("todo: wire")
  );
}

async function waitForRealEditorContent(
  page: Page,
  editorSelector = '[data-testid="file-view"], [data-testid="code-editor"], [data-testid="markdown-renderer"]',
): Promise<boolean> {
  const editor = page
    .getByTestId("center-panel")
    .locator(editorSelector)
    .first();
  if (!(await softVisible(editor, 3_000))) return false;

  const deadline = Date.now() + 8_000;
  while (Date.now() < deadline) {
    if (looksLikeLoadedContent(await editor.textContent())) return true;
    await page.waitForTimeout(200);
  }

  return looksLikeLoadedContent(await editor.textContent());
}

async function expectNoRetiredDiffControl(editor: Locator): Promise<void> {
  const retiredControl = editor
    .getByRole("button", { name: /diff|compare/i })
    .and(editor.locator('button:not([data-testid="document-preview-compare"])'))
    .or(editor.getByTestId("diff-toggle"));
  await expect(retiredControl).toHaveCount(0);
}

async function createProjectDocumentEditor(page: Page): Promise<Locator> {
  const docName = `content-loading-${Date.now()}.md`;
  const docsSection = await expandProjectDocs(page);
  await clickSidebarSectionAction(
    page,
    "sidebar-docs-tab",
    "Create project doc",
  );

  const modal = page
    .getByRole("dialog")
    .or(page.locator('[data-testid="create-project-doc-modal"]'))
    .first();
  await expect(modal).toBeVisible({ timeout: 5_000 });

  await modal
    .getByRole("button", { name: /plain doc/i })
    .first()
    .click();

  const fileNameInput = modal
    .getByRole("textbox")
    .or(modal.locator('input[type="text"]'))
    .first();
  await fileNameInput.fill(docName);

  await modal
    .getByRole("button", { name: /create/i })
    .last()
    .click();
  await expect(modal).not.toBeVisible({ timeout: 10_000 });

  const newDocItem = docsSection.getByText(docName, { exact: false }).first();
  await expect(newDocItem).toBeVisible({ timeout: 15_000 });
  await newDocItem.click();
  await expect(page.getByText("Preparing draft…").first()).not.toBeVisible({
    timeout: 15_000,
  });

  const editor = page.getByTestId("center-panel").getByTestId("file-view");
  await expect(editor).toBeVisible({ timeout: 15_000 });
  return editor;
}

async function openProjectDocumentEditor(page: Page): Promise<Locator> {
  const docsSection = await expandProjectDocs(page);
  const docRows = docsSection.getByTestId("doc-item");
  if (!(await softVisible(docRows.first(), 5_000))) {
    return createProjectDocumentEditor(page);
  }

  const count = await docRows.count();
  for (let index = 0; index < count; index++) {
    const row = docRows.nth(index);
    await row.click();
    const editor = page.getByTestId("center-panel").getByTestId("file-view");
    if (
      (await softVisible(editor, 2_000)) &&
      (await waitForRealEditorContent(page, '[data-testid="file-view"]'))
    ) {
      return editor;
    }
  }

  return createProjectDocumentEditor(page);
}

async function openRawCodeEditor(page: Page): Promise<Locator> {
  await activateSidebarSection(page, "sidebar-files-tab");
  await expandRepositoryFileRoot(page);
  const fileRows = page
    .getByTestId("sidebar-files-tab")
    .getByTestId("file-tree-entry");
  await expect(
    fileRows.first(),
    "Expected at least one repository file in the seeded workspace",
  ).toBeVisible({ timeout: 15_000 });

  const count = await fileRows.count();
  for (let index = 0; index < Math.min(count, 20); index++) {
    const row = fileRows.nth(index);
    await row.click();
    const editor = page.getByTestId("center-panel").getByTestId("code-editor");
    if (
      (await softVisible(editor, 2_000)) &&
      (await waitForRealEditorContent(page, '[data-testid="code-editor"]'))
    ) {
      await expect(row).toHaveAttribute("aria-selected", "true");
      return editor;
    }
  }

  throw new Error("Expected a raw repository file to open in CodeEditor");
}

async function assertEditorsOmitRetiredDiffControls({
  page,
  workspace,
}: {
  page: Page;
  workspace: WorkspaceClone;
}): Promise<void> {
  await navigateToWorkspace(page, TEST_ORG, workspace.name);

  await expectNoRetiredDiffControl(await openProjectDocumentEditor(page));
  await expectNoRetiredDiffControl(await openRawCodeEditor(page));
}

test.describe("Content Loading", () => {
  let workspace: WorkspaceClone;
  let retiredCalls: string[];

  test.beforeEach(async ({ page }, testInfo) => {
    retiredCalls = [];
    page.on("request", (request) => {
      if (
        /\/api\/v1\/workspaces\/[^/]+\/ontology(?:\/|\?|$)|\/docs\/sync-committed|\/comments\?[^#]*nodeId=/.test(
          request.url(),
        )
      )
        retiredCalls.push(request.url());
    });
    workspace = await createWorkspaceClone(page, {
      namePrefix: `content-loading-${testInfo.title}`,
      start: true,
    });
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, workspace?.id);
    expect(retiredCalls).toEqual([]);
  });

  test("opening a Project Doc loads editable content without node APIs", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, workspace.name);

    await openProjectDocumentEditor(page);

    // Center panel should show real content (not 'Loading...' placeholder)
    const centerPanel = page.getByTestId("center-panel");
    await expect(centerPanel).toBeVisible();

    // Wait for content to fully load
    const editor = centerPanel
      .locator(
        '[data-testid="file-view"], [data-testid="code-editor"], [data-testid="markdown-renderer"]',
      )
      .first();
    await expect(editor).toBeVisible({ timeout: 10_000 });

    // Content should NOT be a hardcoded placeholder
    const content = await editor.textContent();
    expect(looksLikeLoadedContent(content)).toBe(true);
    const source = await openFileSource(page);
    await source.locator(".monaco-editor .view-lines").click();
    await page.keyboard.press("ControlOrMeta+End");
    await page.keyboard.insertText(" retained document draft");
    await expect(source.getByTestId("code-editor-save-button")).toBeEnabled();
  });

  test("opening file tab loads real file content", async ({ page }) => {
    await navigateToWorkspace(page, TEST_ORG, workspace.name);

    const editor = await openRawCodeEditor(page);
    await expect(editor).not.toContainText(/loading(\.\.\.|…)/i, {
      timeout: 15_000,
    });
  });

  test("document and raw-code editors omit retired per-editor diff controls", async ({
    page,
  }) => assertEditorsOmitRetiredDiffControls({ page, workspace }));

  test("current ontology remains visible and readable in Files", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, workspace.name);
    await activateSidebarSection(page, "sidebar-files-tab");
    await expandRepositoryFileRoot(page);
    const files = page.getByTestId("sidebar-files-tab");
    for (const folder of [".g5", "ontology"]) {
      const row = files
        .getByRole("treeitem")
        .filter({ has: page.getByText(folder, { exact: true }) });
      await expect(row).toBeVisible();
      if ((await row.getAttribute("aria-expanded")) === "false")
        await row.click();
      await expect(row).toHaveAttribute("aria-expanded", "true");
    }
    await files.getByText("root.g5.md", { exact: true }).click();
    await expect(page.getByTestId("center-panel")).toContainText(
      "Evaluate arithmetic expressions with deterministic results.",
    );
  });
});
