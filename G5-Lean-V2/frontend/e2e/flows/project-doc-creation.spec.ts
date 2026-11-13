import {
  test,
  expect,
  type Locator,
  type Page,
  type Request,
} from "@playwright/test";
import {
  TEST_ORG,
  activateSidebarSection,
  authedApi,
  clickSidebarSectionAction,
  createWorkspaceClone,
  deleteWorkspaceById,
  navigateToWorkspace,
  openFileSource,
  openFilePreview,
  replaceFileSource,
  showWorkspaceTabs,
  softVisible,
  type WorkspaceClone,
} from "../helpers";
import { readAppliedDiff } from "../source-control";

function docsFromResponse(body: any): Array<Record<string, any>> {
  const data = body?.data ?? body ?? {};
  if (Array.isArray(data)) return data;
  return Array.isArray(data.docs) ? data.docs : [];
}

async function expandProjectDocs(page: Page): Promise<Locator> {
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

async function dropTextFiles(
  page: Page,
  target: Locator,
  files: Array<{ name: string; mimeType: string; content: string }>,
): Promise<void> {
  const dataTransfer = await page.evaluateHandle((payloads) => {
    const transfer = new DataTransfer();
    for (const payload of payloads) {
      transfer.items.add(
        new File([payload.content], payload.name, { type: payload.mimeType }),
      );
    }
    return transfer;
  }, files);

  try {
    await target.dispatchEvent("dragenter", { dataTransfer });
    await target.dispatchEvent("dragover", { dataTransfer });
    await target.dispatchEvent("drop", { dataTransfer });
  } finally {
    await dataTransfer.dispose();
  }
}

async function createAndOpenProjectDoc(
  page: Page,
  testDocName: string,
): Promise<void> {
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
  await fileNameInput.fill(testDocName);

  await modal
    .getByRole("button", { name: /create/i })
    .last()
    .click();
  await expect(modal).not.toBeVisible({ timeout: 10_000 });

  const newDocItem = docsSection
    .getByText(new RegExp(testDocName, "i"))
    .first();
  await expect(newDocItem).toBeVisible({ timeout: 15_000 });
  await newDocItem.click();

  await expect(page.getByText("Preparing draft…").first()).not.toBeVisible({
    timeout: 15_000,
  });
  await waitForEditorDraftSync(page);
}

async function waitForEditorDraftSync(page: Page): Promise<void> {
  await expect(page.getByText("Syncing…").first()).not.toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByText("Syncing...").first()).not.toBeVisible({
    timeout: 30_000,
  });
}

async function captureFirstHtmlPreviewSrcdoc(
  page: Page,
  frameTitle: string,
): Promise<void> {
  await page.evaluate((title) => {
    const state = window as Window & {
      __g5FirstHtmlPreviewSrcdoc?: string | null;
    };
    state.__g5FirstHtmlPreviewSrcdoc = null;

    const matchesTargetFrame = (node: Node): node is HTMLIFrameElement =>
      node instanceof HTMLIFrameElement && node.getAttribute("title") === title;

    const capture = (frame: HTMLIFrameElement): boolean => {
      if (state.__g5FirstHtmlPreviewSrcdoc !== null) return true;
      state.__g5FirstHtmlPreviewSrcdoc = frame.getAttribute("srcdoc") ?? "";
      return true;
    };

    const existingFrame = Array.from(document.querySelectorAll("iframe")).find(
      matchesTargetFrame,
    );
    if (existingFrame) {
      capture(existingFrame);
      return;
    }

    const observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of Array.from(record.addedNodes)) {
          if (matchesTargetFrame(node)) {
            capture(node);
            observer.disconnect();
            return;
          }
          if (node instanceof Element) {
            const nestedFrame = Array.from(
              node.querySelectorAll("iframe"),
            ).find(matchesTargetFrame);
            if (nestedFrame) {
              capture(nestedFrame);
              observer.disconnect();
              return;
            }
          }
        }
      }
    });

    observer.observe(document.body, { childList: true, subtree: true });
  }, frameTitle);
}

async function readFirstHtmlPreviewSrcdoc(page: Page): Promise<string> {
  await page.waitForFunction(
    () =>
      typeof (window as Window & { __g5FirstHtmlPreviewSrcdoc?: unknown })
        .__g5FirstHtmlPreviewSrcdoc === "string",
    undefined,
    { timeout: 10_000 },
  );
  return page.evaluate(
    () =>
      (window as Window & { __g5FirstHtmlPreviewSrcdoc?: string })
        .__g5FirstHtmlPreviewSrcdoc ?? "",
  );
}

