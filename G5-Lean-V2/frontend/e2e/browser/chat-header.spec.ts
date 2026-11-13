import { expect, test, type Page } from "@playwright/test";

// The header title takes the width the other controls leave, on one line,
// truncating with the full name on hover; the rename pencil sits right of it;
// picker rows carry activity and times but no rename controls (issue #823).
const LONG =
  "Investigate why the workspace preview tab opens blank after a VM restart and the session list reorders";

async function open(page: Page, params: string) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route("**/api/**", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.goto(`/e2e/browser/chat-header.html?${params}`);
  await expect(page.getByTestId("chat-active-history")).toBeVisible();
  return errors;
}

async function headerGeometry(page: Page) {
  return page.evaluate(() => {
    const trigger = document.querySelector<HTMLElement>(
      '[data-testid="chat-active-history"]',
    )!;
    const label = trigger.querySelector<HTMLElement>("span span")!;
    const pencil = document.querySelector<HTMLElement>(
      '[data-testid="chat-header-rename"]',
    )!;
    const header = trigger.closest<HTMLElement>(".border-b")!;
    const frame = document.querySelector<HTMLElement>('[data-testid="frame"]')!;
    const rect = (node: HTMLElement) => node.getBoundingClientRect();
    const lineHeight = parseFloat(getComputedStyle(label).lineHeight);
    return {
      trigger: rect(trigger),
      pencil: rect(pencil),
      header: rect(header),
      frame: rect(frame),
      labelHeight: rect(label).height,
      lineHeight,
      truncated: label.scrollWidth > label.clientWidth,
      headerOverflow: header.scrollWidth > header.clientWidth,
      title: trigger.getAttribute("title"),
    };
  });
}

for (const layout of ["center", "panel"])
  for (const width of [1100, 420, 320]) {
    test(`${layout} at ${width}px: flexible single-line title and header rename`, async ({
      page,
    }, testInfo) => {
      const errors = await open(
        page,
        `layout=${layout}&width=${width}&title=${encodeURIComponent(LONG)}`,
      );
      const geometry = await headerGeometry(page);
      // One line, truncated only because it does not fit, full name on hover.
      expect(geometry.labelHeight).toBeLessThanOrEqual(geometry.lineHeight + 1);
      expect(geometry.title).toBe(LONG);
      // Truncated only where it does not fit.
      expect(geometry.truncated).toBe(width < 1100);
      expect(geometry.headerOverflow).toBe(false);
      // The pencil sits immediately right of the title control, inside the frame.
      expect(geometry.pencil.left).toBeGreaterThanOrEqual(
        geometry.trigger.right - 1,
      );
      expect(geometry.pencil.left - geometry.trigger.right).toBeLessThan(8);
      expect(geometry.pencil.right).toBeLessThanOrEqual(geometry.frame.right);
      // Wider frames give the title more room than the old 160px cap.
      if (width >= 1100) expect(geometry.trigger.width).toBeGreaterThan(400);
      await page.screenshot({
        path: testInfo.outputPath(`header-${layout}-${width}.png`),
      });

      await page.getByTestId("chat-header-rename").click();
      const input = page.getByTestId("chat-header-title-input");
      await expect(input).toBeFocused();
      await input.fill("Renamed in the header");
      await input.press("Enter");
      await expect(page.getByTestId("chat-header-title-save")).toHaveText(
        "Saving…",
      );
      await expect(page.getByTestId("chat-active-history")).toContainText(
        "Renamed in the header",
      );
      const short = await headerGeometry(page);
      expect(short.truncated).toBe(width < 420);
      expect(errors).toEqual([]);
    });
  }

test("a failed header rename keeps the editor and explains why", async ({
  page,
}) => {
  await open(page, "layout=center&width=900&fail=1");
  await page.getByTestId("chat-header-rename").click();
  await page.getByTestId("chat-header-title-input").fill("Refused");
  await page.getByTestId("chat-header-title-input").press("Enter");
  await expect(page.getByRole("alert")).toHaveText(
    "Could not rename: Workspace is read-only",
  );
  await page.getByTestId("chat-header-title-input").press("Escape");
  await expect(page.getByTestId("chat-active-history")).toContainText(
    "Short title",
  );
});

for (const width of [1100, 320])
  test(`picker rows at ${width}px show activity and times without rename controls`, async ({
    page,
  }, testInfo) => {
    await open(page, `layout=panel&width=${width}`);
    await page.getByTestId("chat-active-history").click();
    const rows = page.getByTestId("chat-session-select");
    await expect(rows).toHaveCount(4);
    await expect(rows.nth(0)).toContainText("Started 3h ago · Active now");
    await expect(rows.nth(1)).toContainText(
      "Started 2h ago · Last active 5m ago",
    );
    await expect(rows.nth(3)).toContainText(
      "Started 1d ago · No finished turns",
    );
    const states = await rows.evaluateAll((items) =>
      items.map((item) =>
        item
          .querySelector('[data-testid="chat-activity-indicator"]')
          ?.getAttribute("data-state"),
      ),
    );
    expect(states).toEqual(["active", "idle", "unseen", "idle"]);
    await expect(
      rows.nth(0).getByRole("img", { name: "Turn running" }),
    ).toBeVisible();
    await expect(page.locator('[data-testid^="rename-session-"]')).toHaveCount(
      0,
    );
    const spinning = await rows
      .nth(0)
      .locator("svg")
      .evaluate((node) => getComputedStyle(node).animationName);
    expect(spinning).not.toBe("none");
    // The menu stays within the viewport.
    const menu = await rows.nth(0).evaluate((node) => {
      const box = node.closest(".absolute")!.getBoundingClientRect();
      return { left: box.left, right: box.right };
    });
    expect(menu.left).toBeGreaterThanOrEqual(0);
    await page.screenshot({ path: testInfo.outputPath(`picker-${width}.png`) });
  });
