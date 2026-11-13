import { expect, test, type Page } from "@playwright/test";

// Ontology Preview in a real browser: statement folds, verification status and
// details, and link resolution (g5.documents.ontology.disclosure,
// g5.documents.ontology.verification, g5.documents.ontology.links).

const shell = (page: Page, id: string) =>
  page
    .locator(`[data-record-span="${id}"]`)
    .locator("xpath=ancestor::*[@data-testid='semantic-record-shell'][1]");
const status = (page: Page, id: string) =>
  shell(page, id).getByTestId("verification-status").first();
// What the fixture recorded of files opened and navigation requested.
type Recorded = {
  opened: { path: string; line?: number }[];
  navigated: { type: string; id: string }[];
};
const recorded = (page: Page) =>
  page.evaluate(() => {
    const { opened, navigated } = (
      window as unknown as { ontologyTest: Recorded }
    ).ontologyTest;
    return { opened, navigated };
  });

test.beforeEach(async ({ page }) => {
  await page.goto("/e2e/browser/ontology-verification.html");
  await expect(
    page.getByText("Every receipt needs one editable"),
  ).toBeVisible();
});

test("folds a statement's details and children behind one triangle", async ({
  page,
}) => {
  const toggle = shell(page, "receipt-number").getByTestId(
    "ontology-statement-toggle",
  );
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await expect(
    page.getByText("Suggest one more than the highest"),
  ).toBeHidden();
  await toggle.focus();
  await page.keyboard.press("Enter");
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.getByText("Suggest one more than the highest"),
  ).toBeVisible();
  await expect(
    page.getByText("The number can contain at most 20 characters."),
  ).toBeVisible();
  await expect(
    shell(page, "receipt-access").getByTestId("ontology-statement-dot"),
  ).toBeVisible();
  // One drawn dot, just smaller than the triangle, and none on the row itself.
  const dot = await shell(page, "receipt-access")
    .getByTestId("ontology-statement-dot")
    .evaluate((el) => getComputedStyle(el, "::before").width);
  expect(dot).toBe("9px");
  const rowDot = await shell(page, "receipt-access").evaluate(
    (el) => getComputedStyle(el, "::before").content,
  );
  expect(rowDot).toBe("none");
  // The marker column: a 24px triangle beside 18px statement text.
  const width = await toggle
    .locator("svg")
    .evaluate((el) => getComputedStyle(el).width);
  expect(width).toBe("24px");
  const size = await page
    .getByText("Every receipt needs one editable")
    .evaluate((el) => getComputedStyle(el).fontSize);
  expect(size).toBe("18px");
  await page.getByTestId("ontology-expand-all").click();
  await expect(
    page.getByText("Submission makes the receipt read-only."),
  ).toBeVisible();
  await page.getByTestId("ontology-collapse-all").click();
  await expect(
    page.getByText("Submission makes the receipt read-only."),
  ).toBeHidden();
});

test("shows each status and the saved evidence behind Design verified", async ({
  page,
}) => {
  await expect(status(page, "receipt-number.suggestion")).toHaveText(
    "Design verified",
  );
  await expect(status(page, "receipt-number")).toHaveText("Design verified");
  await expect(status(page, "receipt-number.rules")).toHaveText(
    "Test Coverage",
  );
  await expect(status(page, "receipt-draft")).toHaveText("Unverified");
  await expect(status(page, "receipt-access")).toHaveText("Unverified");

  await status(page, "receipt-number").click();
  const parentPanel = page.getByTestId("verification-panel");
  await expect(parentPanel).toContainText(
    "Design verified through child statements:",
  );
  await parentPanel.getByTestId("verification-child-link").click();
  await expect(
    page.locator('[data-record-span="receipt-number.suggestion"]'),
  ).toBeInViewport();
  await status(page, "receipt-number").click();

  await status(page, "receipt-number.suggestion").click();
  const panel = page.getByTestId("verification-panel");
  await expect(
    panel.getByRole("heading", {
      level: 4,
      name: /What this state machine checks/,
    }),
  ).toHaveText("Receiving numbering: What this state machine checks");
  await expect(panel).toContainText("Is the receipt number unique per vendor");
  await panel.getByText("State machine and verification details").click();
  await expect(panel.getByText("receiving-numbering bindings")).toBeVisible();
  await expect(panel.getByText("PASS", { exact: true })).toBeVisible();
  await expect(panel.getByText("EXPECTED_VIOLATION")).toBeVisible();
  await expect(panel.getByText("26,827")).toBeVisible();
  // Linked bugs and simplifications, with severity and decision
  // (g5.documents.findings).
  const findings = panel.getByTestId("verification-findings");
  await expect(findings).toContainText("Related bugs and simplifications");
  await expect(findings.getByTestId("verification-finding")).toHaveText([
    "High Suggested numbers can repeat after a draft is deleted · Need fix G5L-7",
    "Low Use one numbering sequence for receipts and orders · Open",
  ]);
  await expect(findings.locator(".finding-severity").first()).toHaveCSS(
    "background-color",
    "rgb(220, 38, 38)",
  );
  await findings
    .getByRole("button", {
      name: "Use one numbering sequence for receipts and orders",
    })
    .click();
  expect((await recorded(page)).navigated).toContainEqual(
    expect.objectContaining({
      type: "file",
      id: ".g5/TLA_verification/design_simplification.md",
      intentRecordNumber: "#simp-num-1",
    }),
  );
  await panel.getByRole("button", { name: "Open" }).first().click();
  expect((await recorded(page)).opened).toContainEqual({
    path: ".g5/TLA_verification/results/receiving-numbering/20261024T014713Z-asbuilt-safety-a/receipt.json",
  });
  // BINDINGS.md links resolve from its own folder and open at their line.
  await panel.getByRole("link", { name: "Handler" }).click();
  await expect
    .poll(async () => (await recorded(page)).opened)
    .toContainEqual({ path: "src/Receiving.cs", line: 16 });

  // Unverified opens nothing, even for a design-verified tag without a run.
  await expect(status(page, "receipt-draft")).not.toHaveRole("button");
  await status(page, "receipt-draft").click();
  await expect(
    shell(page, "receipt-draft").getByTestId("verification-panel"),
  ).toHaveCount(0);

  // Other tags show what the statement's metadata records.
  await status(page, "receipt-number.rules").click();
  const tested = shell(page, "receipt-number.rules").getByTestId(
    "verification-panel",
  );
  await expect(tested.getByTestId("verification-metadata")).toContainText(
    "cover length and reuse",
  );
  await expect(tested).not.toContainText("test-coverage");
  await expect(tested).not.toContainText("Expands");
});

