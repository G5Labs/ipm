import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page, request }) => {
  const html = await (await request.get("/e2e/fixtures/issue841.html")).text();
  await page.route("**/g5labs/**", (route) =>
    route.request().resourceType() === "document"
      ? route.fulfill({ contentType: "text/html", body: html })
      : route.continue(),
  );
  await page.addInitScript(() =>
    localStorage.setItem(
      "g5-user-settings",
      JSON.stringify({
        sidebarDisplayMode: "expanded",
        showTabBar: true,
        showGuidedJourney: false,
        actionBarStyle: "simplified",
      }),
    ),
  );
});
const url = "/g5labs/Task%20Graph%20Rework?view=activity";

for (const mode of ["separate", "combined"]) {
  test(`merge-base metadata and opt-in checkbox with ${mode} bases`, async ({
    page,
  }, info) => {
    await page.addInitScript(
      (mode) => sessionStorage.setItem("issue931-bases", mode),
      mode,
    );
    await page.goto(url.replace("view=activity", "view=source-control"));
    const heading = page.getByRole("button", {
      name: "Source Control",
      exact: true,
    });
    if ((await heading.getAttribute("aria-expanded")) === "false")
      await heading.click();
    const repo = page.getByRole("region", { name: "G5-Lean", exact: true });
    const row = repo.locator("label").filter({ hasText: "Shared ancestor" });
    const checkbox = row.getByRole("checkbox");
    await expect(row).toContainText(
      mode === "combined" ? "Merge base · Workspace base" : "Merge base",
    );
    await expect(row).toContainText("Author · Committed");
    await expect(row.locator("code")).toHaveText("0000000000");
    await expect(checkbox).not.toBeChecked();
    await expect(repo.getByText("Workspace base", { exact: true })).toHaveCount(
      mode === "combined" ? 0 : 1,
    );
    await expect(
      repo.getByText("Since workspace base · 2 commits", { exact: true }),
    ).toHaveCount(mode === "combined" ? 0 : 1);
    await expect(repo.locator("details[open]")).toHaveCount(0);
    await page.getByRole("button", { name: "Show", exact: true }).click();
    await expect(page.getByTestId("source-control-diff-stream")).toBeVisible();
    await expect(page.getByTestId("diff-line-code")).toHaveCount(3);
    const before = await page
      .getByTestId("source-control-diff-stream")
      .textContent();
    await checkbox.focus();
    await page.keyboard.press("Space");
    await expect(checkbox).toBeChecked();
    await expect(
      repo.getByText(/Selected revision .* is unavailable/),
    ).toHaveCount(0);
    expect(
      await page.getByTestId("source-control-diff-stream").textContent(),
    ).toBe(before);
    await page.getByRole("button", { name: "Show", exact: true }).click();
    await expect
      .poll(async () =>
        page.evaluate(() =>
          JSON.parse(sessionStorage.getItem("issue931-last-apply") ?? "null")
            ?.repos?.find((r: { repoId: string }) => r.repoId === "code")
            ?.commits?.includes((30).toString(16).padStart(40, "0")),
        ),
      )
      .toBe(true);
    await page
      .getByRole("checkbox", { name: "All changes", exact: true })
      .check();
    await expect(checkbox).not.toBeChecked();
    await page.getByRole("button", { name: "Show", exact: true }).click();
    await expect
      .poll(async () =>
        page.evaluate(
          () =>
            JSON.parse(
              sessionStorage.getItem("issue931-last-apply") ?? "null",
            )?.repos?.find((r: { repoId: string }) => r.repoId === "code")
              ?.allPushed,
        ),
      )
      .toBe(true);
    await expect(page.getByTestId("diff-line-code")).toHaveCount(3);
    await expect(page.getByTestId("source-control-diff-stream")).toHaveText(
      before!,
    );
    await row.scrollIntoViewIfNeeded();
    await repo
      .getByText("Earlier history", { exact: true })
      .scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath(`merge-base-${mode}.png`) });
  });
}

test("Base workspace retains disabled outgoing work and collapsed selectable history", async ({
  page,
}) => {
  await page.addInitScript(() =>
    sessionStorage.setItem("issue931-bases", "base"),
  );
  await page.goto(url.replace("view=activity", "view=source-control"));
  const heading = page.getByRole("button", {
    name: "Source Control",
    exact: true,
  });
  if ((await heading.getAttribute("aria-expanded")) === "false")
    await heading.click();
  const repo = page.getByRole("region", { name: "G5-Lean", exact: true });
  await expect(
    repo.getByRole("checkbox", { name: "Not yet pushed in G5-Lean" }),
  ).toBeDisabled();
  await expect(repo.getByText("No outgoing comparison")).toBeVisible();
  await expect(repo.getByText(/Merge base|Workspace base/)).toHaveCount(0);
  await expect(repo.locator("details[open]")).toHaveCount(0);
  await repo.getByText("Show history", { exact: true }).click();
  const row = repo
    .locator("label")
    .filter({ hasText: "Imported parent change" });
  await expect(row).toContainText("Author · Committed");
  await row.getByRole("checkbox").check();
  await expect(row.getByRole("checkbox")).toBeChecked();
});

