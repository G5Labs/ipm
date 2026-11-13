import { expect, test, type Page } from "@playwright/test";
import { openFileSource, openFilePreview, replaceFileSource } from "../helpers";
const route = "/e2e/fixtures/file-editor.html";
const source = (page: Page) =>
  page.locator(".monaco-editor .view-lines").first();
const value = (page: Page) =>
  page.evaluate(() =>
    (window as any).editorTest.monaco.editor
      .getModels()
      .find((m: any) => m.uri.scheme === "g5-file")
      ?.getValue(),
  );
async function edit(page: Page, text: string) {
  await source(page).click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText(text);
}
/** The switch at the foot of the view between rich and plain text. */
const toggle = (page: Page) => page.getByTestId("file-view-switch-mode");
/** Done: back to the reading view, with the unsaved draft applied. */
const done = (page: Page) => page.getByTestId("file-view-done").click();
/** Edit Markdown as plain text: Edit opens Rich text; the switch leads on. */
async function editSource(page: Page) {
  await page.getByTestId("file-view-edit").click();
  await toggle(page).click();
}

test("HTML answers become writable after delayed Save and permission recovery", async ({
  page,
}) => {
  await page.goto(`${route}?html=1`);
  const frame = page.frameLocator('iframe[title="Shared HTML preview"]');
  const propose = frame.getByRole("button", { name: "Propose answer" });
  await expect(propose).toHaveAttribute("data-ready", "true");
  await page.evaluate(() => (window as any).editorTest.hold());
  await propose.click();
  await page.getByTestId("html-doc-answer-apply").click();
  await expect(frame.locator("#g5-doc-answers")).toHaveJSProperty(
    "textContent",
    '{"choice":"after"}',
  );
  await expect(propose).toHaveAttribute("data-ready", "false");
  await page.evaluate(() => (window as any).editorTest.release());
  await expect(propose).toHaveAttribute("data-ready", "true");
  expect(
    await page.evaluate(() => (window as any).editorTest.submitted()[0]),
  ).toMatchObject({ commitAndPush: false });
  await page.getByRole("button", { name: "Change permission" }).click();
  await expect(propose).toHaveAttribute("data-ready", "false");
  await page.getByRole("button", { name: "Change permission" }).click();
  await expect(propose).toHaveAttribute("data-ready", "true");
  await propose.click();
  await expect(page.getByTestId("html-doc-answer-apply")).toBeVisible();
});

test("large HTML renders automatically and keeps complete editable source", async ({
  page,
}) => {
  await page.goto(`${route}?html=1`);
  const html = "<h1>Large HTML preview</h1>".padEnd(3 * 1024 * 1024, " ");
  await page.evaluate(
    (content) => (window as any).editorTest.external(content),
    html,
  );
  const iframe = page.locator('iframe[title="Shared HTML preview"]');
  await expect(iframe).toHaveAttribute("sandbox", "allow-scripts");
  await expect(
    page
      .frameLocator('iframe[title="Shared HTML preview"]')
      .getByRole("heading", { name: "Large HTML preview" }),
  ).toBeVisible();
  await expect(page.getByTestId("file-view-load-preview")).toHaveCount(0);
  await page.getByTestId("file-view-edit").click();
  await expect(source(page)).toBeVisible();
  expect(await value(page)).toBe(html);
  await source(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("<p>Retained ending</p>");
  expect(await value(page)).toBe(html + "<p>Retained ending</p>");
  await done(page);
  await expect(
    page
      .frameLocator('iframe[title="Shared HTML preview"]')
      .getByText("Retained ending"),
  ).toBeVisible();
  await expect(page.getByTestId("file-view-load-preview")).toHaveCount(0);
});

test("Docs and Files share exact source, preview, native undo and saved history", async ({
  page,
}) => {
  await page.goto(route);
  await editSource(page);
  await expect(source(page)).toBeVisible();
  const text = "# Changed café 中文 😀\r\n\r\n  *literal*  \r\n";
  await edit(page, text);
  await done(page);
  await expect(page.getByTestId("document-preview")).toContainText(
    "Changed café 中文 😀",
  );
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await editSource(page);
  expect(await value(page)).toBe(text);
  await page.getByRole("button", { name: "Away", exact: true }).click();
  await page.getByRole("button", { name: "Docs", exact: true }).click();
  await expect(source(page)).toBeVisible();
  expect(await value(page)).toBe(text);
  await source(page).click();
  const initial = await page.evaluate(() => (window as any).editorTest.initial);
  for (let i = 0; i < 32 && (await value(page)) !== initial; i++)
    await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => value(page)).toBe(initial);
  for (let i = 0; i < 32 && (await value(page)) !== text; i++)
    await page.keyboard.press("ControlOrMeta+Shift+z");
  await expect.poll(() => value(page)).toBe(text);
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  expect(
    await page.evaluate(
      () => (window as any).editorTest.submitted()[0].content,
    ),
  ).toBe(text);
  await page.getByRole("button", { name: "Away", exact: true }).click();
  await page.getByRole("button", { name: "Files", exact: true }).click();
  // Left while editing plain text: it comes back editing plain text.
  await expect(page.getByTestId("file-view-done")).toBeVisible();
  await expect(toggle(page)).toHaveText("Switch to rich text editing");
  await source(page).click();
  await page.keyboard.press("ControlOrMeta+z");
  for (let i = 0; i < 32 && (await value(page)) !== initial; i++)
    await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => value(page)).toBe(initial);
});

test("a file retains its mode, and reading after Done, when reopened from another entry point", async ({
  page,
}) => {
  await page.goto(route);
  await expect(toggle(page)).toHaveText("Switch to plain text");
  await toggle(page).click();
  await expect(source(page)).toBeVisible();
  await expect(toggle(page)).toHaveText("Switch to rich text");
  await page.getByRole("button", { name: "Away", exact: true }).click();
  await page.getByRole("button", { name: "Docs", exact: true }).click();
  await expect(source(page)).toBeVisible();
  // Edit starts in the mode being read: plain text.
  await page.getByTestId("file-view-edit").click();
  await expect(toggle(page)).toHaveText("Switch to rich text editing");
  await edit(page, "# Remember this preview\n");
  await done(page);
  await expect(page.getByTestId("file-view-done")).toHaveCount(0);
  await page.getByRole("button", { name: "Away", exact: true }).click();
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await expect(page.getByTestId("document-preview")).toContainText(
    "Remember this preview",
  );
  await expect(page.getByTestId("file-view-done")).toHaveCount(0);
  await expect(page.getByTestId("file-view-discard")).toBeVisible();
});

test("a delayed Save acknowledges only submitted bytes across navigation", async ({
  page,
}) => {
  await page.goto(route);
  await editSource(page);
  await edit(page, "submitted");
  await page.evaluate(() => (window as any).editorTest.hold());
  await page.getByTestId("code-editor-save-button").click();
  await edit(page, "later typing 中文");
  await page.getByRole("button", { name: "Away", exact: true }).click();
  await page.getByRole("button", { name: "Docs", exact: true }).click();
  await expect(page.getByTestId("code-editor-save-button")).toBeDisabled();
  await page.evaluate(() => (window as any).editorTest.release());
  await expect(page.getByTestId("code-editor-save-button")).toBeEnabled();
  expect(await value(page)).toBe("later typing 中文");
  expect(
    await page.evaluate(
      () => (window as any).editorTest.draft().baseline.content,
    ),
  ).toBe("submitted");
  await done(page);
  await page.evaluate(() => (window as any).editorTest.fail(true));
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByText(/Save failed:/)).toBeVisible();
  await expect(page.getByTestId("document-preview")).toContainText(
    "later typing 中文",
  );
});

