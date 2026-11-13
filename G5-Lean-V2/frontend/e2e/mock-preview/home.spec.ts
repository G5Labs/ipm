import { expect, test, type Page } from "@playwright/test";

// The frontend-only preview shows the alignment Home with every tile
// populated by the mock API (src/mocks/home-preview.ts), and the Issues and
// tasks tile's search, filters, Link and Create work against it.

const HOME = "/g5labs/home";
const issueRow = (page: Page, number: number, repositoryId = "880001") =>
  page.getByTestId(`home-issue-${repositoryId}${number}`);

/** Console warnings from mock routes the preview does not answer. */
function collectUnhandled(page: Page): string[] {
  const unhandled: string[] = [];
  page.on("console", (message) => {
    if (message.text().includes("[MockFetch] No handler")) {
      unhandled.push(message.text());
    }
  });
  return unhandled;
}

test("Home renders the alignment tiles with data and no missing routes", async ({
  page,
}) => {
  const unhandled = collectUnhandled(page);
  await page.goto(HOME);

  await expect(page.getByTestId("home-workspaces-count")).toHaveText("4");
  await expect(page.getByTestId("home-workspace-base")).toBeVisible();

  const attention = page.getByTestId("home-needs-attention");
  await expect(
    attention.getByTestId("attention-kind-badge-approval"),
  ).toHaveCount(2);
  await expect(
    attention.getByTestId("attention-kind-badge-open_question"),
  ).toHaveCount(2);
  await expect(page.getByTestId("home-attention-retry")).toHaveCount(0);

  // Assigned to me first (Home defaults to the caller's own); All is a
  // press away.
  await expect(page.getByTestId("home-issues-filter-me")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("home-issues-count")).toHaveText("3");
  await page.getByTestId("home-issues-filter-any").click();
  await expect(page.getByTestId("home-issues-count")).toHaveText("10");
  await expect(issueRow(page, 742)).toContainText(
    "Home tile header actions break onto a new line",
  );
  await expect(
    issueRow(page, 742).getByTestId("home-issue-workspace-chip"),
  ).toHaveText("Task Graph Rework");

  await expect(page.getByTestId("home-compute-count")).toHaveText("3");
  await expect(page.getByTestId("home-compute-filter-mine")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByTestId("home-compute-base")).toContainText("4 vCPU");
  await expect(
    page.getByTestId("home-compute-base").getByTestId("home-compute-open"),
  ).toHaveAttribute("href", "/g5labs/base");
  await expect(
    page.getByTestId("home-compute-base").getByTestId("home-compute-shutdown"),
  ).toHaveText("Stop");
  await expect(page.getByTestId("home-compute-auth-refactor")).toContainText(
    "starting",
  );

  expect(unhandled).toEqual([]);
});

test("issue search narrows by terms and labels and clearing restores the slice", async ({
  page,
}) => {
  await page.goto(HOME);
  const tile = page.getByTestId("home-issues-tile");
  const search = page.getByTestId("home-issues-search");
  await page.getByTestId("home-issues-filter-any").click();
  await expect(page.getByTestId("home-issues-count")).toHaveText("10");

  await search.fill("queue");
  await search.press("Enter");
  await expect(page.getByTestId("home-issues-count")).toHaveText("1");
  await expect(issueRow(page, 728)).toBeVisible();

  await search.fill("label:docs");
  await search.press("Enter");
  await expect(page.getByTestId("home-issues-count")).toHaveText("2");
  await expect(issueRow(page, 118, "880002")).toBeVisible();
  await expect(issueRow(page, 112, "880002")).toBeVisible();

  await search.fill("no such words");
  await search.press("Enter");
  await expect(tile).toContainText("No open issues match “no such words”");

  await tile.getByRole("button", { name: "Clear search" }).click();
  await expect(page.getByTestId("home-issues-count")).toHaveText("10");
});

