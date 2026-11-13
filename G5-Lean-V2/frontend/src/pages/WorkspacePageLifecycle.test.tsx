import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes, useNavigate } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  createMockUser,
  createMockWorkspaceVm,
  mockWorkspaceDetails,
} from "../test/fixtures";
import { renderApp } from "../test/render";
import { server } from "../test/msw/server";
import { WorkspacePage } from "./WorkspacePage";
import { resetCapacityWaitAnnouncementsForTests } from "../lib/capacityWaitNotice";

vi.mock("../components/execution/WorkspaceTerminalPanel", () => ({
  WorkspaceTerminalPanel: () => <div />,
}));

// Every toast the page pushes, by title, so a notice's count is asserted
// without racing its auto-dismiss. The wrapper is as stable as the real
// `pushToast`, so the page's effects keep their real dependency graph.
const pushedToastTitles: string[] = [];
vi.mock("../hooks/useClientToasts", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../hooks/useClientToasts")>();
  const { useMemo } = await import("react");
  function useCountingClientToasts() {
    const toasts = actual.useClientToasts();
    const original = toasts.pushToast;
    const pushToast = useMemo(
      () =>
        (...args: Parameters<typeof original>) => {
          pushedToastTitles.push(args[1]);
          return original(...args);
        },
      [original],
    );
    return { ...toasts, pushToast };
  }
  return { ...actual, useClientToasts: useCountingClientToasts };
});

function NavigablePage() {
  const navigate = useNavigate();
  return (
    <>
      <button onClick={() => navigate("/g5labs/a")}>Go to A</button>
      <button onClick={() => navigate("/g5labs/b")}>Go to B</button>
      <WorkspacePage />
    </>
  );
}

