import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  PlatformIssueReportButton,
  PlatformIssueReportProvider,
  PlatformIssueReportTrigger,
} from "./PlatformIssueReportButton";
import { PlatformIssueReportChromePending } from "./PlatformIssueReportChromePending";
import { Modal } from "../ui/Modal";
import type { InstanceInfoDTO, MutationHook } from "../../lib/types";

const { useInstanceInfoMock, reportMutateAsync, useReportMock, captureMock } =
  vi.hoisted(() => ({
    useInstanceInfoMock: vi.fn(),
    reportMutateAsync: vi.fn(),
    useReportMock: vi.fn(),
    captureMock: vi.fn(),
  }));

vi.mock("../../hooks/useSettings", () => ({
  useInstanceInfo: useInstanceInfoMock,
}));
vi.mock("../../hooks/usePlatformIssues", () => ({
  usePlatformIssueReport: useReportMock,
}));
vi.mock("./captureScreenshot", () => ({ captureScreenshot: captureMock }));

function instanceInfo(features: string[]): {
  status: "ok";
  data: InstanceInfoDTO;
} {
  return {
    status: "ok",
    data: {
      version: "1.2.3",
      features,
      authProviders: [],
      maxUploadSizeMb: 100,
      selfHosted: false,
      devLoginEnabled: false,
    },
  };
}

function reportHook(): MutationHook<unknown, { submissionId: string }> {
  return {
    mutate: vi.fn(),
    mutateAsync: reportMutateAsync,
    isLoading: false,
    error: null,
    data: undefined,
    reset: vi.fn(),
  };
}