test("Assigned to me, the default, keeps the connected account's issues", async ({
  page,
}) => {
  await page.goto(HOME);
  await expect(page.getByTestId("home-issues-filter-me")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(issueRow(page, 742)).toBeVisible();
  await expect(issueRow(page, 728)).toBeVisible();
  await expect(issueRow(page, 711)).toBeVisible();
  await expect(issueRow(page, 739)).toHaveCount(0);

  await page.getByTestId("home-issues-filter-none").click();
  await expect(issueRow(page, 731)).toBeVisible();
  await expect(issueRow(page, 742)).toHaveCount(0);

  await page.getByTestId("home-issues-filter-any").click();
  await expect(page.getByTestId("home-issues-count")).toHaveText("10");
});

test("Link existing makes the chosen workspace the issue's primary, as the workspace shows", async ({
  page,
}) => {
  await page.goto(HOME);
  await page.getByTestId("home-issues-filter-any").click();
  await expect(page.getByTestId("home-issues-count")).toHaveText("10");
  const row = issueRow(page, 739);
  await row.getByTestId("home-issue-link-existing").click();
  await row.getByTestId("home-issue-link-target").selectOption({
    label: "Auth Refactor",
  });
  await row.getByTestId("home-issue-link-submit").click();

  await expect(row.getByTestId("home-issue-workspace-chip")).toHaveText(
    "Auth Refactor",
  );
  await expect(row.getByTestId("home-issue-open")).toBeVisible();

  // The workspace reads the same link: it opens with the issue as primary.
  await row.getByTestId("home-issue-open").click();
  await expect(page).toHaveURL(/\/g5labs\/Auth%20Refactor(\?|$)/);
  await expect(page.getByText("g5labs/platform#739").first()).toBeVisible();
});

test("Create opens the new issue's own page", async ({ page }) => {
  await page.goto(HOME);
  await expect(page.getByTestId("home-issues-count")).toHaveText("3");
  await page.getByTestId("home-issues-create").click();
  await expect(page.getByTestId("create-issue-destination")).toContainText(
    "g5labs/platform",
  );
  await page.getByTestId("create-issue-title").fill("Preview the real Home");
  await page.getByTestId("create-issue-submit").click();

  await expect(page).toHaveURL(/\/g5labs\/issues\/github\/880001\/743$/);
  await expect(page.getByText("Preview the real Home").first()).toBeVisible();
});

test("Create lists the new issue on Home within the session", async ({
  page,
}) => {
  await page.goto(HOME);
  await expect(page.getByTestId("home-issues-count")).toHaveText("3");
  await page.getByTestId("home-issues-create").click();
  await page.getByTestId("create-issue-title").fill("Listed after creation");
  await page.getByTestId("create-issue-submit").click();
  await expect(page).toHaveURL(/\/issues\/github\/880001\/743$/);

  await page.goBack();
  await expect(page).toHaveURL(new RegExp(`${HOME}$`));
  // The new issue is unassigned, so it shows under All.
  await expect(page.getByTestId("home-issues-count")).toHaveText("3");
  await page.getByTestId("home-issues-filter-any").click();
  await expect(page.getByTestId("home-issues-count")).toHaveText("11");
  await expect(issueRow(page, 743)).toContainText("Listed after creation");
});

test("the search sits in the band below the header; filters, Create and Refresh share one row", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto(HOME);
  await expect(page.getByTestId("home-issues-count")).toHaveText("3");

  const box = async (testId: string) => {
    const rect = await page.getByTestId(testId).boundingBox();
    if (!rect) throw new Error(`${testId} is not rendered`);
    return rect;
  };
  const title = await box("home-issues-count");
  const search = await box("home-issues-search");
  const all = await box("home-issues-filter-any");
  const create = await box("home-issues-create");
  const refresh = await box("home-issues-refresh");

  // The search lives in the list's toolbar band, below the header.
  expect(search.y).toBeGreaterThan(title.y + title.height - 1);
  expect(search.y).toBeGreaterThan(all.y + all.height - 1);
  // Filters, Create and Refresh share one row: the refresh icon never sits
  // alone on a line of its own.
  const middle = (r: { y: number; height: number }) => r.y + r.height / 2;
  expect(Math.abs(middle(create) - middle(all))).toBeLessThan(4);
  expect(Math.abs(middle(refresh) - middle(create))).toBeLessThan(4);
  expect(refresh.x).toBeGreaterThan(create.x);
});
