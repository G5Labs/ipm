import { randomUUID } from "node:crypto";
import {
  expect,
  test,
  type BrowserContext,
  type Page,
  type WebSocket as PlaywrightWebSocket,
  request as playwrightRequest,
} from "@playwright/test";
import {
  APP_BASE_URL,
  REFRESH_COOKIE_NAME,
  TEST_ORG,
  USING_OAUTH_STUB,
  authedApiViaRequest,
  assertActiveSubject,
  jwtSubject,
  captureChatTurn,
  createWorkspaceClone,
  deleteWorkspaceById,
  expectSelectedAgentProvider,
  openRailTerminal,
  openRibbonPreview,
  openWorkspacePath,
  patchStoredUserSettings,
  readCurrentOrgContext,
  readWorkspaceChanges,
  recordExpectedSubject,
  releaseChatTurn,
  runWorkspaceVmAction,
  selectAgentProviderViaSettings,
  showWorkspaceTabs,
  type ChatTurnIdentity,
  type WorkspaceClone,
  selectWorkspaceDefaultPreview,
  waitForLiveWorkspaceChanges,
  waitForWorkspaceLoad,
  waitForWorkspaceVmStatus,
} from "../helpers";
import { loginWithOAuthStub, SEEDED_FORK_MEMBER_IDENTITY } from "../oauth";

const PREVIEW_MARKER = "G5_WORKSPACE_PREVIEW_RENDERED";

// Previews live on https://<task>.<suffix>/ hosts. Chromium resolves that
// wildcard through its host-resolver rule, but Node-side requests cannot, so
// a direct fetch of a preview URL goes to the edge on the app's own host with
// the preview host in `Host` (the edge and the API both route on it) and the
// browser context's cookies for that preview host attached explicitly, from a
// cookie-less request context so nothing of the app origin leaks in.
async function fetchPreviewUrl(
  page: Page,
  url: string,
): Promise<{ status: number; text: string }> {
  const target = new URL(url);
  const viaEdge = new URL(target.pathname + target.search, APP_BASE_URL);
  const cookie = (await page.context().cookies(url))
    .map((c) => `${c.name}=${c.value}`)
    .join("; ");
  const direct = await playwrightRequest.newContext({
    ignoreHTTPSErrors: true,
  });
  try {
    const response = await direct.get(viaEdge.toString(), {
      headers: { host: target.host, ...(cookie ? { cookie } : {}) },
      maxRedirects: 0,
      failOnStatusCode: false,
    });
    return { status: response.status(), text: await response.text() };
  } finally {
    await direct.dispose();
  }
}

interface OpenedFrame {
  terminal_id: string;
  reattached?: boolean;
  offset?: number;
  reset?: boolean;
}

function watchTerminalConnections(page: Page) {
  const sockets: PlaywrightWebSocket[] = [];
  const ids: string[] = [];
  // Every `opened` control frame, in order: how each attach began (fresh,
  // resumed at an offset, or reset to the retained tail).
  const opened: OpenedFrame[] = [];
  // Every byte the page's terminal sockets received, in order: what the
  // viewer was given, independent of what the xterm viewport shows at any
  // instant.
  const received: string[] = [];
  page.on("websocket", (socket) => {
    if (!socket.url().includes("/terminal?")) return;
    sockets.push(socket);
    socket.on("framereceived", ({ payload }) => {
      received.push(payload.toString());
      try {
        const frame = JSON.parse(payload.toString());
        if (frame.type === "opened" && typeof frame.terminal_id === "string") {
          ids.push(frame.terminal_id);
          opened.push(frame as OpenedFrame);
        }
      } catch {
        // PTY output is not a control frame.
      }
    });
  });
  return { sockets, ids, opened, text: () => received.join("") };
}

