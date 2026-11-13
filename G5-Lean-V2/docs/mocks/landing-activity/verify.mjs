import {
  chromium,
  expect,
} from "../../../frontend/node_modules/@playwright/test/index.mjs";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("./", import.meta.url);
const snapshots = new URL("snapshots/", root);
await mkdir(snapshots, { recursive: true });
const browser = await chromium.launch();
const errors = [];
const remoteRequests = [];
try {
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1100 },
  });
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (/^https?:/.test(request.url())) remoteRequests.push(request.url());
  });
  await page.goto(new URL("index.html", root).href);
  const content = page.locator("#page-content");
  const pills = content.locator("[data-type]");
  // Real selectors, semantic state and visible rows verify the reported UX.
  const selected = async () =>
    pills.evaluateAll((buttons) =>
      buttons
        .filter((button) => button.getAttribute("aria-pressed") === "true")
        .map((button) => button.dataset.type),
    );
  const groups = async () =>
    content
      .locator("[data-event]:not([data-current])")
      .evaluateAll((rows) => [
        ...new Set(rows.map((row) => row.dataset.group)),
      ]);
  const surroundingBefore = await page.locator(".toolbar").innerText();
  const guidedBefore = await page.locator(".sidebar").innerText();
  const startCount = await page
    .getByRole("button", { name: "Start", exact: true })
    .count();
  const mergeCount = await page
    .getByRole("button", { name: /Merge to.*parent/ })
    .count();
  expect(await selected()).toEqual(["all"]);
  await expect(content.locator('[data-event="comment-8"]')).toHaveClass(
    /\bcard\b/,
  );
  await expect(content.locator('[data-event="issue-opened"]')).not.toHaveClass(
    /\bcard\b/,
  );
  await expect(content).not.toContainText("Shared discussion");
  await expect(content).not.toContainText("Reviews and comments");
  await expect(content).not.toContainText("Workspace context");
  await expect(content).not.toContainText("Preview exercise");
  expect(await content.getByText("Code 0", { exact: true }).count()).toBe(0);
  await content.locator('[data-type="tasks"]').click();
  expect(await selected()).toEqual(["tasks"]);
  expect(await groups()).toEqual(["tasks"]);
  await content.locator('[data-type="approvals"]').click();
  expect(await selected()).toEqual(["tasks", "approvals"]);
  await content.locator('[data-type="tasks"]').click();
  expect(await selected()).toEqual(["approvals"]);
  await content.locator('[data-type="approvals"]').click();
  expect(await selected()).toEqual(["all"]);
  // Keyboard activation, including focus retention after rendering.
  await content.locator('[data-type="tasks"]').focus();
  await page.keyboard.press("Space");
  expect(await selected()).toEqual(["tasks"]);
  await expect(content.locator('[data-type="tasks"]')).toBeFocused();
  await page.keyboard.press("Enter");
  expect(await selected()).toEqual(["all"]);
  // Selecting every category canonicalizes to All.
  for (const type of [
    "issue",
    "conversations",
    "tasks",
    "commits",
    "approvals",
    "prs",
    "guided",
  ]) {
    await content.locator(`[data-type="${type}"]`).click();
  }
  expect(await selected()).toEqual(["all"]);
  await page.screenshot({
    path: fileURLToPath(new URL("landing-proposed.png", snapshots)),
    fullPage: true,
  });
  // Verify old behavior and unchanged surrounding page controls.
  await page.locator("#before").click();
  expect((await selected()).length).toBe(8);
  await content.locator('[data-type="tasks"]').click();
  expect(await selected()).not.toContain("tasks");
  expect(await page.locator(".toolbar").innerText()).toBe(surroundingBefore);
  expect(await page.locator(".sidebar").innerText()).toBe(guidedBefore);
  expect(
    await page.getByRole("button", { name: "Start", exact: true }).count(),
  ).toBe(startCount);
  expect(
    await page.getByRole("button", { name: /Merge to.*parent/ }).count(),
  ).toBe(mergeCount);
  await page.locator("#before").click();
  await page.screenshot({
    path: fileURLToPath(new URL("landing-current.png", snapshots)),
    fullPage: true,
  });
  await page.locator("#proposed").click();
  await page.locator("#example").selectOption("current");
  await content.locator('[data-type="tasks"]').click();
  expect(await groups()).toEqual(["tasks"]);
  expect(await content.locator('[data-current="true"]').count()).toBe(2);
  await expect(content.locator('[data-event="current-chat"]')).toContainText(
    "Current · Running",
  );
  await expect(
    content.locator('[data-event="current-chat"]'),
  ).not.toContainText("last active");
  await content.locator("[data-phase]").selectOption("be");
  await expect(content).toContainText("No matching history");
  expect(await content.locator('[data-current="true"]').count()).toBe(2);
  await content.locator('[data-type="all"]').click();
  await expect(content.locator("[data-phase]")).toHaveValue("be");
  await page.locator("#example").selectOption("review");
  // Boxes identify written messages, including standalone replies and decisions.
  for (const id of ["thread", "reply-1", "approval"]) {
    await expect(content.locator(`[data-event="${id}"]`)).toHaveClass(
      /\bcard\b/,
    );
  }
  for (const id of ["request", "commit"]) {
    await expect(content.locator(`[data-event="${id}"]`)).not.toHaveClass(
      /\bcard\b/,
    );
  }
  expect(
    await content
      .getByText(
        "The missing-memory case now keeps the workspace link available.",
        { exact: true },
      )
      .count(),
  ).toBe(1);
  await content.locator('[data-event="thread"] .event-heading button').click();
  await expect(page.locator("#reader")).toBeVisible();
  await expect(page.locator("#reader")).toContainText(
    "The missing-memory case now keeps the workspace link available.",
  );
  await page.keyboard.press("Escape");
  await page.locator("#example").selectOption("no-pr");
  await expect(content).toContainText("No implementation description yet.");
  expect(await content.locator('[data-group="prs"]').count()).toBe(0);
  await page.locator("#example").selectOption("github");
  expect(await content.locator('[aria-label="Source: GitHub"]').count()).toBe(
    5,
  );
  await content.locator('[data-event="comment-8"] time button').click();
  await expect(page.locator("#reader-title")).toContainText("Jordan commented");
  await page.keyboard.press("Escape");
  await page.goto(new URL("filters.html", root).href);
  const lab = page.locator(".filter-lab");
  await expect(lab).toBeVisible();
  await lab.locator('[data-type="tasks"]').click();
  await expect(page.locator("#selection-readout")).toContainText(
    "Selected: Tasks",
  );
  await lab.locator('[data-type="approvals"]').click();
  await expect(page.locator("#selection-readout")).toContainText(
    "Tasks + Approvals",
  );
  await page.screenshot({
    path: fileURLToPath(new URL("filters-combined.png", snapshots)),
    fullPage: true,
  });
  await page.getByRole("button", { name: "Show a zero-result state" }).click();
  await expect(lab).toContainText("No matching history");
  await page.getByRole("button", { name: "Show pinned ongoing work" }).click();
  await lab.locator('[data-type="tasks"]').click();
  expect(await lab.locator('[data-current="true"]').count()).toBe(2);
  // Both artifacts in both themes must fit their viewport. The app toolbar
  // intentionally scrolls internally at narrow widths; it is not redesigned.
  for (const file of ["index.html", "filters.html"]) {
    for (const theme of ["dark", "light"]) {
      await page.goto(new URL(file, root).href);
      await page.locator("#theme").selectOption(theme);
      for (const width of [1440, 900, 390, 320]) {
        await page.setViewportSize({ width, height: 1000 });
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth > innerWidth + 1,
        );
        expect(
          overflow,
          `${file}, ${theme}, ${width}px has document overflow`,
        ).toBe(false);
      }
    }
  }
  expect(errors).toEqual([]);
  expect(remoteRequests).toEqual([]);
  console.log(
    "PASS: mock interactions, focus, readers, Current exception, unchanged surrounding controls, dark/light and 320–1440px layouts; no page errors or network calls.",
  );
} finally {
  await browser.close();
}