test("permission, role and person changes preserve only authorized recovery", async ({
  page,
}) => {
  await page.goto(route);
  await editSource(page);
  await edit(page, "private draft");
  await page.getByRole("button", { name: "Change permission" }).click();
  expect(await value(page)).toBe("private draft");
  await expect(page.getByTestId("code-editor-save-button")).toHaveCount(0);
  await page.getByRole("button", { name: "Change permission" }).click();
  await page.getByRole("button", { name: "Change role" }).click();
  await expect(page.getByTestId("file-view-role-changed")).toBeVisible();
  await expect(page.getByTestId("file-view-push-review")).toBeDisabled();
  await page.getByRole("button", { name: "Change person" }).click();
  await expect(page.getByTestId("document-preview")).not.toContainText(
    "private draft",
  );
  expect(
    await page.evaluate(() => (window as any).editorTest.draft()),
  ).toBeNull();
});

test("external changes and VM replacement keep the draft pending explicit recovery", async ({
  page,
}) => {
  await page.goto(route);
  await editSource(page);
  await edit(page, "mine");
  await page.evaluate(() =>
    (window as any).editorTest.external("changed externally", "vm2"),
  );
  await expect(page.getByTestId("code-editor-disk-change")).toBeVisible();
  expect(await value(page)).toBe("mine");
  await page.getByRole("button", { name: "Away", exact: true }).click();
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await expect(page.getByTestId("code-editor-disk-change")).toBeVisible();
  await done(page);
  await expect(page.getByTestId("document-preview")).toContainText("mine");
  await page.getByTestId("code-editor-save-button").click();
  await expect(
    page.getByText(/Save failed: Workspace was replaced/),
  ).toBeVisible();
  expect(
    await page.evaluate(() => (window as any).editorTest.draft().content),
  ).toBe("mine");
});

test("native IME composition commits Unicode through Monaco", async ({
  page,
}) => {
  await page.goto(route);
  await editSource(page);
  await source(page).click();
  await page.keyboard.press("ControlOrMeta+a");
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Input.imeSetComposition", {
    text: "にほん",
    selectionStart: 3,
    selectionEnd: 3,
  });
  await cdp.send("Input.imeSetComposition", {
    text: "日本語😀",
    selectionStart: 5,
    selectionEnd: 5,
  });
  await cdp.send("Input.insertText", { text: "日本語😀" });
  await expect.poll(() => value(page)).toBe("日本語😀");
  await done(page);
  await expect(page.getByTestId("document-preview")).toContainText("日本語😀");
  await page.getByTestId("code-editor-save-button").click();
  await expect
    .poll(() =>
      page.evaluate(() => (window as any).editorTest.submitted()[0]?.content),
    )
    .toBe("日本語😀");
});

test("question and comment controls use the shared source through keyboard actions", async ({
  page,
}) => {
  await page.goto(route);
  const text = [
    "# Decision",
    "",
    "[Q1. Color]",
    "",
    "```g5-question",
    "schema: 1",
    "id: color",
    "status: open",
    "prompt: Pick a color",
    "options:",
    "  - label: Blue",
    "  - label: Red",
    "```",
    "",
    "Keep  these bytes.",
    "",
  ].join("\r\n");
  await page.evaluate(
    (text) => (window as any).editorTest.external(text),
    text,
  );
  const option = page.getByTestId("question-option-toggle").first();
  await option.focus();
  await page.keyboard.press("Enter");
  await page.getByTestId("question-answer-button").focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("code-editor-dirty-indicator")).toBeVisible();
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  const saved = await page.evaluate(
    () => (window as any).editorTest.submitted()[0].content,
  );
  expect(saved).toContain("status: answered");
  expect(saved).toContain("[Q1. Color] (Tags: answered)\r\n");
  expect(saved).toContain("\r\nKeep  these bytes.\r\n");
  expect(saved.replaceAll("\r\n", "")).not.toContain("\n");
  await page.getByTestId("question-clear-answer").click();
  // A domain write stays in the reading view; Edit continues from its draft.
  await expect(page.getByTestId("file-view-done")).toHaveCount(0);
  await editSource(page);
  await expect.poll(() => value(page)).not.toContain("Tags: answered");
  await expect.poll(() => value(page)).toContain("status: open");
  await source(page).click();
  await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => value(page)).toBe(saved);
  await done(page);
  await page.evaluate(() => (window as any).editorTest.enableComments());
  await expect(page.getByTestId("code-editor-comment-margin")).toHaveCount(0);
  // Selected current text offers Comment (`g5.documents.comments.identity`).
  await page
    .getByTestId("document-preview-content")
    .getByText("Keep these bytes.")
    .evaluate((element) => {
      const range = document.createRange();
      range.selectNodeContents(element);
      window.getSelection()?.removeAllRanges();
      window.getSelection()?.addRange(range);
      element.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });
  await page.getByTestId("document-preview-selection-comment-button").focus();
  await page.keyboard.press("Enter");
  const draft = page.getByRole("textbox", { name: "Comment", exact: true });
  await expect(draft).toBeVisible();
  await draft.fill("Question source comment");
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(() => (window as any).editorTest.comments().length),
    )
    .toBe(1);
  expect(
    (await page.evaluate(() => (window as any).editorTest.comments()[0])).anchor
      .lineNumber,
  ).toBe(
    ((await value(page)) as string)
      .split(/\r?\n/)
      .findIndex((line) => line.includes("Keep  these bytes.")) + 1,
  );
});

test("large Markdown defers preview while Source remains editable above the former cap", async ({
  page,
}) => {
  await page.goto(route);
  await page.evaluate(() => (window as any).editorTest.large(3 * 1024 * 1024));
  await expect(page.getByTestId("file-view-deferred-preview")).toBeVisible();
  // A deferred preview edits in Source; Rich text waits for Load preview.
  await page.getByTestId("file-view-edit").click();
  await expect(source(page)).toBeVisible();
  expect((await value(page)).length).toBe(3 * 1024 * 1024);
  await source(page).click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText(" retained ending");
  await expect
    .poll(async () => (await value(page)).endsWith(" retained ending"))
    .toBe(true);
  await expect(toggle(page)).toHaveText("Switch to rich text editing");
  await toggle(page).click();
  await expect(page.getByTestId("file-view-deferred-preview")).toBeVisible();
  await expect(page.getByTestId("rich-editor")).toHaveCount(0);
  expect(
    await page.evaluate(
      () => (window as any).editorTest.draft().content.length,
    ),
  ).toBeGreaterThan(3 * 1024 * 1024);
});

test("review controls preserve their own draft and leave canonical Source untouched", async ({
  page,
}) => {
  await page.goto(route);
  await page.evaluate(() => {
    const fixture = (window as any).editorTest;
    fixture.external("Original.\n");
    fixture.suggestions([
      {
        id: "review",
        publicationId: "pub",
        findingId: null,
        repoId: "repo",
        path: "docs/shared.md",
        kind: "hunk",
        beforeLines: "Original.\n",
        afterLines: "Proposed.\n",
        anchorStart: 1,
        contentHash: null,
        anchorContext: null,
        assessment: null,
        authorKind: "human",
        authorLabel: "Reviewer",
        note: "",
        commentId: null,
        status: "open",
        statusBy: null,
        statusAt: null,
        supersedes: null,
        adoptedFrom: null,
        createdAt: "2026-10-01T00:00:00Z",
      },
    ]);
  });
  const card = page.getByTestId("spec-suggestion-card-review");
  await card.getByTestId("review-suggestion-edit-toggle-review").focus();
  await page.keyboard.press("Enter");
  const replacement = card.getByTestId("review-suggestion-edit-review");
  await replacement.fill("An unsent proposal");
  await toggle(page).click();
  expect(await value(page)).toBe("Original.\n");
  await toggle(page).click();
  await expect(card.getByTestId("review-suggestion-edit-review")).toHaveValue(
    "An unsent proposal",
  );
  expect(
    await page.evaluate(() => (window as any).editorTest.submitted()),
  ).toEqual([]);
});

test("a clean external EOL change updates Monaco without manufacturing a draft", async ({
  page,
}) => {
  await page.goto(route);
  await toggle(page).click();
  await expect(source(page)).toBeVisible();
  await page.evaluate(() =>
    (window as any).editorTest.external("# New source\n\nLF lines\n"),
  );
  await expect.poll(() => value(page)).toBe("# New source\n\nLF lines\n");
  expect(
    await page.evaluate(() => (window as any).editorTest.draft()),
  ).toBeNull();
});

