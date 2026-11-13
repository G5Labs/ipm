// Issue #925 regressions, checked against the real browser and bundled Monaco
// through the production FileView (`e2e/fixtures/file-editor.tsx`).
import { expect, test, type Locator, type Page } from "@playwright/test";

const route = "/e2e/fixtures/file-editor.html";
const comments = (page: Page) =>
  page.evaluate(() => (window as any).editorTest.comments());

/** Drags the mouse across `text` inside `within`, as a person selects. */
async function dragSelect(page: Page, within: Locator, text: string) {
  const box = await within.evaluate((element, text) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const at = node.textContent?.indexOf(text) ?? -1;
      if (at < 0) continue;
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + text.length);
      const rect = range.getBoundingClientRect();
      return { x: rect.left, y: rect.top + rect.height / 2, w: rect.width };
    }
    return null;
  }, text);
  if (!box) throw new Error(`no text ${text}`);
  await page.mouse.move(box.x + 1, box.y);
  await page.mouse.down();
  await page.mouse.move(box.x + box.w - 1, box.y, { steps: 5 });
  await page.mouse.up();
  return { x: box.x + box.w / 2, y: box.y };
}

test("Monaco's own selection offers Comment and Add Comment with its quote and line", async ({
  page,
}) => {
  // `g5.documents.comments.identity`: Source owns selection commenting.
  await page.goto(route);
  await page.evaluate(() => (window as any).editorTest.enableComments());
  await page.getByTestId("file-view-switch-mode").click();
  const lines = page.locator(".monaco-editor .view-lines");
  await expect(lines).toContainText("Original.");
  await dragSelect(page, lines, "Original");
  await page.getByTestId("code-editor-selection-comment-button").click();
  const draft = page.getByRole("textbox", { name: "Comment", exact: true });
  await expect(draft).toBeVisible();
  await draft.fill("Selection in Monaco");
  await page.getByRole("button", { name: "Comment", exact: true }).click();
  await expect.poll(async () => (await comments(page)).length).toBe(1);
  const first = (await comments(page))[0];
  expect(first.anchor.lineNumber).toBe(3);
  expect(first.content).toContain("Original");
  expect(first.content).toContain("Selection in Monaco");

  // Right-click → Add Comment quotes Monaco's selection too.
  const shared = await dragSelect(page, lines, "Shared");
  await page.mouse.click(shared.x, shared.y, { button: "right" });
  // Monaco's menu takes a pointer that moved onto the item, as a person's does.
  const addComment = page
    .locator(".monaco-menu")
    .getByText("Add Comment", { exact: true });
  await addComment.hover();
  await addComment.click();
  await expect(page.getByTestId("draft-comment-quote")).toContainText("Shared");
});

test("Monaco's Comment sits centred over the selection, as in the HTML viewer", async ({
  page,
}) => {
  await page.goto(route);
  await page.evaluate(() => {
    const fixture = (window as any).editorTest;
    fixture.enableComments();
    fixture.external(
      "# Centre\n\nshort\nA much longer second line of words to select across.\n",
    );
  });
  await page.getByTestId("file-view-switch-mode").click();
  const lines = page.locator(".monaco-editor .view-lines");
  await expect(lines).toContainText("second line");
  // The selection as Monaco draws it, and the popover's centre.
  const centres = () =>
    page.evaluate(() => {
      const boxes = [
        ...document.querySelectorAll(".monaco-editor .selected-text"),
      ].map((element) => element.getBoundingClientRect());
      const left = Math.min(...boxes.map((box) => box.left));
      const right = Math.max(...boxes.map((box) => box.right));
      const popover = document
        .querySelector('[data-testid="code-editor-selection-comment-button"]')!
        .parentElement!.getBoundingClientRect();
      return {
        selection: (left + right) / 2,
        popover: popover.left + popover.width / 2,
      };
    });
  // One line: between the selection's ends.
  // Monaco renders spaces as no-break spaces.
  await dragSelect(page, lines, "longer\u00a0second\u00a0line");
  await expect(
    page.getByTestId("code-editor-selection-comment-button"),
  ).toBeVisible();
  let at = await centres();
  expect(Math.abs(at.popover - at.selection)).toBeLessThan(4);
  // Two lines: across the box they span.
  const from = await lines.evaluate((element) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    const box = (text: string, offset: number) => {
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const at = node.textContent?.indexOf(text) ?? -1;
        if (at < 0) continue;
        const range = document.createRange();
        range.setStart(node, at + offset);
        range.setEnd(node, at + offset + 1);
        const rect = range.getBoundingClientRect();
        return { x: rect.left, y: rect.top + rect.height / 2 };
      }
      return null;
    };
    return { start: box("short", 2), end: box("second", 3) };
  });
  await page.mouse.move(from.start!.x, from.start!.y);
  await page.mouse.down();
  await page.mouse.move(from.end!.x, from.end!.y, { steps: 5 });
  await page.mouse.up();
  await expect(
    page.getByTestId("code-editor-selection-comment-button"),
  ).toBeVisible();
  at = await centres();
  expect(Math.abs(at.popover - at.selection)).toBeLessThan(4);
});

