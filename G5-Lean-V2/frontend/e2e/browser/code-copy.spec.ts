import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { chatProviders } from "../../src/test/chatRenderingFixtures";

// `g5.documents.markdown.code-copy`: the clipboard receives the exact fence text.

async function readClipboard(page: Page) {
  return page.evaluate(() => navigator.clipboard.readText());
}

test.beforeEach(async ({ context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
});

for (const provider of chatProviders)
  for (const layout of ["center", "panel"])
    test(`${provider} ${layout}: Copy on a chat reply's code block`, async ({
      page,
    }) => {
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.route("**/api/**", (route) =>
        route.fulfill({ json: { data: [] } }),
      );
      await page.goto(
        `/e2e/browser/chat-rendering.html?provider=${provider}&layout=${layout}`,
      );
      await page.getByTestId("chat-input").fill("Show me code");
      await page.getByTestId("chat-panel-send").click();
      await page.getByRole("button", { name: "Next stream chunk" }).click();
      await page.getByRole("button", { name: "Complete reply" }).click();

      const assistant = page
        .getByTestId("assistant-message")
        .filter({ hasText: "The shared renderer is ready." });
      const block = assistant.getByTestId("markdown-code-block").first();
      await expect(block).toContainText("const answer = 42;");
      const copy = block.getByRole("button", { name: "Copy code" });
      await copy.click();
      await expect(block.getByRole("status")).toHaveText("Copied");
      expect(await readClipboard(page)).toBe("const answer = 42;");

      // The control sits inside the block, and the panel does not overflow.
      const [blockBox, copyBox] = await Promise.all([
        block.boundingBox(),
        copy.boundingBox(),
      ]);
      expect(copyBox!.x + copyBox!.width).toBeLessThanOrEqual(
        blockBox!.x + blockBox!.width,
      );
      expect(
        await page.evaluate(
          () => document.documentElement.scrollWidth <= innerWidth,
        ),
      ).toBe(true);
      await expect(block.getByRole("status")).toHaveText("", {
        timeout: 5000,
      });
      expect(errors).toEqual([]);
      await block.screenshot({
        path: test.info().outputPath("chat-code-copy.png"),
      });
    });

test("Copy on trusted, restricted and FILES preview code blocks", async ({
  page,
}) => {
  await page.goto("/e2e/browser/code-copy.html");
  const trusted = page.getByTestId("trusted");
  const blocks = trusted.getByTestId("markdown-code-block");
  // Mermaid renders as a diagram, without a Copy control.
  await expect(blocks).toHaveCount(2);

  await blocks.nth(0).getByRole("button", { name: "Copy code" }).click();
  await expect(blocks.nth(0).getByRole("status")).toHaveText("Copied");
  expect(await readClipboard(page)).toBe(
    'def greet(name):\n\n    return f"<hi> {name}"',
  );

  // A fence without a language still has the header and the control,
  // reached from the keyboard.
  const plainCopy = blocks.nth(1).getByRole("button", { name: "Copy code" });
  await plainCopy.focus();
  await page.keyboard.press("Enter");
  await expect(blocks.nth(1).getByRole("status")).toHaveText("Copied");
  expect(await readClipboard(page)).toBe("  two leading spaces\n\ttab");

  const restricted = page.getByTestId("restricted");
  await restricted.getByRole("button", { name: "Copy code" }).click();
  await expect(restricted.getByRole("status")).toHaveText("Copied");
  expect(await readClipboard(page)).toBe("graph LR\n  A --> B");

  const preview = page.getByTestId("preview");
  await preview.getByRole("button", { name: "Copy code" }).click();
  await expect(preview.getByRole("status")).toHaveText("Copied");
  expect(await readClipboard(page)).toBe('fn main() {\n    println!("hi");\n}');
  await page.screenshot({
    path: test.info().outputPath("code-copy-surfaces.png"),
    fullPage: true,
  });
});

test("a refused clipboard write is reported", async ({ page }) => {
  await page.addInitScript(() => {
    navigator.clipboard.writeText = () =>
      Promise.reject(
        new DOMException("Write permission denied.", "NotAllowedError"),
      );
  });
  await page.goto("/e2e/browser/code-copy.html");
  const block = page
    .getByTestId("trusted")
    .getByTestId("markdown-code-block")
    .first();
  await block.getByRole("button", { name: "Copy code" }).click();
  await expect(block.getByRole("status")).toHaveText("Couldn't copy");
  await expect(block.getByRole("status")).toBeVisible();
});
