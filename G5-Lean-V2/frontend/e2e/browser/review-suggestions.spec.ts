import { expect, test } from "@playwright/test";

const fixture = "/e2e/browser/review-suggestions.html";

for (const view of ["document", "sidebar"]) {
  test(`${view} retains discussion and rejection behavior`, async ({
    page,
  }) => {
    const editId = view === "document" ? "leading-space" : undefined;
    const id = view === "document" ? "whitespace" : "sidebar-13";
    let reply: unknown;
    let rejection: unknown;
    await page.route(`**/review/suggestions/${id}/replies*`, async (route) => {
      if (route.request().method() === "POST")
        reply = route.request().postDataJSON();
      await route.fulfill({ json: { data: { replies: [] } } });
    });
    await page.route(`**/review/suggestions/${id}/reject`, async (route) => {
      rejection = route.request().postDataJSON();
      await route.fulfill({
        status: 409,
        json: {
          error: { code: "CONFLICT", message: "Source changed; review again." },
        },
      });
    });
    await page.goto(`${fixture}?view=${view}`);
    const card =
      view === "document"
        ? page.getByTestId("spec-suggestion-card-whitespace:leading-space")
        : page.getByTestId("suggestion-fallback-sidebar-13");
    await card.getByRole("button", { name: "Discussion", exact: true }).click();
    await expect(card.getByText("No replies yet.")).toBeVisible();
    await card
      .getByRole("textbox", { name: "Reply", exact: true })
      .fill("Please keep the blank line.");
    await card.getByRole("button", { name: "Reply", exact: true }).click();
    await expect
      .poll(() => reply)
      .toEqual({
        content: "Please keep the blank line.",
        ...(editId ? { editId } : {}),
      });
    await card.getByRole("button", { name: "Reject", exact: true }).click();
    await card
      .getByRole("textbox", { name: "Reason" })
      .fill("Keep current spacing.");
    const cardId = editId ? `${id}:${editId}` : id;
    await card
      .getByTestId(`review-suggestion-reject-confirm-${cardId}`)
      .click();
    await expect
      .poll(() => rejection)
      .toEqual({
        reason: "Keep current spacing.",
        ...(editId ? { editId } : {}),
      });
    await expect(
      card.getByTestId(`review-suggestion-error-${cardId}`),
    ).toBeVisible();
    await expect(card.getByRole("textbox", { name: "Reason" })).toHaveValue(
      "Keep current spacing.",
    );
  });
}

test("whitespace source diff stays at its block with an adjacent working card", async ({
  page,
}) => {
  await page.goto(fixture);
  const preview = page.getByTestId("spec-review-source-diff");
  const card = page.getByTestId(
    "spec-suggestion-card-whitespace:leading-space",
  );
  await expect(preview).toBeVisible();
  await expect(preview.locator("pre")).toHaveText("·LGTM");
  const [previewBox, cardBox, firstBox, lastBox] = await Promise.all([
    preview.boundingBox(),
    card.boundingBox(),
    page
      .getByTestId("document-preview")
      .getByText("Intro.", { exact: true })
      .boundingBox(),
    page
      .getByTestId("document-preview")
      .getByText("End.", { exact: true })
      .boundingBox(),
  ]);
  expect(previewBox!.y).toBeGreaterThan(firstBox!.y);
  expect(previewBox!.y).toBeLessThan(lastBox!.y);
  expect(cardBox!.x).toBeGreaterThan(previewBox!.x + previewBox!.width);
  await preview
    .getByRole("button", { name: "Suggested addition: ·", exact: true })
    .click();
  await expect(
    page.getByTestId("spec-review-edit-whitespace:leading-space"),
  ).toBeFocused();
  await card.getByRole("button", { name: "Edit", exact: true }).click();
  await expect(
    card.getByRole("textbox", { name: "Edited replacement" }),
  ).toHaveValue(" ");
  await card.getByRole("textbox", { name: "Edited replacement" }).fill("  ");
  const request = page.waitForRequest((r) =>
    r.url().endsWith("/whitespace/accept"),
  );
  await page.route("**/whitespace/accept", (route) =>
    route.fulfill({
      status: 409,
      json: {
        error: { code: "CONFLICT", message: "Source changed; review again." },
      },
    }),
  );
  await card
    .getByRole("button", { name: "Accept edited", exact: true })
    .click();
  expect((await request).postDataJSON()).toEqual({
    editId: "leading-space",
    afterLines: "  ",
  });
});