describe("PlatformIssueReportButton", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState({}, "", "/");
    useInstanceInfoMock.mockReturnValue(
      instanceInfo(["platform_issue_reporting"]),
    );
    reportMutateAsync.mockResolvedValue({ submissionId: "sub-123" });
    useReportMock.mockReturnValue(reportHook());
    captureMock.mockResolvedValue(
      new Blob(["png-bytes"], { type: "image/png" }),
    );
  });

  it("is hidden when the platform_issue_reporting feature is absent", () => {
    useInstanceInfoMock.mockReturnValue(instanceInfo(["chat"]));
    render(<PlatformIssueReportButton />);
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
  });

  it("renders an inline workspace-top-bar trigger when requested", () => {
    render(<PlatformIssueReportButton placement="topbar" />);
    expect(
      screen.getByTestId("platform-bug-report-topbar-button"),
    ).toBeTruthy();
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
  });

  it("captures a screenshot, opens the drawer, and submits the report", async () => {
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));

    // Drawer opens immediately; capture runs in the background.
    await screen.findByTestId("platform-bug-report-drawer");
    // Captured screenshot is previewed once the background capture settles.
    await screen.findByTestId("platform-bug-report-screenshot-preview");
    await waitFor(() => expect(captureMock).toHaveBeenCalledTimes(1));

    fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
      target: { value: "Tab bar loses state" },
    });
    fireEvent.click(screen.getByTestId("platform-bug-report-submit"));

    await waitFor(() => expect(reportMutateAsync).toHaveBeenCalledTimes(1));
    const payload = reportMutateAsync.mock.calls[0][0];
    expect(payload.title).toBe("Tab bar loses state");
    expect(payload.severity).toBe("medium");
    expect(payload.context.appVersion).toBe("1.2.3");
    expect(payload.screenshot).toBeInstanceOf(Blob);

    // Confirmation shows the submission id.
    const ack = await screen.findByTestId("platform-bug-report-submission-id");
    expect(ack.textContent).toContain("sub-123");
  });

  it("rejects a zero-byte capture and attaches no screenshot", async () => {
    // A failed rasterization can resolve to an empty blob; it must not attach.
    captureMock.mockResolvedValue(new Blob([], { type: "image/png" }));
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));

    await screen.findByTestId("platform-bug-report-drawer");
    // Empty capture is treated as "no screenshot", never a preview.
    await screen.findByTestId("platform-bug-report-no-screenshot");
    await waitFor(() => expect(captureMock).toHaveBeenCalledTimes(1));
    expect(
      screen.queryByTestId("platform-bug-report-screenshot-preview"),
    ).toBeNull();

    // Submit still works and carries no screenshot.
    fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
      target: { value: "No shot" },
    });
    fireEvent.click(screen.getByTestId("platform-bug-report-submit"));
    await waitFor(() => expect(reportMutateAsync).toHaveBeenCalledTimes(1));
    expect(reportMutateAsync.mock.calls[0][0].screenshot).toBeNull();
  });

  it("opens the drawer immediately and shows a capturing status", async () => {
    let resolveCapture: (blob: Blob) => void = () => {};
    captureMock.mockReturnValueOnce(
      new Promise<Blob>((resolve) => {
        resolveCapture = resolve;
      }),
    );
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));

    // Drawer is present without awaiting rasterization.
    expect(screen.getByTestId("platform-bug-report-drawer")).toBeTruthy();
    expect(
      screen.getByTestId("platform-bug-report-screenshot-capturing"),
    ).toBeTruthy();
    expect(
      screen.queryByTestId("platform-bug-report-screenshot-preview"),
    ).toBeNull();

    await act(async () => {
      resolveCapture(new Blob(["png-bytes"], { type: "image/png" }));
    });
    await screen.findByTestId("platform-bug-report-screenshot-preview");
    expect(
      screen.queryByTestId("platform-bug-report-screenshot-capturing"),
    ).toBeNull();
  });

  it("disables Submit while capturing and enables it once capture settles", async () => {
    let resolveCapture: (blob: Blob) => void = () => {};
    captureMock.mockReturnValueOnce(
      new Promise<Blob>((resolve) => {
        resolveCapture = resolve;
      }),
    );
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");

    fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
      target: { value: "Has a title already" },
    });
    // Title is present but capture is still in flight: Submit stays disabled so
    // the preview-and-remove privacy decision is always possible first.
    expect(screen.getByTestId("platform-bug-report-submit")).toBeDisabled();

    await act(async () => {
      resolveCapture(new Blob(["png-bytes"], { type: "image/png" }));
    });
    await waitFor(() =>
      expect(
        screen.getByTestId("platform-bug-report-submit"),
      ).not.toBeDisabled(),
    );
  });

  it("ignores a capture that resolves after the drawer was closed", async () => {
    let resolveStale: (blob: Blob | null) => void = () => {};
    captureMock
      .mockReturnValueOnce(
        new Promise<Blob | null>((resolve) => {
          resolveStale = resolve;
        }),
      )
      .mockResolvedValueOnce(null);
    render(<PlatformIssueReportButton />);

    // Open, then close while the first capture is still in flight.
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-screenshot-capturing");
    await waitFor(() => expect(captureMock).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByTestId("platform-bug-report-drawer")).toBeNull(),
    );

    // Reopen: the second capture resolves to no screenshot.
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-no-screenshot");

    // The first (stale) capture now resolves with a blob — it must be ignored.
    await act(async () => {
      resolveStale(new Blob(["stale-bytes"], { type: "image/png" }));
    });
    expect(
      screen.queryByTestId("platform-bug-report-screenshot-preview"),
    ).toBeNull();
    await screen.findByTestId("platform-bug-report-no-screenshot");
  });

  it("does not start capture after closing before the animation frame", async () => {
    let scheduledFrame: FrameRequestCallback | undefined;
    const requestAnimationFrameSpy = vi
      .spyOn(globalThis, "requestAnimationFrame")
      .mockImplementation((callback) => {
        scheduledFrame = callback;
        return 1;
      });

    try {
      render(<PlatformIssueReportButton />);
      fireEvent.click(screen.getByTestId("platform-bug-report-button"));
      await screen.findByTestId("platform-bug-report-drawer");

      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      await waitFor(() =>
        expect(screen.queryByTestId("platform-bug-report-drawer")).toBeNull(),
      );

      expect(scheduledFrame).toBeTypeOf("function");
      await act(async () => {
        scheduledFrame?.(0);
      });
      expect(captureMock).not.toHaveBeenCalled();
    } finally {
      requestAnimationFrameSpy.mockRestore();
    }
  });

  it("captures with a filter that excludes flagged report-UI nodes", async () => {
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-screenshot-preview");

    await waitFor(() => expect(captureMock).toHaveBeenCalledTimes(1));
    const options = captureMock.mock.calls[0][0];
    expect(typeof options?.filter).toBe("function");

    const flagged = document.createElement("div");
    flagged.setAttribute("data-screenshot-exclude", "");
    const plain = document.createElement("div");
    expect(options.filter(flagged)).toBe(false);
    expect(options.filter(plain)).toBe(true);
  });

  it("lets the user remove the captured screenshot before submitting", async () => {
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-screenshot-preview");

    fireEvent.click(
      screen.getByTestId("platform-bug-report-remove-screenshot"),
    );
    await waitFor(() =>
      expect(
        screen.queryByTestId("platform-bug-report-screenshot-preview"),
      ).toBeNull(),
    );

    fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
      target: { value: "no screenshot bug" },
    });
    fireEvent.click(screen.getByTestId("platform-bug-report-submit"));
    await waitFor(() => expect(reportMutateAsync).toHaveBeenCalledTimes(1));
    expect(reportMutateAsync.mock.calls[0][0].screenshot).toBeNull();
  });

  it("does not submit without a title", async () => {
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");
    // Let capture settle so the disabled state reflects the missing title,
    // not the in-flight capture.
    await screen.findByTestId("platform-bug-report-screenshot-preview");
    // Submit is disabled until a title is entered; clicking is a no-op.
    expect(screen.getByTestId("platform-bug-report-submit")).toBeDisabled();
    fireEvent.click(screen.getByTestId("platform-bug-report-submit"));
    expect(reportMutateAsync).not.toHaveBeenCalled();
  });

  it("opens the drawer when screenshot capture rejects", async () => {
    captureMock.mockRejectedValueOnce(new Error("rasterizer failed"));
    render(<PlatformIssueReportButton />);

    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");
    // Capture rejects: the slot settles on "no screenshot attached".
    await screen.findByTestId("platform-bug-report-no-screenshot");
    expect(
      screen.queryByTestId("platform-bug-report-screenshot-preview"),
    ).toBeNull();
  });

  it("drops a client screenshot larger than the API cap", async () => {
    captureMock.mockResolvedValueOnce(
      new Blob([new Uint8Array(5 * 1024 * 1024 + 1)], { type: "image/png" }),
    );
    render(<PlatformIssueReportButton />);

    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");
    // Over-cap blob is discarded: the slot settles on "no screenshot attached".
    await screen.findByTestId("platform-bug-report-no-screenshot");
    expect(
      screen.queryByTestId("platform-bug-report-screenshot-preview"),
    ).toBeNull();
  });

  it("submits path-only context with route slugs", async () => {
    window.history.replaceState(
      {},
      "",
      "/acme/demo/issues?token=secret#fragment",
    );
    render(<PlatformIssueReportButton isWorkspaceRoute />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");
    // Submit is gated until the background capture settles.
    await screen.findByTestId("platform-bug-report-screenshot-preview");
    fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
      target: { value: "Route leaks query data" },
    });
    fireEvent.click(screen.getByTestId("platform-bug-report-submit"));

    await waitFor(() => expect(reportMutateAsync).toHaveBeenCalledTimes(1));
    expect(reportMutateAsync.mock.calls[0][0].context).toMatchObject({
      route: "/acme/demo/issues",
      orgSlug: "acme",
      workspaceSlug: "demo",
    });
  });

  it("omits workspace context on non-workspace routes", async () => {
    window.history.replaceState({}, "", "/login/denied");
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");
    // Submit is gated until the background capture settles.
    await screen.findByTestId("platform-bug-report-screenshot-preview");
    fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
      target: { value: "Login redirect issue" },
    });
    fireEvent.click(screen.getByTestId("platform-bug-report-submit"));

    await waitFor(() => expect(reportMutateAsync).toHaveBeenCalledTimes(1));
    const { context } = reportMutateAsync.mock.calls[0][0];
    expect(context.route).toBe("/login/denied");
    expect(context.orgSlug).toBeUndefined();
    expect(context.workspaceSlug).toBeUndefined();
  });

  it.each(["/landing/extra/fallback", "/unknown/extra/fallback"])(
    "omits workspace context for unknown fallback route %s",
    async (route) => {
      window.history.replaceState({}, "", route);
      render(<PlatformIssueReportButton />);
      fireEvent.click(screen.getByTestId("platform-bug-report-button"));
      await screen.findByTestId("platform-bug-report-drawer");
      // Submit is gated until the background capture settles.
      await screen.findByTestId("platform-bug-report-screenshot-preview");
      fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
        target: { value: "Unknown route issue" },
      });
      fireEvent.click(screen.getByTestId("platform-bug-report-submit"));

      await waitFor(() => expect(reportMutateAsync).toHaveBeenCalledTimes(1));
      const { context } = reportMutateAsync.mock.calls[0][0];
      expect(context.route).toBe(route);
      expect(context.orgSlug).toBeUndefined();
      expect(context.workspaceSlug).toBeUndefined();
    },
  );

  it("omits workspace context on the CLI authorization route", async () => {
    window.history.replaceState({}, "", "/cli/authorize?userCode=5F2B-D227");
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");
    // Submit is gated until the background capture settles.
    await screen.findByTestId("platform-bug-report-screenshot-preview");
    fireEvent.change(screen.getByTestId("platform-bug-report-title"), {
      target: { value: "CLI authorization issue" },
    });
    fireEvent.click(screen.getByTestId("platform-bug-report-submit"));

    await waitFor(() => expect(reportMutateAsync).toHaveBeenCalledTimes(1));
    const { context } = reportMutateAsync.mock.calls[0][0];
    expect(context.route).toBe("/cli/authorize");
    expect(context.orgSlug).toBeUndefined();
    expect(context.workspaceSlug).toBeUndefined();
  });

  it("caps the report body at the server's 20,000-character limit", async () => {
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");

    expect(screen.getByTestId("platform-bug-report-body")).toHaveAttribute(
      "maxlength",
      "20000",
    );
  });

  it("traps focus in the drawer and restores its invoking trigger on close", async () => {
    render(<PlatformIssueReportButton />);
    const trigger = screen.getByTestId("platform-bug-report-button");
    trigger.focus();
    fireEvent.click(trigger);
    const drawer = await screen.findByTestId("platform-bug-report-drawer");

    const controls = Array.from(
      drawer.querySelectorAll<HTMLElement>("button, input, textarea, select"),
    ).filter((control) => !control.hasAttribute("disabled"));
    // Retry the whole focus→Tab→wrap probe as a unit: the drawer's DOM can be
    // queryable a beat before useDialogFocus's effect installs the document
    // keydown listener (seen under loaded CI full-suite runs), and a Tab
    // fired into the not-yet-armed trap moves nothing.
    await waitFor(() => {
      controls[controls.length - 1].focus();
      fireEvent.keyDown(controls[controls.length - 1], { key: "Tab" });
      expect(document.activeElement).toBe(controls[0]);
    });

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByTestId("platform-bug-report-drawer")).toBeNull(),
    );
    expect(document.activeElement).toBe(trigger);
  });

  it("closes on a document-level Escape, with no focus inside the drawer", async () => {
    render(<PlatformIssueReportButton />);
    fireEvent.click(screen.getByTestId("platform-bug-report-button"));
    await screen.findByTestId("platform-bug-report-drawer");
    // findBy resolves on the DOM commit; under a CPU-starved full-suite run
    // the passive effect that installs the shared dialog keydown listener
    // (useDialogFocus) may not have flushed yet, so an immediately dispatched
    // Escape falls into the void and the drawer stays mounted (issue #1805).
    // An empty async act flushes all pending effects deterministically.
    await act(async () => {});
    (document.activeElement as HTMLElement | null)?.blur();

    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    // waitFor instead of an immediate query: the close is a React state
    // update whose unmount may flush asynchronously under load.
    await waitFor(() => {
      expect(screen.queryByTestId("platform-bug-report-drawer")).toBeNull();
    });
  });
});