test("shared Preview retains GFM list markers, task state, tables and math", async ({
  page,
}) => {
  await page.goto(route);
  await page.evaluate(() =>
    (window as any).editorTest.external(
      [
        "# Shared formatting",
        "",
        "- Bullet",
        "  - Nested bullet",
        "",
        "1. Numbered",
        "   1. Nested number",
        "",
        "- [x] Finished",
        "",
        "| Feature | Status |",
        "| --- | --- |",
        "| Source | Ready |",
        "",
        "$x^2 + y^2$",
        "",
      ].join("\n"),
    ),
  );
  const preview = page.getByTestId("document-preview");
  await expect(
    preview.locator("ul:not(.contains-task-list)").first(),
  ).toHaveCSS("list-style-type", "disc");
  await expect(preview.locator("ol").first()).toHaveCSS(
    "list-style-type",
    "decimal",
  );
  await expect(preview.getByRole("checkbox")).toBeChecked();
  await expect(preview.getByRole("checkbox")).toBeDisabled();
  await expect(preview.getByRole("table")).toContainText("Source");
  await expect(preview.locator(".katex")).toBeVisible();
  expect(
    await page.evaluate(() => (window as any).editorTest.draft()),
  ).toBeNull();
});

test("the live-flow controls preserve authored records and native Undo through Preview", async ({
  page,
}) => {
  await page.goto(route);
  const text =
    "# Authored\n\n[I1. Intent]\n\nBody.\n\n[D1. Decision]\n\nDecision.\n\n[Q1. Question]\n\nQuestion.\n\n```mermaid\ngraph TD\n  A --> B\n```\n";
  await replaceFileSource(page, text);
  const preview = await openFilePreview(page);
  for (const id of ["I1", "D1", "Q1"])
    await expect(
      preview.locator(
        `[data-testid="semantic-record"][data-intent-record="${id}"]`,
      ),
    ).toBeVisible();
  await expect(
    preview.locator('[data-testid="mermaid-zoom-viewport"] svg'),
  ).toBeVisible();
  await openFileSource(page);
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("extra");
  await openFilePreview(page);
  await expect(preview).toContainText("extra");
  await openFileSource(page);
  await page.keyboard.press("ControlOrMeta+z");
  await openFilePreview(page);
  await expect(preview).not.toContainText("extra");
  await replaceFileSource(page, "");
  await openFilePreview(page);
  await expect(preview.getByTestId("semantic-record")).toHaveCount(0);
  await openFileSource(page);
  await page.keyboard.press("ControlOrMeta+z");
  await openFilePreview(page);
  await expect(preview.getByTestId("semantic-record")).toHaveCount(3);
});