test.describe("Workspace Preview", () => {
  let workspace: WorkspaceClone;
  let secondWorkspace: WorkspaceClone | undefined;
  let newOwnerContext: BrowserContext | undefined;
  let newOwnerPage: Page | undefined;
  let heldTurn: ChatTurnIdentity | undefined;
  let secondTurn: ChatTurnIdentity | undefined;

  test.beforeEach(async ({ page }, testInfo) => {
    await showWorkspaceTabs(page);
    workspace = await createWorkspaceClone(page, {
      namePrefix: `workspace-preview-${testInfo.title}`,
    });
    await runWorkspaceVmAction(page, workspace.id, "resume");
    await waitForWorkspaceVmStatus(page, workspace.id, "running");
    await openWorkspacePath(page, workspace.path);
  });

  test.afterEach(async ({ page }) => {
    if (heldTurn) await releaseChatTurn(page, heldTurn).catch(() => {});
    if (secondTurn) await releaseChatTurn(page, secondTurn).catch(() => {});
    await deleteWorkspaceById(page, workspace?.id).catch(() => {});
    if (secondWorkspace) {
      await deleteWorkspaceById(page, secondWorkspace.id).catch(() => {});
    }
    secondWorkspace = undefined;
    secondTurn = undefined;
    if (newOwnerPage) {
      await deleteWorkspaceById(newOwnerPage, workspace?.id).catch(() => {});
    }
    await newOwnerContext?.close().catch(() => {});
    newOwnerContext = undefined;
    newOwnerPage = undefined;
    heldTurn = undefined;
  });

  test("launches, serves, records its tracked write, refuses a second start, and closes with cleanup", async ({
    page,
  }) => {
    await openRibbonPreview(page);

    await selectWorkspaceDefaultPreview(page);
    const start = page.getByTestId("workspace-preview-start");
    await expect(start).toBeEnabled({ timeout: 30_000 });
    const startResponse = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname.endsWith(
          `/workspaces/${workspace.id}/preview`,
        ) && response.request().method() === "POST",
    );
    await start.click();
    expect((await startResponse).status()).toBe(200);

    // Fail by name if the preview task ends before it is ready: the UI stops
    // polling on a terminal status, so waiting on the button alone would only
    // time out without saying why the app exited.
    await expect
      .poll(
        async () => {
          const current = await authedApiViaRequest(
            page,
            `/api/v1/workspaces/${workspace.id}/preview`,
          );
          expect(current.status, JSON.stringify(current.body)).toBe(200);
          const preview = current.body?.data ?? current.body ?? {};
          if (["failed", "cancelled", "succeeded"].includes(preview?.status)) {
            const task = preview?.taskId
              ? await authedApiViaRequest(
                  page,
                  `/api/v1/tasks/${preview.taskId}`,
                )
              : null;
            throw new Error(
              `preview task ended with status ${preview.status} before ready: ${JSON.stringify(task?.body ?? preview).slice(0, 1500)}`,
            );
          }
          return preview?.ready === true;
        },
        { timeout: 120_000, intervals: [1_000, 2_000] },
      )
      .toBe(true);

    const open = page.getByTestId("workspace-preview-open");
    await expect(open).toBeEnabled({ timeout: 30_000 });
    const popupPromise = page.waitForEvent("popup");
    await open.click();
    const previewPage = await popupPromise;
    await previewPage.waitForLoadState("domcontentloaded");
    await expect(previewPage.locator("body")).toContainText(PREVIEW_MARKER, {
      timeout: 30_000,
    });

    // Exercise the real VM → tunnel → relay → browser boundary on the
    // preview's own isolated host: scripts run, the app's storage and cookies
    // stay on that host, and nothing lands on the platform origin.
    await page.evaluate(() => localStorage.removeItem("__g5_preview_probe"));
    const probeUrl = new URL("__g5_s11_probe", previewPage.url());
    expect(probeUrl.pathname.startsWith("/api/v1/preview/")).toBe(false);
    expect(probeUrl.protocol).toBe("https:");
    const probeResponse = await previewPage.goto(probeUrl.href);
    expect(probeResponse?.ok()).toBe(true);
    await expect(previewPage.locator("body")).toContainText(
      "G5_PREVIEW_ISOLATION_PROBE",
    );
    await expect(previewPage.locator("body")).toHaveAttribute(
      "data-script-ran",
      "1",
    );
    expect(
      await page.evaluate(() => localStorage.getItem("__g5_preview_probe")),
    ).toBeNull();
    const probeCookies = (await page.context().cookies()).filter(
      (cookie) => cookie.name === "__g5_preview_probe_cookie",
    );
    // The isolated app may set its own server-side cookie, scoped to its
    // preview host. It must not become a platform-origin cookie.
    expect(probeCookies).toHaveLength(1);
    expect(probeCookies[0].domain).toBe(probeUrl.hostname);
    // A preview script can plant parent-domain cookies, including longer-path
    // legacy credentials. Neither protected platform name may be transplanted.
    const intendedSubject = await assertActiveSubject(
      page,
      "before preview cookie planting",
    );
    const context = page.context();
    const refreshCookie = (await context.cookies(APP_BASE_URL)).find(
      (cookie) => cookie.name === REFRESH_COOKIE_NAME,
    );
    expect(refreshCookie).toBeDefined();
    expect(REFRESH_COOKIE_NAME).toMatch(/^__Host-/);
    const platformLabels = new URL(APP_BASE_URL).hostname.split(".").reverse();
    const previewLabels = probeUrl.hostname.split(".").reverse();
    const common: string[] = [];
    for (
      let i = 0;
      i < platformLabels.length && platformLabels[i] === previewLabels[i];
      i++
    ) {
      common.push(platformLabels[i]);
    }
    expect(common.length).toBeGreaterThanOrEqual(2);
    const domain = common.reverse().join(".");
    const pkceName = "__Host-g5_github_app_pkce";
    // API tests verify issuance; this synthetic attempt exercises the browser boundary.
    await context.addCookies([
      {
        name: pkceName,
        value: "victim-attempt",
        url: APP_BASE_URL,
        httpOnly: true,
        secure: true,
        sameSite: "Lax",
      },
    ]);
    await previewPage.evaluate(
      ({ domain, refreshName, pkceName }) => {
        // Browsers forward raw UTF-8 here. Unrelated app data must not hide
        // the platform's protected credential in the same Cookie header.
        document.cookie = `preview_utf8_probe=café; Domain=${domain}; Path=/; Secure; SameSite=Lax`;
        for (const [name, path] of [
          [refreshName, "/api/v1/auth/refresh"],
          [pkceName, "/api/v1/github-app/oauth/callback"],
        ]) {
          document.cookie = `${name.replace(/^__Host-/, "")}=attacker; Domain=${domain}; Path=${path}; Secure; SameSite=Lax`;
          document.cookie = `${name}=attacker; Domain=${domain}; Path=/; Secure; SameSite=Lax`;
          document.cookie = `${name}=attacker; Domain=${domain}; Path=${path}; Secure; SameSite=Lax`;
          document.cookie = `${name}=preview-only; Path=/; Secure; SameSite=Lax`;
        }
      },
      { domain, refreshName: REFRESH_COOKIE_NAME, pkceName },
    );
    expect(
      (await context.cookies(APP_BASE_URL)).find(
        (cookie) => cookie.name === "preview_utf8_probe",
      )?.value,
    ).toBe("café");
    for (const [name, path, expected] of [
      [REFRESH_COOKIE_NAME, "/api/v1/auth/refresh", refreshCookie!.value],
      [pkceName, "/api/v1/github-app/oauth/callback", "victim-attempt"],
    ]) {
      const cookies = await context.cookies(new URL(path, APP_BASE_URL).href);
      expect(
        cookies
          .filter((cookie) => cookie.name === name)
          .map((cookie) => cookie.value),
      ).toEqual([expected]);
      expect(
        cookies.some(
          (cookie) =>
            cookie.name === name.replace(/^__Host-/, "") &&
            cookie.value === "attacker",
        ),
      ).toBe(true);
    }
    const renewal = await page.evaluate(async () => {
      const response = await fetch("/api/v1/auth/refresh", {
        method: "POST",
        credentials: "include",
      });
      return { status: response.status, body: await response.json() };
    });
    expect(renewal.status).toBe(200);
    expect(jwtSubject(renewal.body.data.jwt)).toBe(intendedSubject);
    expect(renewal.body.data.refreshToken).toBeUndefined();
    await context.clearCookies({ name: "preview_utf8_probe" });
    await context.clearCookies({
      name: REFRESH_COOKIE_NAME.replace(/^__Host-/, ""),
    });
    await context.clearCookies({
      name: REFRESH_COOKIE_NAME,
      domain: probeUrl.hostname,
    });
    await context.clearCookies({ name: "g5_github_app_pkce" });
    await context.clearCookies({ name: pkceName });
    await previewPage.close();

    await expect
      .poll(
        async () => {
          const changes = await readWorkspaceChanges(page, workspace.id);
          return changes.members
            .flatMap((member) => member.dirty ?? [])
            .map((entry) => entry.path);
        },
        { timeout: 30_000, intervals: [500, 1_000] },
      )
      .toContain("e2e-preview-write.txt");

    const active = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/preview`,
    );
    expect(active.status, JSON.stringify(active.body)).toBe(200);
    const taskId = (active.body?.data ?? active.body)?.taskId;
    expect(taskId).toBeTruthy();

    // One preview at a time: a second start is refused while this one runs,
    // and the running preview is untouched by the refusal.
    const secondStart = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/preview`,
      "POST",
      {
        repoId: (active.body?.data ?? active.body)?.repoId,
        idempotencyKey: randomUUID(),
      },
    );
    expect(secondStart.status, JSON.stringify(secondStart.body)).toBe(409);
    const stillActive = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/preview`,
    );
    expect((stillActive.body?.data ?? stillActive.body)?.taskId).toBe(taskId);
    expect((stillActive.body?.data ?? stillActive.body)?.status).toBe(
      "running",
    );

    // Close stops the app and runs the profile's cleanup command, which the
    // fixture makes visible as a second tracked write in the same checkout.
    const closeResponse = page.waitForResponse(
      (response) =>
        response.url().includes(`/workspaces/${workspace.id}/preview/cancel`) &&
        response.request().method() === "POST",
    );
    await page.getByTestId("workspace-preview-stop").click();
    expect((await closeResponse).status()).toBe(200);
    await expect(page.getByTestId("workspace-preview-state")).toContainText(
      "Stopping the application",
      { timeout: 10_000 },
    );

    await expect
      .poll(
        async () => {
          const response = await authedApiViaRequest(
            page,
            `/api/v1/tasks/${taskId}`,
          );
          return (response.body?.data ?? response.body)?.status;
        },
        { timeout: 60_000, intervals: [500, 1_000] },
      )
      .toBe("cancelled");
    const closed = await authedApiViaRequest(
      page,
      `/api/v1/workspaces/${workspace.id}/preview`,
    );
    const closedPreview = closed.body?.data ?? closed.body;
    expect(closedPreview?.taskId).toBe(taskId);
    expect(closedPreview?.closing).toBe(false);
    expect(closedPreview?.close, JSON.stringify(closedPreview)).toBe("clean");
    await expect
      .poll(
        async () => {
          const changes = await readWorkspaceChanges(page, workspace.id);
          return changes.members
            .flatMap((member) => member.dirty ?? [])
            .map((entry) => entry.path);
        },
        { timeout: 30_000, intervals: [500, 1_000] },
      )
      .toContain("e2e-preview-cleanup.txt");
    // The dialog offers a fresh start again, with no cleanup warning.
    await expect(page.getByTestId("workspace-preview-start")).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByTestId("workspace-preview-last-close-failed"),
    ).toHaveCount(0);
  });

  test.describe(() => {
    test.describe.configure({ timeout: 420_000 });
    test("streams the preview's output on the terminal path: live run log, reload, close, lifecycle-only journal, undelayed dispatch elsewhere (g5.preview.run-log)", async ({
      page,
    }) => {
      // A second workspace whose chat must dispatch while the first one's
      // noisy build streams: the build's output is VM-local terminal
      // scrollback, never task events for the shared reconciler.
      const otherPage = await page.context().newPage();
      await showWorkspaceTabs(otherPage);
      secondWorkspace = await createWorkspaceClone(otherPage, {
        namePrefix: "workspace-preview-other",
      });
      await runWorkspaceVmAction(otherPage, secondWorkspace.id, "resume");
      await waitForWorkspaceVmStatus(otherPage, secondWorkspace.id, "running");
      await openWorkspacePath(otherPage, secondWorkspace.path);
      await otherPage.getByTestId("center-chat-tab").click();
      await selectAgentProviderViaSettings(otherPage, "claude_code");
      await expectSelectedAgentProvider(otherPage, "claude_code");
      const otherInput = otherPage.getByTestId("chat-input").first();
      await expect(otherInput).toBeVisible({ timeout: 30_000 });

      const terminals = watchTerminalConnections(page);
      await openRibbonPreview(page);
      const picker = page.getByLabel("Preview application");
      await expect(picker).toBeVisible({ timeout: 30_000 });
      const noisyOption = picker.locator("option", { hasText: "noisy-build" });
      await expect(noisyOption).toHaveCount(1, { timeout: 30_000 });
      await picker.selectOption(
        (await noisyOption.getAttribute("value")) ?? "",
      );
      const start = page.getByTestId("workspace-preview-start");
      await expect(start).toBeEnabled({ timeout: 30_000 });
      const startResponse = page.waitForResponse(
        (response) =>
          new URL(response.url()).pathname.endsWith(
            `/workspaces/${workspace.id}/preview`,
          ) && response.request().method() === "POST",
      );
      await start.click();
      const started = await (await startResponse).json();
      const taskId = String((started?.data ?? started)?.taskId ?? "");
      expect(taskId).toMatch(/^[0-9a-f-]{36}$/);

      // The run log opens at once, while the task may still be queued:
      // stages first, then one read-only terminal that waits for the
      // application and attaches to the task's hosted session without ever
      // being rebuilt (`g5.preview.run-log.attach`).
      const logToggle = page.getByTestId("workspace-preview-log-toggle");
      await expect(logToggle).toBeVisible({ timeout: 30_000 });
      await logToggle.click();
      const runLog = page.getByTestId("workspace-preview-run-log");
      await expect(runLog).toBeVisible();
      const dialog = page.getByTestId("workspace-preview-dialog");
      await expect(dialog).toHaveClass(/max-w-5xl/);
      const viewer = runLog.getByTestId("workspace-terminal-viewer");
      await expect(viewer).toBeVisible();
      const firstViewerElement = await viewer.elementHandle();
      const firstTerminalElement = await runLog
        .getByTestId("workspace-terminal")
        .elementHandle();
      await expect(
        runLog.getByTestId("workspace-preview-stage-building"),
      ).toHaveAttribute("data-state", "current", { timeout: 60_000 });
      await expect(viewer).toHaveAttribute("data-viewer-state", "attached", {
        timeout: 60_000,
      });
      // The same viewer and xterm host from Log open to attached: neither
      // the wait nor a retry replaced them.
      expect(
        await firstViewerElement?.evaluate((element) => element.isConnected),
      ).toBe(true);
      expect(
        await firstTerminalElement?.evaluate((element) => element.isConnected),
      ).toBe(true);
      // The first attach waited for the module start, so at most one
      // connection could have been refused before this one.
      expect(terminals.sockets.length).toBeLessThanOrEqual(2);
      await expect(
        runLog.getByTestId("workspace-terminal-readonly-badge"),
      ).toHaveText("read-only");
      await expect(
        runLog.getByTestId("workspace-terminal-viewer-status"),
      ).toHaveText("attached and following");
      await expect.poll(() => terminals.ids.at(-1)).toBe(`preview:${taskId}`);
      const transcript = runLog.locator(".xterm-accessibility-tree");
      await expect(transcript).toContainText("G5_NOISY_LINE", {
        timeout: 60_000,
      });

      // While the build still prints, a chat elsewhere leaves the queue
      // within the ordinary dispatch cadence.
      await otherInput.fill("Say hello in one short sentence.");
      const sendResponse = otherPage.waitForResponse((response) => {
        if (response.request().method() !== "POST") return false;
        const path = new URL(response.url()).pathname;
        return (
          /\/api\/v1\/sessions\/[^/]+\/messages$/.test(path) ||
          /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(path)
        );
      });
      await otherInput.press("Enter");
      secondTurn = await captureChatTurn(
        otherPage,
        await sendResponse,
        (created) => {
          secondTurn = created;
        },
      );
      expect(secondTurn.taskId).toMatch(/^[0-9a-f-]{36}$/);
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              otherPage,
              `/api/v1/tasks/${secondTurn!.taskId}`,
            );
            return (response.body?.data ?? response.body)?.status;
          },
          { timeout: 20_000, intervals: [250, 500] },
        )
        .not.toBe("queued");

      // The build ends and the application serves; the log followed it.
      await expect(transcript).toContainText("G5_NOISY_BUILD_DONE", {
        timeout: 90_000,
      });
      await expect(transcript).toContainText("port 4174 listening", {
        timeout: 60_000,
      });
      await expect(
        runLog.getByTestId("workspace-preview-stage-serving"),
      ).toHaveAttribute("data-state", "current", { timeout: 30_000 });

      // A second window on the same log (one viewer at a time,
      // `g5.preview.output.live-tail.viewer`): the newer viewer follows, the
      // older one is told it was superseded and stops reconnecting instead
      // of taking the session back by itself; Take over swaps the roles.
      const twinPage = await page.context().newPage();
      const twinTerminals = watchTerminalConnections(twinPage);
      await showWorkspaceTabs(twinPage);
      await openWorkspacePath(twinPage, workspace.path);
      await openRibbonPreview(twinPage);
      await twinPage.getByTestId("workspace-preview-log-toggle").click();
      const twinLog = twinPage.getByTestId("workspace-preview-run-log");
      const twinViewer = twinLog.getByTestId("workspace-terminal-viewer");
      await expect(twinViewer).toHaveAttribute(
        "data-viewer-state",
        "attached",
        {
          timeout: 60_000,
        },
      );
      await expect(viewer).toHaveAttribute("data-viewer-state", "superseded", {
        timeout: 30_000,
      });
      await expect(
        runLog.getByTestId("workspace-terminal-viewer-status"),
      ).toHaveText("viewing in another window");
      // The superseded viewer keeps what it showed and does not reconnect:
      // after a wait both pages hold the connections they had, and the newer
      // viewer is still attached and following.
      const socketsBefore = terminals.sockets.length;
      const twinSocketsBefore = twinTerminals.sockets.length;
      await page.waitForTimeout(8_000);
      expect(terminals.sockets.length).toBe(socketsBefore);
      expect(twinTerminals.sockets.length).toBe(twinSocketsBefore);
      await expect(twinViewer).toHaveAttribute("data-viewer-state", "attached");
      await expect(
        twinLog.getByTestId("workspace-terminal-viewer-status"),
      ).toHaveText("attached and following");
      await expect(transcript).toContainText("port 4174 listening");
      expect(terminals.text()).toContain('"reason":"superseded"');

      // Take over: this viewer resumes at its own offset and the other
      // window is superseded in turn. The guest answers the resume with no
      // reset, and the rendered transcript shows no "earlier output" marker
      // (the browser writes that marker itself on a redraw, so received
      // frames cannot show its absence).
      const openedBeforeTakeOver = terminals.opened.length;
      await runLog.getByTestId("workspace-terminal-take-over").click();
      await expect(viewer).toHaveAttribute("data-viewer-state", "attached", {
        timeout: 30_000,
      });
      await expect
        .poll(() => terminals.opened.length)
        .toBeGreaterThan(openedBeforeTakeOver);
      const resumed = terminals.opened.at(-1);
      expect(resumed?.terminal_id).toBe(`preview:${taskId}`);
      expect(resumed?.reattached).toBe(true);
      expect(resumed?.reset).toBe(false);
      await expect(transcript).not.toContainText("earlier output is beyond");
      await expect(twinViewer).toHaveAttribute(
        "data-viewer-state",
        "superseded",
        { timeout: 30_000 },
      );
      await expect(
        twinLog.getByTestId("workspace-terminal-take-over"),
      ).toBeVisible();
      await twinPage.close();

      // The journal carried lifecycle events only: thousands of printed
      // lines produced no task event.
      const timeline = await authedApiViaRequest(
        page,
        `/api/v1/tasks/${taskId}/timeline`,
      );
      expect(timeline.status, JSON.stringify(timeline.body)).toBe(200);
      const entries: Array<{ metadata?: { kind?: string } }> =
        (timeline.body?.data ?? timeline.body)?.entries ?? [];
      const kinds = entries.map((entry) => entry.metadata?.kind ?? "");
      expect(kinds).not.toContain("module.output");
      expect(kinds).toContain("module.preview.ready");
      expect(kinds.length).toBeLessThanOrEqual(10);

      // Reload: the dialog reopens the log and the retained tail is redrawn
      // from the hosted session, then output continues.
      const attachesBefore = terminals.ids.length;
      await page.reload();
      await waitForWorkspaceLoad(page);
      await openRibbonPreview(page);
      await page.getByTestId("workspace-preview-log-toggle").click();
      const reloadedLog = page.getByTestId("workspace-preview-run-log");
      await expect(
        reloadedLog.getByTestId("workspace-terminal-viewer"),
      ).toHaveAttribute("data-viewer-state", "attached", { timeout: 60_000 });
      await expect
        .poll(() => terminals.ids.length)
        .toBeGreaterThan(attachesBefore);
      expect(terminals.ids.at(-1)).toBe(`preview:${taskId}`);
      await expect(
        reloadedLog.locator(".xterm-accessibility-tree"),
      ).toContainText("G5_NOISY_BUILD_DONE", { timeout: 60_000 });
      // The redraw must leave the viewer following; the accessibility tree
      // below reflects the viewport, so the closing lines are visible only
      // while following.
      await expect(
        reloadedLog.getByTestId("workspace-terminal-viewer-status"),
      ).toHaveText("attached and following", { timeout: 15_000 });

      // Close: the shutdown line, the cleanup marker and its output stream
      // into the same session while Closing is current, then the session
      // ends and the dialog returns to the launcher with no log link.
      const closeResponse = page.waitForResponse(
        (response) =>
          response
            .url()
            .includes(`/workspaces/${workspace.id}/preview/cancel`) &&
          response.request().method() === "POST",
      );
      await page.getByTestId("workspace-preview-stop").click();
      expect((await closeResponse).status()).toBe(200);
      // The closing output streams into the same session the viewer holds:
      // the application's shutdown line, the cleanup marker and its output,
      // then the guest's closed/session_ended line. Read from the viewer's
      // socket frames: the dialog returns to the launcher as soon as the task
      // settles, so the xterm viewport is not a stable place to assert this.
      for (const needle of [
        "close requested, stopping application",
        "G5_NOISY_SHUTDOWN",
        "running cleanup",
        "G5_NOISY_CLEANUP",
        "── closed ──",
        '"reason":"session_ended"',
      ]) {
        await expect
          .poll(() => terminals.text().includes(needle), {
            timeout: 60_000,
            intervals: [250, 500],
            message: `the viewer must receive ${needle}`,
          })
          .toBe(true);
      }
      const closing = terminals.text();
      expect(closing.indexOf("G5_NOISY_SHUTDOWN")).toBeLessThan(
        closing.indexOf("running cleanup"),
      );
      expect(closing.indexOf("G5_NOISY_CLEANUP")).toBeLessThan(
        closing.indexOf("── closed ──"),
      );
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/tasks/${taskId}`,
            );
            return (response.body?.data ?? response.body)?.status;
          },
          { timeout: 60_000, intervals: [500, 1_000] },
        )
        .toBe("cancelled");
      await expect(page.getByTestId("workspace-preview-start")).toBeVisible({
        timeout: 15_000,
      });
      await expect(
        page.getByTestId("workspace-preview-log-toggle"),
      ).toHaveCount(0);
      // No log is rendered, so the shell is back at the width it had before
      // the log was opened (`g5.preview.dialog-controls`).
      await expect(page.getByTestId("workspace-preview-dialog")).toHaveClass(
        /max-w-md/,
      );
      await expect
        .poll(
          async () => {
            const changes = await readWorkspaceChanges(page, workspace.id);
            return changes.members
              .flatMap((member) => member.dirty ?? [])
              .map((entry) => entry.path);
          },
          { timeout: 30_000, intervals: [500, 1_000] },
        )
        .toContain("e2e-preview-cleanup.txt");
      await otherPage.close();
    });
  });

  test.describe(() => {
    test.describe.configure({ timeout: 360_000 });
    test("M12 Stop fences a live Preview and Start workspace and preview relaunches it on the owner's request", async ({
      page,
    }) => {
      await openRibbonPreview(page);
      await selectWorkspaceDefaultPreview(page);
      const start = page.getByTestId("workspace-preview-start");
      await expect(start).toBeEnabled({ timeout: 30_000 });
      await start.click();

      let taskId = "";
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/preview`,
            );
            expect(response.status, JSON.stringify(response.body)).toBe(200);
            const preview = response.body?.data ?? response.body;
            if (["failed", "cancelled", "succeeded"].includes(preview.status)) {
              throw new Error(
                `Preview ended before ready: ${JSON.stringify(preview)}`,
              );
            }
            taskId = String(preview.taskId ?? "");
            return preview.ready === true;
          },
          { timeout: 120_000, intervals: [1_000, 2_000] },
        )
        .toBe(true);
      expect(taskId).toMatch(/^[0-9a-f-]{36}$/);

      const popupPromise = page.waitForEvent("popup");
      await page.getByTestId("workspace-preview-open").click();
      const previewPage = await popupPromise;
      await previewPage.waitForLoadState("domcontentloaded");
      await expect(previewPage.locator("body")).toContainText(PREVIEW_MARKER, {
        timeout: 30_000,
      });
      const oldPreviewUrl = previewPage.url();
      const activeResponse = await fetchPreviewUrl(previewPage, oldPreviewUrl);
      expect(activeResponse.status).toBe(200);
      expect(activeResponse.text).toContain(PREVIEW_MARKER);
      const oldVm = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/vm`,
      );
      expect(oldVm.status, JSON.stringify(oldVm.body)).toBe(200);
      const oldVmId = String((oldVm.body?.data ?? oldVm.body)?.vmId ?? "");
      expect(oldVmId).toMatch(/^[0-9a-f-]{36}$/);

      await runWorkspaceVmAction(page, workspace.id, "stop");
      const stopped = await waitForWorkspaceVmStatus(
        page,
        workspace.id,
        "stopped",
      );
      expect(stopped.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/tasks/${taskId}`,
            );
            expect(response.status, JSON.stringify(response.body)).toBe(200);
            return (response.body?.data ?? response.body)?.status;
          },
          { timeout: 30_000, intervals: [500, 1_000] },
        )
        .toBe("cancelled");

      // The already-open browser still carries its old preview cookie. The
      // serving boundary must reject that credential after Stop.
      const staleResponse = await fetchPreviewUrl(previewPage, oldPreviewUrl);
      expect(staleResponse.status).toBe(503);
      await previewPage.close();
      const staleMint = await authedApiViaRequest(
        page,
        `/api/v1/preview/session/${taskId}`,
        "POST",
        {},
      );
      expect(staleMint.status, JSON.stringify(staleMint.body)).toBe(503);
      expect(JSON.stringify(staleMint.body)).toContain("PREVIEW_NOT_READY");

      // Nothing relaunches on its own after a Stop: the ended preview stays
      // the current one until the owner asks again.
      const afterStop = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/preview`,
      );
      expect(afterStop.status, JSON.stringify(afterStop.body)).toBe(200);
      const preview = afterStop.body?.data ?? afterStop.body;
      expect(preview?.status).not.toBe("running");
      expect([undefined, null, taskId]).toContain(preview?.taskId);

      // Start workspace and preview (g5.preview.start-and-preview): on the
      // stopped VM the launcher stays open on the committed configuration,
      // and one click records the Start with the preview bound to it.
      await selectWorkspaceDefaultPreview(page);
      await expect(start).toBeEnabled({ timeout: 30_000 });
      await expect(start).toHaveText(/Start workspace and preview/);
      await start.click();
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/preview`,
            );
            expect(response.status, JSON.stringify(response.body)).toBe(200);
            const next = response.body?.data ?? response.body;
            // The pending state ends the moment the VM reaches ready, so a
            // fast boot may show the new run already starting.
            return (
              next.taskId !== taskId &&
              (next.pendingStart === true ||
                !["failed", "cancelled", "succeeded"].includes(next.status))
            );
          },
          { timeout: 30_000, intervals: [500, 1_000] },
        )
        .toBe(true);
      const resumed = await waitForWorkspaceVmStatus(
        page,
        workspace.id,
        "running",
      );
      expect(resumed.vmId).not.toBe(oldVmId);
      expect(resumed.lastFlushId).toBe(stopped.lastFlushId);
      await expect
        .poll(
          async () => {
            const response = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/preview`,
            );
            expect(response.status, JSON.stringify(response.body)).toBe(200);
            const next = response.body?.data ?? response.body;
            if (["failed", "cancelled", "succeeded"].includes(next.status)) {
              throw new Error(
                `Preview ended before ready: ${JSON.stringify(next)}`,
              );
            }
            return next.taskId !== taskId &&
              next.status === "running" &&
              next.ready === true
              ? String(next.taskId)
              : "";
          },
          { timeout: 120_000, intervals: [1_000, 2_000] },
        )
        .toMatch(/^[0-9a-f-]{36}$/);
      const fresh = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/preview`,
      );
      expect((fresh.body?.data ?? fresh.body)?.taskId).not.toBe(taskId);
      await page.getByTestId("workspace-preview-stop").click();
    });
  });

  test.describe(() => {
    test.describe.configure({ timeout: 600_000 });
    test("M9/M12 transfer fences live chat, Preview, and terminal, retaining files and history", async ({
      page,
      browser,
    }) => {
      expect(USING_OAUTH_STUB, "transfer qualification needs OAuth stub").toBe(
        true,
      );
      await page.setViewportSize({ width: 1440, height: 900 });
      const orgId = (await readCurrentOrgContext(page)).orgId;
      const invite = await authedApiViaRequest(
        page,
        `/api/v1/orgs/${orgId}/members`,
        "POST",
        { email: SEEDED_FORK_MEMBER_IDENTITY.email, role: "member" },
      );
      expect([200, 201], JSON.stringify(invite.body)).toContain(invite.status);
      newOwnerContext = await browser.newContext({
        storageState: { cookies: [], origins: [] },
      });
      newOwnerPage = await newOwnerContext.newPage();
      await showWorkspaceTabs(newOwnerPage);
      // A fresh context misses auth.setup's pin of the full sidebar
      // (g5.ui.sidebar-default); the rail Terminal needs the Files header.
      await patchStoredUserSettings(newOwnerPage, {
        sidebarDisplayMode: "expanded",
      });
      await loginWithOAuthStub(newOwnerPage, SEEDED_FORK_MEMBER_IDENTITY, {
        targetOrgSlug: TEST_ORG,
      });
      const newOwnerId = await recordExpectedSubject(newOwnerPage);
      const detail = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}`,
      );
      expect(detail.status, JSON.stringify(detail.body)).toBe(200);
      const docsRepoId = String((detail.body?.data ?? detail.body)?.docsRepoId);
      const changes = await readWorkspaceChanges(page, workspace.id);
      const codeRepoId = changes.members.find(
        (member) => member.repo_id !== docsRepoId,
      )?.repo_id;
      expect(codeRepoId).toBeTruthy();

      await openRibbonPreview(page);
      await selectWorkspaceDefaultPreview(page);
      const start = page.getByTestId("workspace-preview-start");
      await expect(start).toBeEnabled({ timeout: 30_000 });
      await start.click();
      let oldTaskId = "";
      await expect
        .poll(
          async () => {
            const result = await authedApiViaRequest(
              page,
              `/api/v1/workspaces/${workspace.id}/preview`,
            );
            expect(result.status, JSON.stringify(result.body)).toBe(200);
            const preview = result.body?.data ?? result.body;
            if (["failed", "cancelled", "succeeded"].includes(preview.status)) {
              throw new Error(
                `Preview ended before transfer: ${JSON.stringify(preview)}`,
              );
            }
            oldTaskId = String(preview.taskId ?? "");
            return preview.ready === true;
          },
          { timeout: 120_000, intervals: [1_000, 2_000] },
        )
        .toBe(true);
      expect(oldTaskId).toMatch(/^[0-9a-f-]{36}$/);
      const popupPromise = page.waitForEvent("popup");
      await page.getByTestId("workspace-preview-open").click();
      const oldPreview = await popupPromise;
      await expect(oldPreview.locator("body")).toContainText(PREVIEW_MARKER, {
        timeout: 30_000,
      });
      const oldUrl = oldPreview.url();
      expect((await fetchPreviewUrl(oldPreview, oldUrl)).status).toBe(200);
      await page.getByTestId("workspace-preview-close").click();
      const beforeVm = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/vm`,
      );
      const oldVmId = String(
        (beforeVm.body?.data ?? beforeVm.body)?.vmId ?? "",
      );
      expect(oldVmId).toMatch(/^[0-9a-f-]{36}$/);

      const token = randomUUID();
      const oldTerminal = watchTerminalConnections(page);
      await openRailTerminal(page);
      await expect(
        page.getByText("Workspace terminal · Connected"),
      ).toBeVisible({
        timeout: 30_000,
      });
      await expect.poll(() => oldTerminal.ids.length).toBeGreaterThan(0);
      const oldTerminalId = oldTerminal.ids.at(-1);
      expect(oldTerminalId).toBeTruthy();
      const terminal = page.getByTestId("workspace-terminal");
      const terminalInput = terminal.locator(".xterm-helper-textarea");
      await terminalInput.focus();
      await terminalInput.pressSequentially(
        `export G5_M9_OWNER=${token}; printf 'M9_%s\\n' OLD_TERMINAL`,
      );
      await terminalInput.press("Enter");
      await expect(terminal.locator(".xterm-accessibility-tree")).toContainText(
        "M9_OLD_TERMINAL",
        { timeout: 30_000 },
      );

      // Hold a real provider turn while Preview and the terminal are serving.
      // Transfer must quiesce all three and include the agent's dirty cut.
      const live = await waitForLiveWorkspaceChanges(page, workspace.id);
      const codeMember = live.members.find(
        (member) => member.repo_id === codeRepoId,
      );
      const docsMember = live.members.find(
        (member) => member.repo_id === docsRepoId,
      );
      expect(codeMember?.root_path).toBeTruthy();
      expect(docsMember?.root_path).toBeTruthy();
      const codePath = `e2e-transfer-chat-${token}.txt`;
      const docsPath = `docs/${workspace.id}/e2e-transfer-chat-${token}.md`;
      const gatePath = `e2e-transfer-chat-gate-${token}.txt`;
      const codeBytes = `M9 transfer code ${token}\n`;
      const docsBytes = `# M9 transfer Project Doc ${token}\n`;
      const writeSet = Buffer.from(
        JSON.stringify([
          {
            path: `${codeMember!.root_path}/${codePath}`,
            contentBase64: Buffer.from(codeBytes).toString("base64"),
          },
          {
            path: `${docsMember!.root_path}/${docsPath}`,
            contentBase64: Buffer.from(docsBytes).toString("base64"),
          },
        ]),
      ).toString("base64");
      await page.getByTestId("center-chat-tab").click();
      await selectAgentProviderViaSettings(page, "claude_code");
      await expectSelectedAgentProvider(page, "claude_code");
      const input = page.getByTestId("chat-input").first();
      await expect(input).toBeVisible({ timeout: 30_000 });
      await input.fill(
        `M9 ${token}: write code and a Project Doc, then wait. G5_FS_WRITE_SET_E2E:${writeSet} G5_WAIT_FOR_FILE_E2E:${codeMember!.root_path}/${gatePath}`,
      );
      const sendResponse = page.waitForResponse((response) => {
        if (response.request().method() !== "POST") return false;
        const path = new URL(response.url()).pathname;
        return (
          /\/api\/v1\/sessions\/[^/]+\/messages$/.test(path) ||
          /\/api\/v1\/workspaces\/[^/]+\/sessions\/start$/.test(path)
        );
      });
      await input.press("Enter");
      heldTurn = await captureChatTurn(page, await sendResponse, (created) => {
        heldTurn = created;
      });
      for (const { repoId, path, bytes } of [
        { repoId: codeRepoId!, path: codePath, bytes: codeBytes },
        { repoId: docsRepoId, path: docsPath, bytes: docsBytes },
      ]) {
        await expect
          .poll(
            async () => {
              const response = await authedApiViaRequest(
                page,
                `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
              );
              return response.status === 200
                ? (response.body?.data ?? response.body)?.content
                : null;
            },
            { timeout: 90_000, intervals: [500, 1_000] },
          )
          .toBe(bytes);
      }
      const heldTask = await authedApiViaRequest(
        page,
        `/api/v1/tasks/${heldTurn.taskId}`,
      );
      expect(heldTask.status, JSON.stringify(heldTask.body)).toBe(200);
      expect((heldTask.body?.data ?? heldTask.body)?.status).toBe("running");
      const heldAttemptId = (heldTask.body?.data ?? heldTask.body)
        ?.latestAttempt?.id;
      expect(heldAttemptId, JSON.stringify(heldTask.body)).toBeTruthy();
      expect(oldTerminal.sockets.length).toBeGreaterThan(0);
      expect(oldTerminal.sockets.at(-1)?.isClosed()).toBe(false);

      const transferKey = randomUUID();
      await expect
        .poll(
          async () => {
            try {
              const result = await authedApiViaRequest(
                page,
                `/api/v1/workspaces/${workspace.id}/transfer`,
                "POST",
                { toUserId: newOwnerId, idempotencyKey: transferKey },
              );
              expect(result.status, JSON.stringify(result.body)).toBe(200);
              return (result.body?.data ?? result.body)?.status;
            } catch (error) {
              if (
                error instanceof Error &&
                /timeout|ECONNRESET/.test(error.message)
              ) {
                return "pending";
              }
              throw error;
            }
          },
          { timeout: 360_000, intervals: [1_000, 2_000] },
        )
        .toBe("completed");
      const stopped = await waitForWorkspaceVmStatus(
        newOwnerPage,
        workspace.id,
        "stopped",
      );
      expect(stopped.lastFlushId).toMatch(/^[0-9a-f-]{36}$/);
      await expect
        .poll(
          async () => {
            const task = await authedApiViaRequest(
              newOwnerPage!,
              `/api/v1/tasks/${oldTaskId}`,
            );
            return (task.body?.data ?? task.body)?.status;
          },
          { timeout: 30_000, intervals: [500, 1_000] },
        )
        .toBe("cancelled");
      await expect
        .poll(
          () =>
            oldTerminal.sockets.length > 0 &&
            oldTerminal.sockets.every((socket) => socket.isClosed()),
          { timeout: 30_000, intervals: [500, 1_000] },
        )
        .toBe(true);
      await expect
        .poll(
          async () => {
            const task = await authedApiViaRequest(
              newOwnerPage!,
              `/api/v1/tasks/${heldTurn!.taskId}`,
            );
            return (task.body?.data ?? task.body)?.status;
          },
          { timeout: 30_000, intervals: [500, 1_000] },
        )
        .toBe("waiting");
      const quiesced = await authedApiViaRequest(
        newOwnerPage,
        `/api/v1/tasks/${heldTurn.taskId}`,
      );
      expect(quiesced.status, JSON.stringify(quiesced.body)).toBe(200);
      const quiescedTask = quiesced.body?.data ?? quiesced.body;
      expect(quiescedTask.latestAttempt?.id).toBe(heldAttemptId);
      expect(quiescedTask.latestAttempt?.endedAt).toBeTruthy();
      for (const { repoId, path } of [
        { repoId: codeRepoId!, path: codePath },
        { repoId: docsRepoId, path: docsPath },
      ]) {
        // Transfer retains unpublished work for the recipient's next Start,
        // without exposing it through stopped browsing to either actor.
        for (const actorPage of [newOwnerPage, page]) {
          const file = await authedApiViaRequest(
            actorPage,
            `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
          );
          expect(file.status, JSON.stringify(file.body)).toBe(404);
        }
      }
      const oldWrite = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/files/old-owner-${token}.txt?repoId=${codeRepoId}`,
        "PUT",
        { content: "must be refused\n", encoding: "utf-8", expectedHash: null },
      );
      expect(oldWrite.status, JSON.stringify(oldWrite.body)).toBe(403);
      const stale = await fetchPreviewUrl(oldPreview, oldUrl);
      expect(stale.status).toBe(403);
      await oldPreview.close();
      const oldMint = await authedApiViaRequest(
        page,
        `/api/v1/preview/session/${oldTaskId}`,
        "POST",
        {},
      );
      expect(oldMint.status, JSON.stringify(oldMint.body)).toBe(403);
      const oldRestart = await authedApiViaRequest(
        page,
        `/api/v1/workspaces/${workspace.id}/preview`,
        "POST",
        { repoId: codeRepoId, idempotencyKey: randomUUID() },
      );
      expect(oldRestart.status).toBe(403);

      await runWorkspaceVmAction(newOwnerPage, workspace.id, "resume");
      const resumed = await waitForWorkspaceVmStatus(
        newOwnerPage,
        workspace.id,
        "running",
      );
      expect(resumed.vmId).not.toBe(oldVmId);
      expect(resumed.lastFlushId).toBe(stopped.lastFlushId);
      const newVm = await authedApiViaRequest(
        newOwnerPage,
        `/api/v1/workspaces/${workspace.id}/vm`,
      );
      expect(newVm.status, JSON.stringify(newVm.body)).toBe(200);
      expect((newVm.body?.data ?? newVm.body)?.ownerUserId).toBe(newOwnerId);
      // The new owner's Start returns the transferred turn to the queue
      // (`g5.lifecycle.start.held-work`): a new attempt continues it under the
      // new owner, and the held attempt stays ended.
      await expect
        .poll(
          async () => {
            const task = await authedApiViaRequest(
              newOwnerPage!,
              `/api/v1/tasks/${heldTurn!.taskId}`,
            );
            expect(task.status, JSON.stringify(task.body)).toBe(200);
            const current = task.body?.data ?? task.body;
            return current?.latestAttempt?.id !== heldAttemptId
              ? current?.status
              : "held";
          },
          { timeout: 120_000, intervals: [500, 1_000] },
        )
        .toBe("succeeded");
      for (const { repoId, path, bytes } of [
        { repoId: codeRepoId!, path: codePath, bytes: codeBytes },
        { repoId: docsRepoId, path: docsPath, bytes: docsBytes },
      ]) {
        await expect
          .poll(
            async () => {
              const file = await authedApiViaRequest(
                newOwnerPage!,
                `/api/v1/workspaces/${workspace.id}/files/${path}?repoId=${repoId}`,
              );
              return file.status === 200
                ? (file.body?.data ?? file.body)?.content
                : null;
            },
            { timeout: 90_000, intervals: [500, 1_000] },
          )
          .toBe(bytes);
      }
      const history = await authedApiViaRequest(
        newOwnerPage,
        `/api/v1/sessions/${heldTurn.sessionId}/messages`,
      );
      expect(history.status, JSON.stringify(history.body)).toBe(200);
      expect(JSON.stringify(history.body)).toContain(token);
      const inheritedPreview = await authedApiViaRequest(
        newOwnerPage,
        `/api/v1/workspaces/${workspace.id}/preview`,
      );
      expect(inheritedPreview.status).toBe(200);
      expect(
        (inheritedPreview.body?.data ?? inheritedPreview.body)?.status,
      ).not.toBe("running");
      expect([undefined, null, oldTaskId]).toContain(
        (inheritedPreview.body?.data ?? inheritedPreview.body)?.taskId,
      );
      await openWorkspacePath(newOwnerPage, workspace.path);
      await newOwnerPage.setViewportSize({ width: 1440, height: 900 });
      const freshTerminal = watchTerminalConnections(newOwnerPage);
      await openRailTerminal(newOwnerPage);
      await expect(
        newOwnerPage.getByText("Workspace terminal · Connected"),
      ).toBeVisible({ timeout: 30_000 });
      await expect.poll(() => freshTerminal.ids.length).toBeGreaterThan(0);
      expect(freshTerminal.ids.at(-1)).not.toBe(oldTerminalId);
      const freshInput = newOwnerPage
        .getByTestId("workspace-terminal")
        .locator(".xterm-helper-textarea");
      await freshInput.focus();
      await freshInput.pressSequentially(
        "printf 'M9_NEW_TERMINAL_%s\\n' \"${G5_M9_OWNER-unset}\"",
      );
      await freshInput.press("Enter");
      await expect(
        newOwnerPage
          .getByTestId("workspace-terminal")
          .locator(".xterm-accessibility-tree"),
      ).toContainText("M9_NEW_TERMINAL_unset", { timeout: 30_000 });
      const sessionList = await authedApiViaRequest(
        newOwnerPage,
        `/api/v1/workspaces/${workspace.id}/sessions`,
      );
      expect(sessionList.status, JSON.stringify(sessionList.body)).toBe(200);
      const retainedTitle = (sessionList.body?.data ?? sessionList.body)?.find(
        (session: { id: string }) => session.id === heldTurn!.sessionId,
      )?.title;
      expect(retainedTitle).toBeTruthy();
      await newOwnerPage.getByTestId("center-chat-tab").click();
      const historyPicker = newOwnerPage.getByTestId("chat-center-history");
      await expect(historyPicker).toBeVisible({ timeout: 30_000 });
      await historyPicker.click();
      await newOwnerPage
        .getByPlaceholder("Search sessions…")
        .fill(retainedTitle);
      await newOwnerPage
        .getByTestId("chat-session-select")
        .filter({ hasText: retainedTitle })
        .click();
      await expect(newOwnerPage.getByTestId("chat-messages")).toContainText(
        token,
        { timeout: 30_000 },
      );
      await openRibbonPreview(newOwnerPage);
      await selectWorkspaceDefaultPreview(newOwnerPage);
      const newStart = newOwnerPage.getByTestId("workspace-preview-start");
      await expect(newStart).toBeEnabled({ timeout: 30_000 });
      await newStart.click();
      await expect
        .poll(
          async () => {
            const result = await authedApiViaRequest(
              newOwnerPage!,
              `/api/v1/workspaces/${workspace.id}/preview`,
            );
            const preview = result.body?.data ?? result.body;
            return preview?.ready === true && preview?.taskId !== oldTaskId
              ? String(preview.taskId)
              : "";
          },
          { timeout: 120_000, intervals: [1_000, 2_000] },
        )
        .toMatch(/^[0-9a-f-]{36}$/);
      const freshPopupPromise = newOwnerPage.waitForEvent("popup");
      await newOwnerPage.getByTestId("workspace-preview-open").click();
      const freshPreview = await freshPopupPromise;
      await expect(freshPreview.locator("body")).toContainText(PREVIEW_MARKER, {
        timeout: 30_000,
      });
      await freshPreview.close();
      await newOwnerPage.getByTestId("workspace-preview-stop").click();
    });
  });
});
