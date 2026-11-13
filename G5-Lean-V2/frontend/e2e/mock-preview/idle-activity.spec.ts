import { expect, test } from "@playwright/test";

// Real browser events and the real xterm parser: DOM-only tests cannot establish
// isTrusted, paste dispatch, or distinguish emulator replies from keyboard input.
test("trusted workspace input and terminal typing renew; passive traffic and emulator replies do not", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  // Keyboard and paste must also work without the deprecated Window.event.
  await page.addInitScript(() => {
    Object.defineProperty(window, "event", {
      value: undefined,
      configurable: true,
    });
  });
  await page.goto("/");
  await page.evaluate(async () => {
    const { observeWorkspaceUserActivity } =
      await import("/src/hooks/useWorkspaceUserActivity.ts");
    const { WorkspaceTerminalPanel } =
      await import("/src/components/execution/WorkspaceTerminalPanel.tsx");
    const { api } = await import("/src/lib/api.ts");
    const { default: React } =
      await import("/node_modules/.vite/deps/react.js");
    const {
      default: { createRoot },
    } = await import("/node_modules/.vite/deps/react-dom_client.js");
    api.post = async () => ({ ticket: "test-ticket" });
    const messages: Record<string, unknown>[] = [];
    const activity: string[] = [];
    const sockets: Socket[] = [];
    class Socket {
      static OPEN = 1;
      readyState = 1;
      onmessage: ((event: { data: unknown }) => void) | null = null;
      onopen: (() => void) | null = null;
      constructor() {
        sockets.push(this);
        setTimeout(() => {
          this.onopen?.();
          this.onmessage?.({
            data:
              JSON.stringify({ type: "opened", terminal_id: "t", offset: 0 }) +
              "\n",
          });
        }, 0);
      }
      send(data: string) {
        messages.push(JSON.parse(data));
      }
      close() {
        this.readyState = 3;
      }
    }
    window.WebSocket = Socket as unknown as typeof WebSocket;
    const host = document.createElement("div");
    host.style.cssText =
      "position:fixed;inset:0;background:white;z-index:999999";
    host.innerHTML =
      '<button id="owner-action">Owner action</button><div id="idle-terminal" style="height:400px;width:800px"></div>';
    document.body.append(host);
    observeWorkspaceUserActivity(document, (reason) => activity.push(reason));
    createRoot(host.querySelector("#idle-terminal")!).render(
      React.createElement(WorkspaceTerminalPanel, {
        workspaceId: "idle-test",
        enabled: true,
      }),
    );
    Object.assign(window, { idleTest: { activity, messages, sockets } });
  });
  const state = () => page.evaluate(() => (window as any).idleTest);
  await expect(page.getByText("Workspace terminal · Connected")).toBeVisible();
  expect((await state()).activity).toEqual([]);
  await page.evaluate(() => {
    window.dispatchEvent(new Event("online"));
    window.dispatchEvent(new Event("focus"));
    document.dispatchEvent(new Event("visibilitychange"));
    document.getElementById("owner-action")!.click();
  });
  expect((await state()).activity).toEqual([]);
  const input = page.locator("#idle-terminal .xterm-helper-textarea");
  await input.focus();
  await page.keyboard.type("abc");
  await expect
    .poll(async () =>
      (await state()).messages
        .filter((m: any) => m.type === "input")
        .map((m: any) => m.data)
        .join(""),
    )
    .toBe("abc");
  await page.evaluate(() => navigator.clipboard.writeText("pasted text"));
  await page.keyboard.press("ControlOrMeta+Shift+V");
  await expect
    .poll(async () =>
      (await state()).messages
        .filter((m: any) => m.type === "input")
        .map((m: any) => m.data)
        .join(""),
    )
    .toContain("pasted text");
  const count = (await state()).messages.filter(
    (m: any) => m.type === "input",
  ).length;
  await page.evaluate(() => {
    const socket = (window as any).idleTest.sockets.at(-1);
    socket.onmessage({ data: new TextEncoder().encode("\x1b[5n").buffer });
  });
  await expect
    .poll(
      async () =>
        (await state()).messages.filter((m: any) => m.type === "response")
          .length,
    )
    .toBeGreaterThan(0);
  expect(
    (await state()).messages.filter((m: any) => m.type === "input"),
  ).toHaveLength(count);
  expect((await state()).activity).toEqual([]);
  await page.locator("#owner-action").click();
  expect((await state()).activity).toEqual(["pointer"]);
});