test("ontology Compare retains reading context, exact Save and native Undo", async ({
  page,
}) => {
  await page.goto(`${route}?ontology=1`);
  const view = page.getByTestId("file-view");
  const preview = view.getByTestId("file-view-preview");
  const compare = view.getByRole("button", { name: "Compare", exact: true });
  await expect(compare).toHaveAttribute("aria-pressed", "true");
  await expect(view.getByTestId("ontology-comparison-status")).toContainText(
    "0 statement changes",
  );
  await expect(
    preview.getByText("Reading context remains visible."),
  ).toBeVisible();
  // The retained read-only Source copy must stay hidden; scope reading assertions to Preview.
  await expect(view.getByTestId("code-line-3")).toBeHidden();
  const initial = await page.evaluate(() => (window as any).editorTest.initial);
  // A replacement, so the pair shows its whole Before row: one that only
  // adds text hides it (g5.documents.ontology.change-highlights).
  const edited = initial.replace("Original statement.", "Edited statement.");
  await replaceFileSource(page, edited);
  await openFilePreview(page);
  const before = preview.getByRole("region", {
    name: "Before statement browser.a",
    exact: true,
  });
  await expect(before).toContainText("Original statement.");
  await expect(before).not.toContainText("Edited");
  await expect(
    before.locator('[contenteditable="true"], [data-block-index], a[href]'),
  ).toHaveCount(0);
  // Saved from the reading view after Done: nothing is left to save.
  await view.getByTestId("code-editor-save-button").click();
  await expect(view.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  await expect(view.getByTestId("code-editor-save-button")).toHaveCount(0);
  expect(
    await page.evaluate(
      () => (window as any).editorTest.submitted()[0].content,
    ),
  ).toBe(edited);
  await page.getByRole("button", { name: "Away", exact: true }).click();
  await page.getByRole("button", { name: "Docs", exact: true }).click();
  await expect(before).toBeVisible();
  await replaceFileSource(page, "");
  await openFilePreview(page);
  await expect(
    preview.getByRole("region", {
      name: "Deleted statement browser.a",
      exact: true,
    }),
  ).toBeVisible();
  await openFileSource(page);
  await source(page).click();
  await page.keyboard.press("ControlOrMeta+z");
  await openFilePreview(page);
  await expect(before).toBeVisible();
  expect(await value(page)).toBe(edited);
  await compare.click();
  await expect(before).toHaveCount(0);
  await expect(
    preview.getByText("Reading context remains visible."),
  ).toBeVisible();
  await compare.click();
  await expect(before).toBeVisible();
  expect(await value(page)).toBe(edited);
});

// `g5.documents.preview.rich-editing`: Markdown edits as a rendered document
// in Rich text, which writes the file's Markdown through the same draft,
// Save, Push, Undo and conflict paths as Source.
const rich = (page: Page) => page.getByTestId("rich-editor-content");
const draftContent = (page: Page, person?: string) =>
  page.evaluate(
    (person) => (window as any).editorTest.draft(person)?.content ?? null,
    person,
  );
const submittedContents = (page: Page): Promise<string[]> =>
  page.evaluate(() =>
    (window as any).editorTest
      .submitted()
      .map((entry: { content: string }) => entry.content),
  );
/**
 * Wait until both the editor's caret and the browser's selection are in the
 * block that shows `text`: a key pressed before both follow a click acts at
 * the old place.
 */
const caretIn = (page: Page, text: string) =>
  page.waitForFunction((text) => {
    const content = document.querySelector(
      '[data-testid="rich-editor-content"]',
    ) as (Element & { editor?: any }) | null;
    const anchor = document.getSelection()?.anchorNode;
    const block = (
      anchor instanceof Element ? anchor : anchor?.parentElement
    )?.closest("p, h1, h2, h3, li");
    return (
      !!content?.editor?.state.selection.$from.parent.textContent.includes(
        text,
      ) && !!block?.textContent?.includes(text)
    );
  }, text);
/**
 * Put the caret at the start or end of the text block that shows `text`
 * through the editor, which moves its own selection and the browser's
 * together. A click or a caret key followed at once by an editing key can act
 * at the old place: ProseMirror only reads the browser's selection when a
 * later `selectionchange` arrives.
 */
async function caretAt(page: Page, text: string, at: "start" | "end") {
  await page.evaluate(
    ({ text, at }) => {
      const editor = (
        document.querySelector('[data-testid="rich-editor-content"]') as
          | (Element & { editor?: any })
          | null
      )?.editor;
      let position = -1;
      editor.state.doc.descendants((node: any, pos: number) => {
        if (position < 0 && node.isTextblock && node.textContent === text)
          position = at === "start" ? pos + 1 : pos + node.nodeSize - 1;
      });
      if (position < 0) throw new Error(`No block shows ${text}`);
      editor.commands.focus();
      editor.commands.setTextSelection(position);
    },
    { text, at },
  );
  await caretIn(page, text);
}
/** Put the caret at the end of the rendered text block that shows `text`. */
async function caretAfter(page: Page, text: string) {
  await rich(page).locator("p, h1, h2, h3", { hasText: text }).first().click();
  await caretIn(page, text);
  await page.keyboard.press("End");
}
const richSelected = (page: Page) =>
  expect(toggle(page)).toHaveText("Switch to plain text editing");

test("Rich text edits rendered Markdown, saves its exact Markdown and undoes in Source", async ({
  page,
}) => {
  await page.goto(route);
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  await page.getByTestId("file-view-edit").click();
  await richSelected(page);
  await expect(rich(page)).toBeFocused();
  await caretAfter(page, "Original.");
  await page.keyboard.type(" Now ");
  await page.keyboard.press("ControlOrMeta+b");
  await page.keyboard.type("bold");
  await page.keyboard.press("ControlOrMeta+b");
  await page.keyboard.press("Enter");
  await page.keyboard.type("# New heading");
  await expect(rich(page).locator("strong")).toHaveText("bold");
  await expect(rich(page).locator("h1").last()).toHaveText("New heading");
  // Save writes what Rich text holds first: the edited block and the new
  // one follow the file's style; CRLF, the title and the final newline stay.
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  const expected = initial.replace(
    "Original.\r\n",
    "Original. Now **bold**\r\n\r\n# New heading\r\n",
  );
  expect(await submittedContents(page)).toEqual([expected]);
  await richSelected(page);
  await toggle(page).click();
  await expect.poll(() => value(page)).toBe(expected);
  // The retained Monaco model adopted each write as one edit.
  await source(page).click();
  for (let i = 0; i < 32 && (await value(page)) !== initial; i++)
    await page.keyboard.press("ControlOrMeta+z");
  await expect.poll(() => value(page)).toBe(initial);
});

test("a code block from the toolbar takes typed lines and saves as a fence", async ({
  page,
}) => {
  await page.goto(route);
  await page.getByTestId("file-view-edit").click();
  await caretAfter(page, "Original.");
  await page.keyboard.press("Enter");
  await page.getByTestId("rich-toolbar-code-block").click();
  // The shared Markdown stylesheet hides breaks inside code; an empty code
  // block must still hold the caret and show what is typed.
  await page.keyboard.type("const x = 1;");
  await page.keyboard.press("Enter");
  await page.keyboard.type("const y = 2;");
  await expect(rich(page).locator("pre code")).toHaveText(
    "const x = 1;\nconst y = 2;",
  );
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  expect(await submittedContents(page)).toEqual([
    "# Shared café 中文 😀\r\n\r\nOriginal.\r\n\r\n```\r\nconst x = 1;\r\nconst y = 2;\r\n```\r\n",
  ]);
});

test("a table cell edits in place and Mod+S saves the table in the file's own alignment", async ({
  page,
}) => {
  await page.goto(route);
  const text =
    "Intro.\n\n| Feature | Status |\n| :------ | -----: |\n| Export  | done   |\n";
  await page.evaluate(
    (text) => (window as any).editorTest.external(text),
    text,
  );
  await expect(page.getByTestId("document-preview")).toContainText("Export");
  await page.getByTestId("file-view-edit").click();
  await expect(page.getByTestId("rich-table-controls")).toHaveCount(0);
  await caretAfter(page, "done");
  await expect(page.getByTestId("rich-table-controls")).toBeVisible();
  for (let i = 0; i < 4; i++) await page.keyboard.press("Backspace");
  await page.keyboard.type("ok");
  await page.keyboard.press("ControlOrMeta+s");
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  expect(await submittedContents(page)).toEqual([
    "Intro.\n\n| Feature | Status |\n| :------ | -----: |\n| Export  |     ok |\n",
  ]);
  await richSelected(page);
  await expect(rich(page).locator("td").last()).toHaveText("ok");
});

test("a double-click opens Rich text at that word; Done returns to the read-only reading view, where a double-click resumes", async ({
  page,
}) => {
  await page.goto(route);
  const preview = page.getByTestId("document-preview");
  const paragraph = preview.locator("p", { hasText: "Original" });
  // On the word "Original".
  await paragraph.dblclick({ position: { x: 12, y: 8 } });
  await richSelected(page);
  await page.keyboard.type(" text");
  await expect(rich(page).locator("p")).toHaveText("Original text.");
  await expect
    .poll(() => draftContent(page))
    .toBe("# Shared café 中文 😀\r\n\r\nOriginal text.\r\n");
  await done(page);
  await expect(preview).toContainText("Original text.");
  await expect(preview.locator('[contenteditable="true"]')).toHaveCount(0);
  await expect(page.getByTestId("rich-editor")).toHaveCount(0);
  await expect(toggle(page)).toHaveText("Switch to plain text");
  // From the reading view, a double-click returns to Rich text there.
  await preview.locator("h1").dblclick({ position: { x: 20, y: 12 } });
  await richSelected(page);
  await page.keyboard.type("!");
  await expect(rich(page).locator("h1")).toHaveText("Shared! café 中文 😀");
  // Beside a block's text rather than on a word, it continues the block.
  await done(page);
  await paragraph.dblclick();
  await richSelected(page);
  await page.keyboard.type(" More.");
  await expect(rich(page).locator("p")).toHaveText("Original text. More.");
});

test("`e` in the reading Preview opens Rich text without writing a draft", async ({
  page,
}) => {
  await page.goto(route);
  await page
    .getByTestId("document-preview")
    .locator("p", { hasText: "Original." })
    .click();
  await page.keyboard.press("e");
  await richSelected(page);
  await expect(rich(page)).toBeFocused();
  await expect(rich(page)).toContainText("Original.");
  expect(await draftContent(page)).toBeNull();
});

test("Push review carries a reader's Rich text edit, including edits not yet written", async ({
  page,
}) => {
  await page.goto(route);
  await page.getByRole("button", { name: "Change role" }).click();
  await page.getByTestId("file-view-suggest").click();
  await richSelected(page);
  await caretAfter(page, "Original.");
  await page.keyboard.press("Shift+Home");
  await page.keyboard.type("A reader's wording.");
  await expect(page.getByTestId("file-view-suggesting")).toBeVisible();
  await expect(page.getByTestId("code-editor-save-button")).toHaveCount(0);
  await page.getByTestId("file-view-push-review").click();
  await page.getByTestId("file-view-push-submit").click();
  await expect(page.getByTestId("file-view-push-done")).toContainText(
    "Pushed 1 suggestion",
  );
  const initial = await page.evaluate(() => (window as any).editorTest.initial);
  const pushed = await page.evaluate(() => (window as any).editorTest.pushed());
  expect(pushed).toEqual([
    {
      repoId: "repo",
      path: "docs/shared.md",
      baseContent: initial,
      content: initial.replace("Original.", "A reader's wording."),
      allowStaleBase: false,
    },
  ]);
  expect(await draftContent(page)).toBeNull();
});

test("a person change drops Rich text edits and their pending text", async ({
  page,
}) => {
  await page.goto(route);
  await page.getByTestId("file-view-edit").click();
  await caretAfter(page, "Original.");
  await page.keyboard.type(" Written.");
  await expect.poll(() => draftContent(page)).toContain("Written.");
  await page.keyboard.type(" Pending.");
  await page.getByRole("button", { name: "Change person" }).click();
  await expect(page.getByTestId("rich-editor")).toHaveCount(0);
  const preview = page.getByTestId("document-preview");
  await expect(preview).toContainText("Original.");
  await expect(preview).not.toContainText("Written.");
  await page.waitForTimeout(500);
  expect(await draftContent(page)).toBeNull();
  expect(await draftContent(page, "other")).toBeNull();
});

test("Rich text saved over an external change takes the unchanged conflict path", async ({
  page,
}) => {
  await page.goto(route);
  await page.getByTestId("file-view-edit").click();
  await caretAfter(page, "Original.");
  await page.keyboard.press("Shift+Home");
  await page.keyboard.type("Mine.");
  await expect
    .poll(() => draftContent(page))
    .toBe("# Shared café 中文 😀\r\n\r\nMine.\r\n");
  await page.evaluate(() =>
    (window as any).editorTest.external("# Theirs\r\n\r\nOriginal.\r\n"),
  );
  await expect(page.getByTestId("code-editor-disk-change")).toBeVisible();
  await expect(rich(page)).toContainText("Mine.");
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-conflict-dialog")).toBeVisible();
  expect(await draftContent(page)).toBe(
    "# Shared café 中文 😀\r\n\r\nMine.\r\n",
  );
  await page.getByTestId("code-editor-conflict-reload").click();
  await expect(rich(page).locator("h1")).toHaveText("Theirs");
});

test("ontology prose edits in Rich text while record lines stay protected, and Compare reads the draft after Done", async ({
  page,
}) => {
  await page.goto(`${route}?ontology=1`);
  const view = page.getByTestId("file-view");
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  await page.getByTestId("file-view-edit").click();
  await richSelected(page);
  // Anchors, identity lines and the Metadata footer stay in the document,
  // not editable and not shown; plain text is the way to change them.
  const hidden = rich(page).getByTestId("rich-hidden-line");
  await expect(hidden).toHaveCount(7);
  expect(
    await hidden.evaluateAll((nodes) =>
      nodes.every((node) => node.getAttribute("contenteditable") === "false"),
    ),
  ).toBe(true);
  await expect(rich(page).getByTestId("rich-opaque-block")).toHaveCount(0);
  await expect(rich(page).locator("h2", { hasText: "Metadata" })).toHaveCount(
    0,
  );
  await caretAfter(page, "Original statement.");
  await page.keyboard.type(" Edited.");
  // Deleting everything would remove record lines: refused, with the reason.
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Backspace");
  await expect(page.getByTestId("rich-editor-notice")).toContainText(
    "only change in plain text",
  );
  await expect(rich(page)).toContainText("Original statement. Edited.");
  await done(page);
  const edited = initial.replace(
    "Original statement.",
    "Original statement. Edited.",
  );
  await expect.poll(() => value(page)).toBe(edited);
  const compare = view.getByRole("button", { name: "Compare", exact: true });
  await expect(compare).toHaveAttribute("aria-pressed", "true");
  await expect(view.getByTestId("ontology-comparison-status")).toContainText(
    "1 statement change",
  );
  await view.getByTestId("code-editor-save-button").click();
  await expect(view.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  await expect(view.getByTestId("code-editor-save-button")).toHaveCount(0);
  expect(await submittedContents(page)).toEqual([edited]);
});

// `g5.documents.editor.modes`: one surface, two modes, Done always available.
test("Done returns to reading with the draft applied; Save and Discard stay until decided", async ({
  page,
}) => {
  await page.goto(route);
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  await expect(page.getByRole("tablist")).toHaveCount(0);
  await page.getByTestId("file-view-edit").click();
  await richSelected(page);
  await caretAfter(page, "Original.");
  await page.keyboard.type(" Kept.");
  await done(page);
  const preview = page.getByTestId("document-preview");
  await expect(preview).toContainText("Original. Kept.");
  await expect(page.getByTestId("rich-editor")).toHaveCount(0);
  await expect(page.getByTestId("file-view-done")).toHaveCount(0);
  await expect(page.getByTestId("file-view-discard")).toBeVisible();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toBeVisible();
  await expect(toggle(page)).toHaveText("Switch to plain text");
  const edited = initial.replace("Original.\r\n", "Original. Kept.\r\n");
  // Plain text while reading shows the draft, read-only.
  await toggle(page).click();
  await expect(source(page)).toBeVisible();
  expect(await value(page)).toBe(edited);
  expect(
    await page.evaluate(() => {
      const m = (window as any).editorTest.monaco;
      return m.editor
        .getEditors()
        .find((e: any) => e.getModel()?.uri.scheme === "g5-file")
        .getOption(m.editor.EditorOption.readOnly);
    }),
  ).toBe(true);
  await expect(toggle(page)).toHaveText("Switch to rich text");
  await toggle(page).click();
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  expect(await submittedContents(page)).toEqual([edited]);
  await expect(page.getByTestId("file-view-discard")).toHaveCount(0);
  await expect(page.getByTestId("file-view-edit")).toBeVisible();
});

// `g5.documents.text-measure`: plain text wraps at 8 inches; HTML does not.
const wrapState = (page: Page) =>
  page.evaluate(() => {
    const m = (window as any).editorTest.monaco;
    const editor = m.editor
      .getEditors()
      .find((e: any) => e.getModel()?.uri.scheme === "g5-file");
    const spans = Array.from(
      editor.getDomNode().querySelectorAll(".view-line > span"),
    ) as HTMLElement[];
    return {
      wordWrap: editor.getOption(m.editor.EditorOption.wordWrap) as string,
      column: editor.getOption(m.editor.EditorOption.wordWrapColumn) as number,
      charWidth: editor.getOption(m.editor.EditorOption.fontInfo)
        .typicalHalfwidthCharacterWidth as number,
      viewLines: spans.length,
      modelLines: editor.getModel().getLineCount() as number,
      widestLine: Math.max(
        ...spans.map((span) => span.getBoundingClientRect().width),
      ),
    };
  });

test("plain text wraps lines at the 8-inch measure, and HTML keeps them unwrapped", async ({
  page,
}) => {
  await page.goto(route);
  const long = "word ".repeat(120).trim();
  await page.evaluate(
    (long) => (window as any).editorTest.external(`# Wide\n\n${long}\n`),
    long,
  );
  await toggle(page).click();
  await expect(source(page)).toBeVisible();
  await expect
    .poll(async () => (await wrapState(page)).wordWrap)
    .toBe("bounded");
  const wrapped = await wrapState(page);
  expect(wrapped.column).toBe(Math.floor(768 / wrapped.charWidth));
  expect(wrapped.viewLines).toBeGreaterThan(wrapped.modelLines);
  expect(wrapped.widestLine).toBeLessThanOrEqual(768 + wrapped.charWidth);
  await page.goto(`${route}?html=1`);
  await expect(toggle(page)).toHaveText("Switch to plain text");
  await toggle(page).click();
  await expect(source(page)).toBeVisible();
  await expect(toggle(page)).toHaveText("Switch to preview");
  await expect.poll(async () => (await wrapState(page)).wordWrap).toBe("off");
});

// `g5.documents.text-measure`: tables are exempt from the measure. A small
// table keeps the text's left edge; a wide one grows to the pane, centred,
// and scrolls inside itself; the page never scrolls sideways.
const tableState = (page: Page, root: string) =>
  page.evaluate((root) => {
    const scope = document.querySelector(root)!;
    const box = (element: Element) => element.getBoundingClientRect();
    const paragraph = scope.querySelector("p")!;
    const [small, wide] = Array.from(scope.querySelectorAll("table"));
    const pane = document.querySelector('[data-testid="center-panel"]')!;
    return {
      paragraph: { x: box(paragraph).x, width: box(paragraph).width },
      small: { x: box(small).x, width: box(small).width },
      wide: {
        x: box(wide).x,
        width: box(wide).width,
        scrollWidth: wide.scrollWidth,
        clientWidth: wide.clientWidth,
      },
      pane: { x: box(pane).x, width: box(pane).width },
      pageOverflow: document.documentElement.scrollWidth > innerWidth,
    };
  }, root);

test("a small table keeps the text's edge and a wide one grows to the pane and scrolls inside, reading and editing", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1000 });
  await page.goto(route);
  const heads = Array.from(
    { length: 14 },
    (_, i) => `Column${i}WithAnUnbreakableHeading`,
  );
  const wide =
    `| ${heads.join(" | ")} |\n| ${heads.map(() => "---").join(" | ")} |\n` +
    `| ${heads.map((_, i) => `v${i}`).join(" | ")} |\n`;
  const small = "| A | B |\n| --- | --- |\n| 1 | 2 |\n";
  await page.evaluate(
    (doc) => (window as any).editorTest.external(doc),
    `# Tables\n\n${"Ordinary words ".repeat(10).trim()}\n\n${small}\n${wide}\nAfter.\n`,
  );
  const preview = page.getByTestId("document-preview");
  await expect(preview).toContainText("After.");
  const check = (state: Awaited<ReturnType<typeof tableState>>) => {
    expect(state.paragraph.width).toBeCloseTo(768, 0);
    // The small table's box starts where the text does.
    expect(state.small.x).toBeCloseTo(state.paragraph.x, 0);
    expect(state.wide.width).toBeGreaterThan(768);
    expect(state.wide.width).toBeLessThanOrEqual(state.pane.width);
    expect(state.wide.scrollWidth).toBeGreaterThan(state.wide.clientWidth);
    expect(state.pageOverflow).toBe(false);
  };
  check(await tableState(page, '[data-testid="document-preview-content"]'));
  await page.getByTestId("file-view-edit").click();
  await expect(rich(page)).toContainText("After.");
  check(await tableState(page, '[data-testid="rich-editor-content"]'));
});