test("shared sidebar fills its parent and reaches final controls through one scrollbar", async ({
  page,
}) => {
  await page.goto(`${fixture}?view=sidebar`);
  const sidebar = page.getByTestId("file-sidebar");
  const list = sidebar.getByTestId("suggestion-fallback-list");
  await expect(sidebar).toBeVisible();
  const parentBox = await page.getByTestId("available-parent").boundingBox();
  expect((await sidebar.boundingBox())!.height).toBeCloseTo(
    parentBox!.height,
    0,
  );
  const layout = await list.evaluate((node) => ({
    maxHeight: getComputedStyle(node).maxHeight,
    overflow: getComputedStyle(node).overflowY,
    scrollHeight: node.scrollHeight,
    clientHeight: node.clientHeight,
  }));
  expect(layout.maxHeight).toBe("none");
  expect(layout.overflow).toBe("visible");
  expect(layout.scrollHeight).toBe(layout.clientHeight);
  const finalCard = sidebar.getByTestId("suggestion-fallback-sidebar-13");
  await finalCard
    .getByRole("button", { name: "Edit", exact: true })
    .scrollIntoViewIfNeeded();
  await expect(
    finalCard.getByRole("button", { name: "Edit", exact: true }),
  ).toBeInViewport();
  expect(await sidebar.evaluate((node) => node.scrollTop)).toBeGreaterThan(0);
  expect(
    await page
      .getByTestId("file-document")
      .evaluate((node) => node.parentElement!.scrollTop),
  ).toBe(0);
  await finalCard.getByRole("button", { name: "Edit", exact: true }).click();
  await expect(
    finalCard.getByRole("textbox", { name: "Edited replacement" }),
  ).toHaveValue(" LGTM 13\n");
});

test("below-editor fallback keeps its own height and scrollbar", async ({
  page,
}) => {
  await page.goto(`${fixture}?view=below-editor`);
  const list = page.getByTestId("suggestion-fallback-list");
  await expect(list).toBeVisible();
  expect(await list.evaluate((node) => getComputedStyle(node).overflowY)).toBe(
    "auto",
  );
  const parentBox = await page.getByTestId("available-parent").boundingBox();
  expect((await list.boundingBox())!.height).toBeLessThanOrEqual(
    parentBox!.height * 0.4 + 1,
  );
});

