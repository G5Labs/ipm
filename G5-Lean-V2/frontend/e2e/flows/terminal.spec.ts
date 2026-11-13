import { expect, test, type Page } from "@playwright/test";
import {
  createWorkspaceClone,
  deleteWorkspaceById,
  openRailTerminal,
  openWorkspacePath,
  runWorkspaceVmAction,
  showWorkspaceTabs,
  type WorkspaceClone,
  waitForWorkspaceVmStatus,
} from "../helpers";

type TerminalControl = {
  type: "opened" | "rejected";
  terminal_id?: string;
  reattached?: boolean;
};

function watchTerminalControls(page: Page): TerminalControl[] {
  const controls: TerminalControl[] = [];
  page.on("websocket", (socket) => {
    if (!socket.url().includes("/terminal?")) return;
    socket.on("framereceived", ({ payload }) => {
      try {
        const control = JSON.parse(payload.toString()) as TerminalControl;
        if (control.type === "opened" || control.type === "rejected") {
          controls.push(control);
        }
      } catch {
        // PTY output is not a control frame.
      }
    });
  });
  return controls;
}

async function installTerminalTransportHook(page: Page) {
  const install = () => {
    const browser = window as typeof window & {
      __g5TerminalSockets?: WebSocket[];
    };
    browser.__g5TerminalSockets = [];
    browser.WebSocket = new Proxy(browser.WebSocket, {
      construct(target, args) {
        const socket = Reflect.construct(target, args) as WebSocket;
        if (String(args[0]).includes("/terminal?")) {
          browser.__g5TerminalSockets!.push(socket);
        }
        return socket;
      },
    });
  };
  await page.addInitScript(install);
  await page.evaluate(install);
}