test.describe("Project Doc Creation", () => {
  let isolatedWorkspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    await showWorkspaceTabs(page);
    isolatedWorkspace = await createWorkspaceClone(page, {
      namePrefix: `project-doc-${testInfo.title}`,
      start: true,
    });
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, isolatedWorkspace?.id);
  });

  test("newly created project doc appears under PROJECT DOCS section", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);

    // 1. Expand the Project Docs section if collapsed
    const docsSection = page.getByTestId("sidebar-docs-tab");
    await expect(docsSection).toBeVisible({ timeout: 10_000 });

    // Ensure section is expanded by checking for the section header
    const docsHeader = docsSection.getByRole("button", {
      name: /^(project docs|docs)$/i,
    });
    if (await softVisible(docsHeader, 3_000)) {
      // Check aria-expanded; click to expand if collapsed
      const expanded = await docsHeader.getAttribute("aria-expanded");
      if (expanded === "false") {
        await docsHeader.click();
      }
    }

    // 2. Use the Project Docs contextual action to open the modal
    await clickSidebarSectionAction(
      page,
      "sidebar-docs-tab",
      "Create project doc",
    );

    // 3. The CreateProjectDocModal should be visible
    const modal = page
      .getByRole("dialog")
      .or(page.locator('[data-testid="create-project-doc-modal"]'))
      .first();
    await expect(modal).toBeVisible({ timeout: 5_000 });
    await expect(modal.getByText("Product Manager")).toHaveCount(0);

    // 4. Keep the Project Docs modal in its plain markdown document mode.
    const plainDocOption = modal
      .getByRole("button", { name: /plain doc/i })
      .first();
    await expect(plainDocOption).toBeVisible({ timeout: 3_000 });
    await plainDocOption.click();

    // 5. Enter file name
    const testDocName = `e2e-test-${Date.now()}`;
    const fileNameInput = modal
      .getByRole("textbox")
      .or(modal.locator('input[type="text"]'))
      .first();
    await expect(fileNameInput).toBeVisible({ timeout: 3_000 });
    await fileNameInput.fill(testDocName);

    // 6. Submit the form
    const submitBtn = modal.getByRole("button", { name: /create/i }).last();
    await expect(submitBtn).toBeVisible();
    await submitBtn.click();

    // 7. Modal should close
    await expect(modal).not.toBeVisible({ timeout: 10_000 });

    // 8. The new doc should appear under PROJECT DOCS section
    //    Wait for the doc list to refresh (query invalidation)
    const newDocItem = docsSection
      .getByText(new RegExp(testDocName, "i"))
      .first();
    await expect(newDocItem).toBeVisible({ timeout: 15_000 });

    const newDocRow = docsSection
      .getByTestId("doc-item")
      .filter({
        hasText: testDocName,
      })
      .first();
    await expect(newDocRow).toBeVisible();
    await expect(newDocRow.locator('[title^="Sync:"]')).toHaveCount(0);
    await expect(newDocRow.getByLabel("Source: User")).toBeVisible();

    // No source-category row above the list (owner instruction, 2026-10-30):
    // the row marker is the only source cue.
    await expect(
      docsSection.getByTestId("project-docs-source-filter"),
    ).toHaveCount(0);
  });

  test("create then delete refreshes the checklist only on Save changes and retains the applied snapshot", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);
    const docsSection = await expandProjectDocs(page);
    const testDocName = `e2e-delete-cancel-${Date.now()}`;

    await clickSidebarSectionAction(
      page,
      "sidebar-docs-tab",
      "Create project doc",
    );
    const modal = page.getByRole("dialog").first();
    await expect(modal).toBeVisible({ timeout: 5_000 });
    await modal
      .getByRole("button", { name: /plain doc/i })
      .first()
      .click();
    await modal
      .getByRole("textbox")
      .or(modal.locator('input[type="text"]'))
      .first()
      .fill(testDocName);
    await modal
      .getByRole("button", { name: /create/i })
      .last()
      .click();
    await expect(modal).not.toBeVisible({ timeout: 10_000 });

    const newDocRow = docsSection
      .getByTestId("doc-item")
      .filter({ hasText: testDocName })
      .first();
    await expect(newDocRow).toBeVisible({ timeout: 15_000 });
    await activateSidebarSection(page, "sidebar-source-control-tab");
    const checklist = page.getByTestId("source-control-menu");
    await expect(
      checklist.getByRole("checkbox", { name: /^Not yet pushed in / }).first(),
    ).toBeChecked();
    await checklist.getByRole("button", { name: "Show", exact: true }).click();
    const stream = page.getByTestId("source-control-diff-stream");
    await expect(stream).toContainText(testDocName);
    const applied = await readAppliedDiff(page);

    await activateSidebarSection(page, "sidebar-docs-tab");
    await newDocRow.click({ button: "right" });
    const rowMenu = page.getByRole("menu", { name: "Project Doc actions" });
    await expect(rowMenu).toBeVisible();
    await rowMenu.getByRole("menuitem", { name: "Delete" }).click();

    const confirmation = page.getByRole("dialog").first();
    await expect(
      confirmation.getByRole("heading", { name: "Delete project doc?" }),
    ).toBeVisible();
    await confirmation.getByTestId("project-doc-delete-confirm").click();

    await expect(newDocRow).not.toBeVisible({ timeout: 15_000 });
    await activateSidebarSection(page, "sidebar-source-control-tab");
    expect(await readAppliedDiff(page)).toBe(applied);
    await expect(
      checklist.getByRole("checkbox", { name: /^Not yet pushed in / }).first(),
    ).toBeChecked();
    const check = page.waitForResponse(
      (r) => r.url().endsWith("/source-control/check") && r.status() === 200,
    );
    // Save changes is an agent action, not a menu button; drive its API.
    await expect(
      checklist.getByRole("button", { name: "Save changes", exact: true }),
    ).toHaveCount(0);
    const saved = await authedApi(
      page,
      `/api/v1/workspaces/${isolatedWorkspace.id}/source-control/save`,
      "POST",
      {},
    );
    expect(saved.status, JSON.stringify(saved.body)).toBe(200);
    // Creating then deleting the draft leaves the pushed tips unchanged.
    expect((await (await check).json()).data.changed).toBe(false);
    const cleanRows = checklist.getByRole("checkbox", {
      name: /^Not yet pushed in /,
    });
    await expect(cleanRows).toHaveCount(2);
    for (const row of await cleanRows.all()) {
      await expect(row).toBeEnabled();
      await expect(row).toBeChecked();
    }
    await expect(
      checklist.getByRole("button", { name: "Show", exact: true }),
    ).toBeEnabled();
    expect(await readAppliedDiff(page)).toBe(applied);
    await checklist.getByRole("button", { name: "Show", exact: true }).click();
    await expect(stream).toContainText(
      "No net file changes in the selected comparison.",
    );
    await expect(stream).not.toContainText(testDocName);
  });

  test("created project doc can be opened and shows in center panel", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);

    // Expand Project Docs section
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

    // Create a doc
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

    const testDocName = `e2e-open-${Date.now()}`;
    const fileNameInput = modal
      .getByRole("textbox")
      .or(modal.locator('input[type="text"]'))
      .first();
    await fileNameInput.fill(testDocName);

    const submitBtn = modal.getByRole("button", { name: /create/i }).last();
    await submitBtn.click();
    await expect(modal).not.toBeVisible({ timeout: 10_000 });

    // Wait for the doc to appear
    const newDocItem = docsSection
      .getByText(new RegExp(testDocName, "i"))
      .first();
    await expect(newDocItem).toBeVisible({ timeout: 15_000 });

    // Click on the doc to open it
    await newDocItem.click();

    // The editor should not remain stuck in the draft bootstrap state.
    await expect(page.getByText("Preparing draft…").first()).not.toBeVisible({
      timeout: 15_000,
    });

    // The center panel should show the doc content area
    const centerPanel = page.getByTestId("center-panel");
    await expect(centerPanel).toBeVisible();

    // Opening from the repository-derived list must select the canonical
    // repository/path and render the actual file interface. Tab/header text can appear
    // in more than one piece of center-panel chrome and is not the identity.
    expect(decodeURIComponent(new URL(page.url()).pathname)).toContain(
      `/file/docs/${isolatedWorkspace.id}/`,
    );
    await expect(centerPanel.getByTestId("file-view")).toBeVisible({
      timeout: 10_000,
    });
  });

  test("created project doc persists after page reload", async ({ page }) => {
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);

    // Expand Project Docs section
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

    // Create a doc
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

    const testDocName = `e2e-persist-${Date.now()}`;
    const fileNameInput = modal
      .getByRole("textbox")
      .or(modal.locator('input[type="text"]'))
      .first();
    await fileNameInput.fill(testDocName);

    const submitBtn = modal.getByRole("button", { name: /create/i }).last();
    await submitBtn.click();
    await expect(modal).not.toBeVisible({ timeout: 10_000 });

    // Verify it appears
    const newDocItem = docsSection
      .getByText(new RegExp(testDocName, "i"))
      .first();
    await expect(newDocItem).toBeVisible({ timeout: 15_000 });

    // Reload the page
    await page.reload({ waitUntil: "domcontentloaded" });
    // Wait for workspace to re-load
    await expect(page.getByTestId("left-sidebar")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByTestId("center-panel")).toBeVisible({
      timeout: 30_000,
    });

    // Expand Project Docs again (may collapse on reload)
    const docsSectionAfter = page.getByTestId("sidebar-docs-tab");
    const docsHeaderAfter = docsSectionAfter.getByRole("button", {
      name: /^(project docs|docs)$/i,
    });
    if (await softVisible(docsHeaderAfter, 3_000)) {
      const expanded = await docsHeaderAfter.getAttribute("aria-expanded");
      if (expanded === "false") {
        await docsHeaderAfter.click();
      }
    }

    // The doc should still be there
    const persistedDoc = docsSectionAfter
      .getByText(new RegExp(testDocName, "i"))
      .first();
    await expect(persistedDoc).toBeVisible({ timeout: 15_000 });
  });

  test("project docs deep-linked with a stale branch hint still open their draft", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);

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

    const testDocName = `e2e-search-open-${Date.now()}`;
    const fileNameInput = modal
      .getByRole("textbox")
      .or(modal.locator('input[type="text"]'))
      .first();
    await fileNameInput.fill(testDocName);

    const submitBtn = modal.getByRole("button", { name: /create/i }).last();
    await submitBtn.click();
    await expect(modal).not.toBeVisible({ timeout: 10_000 });

    const newDocItem = docsSection
      .getByText(new RegExp(testDocName, "i"))
      .first();
    await expect(newDocItem).toBeVisible({ timeout: 15_000 });
    await newDocItem.click();
    await expect(page.getByText("Preparing draft…").first()).not.toBeVisible({
      timeout: 15_000,
    });

    await expect(page).toHaveURL(/\/file\//);
    const openedPath = decodeURIComponent(new URL(page.url()).pathname).split(
      "/file/",
    )[1];
    const read = await authedApi(
      page,
      `/api/v1/workspaces/${isolatedWorkspace.id}/docs/by-path?path=${encodeURIComponent(openedPath)}`,
    );
    expect(read.status, JSON.stringify(read.body)).toBe(200);
    const doc = read.body?.data ?? read.body;
    expect(doc?.id).toBeTruthy();
    const docUrl = `${isolatedWorkspace.path}/doc/${encodeURIComponent(doc.id)}`;

    const workspaceDetail = await authedApi(
      page,
      `/api/v1/workspaces/${isolatedWorkspace.id}`,
    );
    expect(workspaceDetail.status).toBe(200);
    const branches =
      workspaceDetail.body?.data?.branches ??
      workspaceDetail.body?.branches ??
      [];
    const nonDocsBranch = Array.isArray(branches)
      ? branches.find(
          (branch: { repoId?: string; repoName?: string }) =>
            branch?.repoId &&
            typeof branch.repoName === "string" &&
            !branch.repoName.toLowerCase().includes("docs"),
        )
      : null;
    expect(nonDocsBranch?.repoId).toBeTruthy();

    await page.goto(
      `${docUrl}${docUrl.includes("?") ? "&" : "?"}branch=${encodeURIComponent(nonDocsBranch!.repoId)}`,
      { waitUntil: "domcontentloaded" },
    );
    await expect(page.getByTestId("left-sidebar")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByTestId("center-panel")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByText("Preparing draft…").first()).not.toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByTestId("tab-bar")).toContainText(doc.name);
  });

  test("new Project Docs edit exact source in Monaco and Preview never writes", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);
    await createAndOpenProjectDoc(page, `e2e-monaco-${Date.now()}`);
    const content = "Hello world café 中文 😀";
    const writes: Request[] = [];
    page.on("request", (request) => {
      if (request.method() === "PUT" && request.url().includes("/files/"))
        writes.push(request);
    });
    const editor = await replaceFileSource(page, content);
    await page.keyboard.insertText("!");
    const preview = await openFilePreview(page);
    await expect(
      preview.getByText(`${content}!`, { exact: true }),
    ).toBeVisible();
    await expect(preview.locator('[contenteditable="true"]')).toHaveCount(0);
    expect(writes).toHaveLength(0);
    const response = page.waitForResponse(
      (r) => r.request().method() === "PUT" && r.url().includes("/files/"),
    );
    await editor.getByTestId("code-editor-save-button").click();
    expect((await response).status()).toBe(200);
    expect(writes).toHaveLength(1);
    expect(writes[0].postDataJSON().content).toBe(`${content}!`);
    await page.reload();
    await expect(page.getByTestId("document-preview")).toContainText(
      `${content}!`,
    );
  });

  test("Project Doc Preview retains semantic records and authored Markdown", async ({
    page,
  }) => {
    await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);
    await createAndOpenProjectDoc(page, `e2e-preview-${Date.now()}`);
    await replaceFileSource(
      page,
      [
        "# New heading",
        "",
        "[I1. Intent]",
        "",
        "New intent content.",
        "",
        "[D1. Decision]",
        "",
        "New decision content.",
        "",
        "[Q1. Question]",
        "",
        "New question content.",
        "",
        "- New item",
        "",
        "| Column 1 | Column 2 |",
        "| --- | --- |",
        "| A | B |",
        "",
        "```mermaid",
        "graph TD",
        "  A --> B",
        "```",
        "",
        "---",
        "",
      ].join("\n"),
    );
    const preview = await openFilePreview(page);
    for (const id of ["I1", "D1", "Q1"])
      await expect(
        preview.locator(
          `[data-testid="semantic-record"][data-intent-record="${id}"]`,
        ),
      ).toBeVisible();
    await expect(
      preview.getByRole("heading", { name: "New heading" }),
    ).toBeVisible();
    await expect(preview.getByText("New item", { exact: true })).toBeVisible();
    await expect(
      preview.getByRole("columnheader", { name: "Column 1" }),
    ).toBeVisible();
    await expect(
      preview.locator('[data-testid="mermaid-zoom-viewport"] svg'),
    ).toBeVisible();
    await expect(preview.locator("hr")).toHaveCount(1);
    await expect(preview.locator('[contenteditable="true"]')).toHaveCount(0);
  });

  for (const [label, content] of [
    [
      "TC-SB-005: headings",
      "# Overview\n\nOverview body.\n\n## Details\n\nDetails body.",
    ],
    [
      "TC-SB-008: paragraphs",
      "Preface paragraph.\n\nSecond paragraph.\n\n- New item",
    ],
  ]) {
    test(`${label} remain flat Preview content without record labels`, async ({
      page,
    }) => {
      await navigateToWorkspace(page, TEST_ORG, isolatedWorkspace.name);
      await createAndOpenProjectDoc(page, `e2e-flat-preview-${Date.now()}`);
      await replaceFileSource(page, content);
      const preview = await openFilePreview(page);
      await expect(preview.getByTestId("semantic-record")).toHaveCount(0);
      await expect(preview.getByTestId("semantic-section")).toHaveCount(0);
      await expect(preview.locator("p")).toHaveCount(2);
      await openFileSource(page);
      await page.keyboard.press("ControlOrMeta+End");
      await page.keyboard.insertText(" extra");
      await openFilePreview(page);
      await expect(preview).toContainText("extra");
      const editor = await openFileSource(page);
      await editor.locator(".monaco-editor .view-lines").click();
      await page.keyboard.press("ControlOrMeta+z");
      await openFilePreview(page);
      await expect(preview).not.toContainText("extra");
    });
  }
});