test("six authors share a single document preview with independent drafts and literal actions", async ({
  page,
}) => {
  await page.goto(`${fixture}?view=overlap`);
  const cards = page.getByTestId(/^spec-suggestion-card-/);
  const previews = page.getByTestId(/^spec-suggestion-preview-/);
  await expect(cards).toHaveCount(6);
  await expect(previews).toHaveCount(1);
  await expect(
    page.getByText("Previewing in document", { exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByText("Original proposal context", { exact: true }),
  ).toHaveCount(0);
  const preview = page.getByTestId("spec-review-source-diff");
  const first = cards.nth(0);
  const [previewBox, cardBox] = await Promise.all([
    preview.boundingBox(),
    first.boundingBox(),
  ]);
  expect(cardBox!.x).toBeGreaterThan(previewBox!.x + previewBox!.width);
  await expect(preview.locator("ins")).toHaveText(
    "·an·inplace·edit·with·leading·space",
  );
  await first.getByRole("button", { name: "Edit", exact: true }).click();
  const draft = first.getByRole("textbox", { name: "Edited replacement" });
  await draft.fill("  owner replacement");
  for (let i = 1; i < 6; i++) {
    const card = cards.nth(i);
    const select = card.getByRole("radio", {
      name: "Preview in document",
      exact: true,
    });
    const previousPreview = await preview.innerText();
    await select.focus();
    await page.keyboard.press("Tab");
    await expect(
      card.getByTestId(
        `spec-review-edit-alternative-${i}:alternative-edit-${i}`,
      ),
    ).toBeFocused();
    await expect(card).toHaveAttribute("data-selected", "false");
    expect(await preview.innerText()).toBe(previousPreview);
    await select.focus();
    await page.keyboard.press("Enter");
    await expect(card).toHaveAttribute("data-selected", "true");
    await expect(
      card.getByTestId(
        `spec-review-edit-alternative-${i}:alternative-edit-${i}`,
      ),
    ).toBeFocused();
    await expect(preview.locator("ins")).toHaveText(
      i === 1 ? "kaihui" : `person-${i}`,
    );
    await expect(preview.locator("pre")).toContainText("-provided");
    await expect(previews).toHaveCount(1);
    await expect(cards).toHaveCount(6);
    await expect(draft).toHaveValue("  owner replacement");
  }
  let submitted: unknown;
  await page.route("**/alternative-0/accept", async (route) => {
    submitted = route.request().postDataJSON();
    await route.fulfill({
      status: 409,
      json: {
        error: { code: "CONFLICT", message: "Source changed; review again." },
      },
    });
  });
  await first
    .getByRole("button", { name: "Accept edited", exact: true })
    .click();
  await expect
    .poll(() => submitted)
    .toEqual({
      editId: "alternative-edit-0",
      afterLines: "  owner replacement",
    });
  await expect(first.getByText("Source changed; review again.")).toBeVisible();
  await expect(draft).toHaveValue("  owner replacement");
  await expect(
    cards.nth(5).getByRole("button", { name: "Accept", exact: true }),
  ).toBeEnabled();
});

test("many-author cards stack below their preview in a narrow editor without horizontal overflow", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(`${fixture}?view=overlap`);
  const preview = page.getByTestId("spec-review-source-diff");
  const cards = page.getByTestId(/^spec-suggestion-card-/);
  await expect(cards).toHaveCount(6);
  const [previewBox, cardBox] = await Promise.all([
    preview.boundingBox(),
    cards.first().boundingBox(),
  ]);
  expect(cardBox!.y).toBeGreaterThan(previewBox!.y + previewBox!.height);
  await cards
    .last()
    .getByRole("radio", { name: "Preview in document" })
    .click();
  await expect(preview.locator("ins")).toHaveText("person-5");
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
});

test("independent conflict groups keep their selections and compatible edits in one preview", async ({
  page,
}) => {
  await page.goto(`${fixture}?view=mixed`);
  const preview = page.getByTestId("spec-review-source-diff");
  await expect(page.getByTestId(/^spec-suggestion-card-/)).toHaveCount(5);
  await expect(preview.locator("ins")).toHaveText(["·North", "East", "Done"]);
  await page.getByTestId("spec-review-select-mixed-a2:mixed-a2-edit").focus();
  await page.keyboard.press("Enter");
  await expect(preview.locator("ins")).toHaveText(["·South", "East", "Done"]);
  await page.getByTestId("spec-review-select-mixed-b2:mixed-b2-edit").click();
  await expect(preview.locator("ins")).toHaveText(["·South", "West", "Done"]);
  await expect(page.getByTestId(/^spec-suggestion-preview-/)).toHaveCount(1);
  await expect(
    page.getByTestId("spec-review-select-mixed-solo:mixed-solo-edit"),
  ).toHaveCount(0);
  await expect(
    page.getByTestId("document-preview").locator('[contenteditable="true"]'),
  ).toHaveCount(0);
  await expect(page.getByTestId(/^spec-suggestion-card-/)).toHaveCount(5);
});
