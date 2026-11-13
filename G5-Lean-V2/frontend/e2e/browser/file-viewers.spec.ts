import { expect, test, type Page } from "@playwright/test";

// `g5.documents.viewers`, `g5.documents.viewers.images` and
// `g5.documents.viewers.pdf` on Chromium's real decoders and PDF.js.

async function open(page: Page, params: Record<string, string>) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("dialog", (dialog) => {
    errors.push(`dialog: ${dialog.message()}`);
    void dialog.dismiss();
  });
  await page.route("**/api/**", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.goto(
    `/e2e/browser/file-viewers.html?${new URLSearchParams(params)}`,
  );
  return errors;
}

async function imageLoaded(page: Page) {
  const image = page.getByTestId("image-view-image");
  await expect(image).toHaveAttribute("data-status", "loaded");
  return image;
}

/** Whether any pixel the page canvas holds is not white. */
async function canvasHasInk(page: Page, pageNumber: number) {
  return page.evaluate((pageNumber) => {
    const canvas = document.querySelector<HTMLCanvasElement>(
      `[data-testid="pdf-page"][data-page-number="${pageNumber}"] canvas`,
    );
    if (!canvas || !canvas.width) return false;
    const context = canvas.getContext("2d");
    if (!context) return false;
    const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
    for (let index = 0; index < data.length; index += 4) {
      if (data[index] < 250 || data[index + 1] < 250 || data[index + 2] < 250)
        return true;
    }
    return false;
  }, pageNumber);
}

test("a transparent PNG decodes over a checkerboard, opens at its own size and steps the zoom", async ({
  page,
}) => {
  const errors = await open(page, { fixture: "transparent.png" });
  const image = await imageLoaded(page);
  await expect(page.getByTestId("image-view-dimensions")).toHaveText(
    /64 × 48 · 6\.1 KB/,
  );
  // Fit never enlarges: a small image shows at its own size.
  await expect(page.getByTestId("image-view-zoom-level")).toHaveText("100%");
  expect((await image.boundingBox())!.width).toBe(64);
  expect(
    await image.evaluate((element) =>
      getComputedStyle(element).backgroundImage.includes("conic-gradient"),
    ),
  ).toBe(true);
  await page.getByTestId("image-view-zoom-in").click();
  await expect(page.getByTestId("image-view-zoom-level")).toHaveText("125%");
  expect((await image.boundingBox())!.width).toBe(80);
  await page.getByTestId("image-view-zoom-in").click();
  await page.getByTestId("image-view-zoom-in").click();
  await expect(page.getByTestId("image-view-zoom-level")).toHaveText("195%");
  await page.getByTestId("image-view-actual").click();
  expect((await image.boundingBox())!.width).toBe(64);
  await page.getByTestId("image-view-stage").screenshot({
    path: test.info().outputPath("transparent-png.png"),
  });

  const download = page.waitForEvent("download");
  await page.getByTestId("image-view-download").click();
  expect((await download).suggestedFilename()).toBe("transparent.png");
  expect(errors).toEqual([]);
});

test("a large image fits the panel, and Actual size scrolls it", async ({
  page,
}) => {
  await open(page, { fixture: "large.png" });
  const image = await imageLoaded(page);
  await expect(page.getByTestId("image-view-dimensions")).toHaveText(
    /1600 × 1200/,
  );
  const stage = page.getByTestId("image-view-stage");
  const panel = (await stage.boundingBox())!;
  const fitted = (await image.boundingBox())!;
  expect(fitted.width).toBeLessThanOrEqual(panel.width);
  expect(fitted.height).toBeLessThanOrEqual(panel.height);
  expect(fitted.width).toBeLessThan(1600);
  const level = await page.getByTestId("image-view-zoom-level").textContent();
  expect(Number(level!.replace("%", ""))).toBeLessThan(100);
  expect(
    await stage.evaluate(
      (element) => element.scrollWidth <= element.clientWidth,
    ),
  ).toBe(true);

  await page.getByTestId("image-view-actual").click();
  await expect(page.getByTestId("image-view-zoom-level")).toHaveText("100%");
  expect(
    await stage.evaluate(
      (element) =>
        element.scrollWidth > element.clientWidth &&
        element.scrollHeight > element.clientHeight,
    ),
  ).toBe(true);
  await page.getByTestId("image-view-fit").click();
  expect((await image.boundingBox())!.width).toBeLessThanOrEqual(panel.width);
});

test("an animated GIF decodes", async ({ page }) => {
  const errors = await open(page, { fixture: "animated.gif" });
  await imageLoaded(page);
  await expect(page.getByTestId("image-view-dimensions")).toHaveText(/16 × 16/);
  expect(errors).toEqual([]);
});