test.describe("Workspace terminal", () => {
  let workspace: WorkspaceClone;

  test.beforeEach(async ({ page }, testInfo) => {
    await showWorkspaceTabs(page);
    await page.setViewportSize({ width: 1440, height: 900 });
    workspace = await createWorkspaceClone(page, {
      namePrefix: `terminal-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
  });

  test("runs a command and resizes the real PTY", async ({ page }) => {
    await page.getByRole("tab", { name: "Chat", exact: true }).click();
    const center = page.getByTestId("center-panel");
    const chatSize = await center.boundingBox();
    await openRailTerminal(page);
    await expect(
      page.getByRole("tab", { name: "Terminal", exact: true }),
    ).toHaveAttribute("aria-selected", "true");
    await expect(
      page.getByRole("tabpanel", { name: "Terminal", exact: true }),
    ).toBeVisible();
    expect(await center.boundingBox()).toEqual(chatSize);
    const terminal = page.getByTestId("workspace-terminal");
    await expect(terminal).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByLabel("Terminal transcript")).toHaveCount(0);

    const input = terminal.locator(".xterm-helper-textarea");
    await input.focus();
    await input.pressSequentially("stty -echo");
    await input.press("Enter");
    await page.waitForTimeout(300);
    await input.pressSequentially(
      "printf 'G5_TERMINAL_ECHO\\n'; G5_BEFORE_SIZE=\"$(stty size)\"",
    );
    await input.press("Enter");
    const terminalRows = terminal.locator(".xterm-accessibility-tree");
    await expect(terminalRows).toContainText("G5_TERMINAL_ECHO", {
      timeout: 30_000,
    });

    // Switching to Chat and back must keep this shell's variables and output.
    await page.getByRole("tab", { name: "Chat", exact: true }).click();
    await expect(terminal).toBeHidden();
    await openRailTerminal(page);
    await expect(
      page.getByRole("tab", { name: "Terminal", exact: true }),
    ).toHaveCount(1);
    await expect(terminalRows).toContainText("G5_TERMINAL_ECHO");
    await input.focus();
    await input.pressSequentially(
      'test -n "$G5_BEFORE_SIZE" && echo G5_TERMINAL_PRESERVED',
    );
    await input.press("Enter");
    await expect(terminalRows).toContainText("G5_TERMINAL_PRESERVED");

    // Stay on the desktop layout: below 1024px the workspace page switches
    // to the mobile (chat-only) layout and unmounts the central panel.
    const widthBefore = await terminal.evaluate(
      (element) => element.clientWidth,
    );
    await page.setViewportSize({ width: 1100, height: 700 });
    await expect
      .poll(async () => terminal.evaluate((element) => element.clientWidth), {
        timeout: 10_000,
      })
      .toBeLessThan(widthBefore);
    await input.focus();
    await input.pressSequentially(
      'G5_AFTER_SIZE="$(stty size)"; if [ "$G5_BEFORE_SIZE" != "$G5_AFTER_SIZE" ]; then echo G5_TERMINAL_RESIZED; else echo G5_TERMINAL_NOT_RESIZED; fi; stty echo',
    );
    await input.press("Enter");
    await expect(terminalRows).toContainText("G5_TERMINAL_RESIZED", {
      timeout: 30_000,
    });
    await expect(terminalRows).not.toContainText("G5_TERMINAL_NOT_RESIZED");
  });

  test.describe(() => {
    test.describe.configure({ timeout: 180_000 });
    test("reload and interrupted transport reattach the same live PTY", async ({
      page,
    }) => {
      const controls = watchTerminalControls(page);
      await installTerminalTransportHook(page);
      await openRailTerminal(page);
      const terminal = page.getByTestId("workspace-terminal");
      await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
        timeout: 30_000,
      });
      await expect.poll(() => controls.length).toBeGreaterThan(0);
      const firstId = controls[0].terminal_id;
      expect(firstId).toBeTruthy();
      expect(controls[0].reattached).toBe(false);
      const input = terminal.locator(".xterm-helper-textarea");
      await input.focus();
      await input.pressSequentially("stty -echo");
      await input.press("Enter");
      await input.pressSequentially(
        "export G5_M13_SHELL=ALIVE; printf '%s_%s\\n' M13 INITIAL",
      );
      await input.press("Enter");
      await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
        "M13_INITIAL",
        { timeout: 30_000 },
      );

      const beforeReload = controls.length;
      await page.reload();
      await openRailTerminal(page);
      await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
        timeout: 30_000,
      });
      await expect.poll(() => controls.length).toBeGreaterThan(beforeReload);
      expect(controls.at(-1)).toMatchObject({
        terminal_id: firstId,
        reattached: true,
      });
      const reloadedTerminal = page.getByTestId("workspace-terminal");
      const reloadedInput = reloadedTerminal.locator(".xterm-helper-textarea");
      await reloadedInput.focus();
      await reloadedInput.pressSequentially(
        "printf '%s_%s_%s\\n' M13 RELOAD \"$G5_M13_SHELL\"",
      );
      await reloadedInput.press("Enter");
      await expect(
        reloadedTerminal.locator(".xterm-accessibility-tree"),
      ).toContainText("M13_RELOAD_ALIVE", { timeout: 30_000 });

      const beforeTransportDrop = controls.length;
      await page.evaluate(() => {
        const browser = window as typeof window & {
          __g5TerminalSockets?: WebSocket[];
        };
        browser.__g5TerminalSockets?.at(-1)?.close();
      });
      await expect(page.getByText("Workspace terminal · Connected")).toBeVisible({
        timeout: 30_000,
      });
      await expect
        .poll(() => controls.length)
        .toBeGreaterThan(beforeTransportDrop);
      expect(controls.at(-1)).toMatchObject({
        terminal_id: firstId,
        reattached: true,
      });
      await reloadedInput.focus();
      await reloadedInput.pressSequentially(
        "printf '%s_%s_%s\\n' M13 RECONNECT \"$G5_M13_SHELL\"",
      );
      await reloadedInput.press("Enter");
      await expect(
        reloadedTerminal.locator(".xterm-accessibility-tree"),
      ).toContainText("M13_RECONNECT_ALIVE", { timeout: 30_000 });
    });
  });
});