// In ontology files Rich text shows prose only: record lines stay in the
// document but are not displayed, each statement keeps the reading view's
// bullet and outline, and editing keys stop at a statement's boundary.
const statementBody = (page: Page, text: string) =>
  rich(page).locator("p", { hasText: text });

test("ontology Rich text shows statement prose with bullets and saves exact bytes", async ({
  page,
}) => {
  await page.goto(`${route}?ontology=1`);
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  await page.getByTestId("file-view-edit").click();
  await richSelected(page);
  for (const line of await rich(page).getByTestId("rich-hidden-line").all())
    await expect(line).toBeHidden();
  const shown = await rich(page).innerText();
  for (const text of ["browser.a", "browser.b", "Metadata", "evidence"])
    expect(shown).not.toContain(text);
  expect(shown).toContain("Reading context remains visible.");
  // Each statement's body carries the reading view's bullet; ordinary prose does not.
  const bullet = (text: string) =>
    statementBody(page, text).evaluate(
      (node) => getComputedStyle(node, "::after").content,
    );
  expect(await bullet("Original statement.")).toContain("•");
  expect(await bullet("Unchanged statement.")).toContain("•");
  expect(await bullet("Reading context remains visible.")).toBe("none");
  // The outline shows on hover only.
  const outline = (text: string) =>
    statementBody(page, text).evaluate(
      (node) => getComputedStyle(node, "::before").borderTopColor,
    );
  const resting = await outline("Unchanged statement.");
  await statementBody(page, "Unchanged statement.").hover();
  await expect.poll(() => outline("Unchanged statement.")).not.toBe(resting);
  await caretAfter(page, "Original statement.");
  await page.keyboard.type(" Typed");
  // Shift+Enter: a new paragraph of the same statement.
  await page.keyboard.press("Shift+Enter");
  await page.keyboard.type("A new paragraph.");
  await expect(statementBody(page, "A new paragraph.")).toHaveClass(
    /g5-rich-statement-last/,
  );
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  expect(await submittedContents(page)).toEqual([
    initial.replace(
      "Original statement.\n",
      "Original statement. Typed\n\nA new paragraph.\n",
    ),
  ]);
});