// platform_reports g5.quality.platform-reports.trigger: the floating trigger
// renders only while no chrome trigger (or placeholder for one) is mounted or
// an overlay at the modal tier covers the chrome.
describe("PlatformIssueReportProvider floating trigger", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.history.replaceState({}, "", "/");
    useInstanceInfoMock.mockReturnValue(
      instanceInfo(["platform_issue_reporting"]),
    );
    reportMutateAsync.mockResolvedValue({ submissionId: "sub-123" });
    useReportMock.mockReturnValue(reportHook());
    captureMock.mockResolvedValue(
      new Blob(["png-bytes"], { type: "image/png" }),
    );
  });

  function Shell({ chrome, modal }: { chrome: boolean; modal: boolean }) {
    return (
      <PlatformIssueReportProvider showShellTrigger>
        {chrome ? <PlatformIssueReportTrigger placement="topbar" /> : null}
        {modal ? (
          <Modal onClose={() => {}} ariaLabel="A modal" testId="probe-modal">
            <button type="button">inside</button>
          </Modal>
        ) : null}
      </PlatformIssueReportProvider>
    );
  }

  it("renders the floating trigger on a page without a chrome trigger", () => {
    render(<Shell chrome={false} modal={false} />);
    expect(screen.getByTestId("platform-bug-report-button")).toBeTruthy();
  });

  it("hides the floating trigger while a chrome trigger is mounted, and shows it while a modal covers the chrome", () => {
    const view = render(<Shell chrome modal={false} />);
    expect(
      screen.getByTestId("platform-bug-report-topbar-button"),
    ).toBeTruthy();
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();

    view.rerender(<Shell chrome modal />);
    expect(screen.getByTestId("platform-bug-report-button")).toBeTruthy();

    view.rerender(<Shell chrome modal={false} />);
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
  });

  it("brings the floating trigger back when the chrome trigger unmounts", () => {
    const view = render(<Shell chrome modal={false} />);
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
    view.rerender(<Shell chrome={false} modal={false} />);
    expect(screen.getByTestId("platform-bug-report-button")).toBeTruthy();
  });

  function LoadingShell({ stage }: { stage: "loading" | "chrome" | "error" }) {
    return (
      <PlatformIssueReportProvider showShellTrigger>
        {stage === "loading" ? <PlatformIssueReportChromePending /> : null}
        {stage === "chrome" ? (
          <PlatformIssueReportTrigger placement="topbar" />
        ) : null}
      </PlatformIssueReportProvider>
    );
  }

  it("keeps the floating trigger hidden from a chrome placeholder through to the chrome", () => {
    const view = render(<LoadingShell stage="loading" />);
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
    view.rerender(<LoadingShell stage="chrome" />);
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
    expect(
      screen.getByTestId("platform-bug-report-topbar-button"),
    ).toBeTruthy();
  });

  it("shows the floating trigger when a chrome placeholder gives way to a page without chrome", () => {
    const view = render(<LoadingShell stage="loading" />);
    expect(screen.queryByTestId("platform-bug-report-button")).toBeNull();
    view.rerender(<LoadingShell stage="error" />);
    expect(screen.getByTestId("platform-bug-report-button")).toBeTruthy();
  });

  it("opens the drawer over a modal from the floating trigger and keeps it mounted for focus restore", async () => {
    render(<Shell chrome modal />);
    const trigger = screen.getByTestId("platform-bug-report-button");
    trigger.focus();
    fireEvent.click(trigger);
    const drawer = await screen.findByTestId("platform-bug-report-drawer");
    expect(drawer).toBeTruthy();
    // The modal is still mounted beneath the drawer, so the trigger stays.
    expect(screen.getByTestId("platform-bug-report-button")).toBeTruthy();

    await act(async () => {});
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    await waitFor(() =>
      expect(screen.queryByTestId("platform-bug-report-drawer")).toBeNull(),
    );
    expect(document.activeElement).toBe(
      screen.getByTestId("platform-bug-report-button"),
    );
    // The modal beneath is untouched by the drawer's Escape.
    expect(screen.getByTestId("probe-modal-overlay")).toBeTruthy();
  });
});
