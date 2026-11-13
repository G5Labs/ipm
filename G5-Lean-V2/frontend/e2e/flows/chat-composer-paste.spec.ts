/**
 * Chat Composer Clipboard Paste — Playwright E2E Tests
 *
 * Real-browser coverage for the clipboard→composer image paste feature
 * (ontology `chat_panel_runtime` D37 / I3; owning scenario
 * `test_contract/chat_agent` I49). The prior automated coverage lived only in
 * `ChatPanel.test.tsx`, which drives a jsdom-synthetic `clipboardData`; the code
 * review of the paste PR flagged the absence of a real-browser exercise of the
 * `onPaste` handler as P1. These specs close that gap: they drive the genuine
 * Chromium/Firefox/WebKit `onPaste` path, real `FileReader` base64 encoding, and
 * real validation, with no mocks or stubs.
 *
 * The paste→chip flow is entirely client-side (no turn is sent), so unlike the
 * turn-based chat specs these tests do NOT require an LLM backend — only a
 * seeded workspace whose composer is enabled.
 *
 * Run: cd frontend && npx playwright test chat-composer-paste
 */

import { test, expect, type Page } from "@playwright/test";
import playwrightConfig from "../../playwright.config";
import {
  navigateToWorkspace,
  softVisible,
  TEST_ORG,
  TEST_WORKSPACE,
} from "../helpers";

// The Async Clipboard API (`navigator.clipboard`) exists only in a secure
// context. Local runs target `http://localhost`, which browsers exempt; CI
// targets `E2E_BASE_URL=http://g5-test-client` — neither HTTPS nor localhost —
// so Chromium never exposes it there and the real-OS-clipboard step failed
// with `navigator.clipboard` undefined (test_contract/chat_agent I49). Treat
// the configured base origin as secure for this file only. The switch is
// honored by the full Chromium build, not by Playwright's default headless
// shell, hence `channel: "chromium"`; every other spec keeps the project
// defaults.
const BASE_ORIGIN = new URL(
  playwrightConfig.use?.baseURL ?? "http://localhost:3000",
).origin;
test.use({
  channel: "chromium",
  launchOptions: {
    args: [`--unsafely-treat-insecure-origin-as-secure=${BASE_ORIGIN}`],
  },
});

// A valid 1×1 transparent PNG. Small (well under the 5 MiB per-file cap),
// non-empty, and a real image/png so it passes the shared ingestion allow-list.
// These bytes must be a genuinely decodable PNG, not merely PNG-shaped: the
// `ClipboardEvent` paths never decode the payload, but `navigator.clipboard
// .write()` does and rejects a corrupt IDAT with "Failed to read or decode
// ClipboardItemData".
const PNG_1x1_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNgAAIAAAUAAen63NgAAAAASUVORK5CYII=";
// A second, byte-distinct 1×1 PNG (opaque red) so a two-image paste is not
// rejected by the byte-identical de-dup guard in `prepareImageAttachment`.
const PNG_1x1_RED_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP4z8DwHwAFAAH/VscvDQAAAABJRU5ErkJggg==";

type PastedFile = { name: string; type: string; base64: string };

/**
 * Navigate to the seeded workspace, ensure the right-hand chat panel is open,
 * create a session so the composer renders, and return the composer textarea
 * once it is visible and enabled. Returns `null` when the target stack renders
 * no enabled composer (the caller soft-skips) so the spec never asserts against
 * an unreachable surface.
 */