test("opens metadata links at their line, with spaces, and from the repository root", async ({
  page,
}) => {
  const details = shell(page, "receipt-number")
    .getByTestId("semantic-record-details-toggle")
    .first();
  await details.click();
  const metadata = shell(page, "receipt-number")
    .getByTestId("semantic-record-metadata")
    .first();
  await metadata.getByRole("link", { name: "Creating a receipt" }).click();
  await expect
    .poll(async () => (await recorded(page)).opened)
    .toContainEqual({ path: "docs/knowledge/receiving.md", line: 26 });
  await metadata.getByRole("link", { name: "guide with spaces" }).click();
  await expect
    .poll(async () => (await recorded(page)).navigated.length)
    .toBe(1);
  expect((await recorded(page)).navigated[0]).toMatchObject({
    type: "file",
    id: "docs/analysis/Receiving Guide.md",
  });
  // Written from the wrong folder: found from the repository root.
  await shell(page, "receipt-access")
    .getByTestId("semantic-record-details-toggle")
    .first()
    .click();
  await shell(page, "receipt-access")
    .getByTestId("semantic-record-metadata")
    .first()
    .getByRole("link", { name: "Access" })
    .click();
  await expect
    .poll(async () => (await recorded(page)).navigated.length)
    .toBe(2);
  expect((await recorded(page)).navigated[1]).toMatchObject({
    id: "docs/knowledge/receiving-access.md",
  });
});

test("keeps Compare's Before and Deleted rows at the text measure", async ({
  page,
}) => {
  await page.evaluate(() => {
    const t = (
      window as unknown as {
        ontologyTest: {
          files: () => Record<string, string>;
          path: () => string;
          load: (f: Record<string, string>, p: string, b?: string) => void;
        };
      }
    ).ontologyTest;
    const current = t.files()[t.path()];
    const baseline =
      current
        .replace("Every receipt needs one editable", "Every receipt needs one")
        .replace(
          "## Metadata",
          '<a id="retired"></a>\n[I · retired]\nA retired rule.\n\n## Metadata',
        ) + "\n### retired\n\n- agent\n";
    t.load(t.files(), t.path(), baseline);
  });
  const before = page.getByRole("region", {
    name: "Before statement receipt-number",
  });
  const deleted = page.getByRole("region", {
    name: "Deleted statement retired",
  });
  // Nothing was removed, so Before waits behind Show before
  // (g5.documents.ontology.change-highlights).
  await page
    .getByRole("button", { name: "Show before, statement receipt-number" })
    .click();
  await expect(before).toBeVisible();
  await expect(deleted).toBeVisible();
  const measure = await page
    .getByText("Receivers record deliveries against a purchase order.")
    .boundingBox();
  // Their prose keeps the measure; the pair's background, change label
  // first, starts where statement text starts.
  for (const row of [before, deleted]) {
    const box = (await row.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(measure!.x - 1);
    const text = (await row
      .getByTestId("semantic-record-blocks")
      .boundingBox())!;
    expect(text.x + text.width).toBeLessThanOrEqual(
      measure!.x + measure!.width + 1,
    );
  }
  await page.screenshot({ path: "/tmp/findings-shots/compare-rows.png" });
  // Before text keeps the editor's size; current statements use 18px.
  const size = await before
    .getByText("Every receipt needs one, suggested")
    .evaluate((el) => getComputedStyle(el).fontSize);
  expect(size).toBe("16px");
});

test("keeps markers neutral in dark mode", async ({ page }) => {
  await page.goto("/e2e/browser/ontology-verification.html?mode=dark");
  const toggle = shell(page, "receipt-number").getByTestId(
    "ontology-statement-toggle",
  );
  const color = await toggle.evaluate((el) => getComputedStyle(el).color);
  const text = await page
    .getByText("Every receipt needs one editable")
    .evaluate((el) => getComputedStyle(el).color);
  expect(color).toBe(text);
});
