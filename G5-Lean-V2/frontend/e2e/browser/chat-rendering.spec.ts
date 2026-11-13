import { expect, test } from "@playwright/test";
import type { Locator } from "@playwright/test";
import {
  chatProviders,
  chatFormats,
} from "../../src/test/chatRenderingFixtures";

async function expectAssistantBubble(assistant: Locator, prompt: Locator) {
  const surface = assistant.getByTestId("assistant-turn-content");
  await expect(surface).toBeVisible();
  const styles = await surface.evaluate((node) => {
    const style = getComputedStyle(node);
    return {
      background: style.backgroundColor,
      radius: style.borderTopLeftRadius,
      padding: style.padding,
      alignment: getComputedStyle(node.parentElement!).alignItems,
    };
  });
  const userStyles = await prompt.evaluate((node) => {
    const style = getComputedStyle(node.firstElementChild!);
    return {
      background: style.backgroundColor,
      radius: style.borderTopLeftRadius,
      padding: style.padding,
    };
  });
  expect(styles.alignment).toBe("flex-start");
  expect(styles.background).not.toBe("rgba(0, 0, 0, 0)");
  expect(styles.background).not.toBe(userStyles.background);
  expect(parseFloat(styles.radius)).toBeGreaterThan(0);
  expect(styles.radius).toBe(userStyles.radius);
  expect(styles.padding).toBe(userStyles.padding);
}

for (const provider of chatProviders)
  for (const format of chatFormats)
    for (const layout of ["center", "panel"])
      for (const mode of ["light", "dark"]) {
        test(`${provider} ${format} ${layout} ${mode}: send, stream, complete/Stop/error and reload history`, async ({
          page,
        }) => {
          const errors: string[] = [];
          page.on("pageerror", (error) => errors.push(error.message));
          await page.route("**/api/**", (route) =>
            route.fulfill({ json: { data: [] } }),
          );
          await page.goto(
            `/e2e/browser/chat-rendering.html?provider=${provider}&format=${format}&layout=${layout}&mode=${mode}`,
          );
          const input = page.getByTestId("chat-input");
          for (const outcome of ["complete", "stopped", "error"]) {
            await input.fill(`Unify rendering: ${outcome}`);
            await page.getByTestId("chat-panel-send").click();
            const assistant = page
              .getByTestId("assistant-message")
              .filter({ hasText: "I will inspect the source." });
            await expect(assistant).toContainText("I will inspect the source.");
            await expect(page.getByLabel("Generating response")).toBeVisible();
            const prompt = page.getByTestId("user-message").last();
            await expectAssistantBubble(assistant, prompt);
            const box = await prompt.evaluate((node) => {
              const bubble = node.firstElementChild!;
              return {
                alignment: getComputedStyle(node).alignItems,
                background: getComputedStyle(bubble).backgroundColor,
                radius: getComputedStyle(bubble).borderTopLeftRadius,
              };
            });
            expect(box.alignment).toBe("flex-end");
            expect(box.background).not.toBe("rgba(0, 0, 0, 0)");
            expect(parseFloat(box.radius)).toBeGreaterThan(0);
            await page
              .getByRole("button", { name: "Next stream chunk" })
              .click();
            await expect(assistant).toContainText(
              "The shared renderer is ready.",
            );
            const tool = assistant.getByTestId("assistant-tool-block");
            if (
              !(await tool.evaluate(
                (node) => (node as HTMLDetailsElement).open,
              ))
            )
              await tool.locator("summary").click();
            await expect(tool).toHaveAttribute("open", "");
            if (outcome === "stopped")
              await page.getByTestId("stop-streaming-button").click();
            else
              await page
                .getByRole("button", {
                  name:
                    outcome === "complete" ? "Complete reply" : "Fail reply",
                })
                .click();
            await expect(page.getByLabel("Generating response")).toHaveCount(0);
            if (outcome === "stopped")
              await expect(assistant).toContainText("Stopped");
            if (outcome === "error")
              await expect(assistant.getByRole("alert")).toContainText(
                "Provider connection failed.",
              );
            await expectAssistantBubble(assistant, prompt);
            await page
              .getByRole("button", { name: "Saved messages only" })
              .click();
            await page.reload();
            await expectAssistantBubble(assistant, prompt);
            await expect(
              assistant.getByTestId("assistant-tool-block"),
            ).toHaveAttribute("open", "");
            await expect(
              assistant.getByText("Inline example", { exact: true }),
            ).toHaveCount(1);
            await expect(
              assistant.getByText("Saved example", { exact: true }),
            ).toHaveCount(1);
            // Reaching the top also loads earlier history; a short transcript
            // can already have loaded it before the explicit button is used.
            await page.getByTestId("chat-messages").evaluate((element) => {
              element.scrollTop = 0;
              element.dispatchEvent(new Event("scroll"));
            });
            await expect(page.getByText("Earlier saved reply")).toBeVisible();
            await expect(
              assistant.getByTestId("assistant-tool-block"),
            ).toHaveAttribute("open", "");
            await expectAssistantBubble(assistant, prompt);
            // No horizontal page overflow even for code in the narrow side panel.
            expect(
              await page.evaluate(
                () => document.documentElement.scrollWidth <= innerWidth,
              ),
            ).toBe(true);
          }
          expect(errors).toEqual([]);
          await page.screenshot({
            path: test.info().outputPath("chat-rendering.png"),
            fullPage: true,
          });
        });
      }

test("native Claude question drafts survive reload and response disclosure hydration", async ({
  page,
}) => {
  await page.route("**/api/**", (route) =>
    route.fulfill({ json: { data: [] } }),
  );
  await page.goto(
    "/e2e/browser/chat-rendering.html?provider=claude_code&layout=panel&mode=dark",
  );
  await page.getByTestId("chat-input").fill("Ask me a question");
  await page.getByTestId("chat-panel-send").click();
  await page.getByRole("button", { name: "Ask a native question" }).click();
  const form = page.getByTestId("agent-question-form");
  await form.getByRole("button", { name: "Blue", exact: true }).click();
  await page.reload();
  await expect(
    form.getByRole("button", { name: "Blue", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("agent-question-submit").click();
  await expect(page.getByTestId("chat-approval-resolved-toggle")).toContainText(
    "Answered",
  );
  await page.reload();
  await expect(page.getByTestId("agent-question-form")).toHaveCount(0);
});