// `g5.documents.ontology.creation`: Enter at a statement's end adds an
// Intent with its anchor and metadata; Backspace in an empty statement
// deletes it whole, at once and with no check of what links to it.
test("ontology Rich text adds a statement on Enter and deletes an empty one on Backspace", async ({
  page,
}) => {
  await page.goto(`${route}?ontology=1`);
  const view = page.getByTestId("file-view");
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  const placeholder = rich(page).getByTestId("rich-statement-placeholder");
  await page.getByTestId("file-view-edit").click();
  await richSelected(page);
  await caretAfter(page, "Original statement.");
  await page.keyboard.press("Enter");
  await expect(placeholder).toBeVisible();
  await expect(placeholder).toHaveText(
    "Type the statement, or press Backspace to remove it",
  );
  await page.keyboard.type("Added in Rich text.");
  await expect(placeholder).toHaveCount(0);
  const added = statementBody(page, "Added in Rich text.");
  await expect(added).toHaveClass(/g5-rich-statement-first/);
  expect(
    await added.evaluate((node) => getComputedStyle(node, "::after").content),
  ).toContain("•");
  await expect(rich(page).getByTestId("rich-opaque-block")).toHaveCount(0);
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  const [saved] = await submittedContents(page);
  const id = /\[I · (user\.[0-9a-f-]{36})\]/.exec(saved)?.[1] ?? "";
  expect(id).not.toBe("");
  expect(saved).toBe(
    initial
      .replace(
        "Original statement.\n",
        `Original statement.\n\n<a id="${id}"></a>\n[I · ${id}]\n\nAdded in Rich text.\n`,
      )
      .replace(
        "- Original evidence.\n",
        `- Original evidence.\n\n### ${id}\n\n- human-provided\n`,
      ),
  );
  // Another, typed and erased with Backspace: one more Backspace removes it.
  await caretAfter(page, "Added in Rich text.");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Gone");
  for (let i = 0; i < 4; i++) await page.keyboard.press("Backspace");
  await expect(placeholder).toBeVisible();
  await page.keyboard.press("Backspace");
  await expect(placeholder).toHaveCount(0);
  await expect(page.getByTestId("rich-editor-notice")).toHaveCount(0);
  await expect(rich(page)).toBeFocused();
  await expect(statementBody(page, "Added in Rich text.")).toHaveClass(
    /g5-rich-statement-last/,
  );
  // Back to what was saved; reading shows the new statement as Added.
  await expect
    .poll(async () => (await draftContent(page)) ?? saved)
    .toBe(saved);
  await done(page);
  await expect(
    view.getByRole("region", { name: `Added statement ${id}` }),
  ).toBeVisible();
});

test("a reader's statement added in Rich text goes out in Push review with its metadata", async ({
  page,
}) => {
  await page.goto(`${route}?ontology=1`);
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  await page.getByRole("button", { name: "Change role" }).click();
  await page.getByTestId("file-view-suggest").click();
  await richSelected(page);
  await caretAfter(page, "Unchanged statement.");
  await page.keyboard.press("Enter");
  await page.keyboard.type("A reader's statement.");
  await page.getByTestId("file-view-push-review").click();
  await page.getByTestId("file-view-push-submit").click();
  await expect(page.getByTestId("file-view-push-done")).toBeVisible();
  const [pushed] = await page.evaluate(() =>
    (window as any).editorTest.pushed(),
  );
  const id = /\[I · (user\.[0-9a-f-]{36})\]/.exec(pushed.content)?.[1] ?? "";
  expect(id).not.toBe("");
  expect(pushed.baseContent).toBe(initial);
  expect(pushed.content).toBe(
    initial
      .replace(
        "Unchanged statement.\n",
        `Unchanged statement.\n\n<a id="${id}"></a>\n[I · ${id}]\n\nA reader's statement.\n`,
      )
      .replace(
        "- Other evidence.\n",
        `- Other evidence.\n\n### ${id}\n\n- human-provided\n`,
      ),
  );
});

test("ontology Rich text keys stop at a statement boundary and arrows skip hidden lines", async ({
  page,
}) => {
  await page.goto(`${route}?ontology=1`);
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  await page.getByTestId("file-view-edit").click();
  await richSelected(page);
  // Backspace at a body's start and Delete at its end change nothing.
  await caretAt(page, "Unchanged statement.", "start");
  await page.keyboard.press("Backspace");
  await caretAt(page, "Original statement.", "end");
  await page.keyboard.press("Delete");
  await caretAt(page, "Unchanged statement.", "end");
  await page.keyboard.press("Delete");
  await expect(page.getByTestId("rich-editor-notice")).toHaveCount(0);
  await expect(statementBody(page, "Original statement.")).toHaveText(
    "Original statement.",
  );
  await expect(statementBody(page, "Unchanged statement.")).toHaveText(
    "Unchanged statement.",
  );
  await page.waitForTimeout(400);
  expect(await draftContent(page)).toBeNull();
  // Arrows cross a boundary to the neighbouring statement's text, at the
  // end of the block before or the start of the block after.
  await caretAt(page, "Unchanged statement.", "start");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.type("1");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.type("2");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.type("3");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.type("4");
  // Down from the last statement: nothing visible follows, so it stays.
  await page.keyboard.press("End");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.type("5");
  await expect(statementBody(page, "Original statement.")).toHaveText(
    "Original statement.13",
  );
  await expect(statementBody(page, "Unchanged statement.")).toHaveText(
    "42Unchanged statement.5",
  );
  await page.getByTestId("code-editor-save-button").click();
  await expect(page.getByTestId("code-editor-dirty-indicator")).toHaveCount(0);
  expect(await submittedContents(page)).toEqual([
    initial
      .replace("Original statement.", "Original statement.13")
      .replace("Unchanged statement.", "42Unchanged statement.5"),
  ]);
});