test("an image declaring more than the pixel bound is refused before decoding", async ({
  page,
}) => {
  await open(page, { fixture: "huge-header.png" });
  const notice = page.getByTestId("image-view-notice");
  await expect(notice).toHaveAttribute("data-reason", "too-large");
  await expect(notice).toContainText("30000 × 30000");
  await expect(page.getByTestId("image-view-image")).toHaveCount(0);
  await expect(page.getByTestId("image-view-notice-download")).toBeVisible();
});

test("a corrupt image and a text file misnamed as an image offer Download", async ({
  page,
}) => {
  await open(page, { fixture: "corrupt.png" });
  await expect(page.getByTestId("image-view-notice")).toHaveAttribute(
    "data-reason",
    "undecodable",
  );
  const download = page.waitForEvent("download");
  await page.getByTestId("image-view-notice-download").click();
  expect((await download).suggestedFilename()).toBe("corrupt.png");

  await open(page, { text: "plain words, not pixels", name: "notes.png" });
  await expect(page.getByTestId("image-view-notice")).toHaveAttribute(
    "data-reason",
    "undecodable",
  );
  await expect(page.getByTestId("file-view")).toHaveCount(0);
});

test("an SVG renders as an image where its script never runs, and edits as plain text", async ({
  page,
}) => {
  const errors = await open(page, { fixture: "scripted.svg" });
  await imageLoaded(page);
  await expect(page.getByTestId("image-view-dimensions")).toHaveText(
    /120 × 80/,
  );
  expect(
    await page.evaluate(() => (window as { __g5SvgRan?: boolean }).__g5SvgRan),
  ).toBeUndefined();
  // The SVG's script and text never enter the document: only the harness's
  // own scripts exist, and no SVG text element renders "SVG".
  expect(await page.locator("script", { hasText: "__g5SvgRan" }).count()).toBe(
    0,
  );
  expect(await page.locator("svg text").count()).toBe(0);
  await expect(page.getByTestId("file-view-switch-mode")).toHaveText(
    "Switch to plain text",
  );
  await page.getByTestId("file-view-switch-mode").click();
  await expect(page.getByTestId("image-view")).toHaveCount(0);
  const editor = page.getByTestId("code-editor");
  await expect(editor.locator(".monaco-editor .view-lines")).toContainText(
    "window.__g5SvgRan",
  );
  await page.getByTestId("file-view-switch-mode").click();
  await imageLoaded(page);
  expect(
    await page.evaluate(() => (window as { __g5SvgRan?: boolean }).__g5SvgRan),
  ).toBeUndefined();
  expect(errors).toEqual([]);
});

test("a PDF renders its pages with selectable text, searches, navigates, zooms, rotates and downloads", async ({
  page,
}) => {
  const errors = await open(page, { fixture: "text.pdf" });
  await expect(page.getByTestId("pdf-view-page-count")).toHaveText("/ 3");
  const first = page.locator('[data-testid="pdf-page"][data-page-number="1"]');
  await expect(first).toHaveAttribute("data-rendered", "true");
  expect(await canvasHasInk(page, 1)).toBe(true);
  // Fit width is the opening zoom: the page spans the stage.
  const stage = page.getByTestId("pdf-view-stage");
  const panel = (await stage.boundingBox())!;
  const pageBox = (await first.boundingBox())!;
  expect(pageBox.width).toBeGreaterThan(panel.width * 0.9);
  expect(pageBox.width).toBeLessThanOrEqual(panel.width);
  await expect(first.locator(".textLayer span").first()).toHaveText(
    "Hello G5 page 1 of 3",
  );
  await page.screenshot({ path: test.info().outputPath("text-pdf.png") });

  // Text is selectable.
  const run = first.locator(".textLayer span", { hasText: "quarterly" });
  await run.click({ clickCount: 3 });
  expect(await page.evaluate(() => getSelection()?.toString() ?? "")).toContain(
    "quarterly",
  );

  // Search steps through the three pages' matching runs.
  await page.getByTestId("pdf-view-search").fill("quarterly");
  await expect(page.getByTestId("pdf-view-search-count")).toHaveText("1 of 3");
  await expect(run).toHaveClass(/g5-pdf-match-current/);
  await page.getByTestId("pdf-view-search-next").click();
  await expect(page.getByTestId("pdf-view-search-count")).toHaveText("2 of 3");
  await expect(page.getByTestId("pdf-view-page")).toHaveValue("2");
  await expect(
    page
      .locator('[data-testid="pdf-page"][data-page-number="2"] .textLayer span')
      .filter({ hasText: "quarterly" }),
  ).toHaveClass(/g5-pdf-match-current/);
  await page.getByTestId("pdf-view-search").fill("");
  await expect(page.getByTestId("pdf-view-search-count")).toHaveText("");

  // Navigation.
  await page.getByTestId("pdf-view-page").fill("3");
  await page.getByTestId("pdf-view-page").press("Enter");
  await expect(page.getByTestId("pdf-view-page")).toHaveValue("3");
  await expect(
    page.locator('[data-testid="pdf-page"][data-page-number="3"]'),
  ).toHaveAttribute("data-rendered", "true");
  await page.getByTestId("pdf-view-prev").click();
  await expect(page.getByTestId("pdf-view-page")).toHaveValue("2");

  // Zoom presets and rotation.
  await page.getByTestId("pdf-view-zoom-out").click();
  await expect(page.getByTestId("pdf-view-zoom-level")).toHaveText(/%$/);
  const narrower = (await first.boundingBox())!;
  expect(narrower.width).toBeLessThan(pageBox.width);
  await page.getByTestId("pdf-view-rotate").click();
  await expect
    .poll(async () => {
      const box = (await first.boundingBox())!;
      return box.width > box.height;
    })
    .toBe(true);
  await page.getByTestId("pdf-view-fit-width").click();
  await expect
    .poll(async () => (await first.boundingBox())!.width)
    .toBeGreaterThan(panel.width * 0.9);

  const download = page.waitForEvent("download");
  await page.getByTestId("pdf-view-download").click();
  expect((await download).suggestedFilename()).toBe("text.pdf");
  expect(errors).toEqual([]);
});