test("large loaded history skips offscreen rendering while preserving checkbox focus and selection", async ({
  page,
}) => {
  test.setTimeout(60_000);
  await page.addInitScript(() =>
    sessionStorage.setItem("issue841-large-history", "1"),
  );
  await page.goto(url);
  await page.getByTestId("landing-summary-code").click();
  const heading = page.getByRole("button", {
    name: "Source Control",
    exact: true,
  });
  if ((await heading.getAttribute("aria-expanded")) === "false")
    await heading.click();
  await page.getByText("Since workspace base · 2 commits").first().click();
  const history = page.locator("details.source-control-history[open]").first();
  const first = history
    .locator("label")
    .filter({ hasText: "Historical commit 0000" })
    .getByRole("checkbox");
  await first.check();
  const before = await page
    .getByTestId("source-control-diff-stream")
    .textContent();
  for (let index = 1; index < 20; index++) {
    await history
      .getByRole("button", { name: "Show more", exact: true })
      .click();
    await expect(history.locator(".source-control-history-page")).toHaveCount(
      index + 1,
    );
  }
  await history
    .getByText("Historical commit 0999", { exact: true })
    .scrollIntoViewIfNeeded();
  await expect(
    history.getByText("Historical commit 0999", { exact: true }),
  ).toBeInViewport();
  await expect(first).toBeChecked();
  expect(
    await first.evaluate((element) =>
      element.checkVisibility({ contentVisibilityAuto: true }),
    ),
  ).toBe(false);
  await first.focus();
  await expect(first).toBeFocused();
  await expect(first).toBeInViewport();
  await page.keyboard.press("Space");
  await expect(first).not.toBeChecked();
  expect(
    await page.evaluate(() => sessionStorage.getItem("issue841-history-reads")),
  ).toBe("20");
  expect(
    await page.getByTestId("source-control-diff-stream").textContent(),
  ).toBe(before);
});

test("count destinations, one-time types, reload, history paging and wide diffs", async ({
  page,
}, info) => {
  await page.goto(url);
  await expect(page.getByTestId("landing-summary-changed")).toHaveText(
    "3 changed files",
  );
  const docs = page.getByRole("button", { name: "Project Docs", exact: true });
  if ((await docs.getAttribute("aria-expanded")) === "true") await docs.click();
  await page.getByTestId("landing-summary-docs").click();
  await expect(docs).toHaveAttribute("aria-expanded", "true");
  await docs.click();
  await page.keyboard.press("Control+Backslash");
  await expect(docs).toHaveCount(0);
  await page.keyboard.press("Control+Backslash");
  await expect(docs).toHaveAttribute("aria-expanded", "false");
  await page.getByTestId("landing-summary-docs").click();
  await expect(docs).toHaveAttribute("aria-expanded", "true");
  await expect(
    page.getByRole("tab", { name: "Project Docs", exact: true }),
  ).toHaveCount(0);
  await page.getByTestId("landing-summary-changed").click();
  await expect(page.getByTestId("source-control-type-summary")).toContainText(
    "3 matching of 3 changed files",
  );
  await expect(page).not.toHaveURL(/scope=|heads=/);
  await expect(
    page.getByRole("tab", { name: "Source Control", exact: true }),
  ).toHaveCount(1);
  await page.getByRole("tab", { name: "Workspace", exact: true }).click();
  await page.getByTestId("landing-summary-code").click();
  await expect(page.getByTestId("source-control-type-summary")).toContainText(
    "Code 1 — 1 matching of 3 changed files",
  );
  const show = page.getByTestId("source-control-show-changes");
  const heading = page.getByRole("button", {
    name: "Source Control",
    exact: true,
  });
  if ((await heading.getAttribute("aria-expanded")) === "false")
    await heading.click();
  await expect(show).toBeInViewport();
  await page.getByRole("checkbox", { name: "Ontology", exact: true }).check();
  await show.click();
  await expect(page.getByTestId("source-control-type-summary")).toContainText(
    "2 matching of 3 changed files",
  );
  await page.reload();
  await expect(page.getByTestId("source-control-type-summary")).toContainText(
    "2 matching of 3 changed files",
  );
  if ((await heading.getAttribute("aria-expanded")) === "false")
    await heading.click();
  await expect(
    page.getByRole("checkbox", { name: "Docs", exact: true }),
  ).not.toBeChecked();
  expect(
    await page.evaluate(() => sessionStorage.getItem("issue841-history-reads")),
  ).toBeNull();
  const before = await page.evaluate(() =>
    sessionStorage.getItem("issue841-diff-reads"),
  );
  await page.getByText("Since workspace base · 2 commits").first().click();
  await expect(page.getByText("Imported parent change")).toBeVisible();
  await page
    .getByRole("button", { name: "Show more", exact: true })
    .first()
    .click();
  await expect(page.getByText("Older parent change")).toBeVisible();
  expect(
    await page.evaluate(() => sessionStorage.getItem("issue841-history-reads")),
  ).toBe("2");
  expect(
    await page.evaluate(() => sessionStorage.getItem("issue841-diff-reads")),
  ).toBe(before);
  await expect(show).toBeInViewport();
  const scroll = page.getByRole("region", { name: "File diff" }).first();
  await expect(scroll).toHaveAttribute("tabindex", "0");
  expect(
    await scroll.evaluate(
      (element) => element.scrollWidth > element.clientWidth,
    ),
  ).toBe(true);
  await scroll.focus();
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(() => scroll.evaluate((element) => element.scrollLeft))
    .toBeGreaterThan(0);
  await page.screenshot({ path: info.outputPath("source-control.png") });
});