test("ontology Rich text takes IME composition in a statement body", async ({
  page,
}) => {
  await page.goto(`${route}?ontology=1`);
  const initial: string = await page.evaluate(
    () => (window as any).editorTest.initial,
  );
  await page.getByTestId("file-view-edit").click();
  await caretAfter(page, "Original statement.");
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Input.imeSetComposition", {
    text: "にほん",
    selectionStart: 3,
    selectionEnd: 3,
  });
  await cdp.send("Input.imeSetComposition", {
    text: "日本語😀",
    selectionStart: 5,
    selectionEnd: 5,
  });
  await cdp.send("Input.insertText", { text: "日本語😀" });
  await expect(statementBody(page, "Original statement.")).toHaveText(
    "Original statement.日本語😀",
  );
  await expect
    .poll(() => draftContent(page))
    .toBe(
      initial.replace("Original statement.", "Original statement.日本語😀"),
    );
});

/** Geometry of comparison rows and statement blocks, measured in the page. */
async function comparisonLayout(page: Page) {
  return page.evaluate(() => {
    const box = (el: Element) => el.getBoundingClientRect();
    // The last block shown: a closed fold's hidden regions take no space.
    const lastShown = (el: Element) =>
      [...el.children].filter((c) => c.getClientRects().length).at(-1)!;
    const region = (name: string) =>
      document.querySelector<HTMLElement>(
        `[role="region"][aria-label="${name}"]`,
      )!;
    const rows = Object.fromEntries(
      [
        ["modified", "Modified statement browser.a"],
        ["before", "Before statement browser.a"],
        ["deleted", "Deleted statement browser.gone"],
        ["added", "Added statement browser.c"],
      ].map(([key, name]) => {
        const row = region(name);
        const pair = row.closest(".ontology-comparison-pair")!;
        // The pair is the list item; its rows and their wrappers carry none.
        const bullet = pair;
        const bulletCount = [
          pair,
          ...pair.querySelectorAll(
            ".ontology-comparison-row, .ontology-statement-bullet",
          ),
        ].filter((el) => getComputedStyle(el).display === "list-item").length;
        const label = row.querySelector(".ontology-comparison-label")!;
        const blocks = row.querySelector(
          '[data-testid="semantic-record-blocks"]',
        )!;
        // With disclosure markers the statement's own marker, or a deleted
        // statement's dot, stands in for the pair's bullet.
        const marker = row.querySelector(".ontology-statement-marker");
        return [
          key,
          {
            markerLeft: marker ? box(marker).left : null,
            deletedMarker: getComputedStyle(pair, "::before").content,
            bulletLeft: box(bullet).left,
            bulletDisplay: getComputedStyle(bullet).display,
            bulletFont: getComputedStyle(bullet).fontSize,
            bulletCount,
            pairLeft: box(pair).left,
            top: box(row).top,
            bottom: box(row).bottom,
            borderTop: getComputedStyle(row).borderTopWidth,
            labelLeft: box(label).left - box(pair).left,
            textLeft: box(blocks).left - box(pair).left,
            // A statement's last block adds no space below the statement body.
            trailing: box(row).bottom - box(lastShown(blocks)).bottom,
            outline: getComputedStyle(pair, "::after").borderTopColor,
          },
        ];
      }),
    );
    const unchanged = document.querySelector<HTMLElement>(
      '[data-record-span="browser.b"]',
    );
    const shell = unchanged?.closest<HTMLElement>(
      '[data-testid="semantic-record-shell"]',
    );
    const unchangedBlocks = unchanged?.querySelector(
      '[data-testid="semantic-record-blocks"]',
    );
    return {
      rows,
      unchanged: shell && {
        markerLeft: box(shell.querySelector(".ontology-statement-marker")!)
          .left,
        contentLeft: box(shell.querySelector(".ontology-statement-content")!)
          .left,
        bulletLeft: box(shell).left,
        bulletFont: getComputedStyle(shell).fontSize,
        trailing:
          box(unchangedBlocks!).bottom -
          box(lastShown(unchangedBlocks!)).bottom,
        lastMargin: getComputedStyle(lastShown(unchangedBlocks!)).marginBottom,
      },
    };
  });
}

test("ontology comparison pairs share one bullet, meet without a gap, start labels after it and match All changes", async ({
  page,
}) => {
  await page.goto(`${route}?ontology&changed`);
  const preview = page.getByTestId("file-view-preview");
  await expect(
    preview.getByRole("region", { name: "Before statement browser.a" }),
  ).toBeVisible();
  const file = await comparisonLayout(page);
  const { modified, before, deleted, added } = file.rows;
  for (const row of [modified, before, deleted, added]) {
    // Each pair is one item. With disclosure markers
    // (`g5.documents.ontology.presentation`) it draws no bullet: its
    // statement's marker hangs where unchanged statements' markers hang, and
    // Before sits under its Current row. Labels and backgrounds start after
    // the marker, where statement text starts.
    expect(row.bulletCount).toBe(0);
    expect(row.pairLeft).toBeCloseTo(file.unchanged!.contentLeft, 0);
    expect(row.labelLeft).toBeGreaterThanOrEqual(0);
    expect(row.trailing).toBeLessThan(1);
  }
  for (const row of [modified, added])
    expect(row.markerLeft).toBeCloseTo(file.unchanged!.markerLeft, 0);
  expect(deleted.deletedMarker).toBe('"•"');
  expect(before.textLeft).toBeCloseTo(modified.textLeft, 0);
  // Current and Before meet with no gap or divider at rest.
  expect(before.top).toBeCloseTo(modified.bottom, 1);
  expect(before.borderTop).toBe("0px");
  expect(file.unchanged!.trailing).toBeLessThan(1);
  expect(file.unchanged!.lastMargin).toBe("0px");
  // The outline shows only on hover or focus.
  expect(modified.outline).toBe("rgba(0, 0, 0, 0)");
  await preview.getByText("Modified statement with a").hover();
  expect((await comparisonLayout(page)).rows.modified.outline).not.toBe(
    "rgba(0, 0, 0, 0)",
  );

  await page.goto(`${route}?ontology&all-changes`);
  await expect(
    page.getByRole("region", { name: "Before statement browser.a" }),
  ).toBeVisible();
  const all = (await comparisonLayout(page)).rows;
  for (const key of ["modified", "before", "deleted", "added"] as const) {
    // All changes reuses the same rows with the same spacing.
    expect(all[key].labelLeft).toBeCloseTo(file.rows[key].labelLeft, 0);
    expect(all[key].textLeft).toBeCloseTo(file.rows[key].textLeft, 0);
    expect(all[key].bulletFont).toBe(file.rows[key].bulletFont);
    expect(all[key].bulletCount).toBe(1);
    expect(all[key].trailing).toBeLessThan(1);
  }
  expect(all.before.top).toBeCloseTo(all.modified.bottom, 1);
  const gaps = await page.evaluate(() =>
    [...document.querySelectorAll(".ontology-comparison-pair")].map(
      (pair) => getComputedStyle(pair).marginBottom,
    ),
  );
  expect(new Set(gaps)).toEqual(new Set(["4px"]));
});