async function openComposer(page: Page): Promise<ReturnType<Page["getByTestId"]> | null> {
  await navigateToWorkspace(page, TEST_ORG, TEST_WORKSPACE);

  // Dismiss any modal/dialog that could intercept pointer/keyboard input.
  if (
    (await page.locator('dialog:visible, [role="dialog"]:visible').count()) > 0
  ) {
    await page.keyboard.press("Escape");
    await page.waitForTimeout(300);
  }

  // Ensure the right panel (chat) is visible.
  const rightPanel = page.getByTestId("right-panel");
  if (!(await softVisible(rightPanel, 5_000))) {
    const toggleButton = page.getByTestId("toggle-right-panel");
    if (await softVisible(toggleButton, 3_000)) {
      await toggleButton.click();
      await softVisible(rightPanel, 5_000);
    }
  }

  // Create a chat session if the composer is not already present.
  const newSessionButton = page
    .getByTestId("new-session-empty")
    .or(page.getByTestId("new-session-header"))
    .first();
  if (await softVisible(newSessionButton, 5_000)) {
    await Promise.all([
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
    await page.waitForTimeout(1_000);
  }

  const chatInput = page.getByTestId("chat-input").first();
  if (!(await softVisible(chatInput, 10_000))) return null;
  // Paste honors the same disabled/streaming gate as the picker; if the seeded
  // stack renders a disabled composer there is nothing to exercise.
  if (!(await chatInput.isEnabled().catch(() => false))) return null;
  return chatInput;
}

/**
 * Dispatch a genuine `paste` ClipboardEvent carrying a DataTransfer (image
 * files and/or plain text) onto the composer textarea. This fires the real
 * React `onPaste` handler; it is the standard cross-browser way to deliver a
 * synthetic-but-real clipboard payload without OS clipboard access.
 */
async function pasteIntoComposer(
  page: Page,
  payload: { files?: PastedFile[]; text?: string },
): Promise<void> {
  await page.getByTestId("chat-input").first().focus();
  await page.evaluate(
    ({ files, text }) => {
      const el = document.querySelector<HTMLTextAreaElement>(
        '[data-testid="chat-input"]',
      );
      if (!el) throw new Error("chat-input textarea not found");
      const dt = new DataTransfer();
      for (const f of files ?? []) {
        const binary = atob(f.base64);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
        dt.items.add(new File([bytes], f.name, { type: f.type }));
      }
      if (typeof text === "string") dt.setData("text/plain", text);
      const event = new ClipboardEvent("paste", {
        bubbles: true,
        cancelable: true,
      });
      // Some engines ignore the `clipboardData` init member for scripted
      // events, so pin it explicitly — the handler reads `event.clipboardData`.
      Object.defineProperty(event, "clipboardData", { value: dt });
      el.dispatchEvent(event);
    },
    { files: payload.files ?? [], text: payload.text },
  );
}

async function pasteImageAddsChip({ page }: { page: Page }): Promise<void> {
  const composer = await openComposer(page);
  test.skip(!composer, "Seeded workspace rendered no enabled composer");

  // Nameless blob (an OS screen capture arrives without a file name) → the
  // ingestion path derives the synthetic `pasted-image.png` name (D35).
  await pasteIntoComposer(page, {
    files: [{ name: "", type: "image/png", base64: PNG_1x1_BASE64 }],
  });

  const tray = page.getByTestId("chat-attachment-tray");
  await expect(tray).toBeVisible({ timeout: 10_000 });
  const chips = page.getByTestId("chat-attachment-chip");
  await expect(chips).toHaveCount(1);
  await expect(chips.first()).toContainText("pasted-image.png");
  // A valid image paste must never surface the inline attachment error.
  await expect(page.getByTestId("chat-attachment-error")).toHaveCount(0);

  // Removal empties the tray (chip is a real, removable pending attachment).
  await page.getByTestId("chat-remove-attachment").first().click();
  await expect(page.getByTestId("chat-attachment-chip")).toHaveCount(0);
}

async function pasteRealOsClipboardImage({
  page,
  browserName,
}: {
  page: Page;
  browserName: string;
}): Promise<void> {
  // Writing an image to the real OS clipboard via the async Clipboard API +
  // ClipboardItem is only reliably supported (and permission-grantable) in
  // Chromium under Playwright. This is the truest reproduction of the user
  // flow: a real Ctrl/Cmd+V paste of a clipboard image.
  test.skip(
    browserName !== "chromium",
    "Real OS-clipboard image write is Chromium-only under Playwright",
  );
  const composer = await openComposer(page);
  test.skip(!composer, "Seeded workspace rendered no enabled composer");

  await page
    .context()
    .grantPermissions(["clipboard-read", "clipboard-write"]);

  await page.evaluate(async (base64) => {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    const blob = new Blob([bytes], { type: "image/png" });
    await navigator.clipboard.write([
      new ClipboardItem({ "image/png": blob }),
    ]);
  }, PNG_1x1_BASE64);

  await composer!.focus();
  await page.keyboard.press("ControlOrMeta+V");

  await expect(page.getByTestId("chat-attachment-chip")).toHaveCount(1, {
    timeout: 10_000,
  });
  await expect(page.getByTestId("chat-attachment-error")).toHaveCount(0);
}

async function pasteTextLeavesComposerClean({
  page,
}: {
  page: Page;
}): Promise<void> {
  const composer = await openComposer(page);
  test.skip(!composer, "Seeded workspace rendered no enabled composer");

  await composer!.fill("draft in progress");
  // A text-only paste carries no image item, so the handler must not consume
  // it: no chip, no error, and the existing draft text is left intact.
  await pasteIntoComposer(page, { text: "pasted words" });

  await expect(page.getByTestId("chat-attachment-chip")).toHaveCount(0);
  await expect(page.getByTestId("chat-attachment-error")).toHaveCount(0);
  await expect(composer!).toHaveValue("draft in progress");
}

async function pasteNonImageIgnored({ page }: { page: Page }): Promise<void> {
  const composer = await openComposer(page);
  test.skip(!composer, "Seeded workspace rendered no enabled composer");

  // A non-image file on the clipboard is left to the browser: no chip is
  // created and, because the paste is not an attempted-but-rejected upload, no
  // inline error is surfaced.
  await pasteIntoComposer(page, {
    files: [{ name: "notes.txt", type: "text/plain", base64: btoaNode("hi") }],
  });

  await expect(page.getByTestId("chat-attachment-chip")).toHaveCount(0);
  await expect(page.getByTestId("chat-attachment-error")).toHaveCount(0);
}

async function pasteMultipleImages({ page }: { page: Page }): Promise<void> {
  const composer = await openComposer(page);
  test.skip(!composer, "Seeded workspace rendered no enabled composer");

  // Two byte-distinct images paste as two chips (sequential ingestion; the
  // de-dup guard only collapses byte-identical uploads).
  await pasteIntoComposer(page, {
    files: [
      { name: "", type: "image/png", base64: PNG_1x1_BASE64 },
      { name: "", type: "image/png", base64: PNG_1x1_RED_BASE64 },
    ],
  });

  await expect(page.getByTestId("chat-attachment-chip")).toHaveCount(2, {
    timeout: 10_000,
  });
  await expect(page.getByTestId("chat-attachment-error")).toHaveCount(0);
}

async function pastedImageSurvivesTypedText({
  page,
}: {
  page: Page;
}): Promise<void> {
  const composer = await openComposer(page);
  test.skip(!composer, "Seeded workspace rendered no enabled composer");

  // Regression for chat_panel_runtime D36: attachments reach the agent only as
  // files materialized into the worktree, so the turn text is the only channel
  // that names them. Typing instructions alongside a pasted image previously
  // replaced the attachment label, silently hiding the image from the agent.
  await pasteIntoComposer(page, {
    files: [{ name: "", type: "image/png", base64: PNG_1x1_BASE64 }],
  });
  await expect(page.getByTestId("chat-attachment-chip")).toHaveCount(1, {
    timeout: 10_000,
  });

  // The typed draft and the chip must coexist: typing neither drops the chip
  // nor clears the pending attachment.
  await composer!.fill("make this button blue");
  await expect(page.getByTestId("chat-attachment-chip")).toHaveCount(1);
  await expect(composer!).toHaveValue("make this button blue");
  await expect(page.getByTestId("chat-attachment-error")).toHaveCount(0);
}

// Node-side base64 helper for building non-image clipboard payloads without a
// browser `btoa`.
function btoaNode(value: string): string {
  return Buffer.from(value, "utf-8").toString("base64");
}

test.describe("Chat Composer Clipboard Paste", () => {
  test(
    "pasted image becomes a chip with the synthetic name and is removable",
    pasteImageAddsChip,
  );
  test(
    "real OS-clipboard image paste (Ctrl/Cmd+V) becomes a chip",
    pasteRealOsClipboardImage,
  );
  test(
    "text-only paste leaves the composer clean and preserves the draft",
    pasteTextLeavesComposerClean,
  );
  test("a pasted non-image is ignored with no chip and no error", pasteNonImageIgnored);
  test("multiple pasted images become multiple chips", pasteMultipleImages);
  test(
    "a pasted image survives typing instructions alongside it (D36)",
    pastedImageSurvivesTypedText,
  );
});