test("repository columns align in both readers at wide and narrow widths", async ({
  page,
}, info) => {
  await page.goto(url);
  await expect(page.getByTestId("landing-repository-row")).toHaveCount(2);
  for (const width of [1440, 1100]) {
    await page.setViewportSize({ width, height: 900 });
    const rows = page.getByTestId("landing-repository-row");
    const positions = await rows.evaluateAll((elements) =>
      elements.map((element) => ({
        name: element.children[0].getBoundingClientRect().x,
        sha: element.children[1].getBoundingClientRect().x,
      })),
    );
    expect(positions[0].name).toBe(positions[1].name);
    expect(positions[0].sha).toBe(positions[1].sha);
    await expect(rows.nth(1)).not.toContainText("Open");
    await page.getByTestId("workspace-read-implementation").click();
    const dialog = page.getByRole("dialog", {
      name: "Implementation description",
    });
    await expect(dialog).toBeVisible();
    const aligned = await dialog
      .getByTestId("landing-repository-row")
      .evaluateAll((elements) =>
        elements.map(
          (element) => element.children[1].getBoundingClientRect().x,
        ),
      );
    expect(aligned[0]).toBe(aligned[1]);
    await page.screenshot({ path: info.outputPath(`repository-${width}.png`) });
    await page.keyboard.press("Escape");
  }
});

test("approval readers preserve expanded history and focus without opening tabs", async ({
  page,
  context,
}, info) => {
  await page.goto(url.replace("activity", "approvals"));
  const timeline = page.getByRole("button", {
    name: "Timeline for Review policy",
    exact: true,
  });
  await timeline.click();
  const inspect = page
    .getByRole("button", { name: "Inspect review", exact: true })
    .first();
  await expect(inspect).toBeVisible();
  const pages = context.pages().length;
  const positions = () =>
    inspect.evaluate((element) => {
      const values: number[] = [];
      for (
        let parent = element.parentElement;
        parent;
        parent = parent.parentElement
      )
        values.push(parent.scrollTop);
      return values;
    });
  // The collapsed Timeline now opens below the requested-approver roster
  // (g5.governance.approval.request-status), so bring the reader into view
  // first: the click itself must not be what moves the page.
  await inspect.scrollIntoViewIfNeeded();
  const scrollBefore = await positions();
  await inspect.click();
  let modal = page.getByRole("dialog", { name: "Inspect review" });
  await expect(modal).toContainText("Not captured yet");
  await expect(modal).toContainText(
    "Review failed during capture: Comparison unavailable",
  );
  await expect(modal.locator("details[open]")).toHaveCount(0);
  await expect(
    modal.getByRole("button", { name: /Retry|Resubmit|Edit|Open in/ }),
  ).toHaveCount(0);
  await page.screenshot({ path: info.outputPath("inspection.png") });
  await page.keyboard.press("Escape");
  await expect(inspect).toBeFocused();
  expect(await positions()).toEqual(scrollBefore);
  await expect(timeline).toHaveAttribute("aria-expanded", "true");
  const report = page
    .getByRole("button", { name: "Read report", exact: true })
    .first();
  await report.click();
  modal = page.getByRole("dialog", { name: "Agent review report" });
  await expect(
    modal.getByRole("heading", { name: "Review report", exact: true }),
  ).toBeVisible();
  await modal.getByRole("button", { name: "Raw", exact: true }).click();
  await expect(modal.getByTestId("approval-report-source")).toContainText(
    "submittedAt: 2026-10-27T10:01:00Z",
  );
  await modal.getByRole("button", { name: "View", exact: true }).click();
  await modal.getByText("Report frontmatter", { exact: true }).click();
  await expect(modal.locator("details[open]")).toHaveCount(1);
  await modal.getByRole("button", { name: "Close", exact: true }).click();
  await expect(report).toBeFocused();
  expect(context.pages()).toHaveLength(pages);
  await expect(timeline).toHaveAttribute("aria-expanded", "true");
});