test.describe("Project Doc HTML Upload", () => {
  let uploadWorkspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    uploadWorkspace = await createWorkspaceClone(page, {
      namePrefix: `project-doc-html-${testInfo.title}`,
      start: true,
    });
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, uploadWorkspace?.id);
  });

  test("uploaded HTML project doc shares Source while retaining its sandboxed Preview", async ({
    page,
  }) => {
    const timestamp = Date.now();
    const uploadedFileName = `e2e-html-${timestamp}.html`;
    const htmlHeading = `E2E HTML ${timestamp}`;
    const uploadedHtml = [
      "<!doctype html>",
      "<html>",
      "<head><title>G5 HTML e2e</title></head>",
      `<body><main><h1>${htmlHeading}</h1><p>Central rich preview</p></main></body>`,
      "</html>",
    ].join("");
    let workspaceId = "";
    let createdDocId = "";

    try {
      await navigateToWorkspace(page, TEST_ORG, uploadWorkspace.name);
      workspaceId = uploadWorkspace.id;

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

      await modal.getByRole("button", { name: /upload a doc/i }).click();
      await modal.locator('input[type="file"]').setInputFiles({
        name: uploadedFileName,
        mimeType: "text/html",
        buffer: Buffer.from(uploadedHtml, "utf-8"),
      });

      await expect(modal.getByText(uploadedFileName)).toBeVisible({
        timeout: 5_000,
      });
      await modal.getByRole("button", { name: /upload doc/i }).click();
      await expect(modal).not.toBeVisible({ timeout: 10_000 });

      const docsResult = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      expect(docsResult.status).toBe(200);
      const docs = docsFromResponse(docsResult.body);
      const createdDoc = Array.isArray(docs)
        ? docs.find(
            (doc: {
              id?: string;
              name?: string;
              filePath?: string;
              documentSource?: "user" | "auxiliary" | null;
            }) =>
              doc.name === uploadedFileName ||
              doc.filePath?.endsWith(`/${uploadedFileName}`),
          )
        : null;
      createdDocId = createdDoc?.id ?? "";
      expect(createdDocId).toBeTruthy();
      expect(createdDoc?.name).toBeTruthy();
      expect(createdDoc?.documentSource).toBe("user");
      const previewTitle = `${uploadedFileName} preview`;

      const uploadedDocItem = docsSection.getByText(uploadedFileName).first();
      await expect(uploadedDocItem).toBeVisible({ timeout: 15_000 });
      const uploadedDocRow = docsSection
        .getByTestId("doc-item")
        .filter({ hasText: uploadedFileName })
        .first();
      await expect(uploadedDocRow.getByLabel("Source: User")).toBeVisible();
      await captureFirstHtmlPreviewSrcdoc(page, previewTitle);
      await uploadedDocItem.click();

      await expect(page.getByText("Preparing draft…").first()).not.toBeVisible({
        timeout: 15_000,
      });

      const centerPanel = page.getByTestId("center-panel");
      await expect(centerPanel).toBeVisible({ timeout: 10_000 });

      const htmlFrame = centerPanel.locator(`iframe[title="${previewTitle}"]`);
      await expect(htmlFrame).toBeVisible({ timeout: 15_000 });
      await expect(htmlFrame).toHaveAttribute("sandbox", "allow-scripts");
      await expect(htmlFrame).toHaveAttribute("referrerpolicy", "no-referrer");

      const firstSrcdoc = await readFirstHtmlPreviewSrcdoc(page);
      expect(firstSrcdoc).toContain(htmlHeading);

      const srcdoc = (await htmlFrame.getAttribute("srcdoc")) ?? "";
      expect(srcdoc).toContain("Content-Security-Policy");
      expect(srcdoc).toContain("connect-src 'none'");
      expect(srcdoc).toContain(htmlHeading);

      await expect(
        centerPanel
          .frameLocator(`iframe[title="${previewTitle}"]`)
          .getByRole("heading", { name: htmlHeading }),
      ).toBeVisible({ timeout: 10_000 });

      const source = await openFileSource(page, false);
      await expect(source).toContainText(htmlHeading);
      await openFilePreview(page);
      await expect(htmlFrame).toBeVisible();
    } finally {
      if (workspaceId && createdDocId) {
        await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs/project-docs/${createdDocId}`,
          "DELETE",
        );
      }
    }
  });
});

// TC-WDF-011 (workspace_docs_folders.test_view.md I14): uploaded `.jsonl`
// project docs render the Claude-transcript rich preview (center-panel D61)
// and open as raw editable JSONL in source mode. Tier 1: no mocks — real
// upload modal, real docs endpoint, real doc tab rendering.

/** Minimal Claude Code session-transcript fixture: one meta record, one user
 * message, one assistant message with a text block and a tool call. */
function buildClaudeTranscriptFixture(
  sessionId: string,
  userText: string,
  assistantText: string,
): string {
  return [
    JSON.stringify({ type: "mode", mode: "normal", sessionId }),
    JSON.stringify({
      type: "user",
      message: { role: "user", content: userText },
      sessionId,
    }),
    JSON.stringify({
      type: "assistant",
      message: {
        role: "assistant",
        content: [
          { type: "text", text: assistantText },
          { type: "tool_use", name: "Bash", input: { command: "ls mocks/" } },
        ],
      },
      sessionId,
    }),
  ].join("\n");
}

test.describe("Project Doc JSONL Upload", () => {
  let uploadWorkspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    uploadWorkspace = await createWorkspaceClone(page, {
      namePrefix: `project-doc-upload-${testInfo.title}`,
      start: true,
    });
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, uploadWorkspace?.id);
  });

  test("uploaded JSONL project doc renders the Claude transcript view and opens as source", async ({
    page,
  }) => {
    const timestamp = Date.now();
    const uploadedFileName = `e2e-claude-session-${timestamp}.jsonl`;
    const userText = `Rename the ship button ${timestamp}`;
    const assistantText = `Here are naming options ${timestamp}`;
    const transcript = buildClaudeTranscriptFixture(
      `e2e-session-${timestamp}`,
      userText,
      assistantText,
    );
    let workspaceId = "";
    let createdDocId = "";

    try {
      await navigateToWorkspace(page, TEST_ORG, uploadWorkspace.name);
      workspaceId = uploadWorkspace.id;

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

      await modal.getByRole("button", { name: /upload a doc/i }).click();
      await modal.locator('input[type="file"]').setInputFiles({
        name: uploadedFileName,
        mimeType: "application/jsonl",
        buffer: Buffer.from(transcript, "utf-8"),
      });

      const queuedJsonl = modal
        .getByTestId("upload-queue-item")
        .filter({ hasText: uploadedFileName });
      await expect(queuedJsonl).toHaveCount(1, { timeout: 5_000 });
      await expect(queuedJsonl).toContainText("queued");
      await modal.getByRole("button", { name: /upload doc/i }).click();
      await expect(modal).not.toBeVisible({ timeout: 10_000 });

      const docsResult = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      expect(docsResult.status).toBe(200);
      const docs = docsFromResponse(docsResult.body);
      const createdDoc = Array.isArray(docs)
        ? docs.find(
            (doc: { id?: string; name?: string; filePath?: string }) =>
              doc.name === uploadedFileName ||
              doc.filePath?.endsWith(`/${uploadedFileName}`),
          )
        : null;
      createdDocId = createdDoc?.id ?? "";
      expect(createdDocId).toBeTruthy();
      expect(createdDoc?.filePath).toBe(
        `docs/${workspaceId}/${uploadedFileName}`,
      );

      const uploadedDocItem = docsSection.getByText(uploadedFileName).first();
      await expect(uploadedDocItem).toBeVisible({ timeout: 15_000 });
      await uploadedDocItem.click();

      await expect(page.getByText("Preparing draft…").first()).not.toBeVisible({
        timeout: 15_000,
      });

      const centerPanel = page.getByTestId("center-panel");
      await expect(centerPanel).toBeVisible({ timeout: 10_000 });

      const transcriptView = centerPanel.getByTestId("jsonl-transcript-view");
      await expect(transcriptView).toBeVisible({ timeout: 15_000 });
      await expect(
        transcriptView.getByText("Claude Code session transcript"),
      ).toBeVisible();
      await expect(transcriptView.getByText(userText)).toBeVisible();
      await expect(transcriptView.getByText(assistantText)).toBeVisible();
      await expect(
        transcriptView.getByRole("button", { name: /Tool: Bash/ }),
      ).toBeVisible();
      await expect(transcriptView.getByText("mode: normal")).toBeVisible();
      // The JSON document is never interpreted as HTML — no preview iframe.
      await expect(
        centerPanel.locator(`iframe[title="${uploadedFileName} preview"]`),
      ).toHaveCount(0);

      const source = await openFileSource(page, false);
      await expect(source).toBeVisible();
      await expect(source).toContainText(userText);
      await expect(transcriptView).toHaveCount(0);
      await openFilePreview(page);
      await expect(transcriptView.getByText(userText)).toBeVisible({
        timeout: 10_000,
      });
    } finally {
      if (workspaceId && createdDocId) {
        await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs/project-docs/${createdDocId}`,
          "DELETE",
        );
      }
    }
  });

  test("multi-file upload keeps a 409 conflict partial, creates Markdown and HTML, and never overwrites", async ({
    page,
  }) => {
    const timestamp = Date.now();
    const conflictName = `e2e-multi-conflict-${timestamp}.md`;
    const markdownName = `e2e-multi-notes-${timestamp}.md`;
    const htmlName = `e2e-multi-report-${timestamp}.html`;
    const createdNames = new Set([conflictName, markdownName, htmlName]);
    const originalConflict = `# Original conflict ${timestamp}\n`;
    const htmlHeading = `E2E Multi HTML ${timestamp}`;
    const uploadedHtml = `<!doctype html><html><body><h1>${htmlHeading}</h1></body></html>`;
    let workspaceId = "";
    const createdDocIds: string[] = [];
    let htmlPreviewTitle = "";

    try {
      await navigateToWorkspace(page, TEST_ORG, uploadWorkspace.name);
      workspaceId = uploadWorkspace.id;
      const conflictPath = `docs/${workspaceId}/${conflictName}`;
      const docsSection = await expandProjectDocs(page);
      await clickSidebarSectionAction(
        page,
        "sidebar-docs-tab",
        "Create project doc",
      );

      const modal = page.getByRole("dialog").first();
      await expect(modal).toBeVisible({ timeout: 5_000 });
      await modal.getByRole("button", { name: /upload a doc/i }).click();
      await modal.locator('input[type="file"]').setInputFiles([
        {
          name: conflictName,
          mimeType: "text/markdown",
          buffer: Buffer.from("# Must not overwrite\n", "utf-8"),
        },
        {
          name: markdownName,
          mimeType: "text/markdown",
          buffer: Buffer.from(`# Multi notes ${timestamp}\n`, "utf-8"),
        },
        {
          name: htmlName,
          mimeType: "text/html",
          buffer: Buffer.from(uploadedHtml, "utf-8"),
        },
      ]);
      await expect(modal.getByTestId("upload-queue-item")).toHaveCount(3);

      // Create the collision after selection so the client preflight was
      // accurate when it ran and the server's authoritative 409 still races it.
      const seededConflict = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs`,
        "POST",
        {
          name: conflictName,
          docType: "design",
          filePath: conflictPath,
          content: originalConflict,
        },
      );
      expect(seededConflict.status).toBe(201);
      const conflictId = seededConflict.body?.data?.id ?? "";
      expect(conflictId).toBeTruthy();
      createdDocIds.push(conflictId);

      await modal.getByRole("button", { name: /upload docs/i }).click();
      await expect(
        modal.getByRole("button", { name: "Retry failed" }),
      ).toBeVisible({ timeout: 20_000 });

      const conflictRow = modal
        .getByTestId("upload-queue-item")
        .filter({ hasText: conflictName });
      const markdownRow = modal
        .getByTestId("upload-queue-item")
        .filter({ hasText: markdownName });
      const htmlRow = modal
        .getByTestId("upload-queue-item")
        .filter({ hasText: htmlName });
      await expect(conflictRow).toContainText("failed");
      await expect(markdownRow).toContainText("succeeded");
      await expect(htmlRow).toContainText("succeeded");

      const conflictRead = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs/by-path?path=${encodeURIComponent(conflictPath)}`,
      );
      expect(conflictRead.status).toBe(200);
      expect(conflictRead.body?.data?.content).toBe(originalConflict);

      const docsResult = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      expect(docsResult.status).toBe(200);
      const docs = docsFromResponse(docsResult.body);
      expect(Array.isArray(docs)).toBe(true);
      for (const name of [markdownName, htmlName]) {
        const doc = docs.find(
          (candidate: { id?: string; name?: string; filePath?: string }) =>
            candidate.name === name || candidate.filePath?.endsWith(`/${name}`),
        );
        expect(doc?.id).toBeTruthy();
        expect(doc?.name).toBeTruthy();
        if (name === htmlName) htmlPreviewTitle = `${htmlName} preview`;
        createdDocIds.push(doc.id);
      }
      expect(htmlPreviewTitle).toBeTruthy();

      await modal.getByRole("button", { name: "Cancel" }).click();
      await expect(modal).not.toBeVisible({ timeout: 10_000 });
      await expect(docsSection.getByText(markdownName).first()).toBeVisible({
        timeout: 15_000,
      });
      const htmlItem = docsSection.getByText(htmlName).first();
      await expect(htmlItem).toBeVisible({ timeout: 15_000 });
      await captureFirstHtmlPreviewSrcdoc(page, htmlPreviewTitle);
      await htmlItem.click();
      const htmlFrame = page
        .getByTestId("center-panel")
        .locator(`iframe[title="${htmlPreviewTitle}"]`);
      await expect(htmlFrame).toBeVisible({ timeout: 15_000 });
      expect(await readFirstHtmlPreviewSrcdoc(page)).toContain(htmlHeading);
    } finally {
      if (workspaceId) {
        const docsResult = await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
        );
        const docs = docsFromResponse(docsResult.body);
        if (docsResult.status === 200 && Array.isArray(docs)) {
          for (const doc of docs) {
            if (
              createdNames.has(doc.name) ||
              [...createdNames].some((name) =>
                doc.filePath?.endsWith(`/${name}`),
              )
            ) {
              createdDocIds.push(doc.id);
            }
          }
        }
      }
      for (const docId of [...new Set(createdDocIds)]) {
        if (workspaceId && docId) {
          await authedApi(
            page,
            `/api/v1/workspaces/${workspaceId}/docs/project-docs/${docId}`,
            "DELETE",
          );
        }
      }
    }
  });

  test("same-tick Retry failed actions start one sequential upload run", async ({
    page,
  }) => {
    const timestamp = Date.now();
    const conflictNames = [
      `e2e-retry-lock-one-${timestamp}.md`,
      `e2e-retry-lock-two-${timestamp}.md`,
    ];
    const createdNames = new Set(conflictNames);
    const originalContents = new Map(
      conflictNames.map((name, index) => [
        name,
        `# Original retry lock ${index + 1} ${timestamp}\n`,
      ]),
    );
    const retryRequestNames: string[] = [];
    const createdDocIds: string[] = [];
    let workspaceId = "";
    let captureRetryRequests = false;
    let activeRetryRequests = 0;
    let maxActiveRetryRequests = 0;

    const isTargetRetryRequest = (request: Request): boolean => {
      if (!captureRetryRequests || request.method() !== "POST") return false;
      if (
        new URL(request.url()).pathname !==
        `/api/v1/workspaces/${workspaceId}/docs`
      ) {
        return false;
      }
      const body = request.postDataJSON() as { name?: string };
      return body.name !== undefined && createdNames.has(body.name);
    };

    page.on("request", (request) => {
      if (!isTargetRetryRequest(request)) return;
      const body = request.postDataJSON() as { name?: string };
      retryRequestNames.push(body.name!);
      activeRetryRequests += 1;
      maxActiveRetryRequests = Math.max(
        maxActiveRetryRequests,
        activeRetryRequests,
      );
    });
    page.on("requestfinished", (request) => {
      if (isTargetRetryRequest(request)) activeRetryRequests -= 1;
    });
    page.on("requestfailed", (request) => {
      if (isTargetRetryRequest(request)) activeRetryRequests -= 1;
    });

    try {
      await navigateToWorkspace(page, TEST_ORG, uploadWorkspace.name);
      workspaceId = uploadWorkspace.id;
      await expandProjectDocs(page);
      await clickSidebarSectionAction(
        page,
        "sidebar-docs-tab",
        "Create project doc",
      );

      const modal = page.getByRole("dialog").first();
      await expect(modal).toBeVisible({ timeout: 5_000 });
      await modal.getByRole("button", { name: /upload a doc/i }).click();
      await modal.locator('input[type="file"]').setInputFiles(
        conflictNames.map((name) => ({
          name,
          mimeType: "text/markdown",
          buffer: Buffer.from(`# Must not overwrite ${name}\n`, "utf-8"),
        })),
      );
      await expect(modal.getByTestId("upload-queue-item")).toHaveCount(2);

      for (const name of conflictNames) {
        const seeded = await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs`,
          "POST",
          {
            name,
            docType: "design",
            filePath: `docs/${workspaceId}/${name}`,
            content: originalContents.get(name),
          },
        );
        expect(seeded.status).toBe(201);
        const docId = seeded.body?.data?.id ?? "";
        expect(docId).toBeTruthy();
        createdDocIds.push(docId);
      }

      await modal.getByRole("button", { name: /upload docs/i }).click();
      const retryButton = modal.getByRole("button", { name: "Retry failed" });
      await expect(retryButton).toBeVisible({ timeout: 20_000 });
      for (const name of conflictNames) {
        await expect(
          modal.getByTestId("upload-queue-item").filter({ hasText: name }),
        ).toContainText("failed");
      }

      captureRetryRequests = true;
      await retryButton.evaluate((button) => {
        (button as HTMLButtonElement).click();
        (button as HTMLButtonElement).click();
      });
      await expect(retryButton).toBeHidden({ timeout: 5_000 });
      await expect(retryButton).toBeVisible({ timeout: 20_000 });
      await expect.poll(() => activeRetryRequests).toBe(0);
      captureRetryRequests = false;

      expect(retryRequestNames).toEqual(conflictNames);
      expect(maxActiveRetryRequests).toBe(1);
      for (const name of conflictNames) {
        const existing = await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs/by-path?path=${encodeURIComponent(`docs/${workspaceId}/${name}`)}`,
        );
        expect(existing.status).toBe(200);
        expect(existing.body?.data?.content).toBe(originalContents.get(name));
      }

      await modal.getByRole("button", { name: "Cancel" }).click();
      await expect(modal).not.toBeVisible({ timeout: 10_000 });
    } finally {
      if (workspaceId) {
        const docsResult = await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
        );
        const docs = docsFromResponse(docsResult.body);
        if (docsResult.status === 200 && Array.isArray(docs)) {
          for (const doc of docs) {
            if (
              createdNames.has(doc.name) ||
              [...createdNames].some((name) =>
                doc.filePath?.endsWith(`/${name}`),
              )
            ) {
              createdDocIds.push(doc.id);
            }
          }
        }
      }
      for (const docId of [...new Set(createdDocIds)]) {
        if (workspaceId && docId) {
          await authedApi(
            page,
            `/api/v1/workspaces/${workspaceId}/docs/project-docs/${docId}`,
            "DELETE",
          );
        }
      }
    }
  });

  test("modal Upload Files drop appends Markdown and HTML and waits for confirmation", async ({
    page,
  }) => {
    const timestamp = Date.now();
    const markdownName = `e2e-modal-drop-${timestamp}.md`;
    const htmlName = `e2e-modal-drop-${timestamp}.html`;
    const createdNames = new Set([markdownName, htmlName]);
    const createdDocIds: string[] = [];
    let workspaceId = "";

    try {
      await navigateToWorkspace(page, TEST_ORG, uploadWorkspace.name);
      workspaceId = uploadWorkspace.id;
      const docsSection = await expandProjectDocs(page);
      await clickSidebarSectionAction(
        page,
        "sidebar-docs-tab",
        "Create project doc",
      );

      const modal = page.getByRole("dialog").first();
      await expect(modal).toBeVisible({ timeout: 5_000 });
      await modal.getByRole("button", { name: /upload a doc/i }).click();
      await dropTextFiles(page, modal.getByTestId("upload-file-drop-zone"), [
        {
          name: markdownName,
          mimeType: "text/markdown",
          content: `# Modal drop ${timestamp}\n`,
        },
        {
          name: htmlName,
          mimeType: "text/html",
          content: `<html><body>Modal drop ${timestamp}</body></html>`,
        },
      ]);
      await expect(modal.getByTestId("upload-queue-item")).toHaveCount(2);

      const beforeConfirm = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      const docsBefore = docsFromResponse(beforeConfirm.body);
      const createdPaths = new Set(
        [...createdNames].map((name) => `docs/${workspaceId}/${name}`),
      );
      expect(
        Array.isArray(docsBefore) &&
          docsBefore.some((doc: { filePath?: string }) =>
            createdPaths.has(doc.filePath ?? ""),
          ),
      ).toBe(false);

      await modal.getByRole("button", { name: /upload docs/i }).click();
      await expect(modal).not.toBeVisible({ timeout: 15_000 });
      for (const name of createdNames) {
        await expect(docsSection.getByText(name).first()).toBeVisible({
          timeout: 15_000,
        });
      }

      const afterConfirm = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      const docsAfter = docsFromResponse(afterConfirm.body);
      for (const doc of Array.isArray(docsAfter) ? docsAfter : []) {
        if (createdPaths.has(doc.filePath)) createdDocIds.push(doc.id);
      }
      expect(createdDocIds).toHaveLength(2);
    } finally {
      for (const docId of createdDocIds) {
        if (workspaceId) {
          await authedApi(
            page,
            `/api/v1/workspaces/${workspaceId}/docs/project-docs/${docId}`,
            "DELETE",
          );
        }
      }
    }
  });

  test("populated Project Docs header drop opens a root-seeded modal and waits for confirmation", async ({
    page,
  }) => {
    const timestamp = Date.now();
    const seedName = `e2e-header-seed-${timestamp}.md`;
    const markdownName = `e2e-header-drop-${timestamp}.md`;
    const htmlName = `e2e-header-drop-${timestamp}.html`;
    const uploadedNames = new Set([markdownName, htmlName]);
    const createdNames = new Set([seedName, ...uploadedNames]);
    const createdDocIds: string[] = [];
    let workspaceId = "";

    try {
      await navigateToWorkspace(page, TEST_ORG, uploadWorkspace.name);
      workspaceId = uploadWorkspace.id;
      const seeded = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs`,
        "POST",
        {
          name: seedName,
          docType: "design",
          filePath: `docs/${workspaceId}/${seedName}`,
          content: `# Existing root doc ${timestamp}\n`,
        },
      );
      expect(seeded.status).toBe(201);
      const seedId = seeded.body?.data?.id ?? "";
      expect(seedId).toBeTruthy();
      createdDocIds.push(seedId);

      await page.reload();
      const docsSection = await expandProjectDocs(page);
      await expect(docsSection.getByText(seedName).first()).toBeVisible({
        timeout: 15_000,
      });
      const docsHeader = docsSection.getByRole("button", {
        name: /^(project docs|docs)$/i,
      });
      await dropTextFiles(page, docsHeader, [
        {
          name: markdownName,
          mimeType: "text/markdown",
          content: `# Header drop ${timestamp}\n`,
        },
        {
          name: htmlName,
          mimeType: "text/html",
          content: `<html><body>Header drop ${timestamp}</body></html>`,
        },
      ]);

      const modal = page.getByRole("dialog").first();
      await expect(modal).toBeVisible({ timeout: 5_000 });
      await expect(modal.getByTestId("upload-queue-item")).toHaveCount(2);
      for (const name of uploadedNames) {
        const row = modal
          .getByTestId("upload-queue-item")
          .filter({ hasText: name });
        await expect(row).toHaveCount(1);
        const destination = await row.locator("p").first().textContent();
        expect(destination?.endsWith(`/${name}`)).toBe(true);
      }

      const beforeConfirm = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      expect(beforeConfirm.status).toBe(200);
      const docsBefore = docsFromResponse(beforeConfirm.body);
      const uploadedPaths = new Set(
        [...uploadedNames].map((name) => `docs/${workspaceId}/${name}`),
      );
      expect(
        Array.isArray(docsBefore) &&
          docsBefore.some((doc: { filePath?: string }) =>
            uploadedPaths.has(doc.filePath ?? ""),
          ),
      ).toBe(false);

      await modal.getByRole("button", { name: /upload docs/i }).click();
      await expect(modal).not.toBeVisible({ timeout: 15_000 });
      for (const name of uploadedNames) {
        await expect(docsSection.getByText(name).first()).toBeVisible({
          timeout: 15_000,
        });
      }

      const afterConfirm = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      expect(afterConfirm.status).toBe(200);
      const docsAfter = docsFromResponse(afterConfirm.body);
      for (const name of uploadedNames) {
        const doc = Array.isArray(docsAfter)
          ? docsAfter.find(
              (candidate: { id?: string; name?: string; filePath?: string }) =>
                candidate.filePath === `docs/${workspaceId}/${name}`,
            )
          : undefined;
        expect(doc?.filePath).toBe(`docs/${workspaceId}/${name}`);
        createdDocIds.push(doc?.id ?? "");
      }
    } finally {
      if (workspaceId) {
        const docsResult = await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
        );
        const docs = docsFromResponse(docsResult.body);
        if (docsResult.status === 200 && Array.isArray(docs)) {
          for (const doc of docs) {
            if (
              createdNames.has(doc.name) ||
              [...createdNames].some((name) =>
                doc.filePath?.endsWith(`/${name}`),
              )
            ) {
              createdDocIds.push(doc.id);
            }
          }
        }
      }
      for (const docId of [...new Set(createdDocIds)].filter(Boolean)) {
        if (workspaceId) {
          await authedApi(
            page,
            `/api/v1/workspaces/${workspaceId}/docs/project-docs/${docId}`,
            "DELETE",
          );
        }
      }
    }
  });

  test("nested Project Docs drop opens one seeded modal at the exact folder and waits for confirmation", async ({
    page,
  }) => {
    const timestamp = Date.now();
    const folderName = `e2e-drop-folder-${timestamp}`;
    const seedName = `seed-${timestamp}.md`;
    const markdownName = `nested-drop-${timestamp}.md`;
    const htmlName = `nested-drop-${timestamp}.html`;
    const createdDocIds: string[] = [];
    let workspaceId = "";

    try {
      await navigateToWorkspace(page, TEST_ORG, uploadWorkspace.name);
      workspaceId = uploadWorkspace.id;
      const seed = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs`,
        "POST",
        {
          name: seedName,
          docType: "design",
          filePath: `docs/${workspaceId}/${folderName}/${seedName}`,
          content: `# Seed ${timestamp}\n`,
        },
      );
      expect(seed.status).toBe(201);
      createdDocIds.push(seed.body?.data?.id ?? "");

      await page.reload();
      const docsSection = await expandProjectDocs(page);
      const folder = docsSection
        .getByTestId("doc-folder")
        .filter({ hasText: folderName })
        .first();
      await expect(folder).toBeVisible({ timeout: 15_000 });
      await dropTextFiles(page, folder, [
        {
          name: markdownName,
          mimeType: "text/markdown",
          content: `# Nested drop ${timestamp}\n`,
        },
        {
          name: htmlName,
          mimeType: "text/html",
          content: `<html><body>Nested drop ${timestamp}</body></html>`,
        },
      ]);

      const modal = page.getByRole("dialog").first();
      await expect(modal).toBeVisible({ timeout: 5_000 });
      await expect(modal.getByTestId("upload-queue-item")).toHaveCount(2);
      await expect(
        modal
          .getByTestId("upload-queue-item")
          .filter({ hasText: `${folderName}/${markdownName}` }),
      ).toHaveCount(1);
      await expect(
        modal
          .getByTestId("upload-queue-item")
          .filter({ hasText: `${folderName}/${htmlName}` }),
      ).toHaveCount(1);

      const beforeConfirm = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      const docsBefore = docsFromResponse(beforeConfirm.body);
      const uploadedPaths = new Set([
        `docs/${workspaceId}/${folderName}/${markdownName}`,
        `docs/${workspaceId}/${folderName}/${htmlName}`,
      ]);
      const createdPaths = new Set([
        `docs/${workspaceId}/${folderName}/${seedName}`,
        ...uploadedPaths,
      ]);
      expect(
        Array.isArray(docsBefore) &&
          docsBefore.some((doc: { filePath?: string }) =>
            uploadedPaths.has(doc.filePath ?? ""),
          ),
      ).toBe(false);

      await modal.getByRole("button", { name: /upload docs/i }).click();
      await expect(modal).not.toBeVisible({ timeout: 15_000 });
      if ((await folder.getAttribute("aria-expanded")) === "false") {
        await folder.click();
      }
      await expect(docsSection.getByText(markdownName).first()).toBeVisible({
        timeout: 15_000,
      });
      await expect(docsSection.getByText(htmlName).first()).toBeVisible({
        timeout: 15_000,
      });

      const afterConfirm = await authedApi(
        page,
        `/api/v1/workspaces/${workspaceId}/docs?surface=workspace`,
      );
      const docsAfter = docsFromResponse(afterConfirm.body);
      const observedCreatedPaths = new Set<string>();
      for (const doc of Array.isArray(docsAfter) ? docsAfter : []) {
        if (createdPaths.has(doc.filePath)) {
          observedCreatedPaths.add(doc.filePath);
          createdDocIds.push(doc.id);
        }
      }
      for (const createdPath of createdPaths) {
        expect(observedCreatedPaths.has(createdPath)).toBe(true);
      }
      expect(observedCreatedPaths.size).toBe(3);
    } finally {
      for (const docId of [...new Set(createdDocIds)].filter(Boolean)) {
        await authedApi(
          page,
          `/api/v1/workspaces/${workspaceId}/docs/project-docs/${docId}`,
          "DELETE",
        );
      }
    }
  });
});
