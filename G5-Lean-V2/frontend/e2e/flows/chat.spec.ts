import { test, expect } from "@playwright/test";
import {
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  openChatComposer,
  openWorkspacePath,
  releaseChatTurn,
  runWorkspaceVmAction,
  softVisible,
  type ChatTurnIdentity,
  type WorkspaceClone,
  waitForWorkspaceLoad,
  waitForWorkspaceVmStatus,
} from "../helpers";

const hasLlm =
  !!process.env.ANTHROPIC_API_KEY ||
  !!process.env.OPENROUTER_API_KEY ||
  !!process.env.OPENAI_API_KEY ||
  !!process.env.LLM_ENABLED;

test.describe("Chat Interaction", () => {
  test.describe.configure({ timeout: 300_000 });
  const activeTurns = new WeakMap<
    import("@playwright/test").Page,
    ChatTurnIdentity
  >();

  // Since the workspace VM cutover the composer stays disabled ("Start this
  // workspace to send messages.") until the actor owns a running VM, and a
  // base workspace cannot be started. Each test therefore chats in its own
  // started fork, like terminal.spec.ts, instead of the seeded workspace.
  let workspace: WorkspaceClone | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    if (!hasLlm) return;
    workspace = await createWorkspaceClone(page, {
      namePrefix: `chat-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
  });

  test.afterEach(async ({ page }) => {
    const activeTurn = activeTurns.get(page);
    try {
      if (activeTurn) {
        await releaseChatTurn(page, activeTurn);
      }
    } finally {
      activeTurns.delete(page);
      if (workspace) {
        await deleteWorkspaceById(page, workspace.id).catch(() => {});
        workspace = undefined;
      }
    }
  });

  test("open chat panel and send message", async ({ page }) => {
    test.skip(
      !hasLlm,
      "Requires LLM backend (set an LLM API key or LLM_ENABLED)",
    );

    // Debug logging — only dumped on failure
    const logs: string[] = [];
    page.on("console", (msg) => {
      if (
        msg.type() === "error" &&
        !msg.text().includes("WebSocket connection")
      ) {
        logs.push(`[console:error] ${msg.text().slice(0, 200)}`);
      }
    });
    page.on("response", (res) => {
      if (res.url().includes("/api/") && res.status() >= 400) {
        logs.push(
          `[HTTP ${res.status()}] ${res.request().method()} ${res.url().slice(0, 150)}`,
        );
      }
    });

    // Set up resolve listener BEFORE navigation to guarantee capture
    const resolvePromise = page.waitForResponse(
      (resp) => resp.url().includes("/api/v1/resolve") && resp.status() < 400,
      { timeout: 20_000 },
    );

    await page.goto(workspace!.path, {
      waitUntil: "domcontentloaded",
    });
    await waitForWorkspaceLoad(page);

    // Wait for slug → UUID resolution (critical for all workspace API calls)
    try {
      await resolvePromise;
    } catch {
      logs.push(
        "[RESOLVE] /api/v1/resolve never returned 2xx — workspaceId not set",
      );
      console.log("CHAT TEST LOGS:", logs.join(" | "));
      throw new Error("Workspace resolve failed. See logs.");
    }
    await page.waitForTimeout(500);

    // Dismiss any open dialogs that might block interactions
    if (
      (await page.locator('dialog:visible, [role="dialog"]:visible').count()) >
      0
    ) {
      await page.keyboard.press("Escape");
      await page.waitForTimeout(300);
    }

    // Ensure right panel is visible
    const rightPanel = page.getByTestId("right-panel");
    if (!(await softVisible(rightPanel, 5_000))) {
      const toggleButton = page.getByTestId("toggle-right-panel");
      if (await softVisible(toggleButton, 3_000)) {
        await toggleButton.click();
        await softVisible(rightPanel, 5_000);
      }
    }

    // Create a chat session if needed
    const newSessionButton = page
      .getByTestId("new-session-empty")
      .or(page.getByTestId("new-session-header"))
      .first();
    if (await softVisible(newSessionButton, 5_000)) {
      const [sessionResp] = await Promise.all([
        page
          .waitForResponse(
            (r) =>
              r.url().includes("/sessions") &&
              r.request().method() === "POST" &&
              r.status() < 400,
            { timeout: 15_000 },
          )
          .catch(() => null),
        newSessionButton.click(),
      ]);
      if (!sessionResp) logs.push("[SESSION] No POST /sessions response");
      await page.waitForTimeout(1_000);
    }

    // Type and send a message
    const chatInput = page.getByTestId("chat-input").first();
    await expect(chatInput).toBeVisible({ timeout: 10_000 });
    await chatInput.fill("Hello, what is this workspace about?");

    const [sendResp] = await Promise.all([
      page.waitForResponse(
        (response) => {
          if (response.request().method() !== "POST") return false;
          const pathname = new URL(response.url()).pathname;
          return (
            /\/api\/v1\/sessions\/[^/]+\/messages$/.test(pathname) ||
            /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(pathname)
          );
        },
        { timeout: 15_000 },
      ),
      chatInput.press("Enter"),
    ]);
    await captureChatTurn(page, sendResp, (turn) =>
      activeTurns.set(page, turn),
    );

    // User message should appear
    try {
      await expect(
        page
          .getByTestId("chat-messages")
          .or(page.getByTestId("right-panel"))
          .first(),
      ).toContainText("Hello, what is this workspace about?", {
        timeout: 15_000,
      });
    } catch (e) {
      console.log("CHAT TEST LOGS:", logs.join(" | "));
      throw e;
    }
  });

  test("streaming response appears progressively", async ({ page }) => {
    test.skip(
      !hasLlm,
      "Requires LLM backend (set an LLM API key or LLM_ENABLED)",
    );

    // Set up resolve listener before navigation
    const resolvePromise = page
      .waitForResponse(
        (resp) => resp.url().includes("/api/v1/resolve") && resp.status() < 400,
        { timeout: 20_000 },
      )
      .catch(() => null);

    await openWorkspacePath(page, workspace!.path);
    await resolvePromise;
    await page.waitForTimeout(500);

    // Dismiss dialogs
    if (
      (await page.locator('dialog:visible, [role="dialog"]:visible').count()) >
      0
    ) {
      await page.keyboard.press("Escape");
      await page.waitForTimeout(300);
    }

    // Create session if needed
    const newSessionButton = page
      .getByTestId("new-session-empty")
      .or(page.getByTestId("new-session-header"))
      .first();
    if (await softVisible(newSessionButton, 3_000)) {
      await newSessionButton.click();
      await page
        .getByTestId("new-session-empty")
        .waitFor({ state: "hidden", timeout: 10_000 })
        .catch(() => {});
    }

    await openChatComposer(page);
    const chatInput = page
      .getByTestId("chat-input")
      .or(page.getByPlaceholder(/message|ask|type|codebase/i))
      .first();

    if (await softVisible(chatInput, 5_000)) {
      await chatInput.fill("Explain the architecture");
      const [sendResponse] = await Promise.all([
        page.waitForResponse(
          (response) => {
            if (response.request().method() !== "POST") return false;
            const pathname = new URL(response.url()).pathname;
            return (
              /\/api\/v1\/sessions\/[^/]+\/messages$/.test(pathname) ||
              /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(pathname)
            );
          },
          { timeout: 15_000 },
        ),
        chatInput.press("Enter"),
      ]);
      await captureChatTurn(page, sendResponse, (turn) =>
        activeTurns.set(page, turn),
      );

      const typingIndicator = page
        .getByTestId("typing-indicator")
        .or(page.getByTestId("streaming-indicator"))
        .first();
      await expect(
        typingIndicator
          .or(page.locator('[data-testid="assistant-message"]'))
          .first(),
      ).toBeVisible({ timeout: 30_000 });
    }
  });
});