test("a sixty-page PDF renders only pages near the viewport and releases the rest", async ({
  page,
}) => {
  await open(page, { fixture: "long.pdf" });
  await expect(page.getByTestId("pdf-view-page-count")).toHaveText("/ 60");
  await expect(
    page.locator('[data-testid="pdf-page"][data-page-number="1"]'),
  ).toHaveAttribute("data-rendered", "true");
  await expect
    .poll(() =>
      page.locator('[data-testid="pdf-page"][data-rendered="true"]').count(),
    )
    .toBeLessThan(10);
  await page.getByTestId("pdf-view-page").fill("60");
  await page.getByTestId("pdf-view-page").press("Enter");
  const last = page.locator('[data-testid="pdf-page"][data-page-number="60"]');
  await expect(last).toHaveAttribute("data-rendered", "true");
  await expect(last.locator(".textLayer span").first()).toHaveText(
    "Hello G5 page 60 of 60",
  );
  await expect(
    page.locator('[data-testid="pdf-page"][data-page-number="1"]'),
  ).toHaveAttribute("data-rendered", "false");
  expect(
    await page.evaluate(
      () =>
        document.querySelector<HTMLCanvasElement>(
          '[data-testid="pdf-page"][data-page-number="1"] canvas',
        )!.width,
    ),
  ).toBe(0);
});

test("a protected PDF asks for its password, refuses a wrong one, opens with the right one, and Cancel keeps Download", async ({
  page,
}) => {
  await open(page, { fixture: "protected.pdf" });
  const form = page.getByTestId("pdf-view-password");
  await expect(form).toContainText("password protected");
  await page.getByTestId("pdf-view-password-input").fill("wrong");
  await page.getByTestId("pdf-view-password-submit").click();
  await expect(form).toContainText("incorrect");
  await page.getByTestId("pdf-view-password-input").fill("secret");
  await page.getByTestId("pdf-view-password-submit").click();
  await expect(page.getByTestId("pdf-view-page-count")).toHaveText("/ 1");
  await expect(
    page.locator('[data-testid="pdf-page"][data-page-number="1"]'),
  ).toHaveAttribute("data-rendered", "true");

  await page.reload();
  await expect(page.getByTestId("pdf-view-password")).toBeVisible();
  await page.getByTestId("pdf-view-password-cancel").click();
  const notice = page.getByTestId("pdf-view-notice");
  await expect(notice).toHaveAttribute("data-reason", "cancelled");
  const download = page.waitForEvent("download");
  await page.getByTestId("pdf-view-notice-download").click();
  expect((await download).suggestedFilename()).toBe("protected.pdf");
  await page.getByTestId("pdf-view-retry").click();
  await expect(page.getByTestId("pdf-view-password")).toBeVisible();
});

test("a PDF without text finds nothing, and a corrupt PDF offers Download", async ({
  page,
}) => {
  await open(page, { fixture: "no-text.pdf" });
  await expect(
    page.locator('[data-testid="pdf-page"][data-page-number="1"]'),
  ).toHaveAttribute("data-rendered", "true");
  expect(await canvasHasInk(page, 1)).toBe(true);
  await page.getByTestId("pdf-view-search").fill("anything");
  await expect(page.getByTestId("pdf-view-search-count")).toHaveText(
    "No matches",
  );

  await open(page, { fixture: "corrupt.pdf" });
  const notice = page.getByTestId("pdf-view-notice");
  await expect(notice).toHaveAttribute("data-reason", "error");
  await expect(notice).toContainText("not a valid PDF");
  const download = page.waitForEvent("download");
  await page.getByTestId("pdf-view-notice-download").click();
  expect((await download).suggestedFilename()).toBe("corrupt.pdf");
});