/** The text each change highlight covers, by name. */
const highlighted = (page: Page) =>
  page.evaluate(() =>
    Object.fromEntries(
      ["ontology-inserted", "ontology-removed"].map((name) => [
        name,
        [...((CSS as any).highlights.get(name) ?? [])]
          .map((range: Range) => range.toString())
          .sort(),
      ]),
    ),
  );

test("change highlights mark what changed inside a modified statement, in color only, and follow unsaved edits", async ({
  page,
}) => {
  await page.goto(`${route}?ontology&changed&sentences`);
  const preview = page.getByTestId("file-view-preview");
  const modified = preview.getByRole("region", {
    name: "Modified statement browser.a",
  });
  const before = preview.getByRole("region", {
    name: "Before statement browser.a",
  });
  await expect(before).toBeVisible();
  // A replaced sentence is marked whole; an edited one only where it grew.
  await expect
    .poll(() => highlighted(page))
    .toEqual({
      "ontology-inserted": [
        "Only the owner may force-push to the branch.",
        "with a clear error",
      ],
      "ontology-removed": ["Each push needs a fresh lease from the server."],
    });
  // Background color only, in light and dark themes: no strikethrough or
  // underline.
  const markStyles = (mode: "light" | "dark") =>
    modified.evaluate((row, mode) => {
      document.documentElement.dataset.mode = mode;
      const text = row.querySelector("p")!;
      const style = (name: string) => {
        const s = getComputedStyle(text, `::highlight(${name})`);
        return {
          background: s.backgroundColor,
          decoration: s.textDecorationLine,
        };
      };
      return {
        inserted: style("ontology-inserted"),
        removed: style("ontology-removed"),
      };
    }, mode);
  const light = await markStyles("light");
  const dark = await markStyles("dark");
  for (const styles of [light, dark]) {
    for (const mark of [styles.inserted, styles.removed]) {
      expect(mark.background).not.toBe("rgba(0, 0, 0, 0)");
      expect(mark.decoration).toBe("none");
    }
    expect(styles.inserted.background).not.toBe(styles.removed.background);
  }
  expect(dark.inserted.background).not.toBe(light.inserted.background);
  await markStyles("light");
  // Before shows only the sentence that lost text, in the removed tint, and
  // the pair still ends on a rounded row; Show full brings back the rest.
  await expect(before.getByTestId("ontology-before-excerpt")).toHaveText(
    "… Each push needs a fresh lease from the server. …",
  );
  const excerptStyle = await before.evaluate((row) => ({
    removed: getComputedStyle(row.querySelector(".ontology-excerpt-removed")!)
      .backgroundColor,
    radius: getComputedStyle(row).borderBottomLeftRadius,
  }));
  expect(excerptStyle.removed).toBe(light.removed.background);
  expect(excerptStyle.radius).toBe("6px");
  await before.getByRole("button", { name: /^Show full/ }).click();
  await expect(before).toContainText(
    "Agents push to the workspace branch. Each push needs a fresh lease from the server. The server rejects stale leases.",
  );
  // The control sits at the bottom of the label column, on the baseline of
  // the row's last line.
  const baselines = await before.evaluate((row) => {
    const baseline = (el: Element) => {
      const probe = document.createElement("span");
      probe.style.cssText = "display: inline-block; vertical-align: baseline";
      el.appendChild(probe);
      const y = probe.getBoundingClientRect().bottom;
      probe.remove();
      return y;
    };
    const lines = row.querySelectorAll(
      "[data-testid='semantic-record-blocks'] p",
    );
    return {
      text: baseline(lines[lines.length - 1]),
      control: baseline(
        row.querySelector("[data-testid='ontology-comparison-toggle']")!,
      ),
    };
  });
  expect(Math.abs(baselines.control - baselines.text)).toBeLessThan(1);
  await before.getByRole("button", { name: /^Show less/ }).click();
  await expect(before.getByTestId("ontology-before-excerpt")).toBeVisible();
  // Selecting the statement reads its own text, with nothing added.
  const selected = await modified.evaluate((row) => {
    const range = document.createRange();
    range.selectNodeContents(row.querySelector("p")!);
    const selection = getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    return selection.toString();
  });
  expect(selected).toBe(
    "Agents push to the workspace branch. Only the owner may force-push to the branch. The server rejects stale leases with a clear error.",
  );

  // An unsaved edit, back in the reading view, is compared at once.
  await editSource(page);
  const text = (await value(page)) as string;
  await edit(page, text.replace("a clear error", "a short notice"));
  await done(page);
  await expect
    .poll(() => highlighted(page))
    .toEqual({
      "ontology-inserted": [
        "Only the owner may force-push to the branch.",
        "with a short notice",
      ],
      "ontology-removed": ["Each push needs a fresh lease from the server."],
    });

  // Once nothing is removed, Before hides behind Show before.
  await editSource(page);
  await edit(
    page,
    ((await value(page)) as string).replace(
      "Only the owner",
      "Each push needs a fresh lease from the server. Only the owner",
    ),
  );
  await done(page);
  await expect(before).toBeHidden();
  const radius = await modified.evaluate(
    (row) => getComputedStyle(row).borderBottomLeftRadius,
  );
  expect(radius).toBe("6px");
  // One control, at the bottom of the Modified row, shows and hides Before
  // without moving.
  const showBefore = modified.getByRole("button", { name: /^Show before/ });
  const place = await showBefore.boundingBox();
  await showBefore.click();
  await expect(before).toContainText(
    "Each push needs a fresh lease from the server.",
  );
  const hideBefore = modified.getByRole("button", { name: /^Hide before/ });
  expect(await hideBefore.boundingBox()).toEqual(place);
  await expect(before.getByTestId("ontology-comparison-toggle")).toHaveCount(0);
  await hideBefore.click();
  await expect(before).toBeHidden();

  // All changes shows the same marks.
  await page.goto(`${route}?ontology&all-changes&sentences`);
  await expect(
    page.getByRole("region", { name: "Before statement browser.a" }),
  ).toBeVisible();
  await expect
    .poll(() => highlighted(page))
    .toEqual({
      "ontology-inserted": [
        "Only the owner may force-push to the branch.",
        "with a clear error",
      ],
      "ontology-removed": ["Each push needs a fresh lease from the server."],
    });
});

test("collapsing a Before row leaves the statement where it is", async ({
  page,
}) => {
  // Narrow, so the full Before row wraps well past the excerpt's height.
  await page.setViewportSize({ width: 560, height: 420 });
  await page.goto(`${route}?ontology&changed&sentences`);
  const preview = page.getByTestId("file-view-preview");
  const modified = preview.getByRole("region", {
    name: "Modified statement browser.a",
  });
  const before = preview.getByRole("region", {
    name: "Before statement browser.a",
  });
  await expect(before.getByTestId("ontology-before-excerpt")).toBeVisible();
  await before.getByRole("button", { name: /^Show full/ }).click();
  const showLess = before.getByRole("button", { name: /^Show less/ });
  // The whole pair in view, a little below the top of the reading pane.
  const place = () =>
    modified.evaluate((row) => {
      let pane = row.parentElement!;
      while (!["auto", "scroll"].includes(getComputedStyle(pane).overflowY))
        pane = pane.parentElement!;
      return { top: row.getBoundingClientRect().top, scroll: pane.scrollTop };
    });
  await modified.evaluate((row) => {
    let pane = row.parentElement!;
    while (!["auto", "scroll"].includes(getComputedStyle(pane).overflowY))
      pane = pane.parentElement!;
    pane.scrollTop +=
      row.getBoundingClientRect().top - pane.getBoundingClientRect().top - 10;
  });
  const start = await place();
  const lessTop = await showLess.evaluate((b) => b.getBoundingClientRect().top);
  await showLess.click();
  const showFull = before.getByRole("button", { name: /^Show full/ });
  await expect(showFull).toBeFocused();
  // Nothing scrolls: the statement stays put and its control comes back
  // higher, by however much of Before collapsed.
  expect(await place()).toEqual(start);
  const fullTop = await showFull.evaluate((b) => b.getBoundingClientRect().top);
  expect(lessTop - fullTop).toBeGreaterThan(10);
});