function mockViewport(mobile: boolean) {
  vi.mocked(window.matchMedia).mockImplementation((query: string) => ({
    matches: mobile
      ? query.includes("max-width: 1023px")
      : query.includes("min-width: 1024px"),
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
}

/** Owned, non-Base workspaces `a` and `b` with no chat provider. */
function workspacePageHandlers() {
  return [
    // This fixture exercises lifecycle controls with no available chat provider.
    http.post("/api/v1/workspaces/:id/config/agent-launch/resolve", () =>
      HttpResponse.json(
        { error: { code: "NOT_FOUND", message: "No provider configured" } },
        { status: 404 },
      ),
    ),
    http.get("/api/v1/workspaces/:id/provider-quota", () =>
      HttpResponse.json(
        { error: { code: "NOT_FOUND", message: "No provider configured" } },
        { status: 404 },
      ),
    ),
    http.get("/api/v1/resolve", ({ request }) =>
      HttpResponse.json({
        data: {
          orgId: "org-g5labs",
          workspaceId: new URL(request.url).searchParams.get("workspace"),
        },
      }),
    ),
    http.get("/api/v1/workspaces/:id", ({ params }) =>
      HttpResponse.json({
        data: {
          ...mockWorkspaceDetails.base,
          id: params.id,
          name: params.id,
          ownerUserId: "user-alex",
          isBase: false,
          clonedFromId: "root",
        },
      }),
    ),
  ];
}

function setup(mobile: boolean, action: "resume" | "stop" | "transfer") {
  mockViewport(mobile);
  const calls: string[] = [];
  const replies = new Map<string, (response: Response) => void>();
  const reads: string[] = [];
  server.use(
    ...workspacePageHandlers(),
    http.get("/api/v1/workspaces/:id/vm", ({ params }) => {
      reads.push(String(params.id));
      return HttpResponse.json({
        data: createMockWorkspaceVm({
          workspaceId: String(params.id),
          ownerUserId: "user-alex",
          status:
            action === "stop" && params.id === "a" ? "running" : "stopped",
        }),
      });
    }),
    http.post(`/api/v1/workspaces/:id/${action}`, ({ params }) => {
      calls.push(String(params.id));
      return new Promise<Response>((resolve) =>
        replies.set(String(params.id), resolve),
      );
    }),
  );
  const { queryClient } = renderApp(
    <Routes>
      <Route path="/:org/:workspace" element={<NavigablePage />} />
    </Routes>,
    {
      route: "/g5labs/a",
      user: createMockUser(),
    },
  );
  return { calls, reads, replies, queryClient };
}

beforeEach(() => {
  pushedToastTitles.length = 0;
  resetCapacityWaitAnnouncementsForTests();
});

describe("WorkspacePage lifecycle navigation", () => {
  for (const mobile of [false, true]) {
    it(`${mobile ? "mobile strip" : "ribbon"} and banner retain independent Starts and late errors`, async () => {
      const { calls, reads, replies } = setup(mobile, "resume");
      const start = () => screen.getByTestId("workspace-vm-start");
      const banner = () =>
        screen.getByTestId("workspace-readonly-banner-start");
      await waitFor(() => expect(start()).toBeEnabled());
      if (mobile)
        expect(screen.getByTestId("workspace-mobile-lifecycle")).toBeVisible();
      await userEvent.click(start());
      await waitFor(() => expect(calls).toEqual(["a"]));
      await userEvent.click(screen.getByText("Go to B"));
      await waitFor(() => expect(start()).toBeEnabled());
      expect(banner()).toBeEnabled();
      await userEvent.click(banner());
      await waitFor(() => expect(calls).toEqual(["a", "b"]));
      await userEvent.click(screen.getByText("Go to A"));
      await waitFor(() => expect(start()).toBeDisabled());
      expect(banner()).toBeDisabled();
      await userEvent.click(screen.getByText("Go to B"));
      await waitFor(() => expect(start()).toBeDisabled());
      const aReads = reads.filter((id) => id === "a").length;
      act(() =>
        replies.get("a")!(
          HttpResponse.json(
            { error: { code: "FORBIDDEN", message: "A start refused" } },
            { status: 403 },
          ),
        ),
      );
      // Settlement invalidates A; showing it again must load it, without A's
      // late error leaking into B's strip or notification area.
      act(() =>
        replies.get("b")!(
          HttpResponse.json({
            data: {
              outcome: "queued_for_capacity",
              vm: createMockWorkspaceVm({
                workspaceId: "b",
                status: "queued_for_capacity",
              }),
            },
          }),
        ),
      );
      await waitFor(() => expect(start()).toBeEnabled());
      expect(screen.queryByText("A start refused")).toBeNull();
      expect(screen.queryByText("Could not start workspace")).toBeNull();
      await userEvent.click(screen.getByText("Go to A"));
      await waitFor(() =>
        expect(screen.getByText("A start refused")).toBeVisible(),
      );
      expect(reads.filter((id) => id === "a").length).toBeGreaterThan(aReads);
    });
  }

  for (const action of ["stop", "transfer"] as const) {
    it(`${action} does not disable B or notify it when A completes`, async () => {
      const { calls, replies, queryClient } = setup(false, action);
      if (action === "stop") {
        await userEvent.click(await screen.findByTestId("workspace-vm-stop"));
      } else {
        await userEvent.click(
          await screen.findByTestId("workspace-owner-trigger"),
        );
        const options = await screen.findAllByTestId("workspace-owner-option");
        await userEvent.click(options[0]);
        await userEvent.click(screen.getByTestId("workspace-owner-transfer"));
      }
      await waitFor(() => expect(calls).toEqual(["a"]));
      await userEvent.click(screen.getByText("Go to B"));
      await waitFor(() =>
        expect(screen.getByTestId("workspace-vm-start")).toBeEnabled(),
      );
      expect(
        screen.getByTestId("workspace-readonly-banner-start"),
      ).toBeEnabled();
      act(() =>
        replies.get("a")!(
          HttpResponse.json({
            data: {
              status: action === "stop" ? "stopped" : "completed",
              outcome: "stopped",
              vm: createMockWorkspaceVm({
                workspaceId: "a",
                status: "stopped",
              }),
            },
          }),
        ),
      );
      await waitFor(() => expect(queryClient.isMutating()).toBe(0));
      expect(screen.queryByText("Workspace transferred")).toBeNull();
      expect(screen.queryByText("Workspace stopped")).toBeNull();
      expect(
        within(screen.getByTestId("workbench-workspace-controls")).getByTestId(
          "workspace-vm-start",
        ),
      ).toBeEnabled();
    });
  }
});

describe("WorkspacePage capacity wait notice", () => {
  // A queued start reaches the owner as a pop-up once per Start, from Start's
  // answer and the VM poll, and never as text among the lifecycle controls;
  // a changed reason updates the banner, a return to the waiting workspace
  // announces nothing new, and a later Start is announced afresh
  // (`g5.ui.capacity-wait-notice`).
  for (const mobile of [false, true]) {
    it(`${mobile ? "phone" : "desktop"}: announces a wait once as a toast and keeps the controls free of it`, async () => {
      mockViewport(mobile);
      const title = "Workspace waiting for capacity";
      const announced = () =>
        pushedToastTitles.filter((pushed) => pushed === title).length;
      let vmStatus: "stopped" | "queued_for_capacity" = "stopped";
      let reason = "preparing_capacity";
      let startIntentId = "start-1";
      let queuedReads = 0;
      const queuedVm = (workspaceId: string) =>
        createMockWorkspaceVm({
          workspaceId,
          ownerUserId: "user-alex",
          status: "queued_for_capacity",
          startIntent: { id: startIntentId, status: "queued_for_capacity" },
          lastError: { reason },
        });
      server.use(
        ...workspacePageHandlers(),
        http.get("/api/v1/workspaces/:id/vm", ({ params }) => {
          const queued =
            params.id === "a" && vmStatus === "queued_for_capacity";
          if (queued) queuedReads += 1;
          return HttpResponse.json({
            data: queued
              ? queuedVm(String(params.id))
              : createMockWorkspaceVm({
                  workspaceId: String(params.id),
                  ownerUserId: "user-alex",
                  status: "stopped",
                }),
          });
        }),
        http.post("/api/v1/workspaces/:id/resume", ({ params }) => {
          vmStatus = "queued_for_capacity";
          return HttpResponse.json({
            data: {
              outcome: "queued_for_capacity",
              vm: queuedVm(String(params.id)),
            },
          });
        }),
      );
      renderApp(
        <Routes>
          <Route path="/:org/:workspace" element={<NavigablePage />} />
        </Routes>,
        { route: "/g5labs/a", user: createMockUser() },
      );
      // The lifecycle controls: the ribbon's on desktop, the strip on a phone.
      const controls = () =>
        screen.getByTestId(
          mobile
            ? "workspace-mobile-lifecycle"
            : "workbench-workspace-controls",
        );
      const waiting = () =>
        mobile
          ? within(controls()).getByText("VM: Waiting for capacity")
          : within(controls()).getByTestId("workspace-vm-pending");
      await userEvent.click(await screen.findByTestId("workspace-vm-start"));
      expect(await screen.findByText(title)).toBeVisible();
      expect(
        within(screen.getByTestId("toast")).getByText(/Preparing capacity/),
      ).toBeVisible();
      expect(announced()).toBe(1);
      // Further polls of the same wait add nothing.
      await waitFor(() => expect(queuedReads).toBeGreaterThanOrEqual(2), {
        timeout: 5_000,
      });
      expect(announced()).toBe(1);
      expect(waiting()).toBeVisible();
      expect(
        within(controls()).queryByTestId("workspace-vm-capacity-reason"),
      ).toBeNull();
      expect(within(controls()).queryByText(/Preparing capacity/)).toBeNull();
      if (!mobile) {
        expect(waiting()).toHaveAttribute(
          "title",
          expect.stringMatching(/Preparing capacity/),
        );
      }
      // The state banner keeps the reason in view for as long as the wait
      // lasts (on a phone, directly above the controls), and follows a
      // changed reason without another pop-up.
      const banner = () => screen.getByTestId("workspace-readonly-banner");
      expect(within(banner()).getByText(/Preparing capacity/)).toBeVisible();
      if (mobile) {
        expect(
          banner().compareDocumentPosition(controls()) &
            Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
      }
      reason = "capacity_stockout";
      await waitFor(
        () =>
          expect(
            within(banner()).getByText(
              /Cloud capacity is temporarily unavailable/,
            ),
          ).toBeVisible(),
        { timeout: 5_000 },
      );
      expect(announced()).toBe(1);
      // Leaving for another workspace and coming back to the same wait is not
      // news either.
      await userEvent.click(screen.getByText("Go to B"));
      await waitFor(() =>
        expect(screen.getByTestId("workspace-vm-start")).toBeEnabled(),
      );
      await userEvent.click(screen.getByText("Go to A"));
      await waitFor(() => expect(waiting()).toBeVisible());
      expect(announced()).toBe(1);
      // A later Start, after this wait ended, is announced afresh.
      vmStatus = "stopped";
      await waitFor(
        () =>
          expect(
            within(controls()).getByTestId("workspace-vm-start"),
          ).toBeEnabled(),
        { timeout: 5_000 },
      );
      startIntentId = "start-2";
      await userEvent.click(
        within(controls()).getByTestId("workspace-vm-start"),
      );
      await waitFor(() => expect(announced()).toBe(2), { timeout: 5_000 });
    }, 30_000);
  }
});