test("a selection in the reading view opens a draft in the margin at the selected line", async ({
  page,
}) => {
  await page.goto(route);
  await page.evaluate(() => (window as any).editorTest.enableComments());
  await dragSelect(
    page,
    page.getByTestId("document-preview-content"),
    "Original",
  );
  // Hovered, it stays an opaque popover over the text, as in plain text.
  const comment = page.getByTestId("document-preview-selection-comment-button");
  await comment.hover();
  expect(
    await comment.evaluate((element) => {
      let floating: Element | null = element;
      while (floating && getComputedStyle(floating).position !== "fixed")
        floating = floating.parentElement;
      return floating && getComputedStyle(floating).backgroundColor;
    }),
  ).toMatch(/^rgb\(/);
  await comment.click();
  // The draft opens in the margin beside the document; its line shows in
  // the thread it starts.
  const draft = page.getByTestId("comment-draft-card");
  await expect(draft).toBeVisible();
  await draft.getByRole("textbox", { name: "Comment" }).fill("From Preview");
  await draft.getByRole("button", { name: "Comment", exact: true }).click();
  await expect.poll(async () => (await comments(page)).length).toBe(1);
  expect((await comments(page))[0].anchor.lineNumber).toBe(3);
  // The same draft is plain text's: nothing is left open there.
  await page.getByTestId("file-view-switch-mode").click();
  await expect(
    page.getByRole("textbox", { name: "Comment", exact: true }),
  ).toHaveCount(0);
});

test("a statement target opens the reading view from plain text and Rich text, once, with a brief highlight", async ({
  page,
}) => {
  // `g5.documents.ontology.links`
  await page.goto(`${route}?ontology`);
  await page.getByTestId("file-view-switch-mode").click();
  await expect(page.getByTestId("document-preview")).toHaveCount(0);
  await page.evaluate(() => (window as any).editorTest.navigate("#browser.b"));
  const target = page.locator('[data-record-span="browser.b"]');
  await expect(target).toHaveClass(/ring-2/);
  await expect(target).not.toHaveClass(/ring-2/, { timeout: 4000 });
  // Handled, it no longer holds the view.
  await page.getByTestId("file-view-switch-mode").click();
  await expect(page.getByTestId("document-preview")).toHaveCount(0);
  await page.getByTestId("file-view-switch-mode").click();
  await page.getByTestId("file-view-edit").click();
  await expect(page.getByTestId("rich-editor-content")).toBeVisible();
  await page.evaluate(() => (window as any).editorTest.navigate("#browser.a"));
  // The target leaves editing, as Done does.
  await expect(page.getByTestId("rich-editor-content")).toHaveCount(0);
  await expect(page.getByTestId("file-view-edit")).toBeVisible();
  await expect(page.locator('[data-record-span="browser.a"]')).toHaveClass(
    /ring-2/,
  );
});

test("the reading view keeps Compare, filters and its position across mode switches", async ({
  page,
}) => {
  // `g5.documents.editor.modes`
  await page.goto(`${route}?ontology`);
  const compare = page.getByRole("button", { name: "Compare" });
  await expect(compare).toHaveAttribute("aria-pressed", "true");
  await compare.click();
  await page.getByTestId("toolbar-tag-human").click();
  await page.getByTestId("file-view-switch-mode").click();
  await page.getByTestId("file-view-switch-mode").click();
  await expect(compare).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByTestId("toolbar-tag-human")).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  await page.goto(route);
  await page.evaluate(() =>
    (window as any).editorTest.external(
      "# Long\n\n" +
        Array.from({ length: 300 }, (_, i) => `Paragraph ${i}.`).join("\n\n") +
        "\n",
    ),
  );
  await expect(page.getByText("Paragraph 299.")).toBeAttached();
  // The document scrolls in its nearest scrolling ancestor.
  const scrollTop = (set?: number) =>
    page.getByTestId("document-preview").evaluate((element, set) => {
      let el = element.parentElement;
      while (el && !/auto|scroll/.test(getComputedStyle(el).overflowY))
        el = el.parentElement;
      if (el && set !== undefined) el.scrollTop = set;
      return el?.scrollTop ?? 0;
    }, set);
  expect(await scrollTop(2000)).toBeGreaterThan(1900);
  await page.getByTestId("file-view-switch-mode").click();
  await page.getByTestId("file-view-switch-mode").click();
  await expect.poll(() => scrollTop()).toBeGreaterThan(1900);
});
