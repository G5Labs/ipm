// =============================================================================
// Global "Report a bug about G5" affordance.
// =============================================================================
// One shell-owned controller keeps the drawer, global shortcut, screenshot,
// and submission state outside page error boundaries.  Page chrome renders a
// trigger-only consumer so there can never be two drawers or shortcuts. The
// floating bottom-left trigger appears only while that chrome is absent or
// covered by an overlay; a loading screen that precedes the chrome holds its
// place (ui g5.quality.platform-reports.trigger).

import {
  type ReactNode,
  type RefObject,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { Bug, Camera, X } from "lucide-react";

import { useInstanceInfo } from "../../hooks/useSettings";
import { usePlatformIssueReport } from "../../hooks/usePlatformIssues";
import type { PlatformIssueContext } from "../../lib/types";
import { CornerButton } from "../ui/CornerButton";
import { Button, IconButton, useDialogFocus } from "../ui/UI";
import {
  REPORT_DRAWER_PRIORITY,
  useCoveringOverlayOpen,
} from "../ui/useDialogFocus";
import { captureScreenshot } from "./captureScreenshot";
import {
  type PlatformIssueReportController,
  PlatformIssueReportContext,
} from "./PlatformIssueReportContext";

const SEVERITIES = ["low", "medium", "high", "critical"] as const;
const REPORT_LABEL = "Report a bug about G5";
const REPORT_TITLE = "Report a bug about G5 (Ctrl/Cmd+Shift+B)";
const MAX_SCREENSHOT_BYTES = 5 * 1024 * 1024;
type Severity = (typeof SEVERITIES)[number];
type Placement = "shell" | "topbar";

/**
 * `modern-screenshot` node filter: drop any element flagged `data-screenshot-exclude`
 * (and, implicitly, its whole subtree) so the report drawer, its backdrop, and the
 * floating/top-bar trigger never appear in a capture taken while the report UI is
 * already in the DOM.
 */
function excludeReportUiFromCapture(node: Node): boolean {
  return !(
    node instanceof Element && node.hasAttribute("data-screenshot-exclude")
  );
}

/** Collect cheap, privacy-safe context the client already has. */
function buildContext(
  appVersion: string | undefined,
  isWorkspaceRoute: boolean,
): PlatformIssueContext {
  const path = window.location.pathname;
  const segments = path.split("/").filter(Boolean);
  const [orgSlug, workspaceSlug] = segments;
  return {
    route: path,
    appVersion,
    userAgent: navigator.userAgent,
    viewport: `${window.innerWidth}x${window.innerHeight}`,
    // These are route slugs, not stable database identifiers.
    orgSlug: isWorkspaceRoute ? orgSlug : undefined,
    workspaceSlug: isWorkspaceRoute ? workspaceSlug : undefined,
  };
}

/**
 * Mounts with the drawer so the shared dialog lifecycle — Escape, Tab trap,
 * initial focus (design-system D15) — is installed only while it is open. The
 * drawer is a side panel, not a centered dialog, so it uses the hook rather
 * than the Modal shell.
 */
function DrawerFocusScope({
  panelRef,
  onClose,
  children,
}: {
  panelRef: RefObject<HTMLElement | null>;
  onClose: () => void;
  children: ReactNode;
}) {
  // The drawer's keyboard tier sits above the modal tiers (50/60): a modal
  // that mounts while the drawer is open must not become keyboard top-most.
  // Its CSS layer is higher still (z-[1000]) so it also covers floating
  // panels (the right-edge Chat starts above z-200).
  useDialogFocus(panelRef, { onClose, priority: REPORT_DRAWER_PRIORITY });
  return <>{children}</>;
}

export interface PlatformIssueReportProviderProps {
  children?: ReactNode;
  /**
   * Offer the floating bottom-left fallback. It renders only while no chrome
   * trigger or chrome placeholder is mounted (settings, account, CLI pages) or
   * an overlay at the modal tier covers the chrome.
   */
  showShellTrigger?: boolean;
  /** App.tsx's explicit route predicate; governs privacy-safe context slugs. */
  isWorkspaceRoute?: boolean;
}

/**
 * Shell-owned platform-report controller. Mount this once around route content;
 * it portals the drawer outside page boundaries and exposes trigger-only access
 * through `PlatformIssueReportTrigger`.
 */
export function PlatformIssueReportProvider({
  children,
  showShellTrigger = true,
  isWorkspaceRoute = false,
}: PlatformIssueReportProviderProps) {
  const instanceInfo = useInstanceInfo();
  const report = usePlatformIssueReport();
  const [open, setOpen] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [screenshot, setScreenshot] = useState<Blob | null>(null);
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [severity, setSeverity] = useState<Severity>("medium");
  const [submittedId, setSubmittedId] = useState<string | null>(null);
  const [chromeTriggers, setChromeTriggers] = useState(0);
  const coveringOverlayOpen = useCoveringOverlayOpen();
  const drawerRef = useRef<HTMLDivElement>(null);
  const restoreFocusRef = useRef<HTMLElement | null>(null);
  // Bumped on every open and on every close so a background capture that
  // settles after its drawer was closed or reopened is recognized as stale.
  const captureTokenRef = useRef(0);

  const enabled =
    instanceInfo.status === "ok" &&
    instanceInfo.data.features.includes("platform_issue_reporting");
  const appVersion =
    instanceInfo.status === "ok" ? instanceInfo.data.version : undefined;

  const clearScreenshot = useCallback(() => {
    setScreenshotUrl((previous) => {
      if (previous) URL.revokeObjectURL(previous);
      return null;
    });
    setScreenshot(null);
  }, []);

  const reset = useCallback(() => {
    // Invalidate any in-flight capture so a late result cannot repopulate the
    // preview after the drawer has closed.
    captureTokenRef.current += 1;
    setOpen(false);
    setCapturing(false);
    setTitle("");
    setBody("");
    setSeverity("medium");
    setSubmittedId(null);
    clearScreenshot();
    report.reset();
  }, [clearScreenshot, report]);

  // User-initiated dismissal (Escape, backdrop, X, Cancel) is suspended
  // while a report submission is in flight, mirroring the Modal shell's
  // closeDisabled semantics; `reset` itself stays callable for the
  // post-submit path.
  const requestClose = useCallback(() => {
    if (!report.isLoading) reset();
  }, [report.isLoading, reset]);

  // Open the drawer immediately, then capture in the background over the
  // underlying app state; the report UI is filtered out of the image, and
  // Submit stays disabled until capture settles so preview-and-remove is always
  // possible. A per-invocation token drops results whose drawer was closed or
  // reopened before they resolved.
  const openReporter = useCallback(async () => {
    if (open || !enabled) return;
    const active = document.activeElement;
    restoreFocusRef.current = active instanceof HTMLElement ? active : null;
    const token = (captureTokenRef.current += 1);
    setOpen(true);
    setCapturing(true);
    try {
      // One frame so the drawer commits before rasterization runs.
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => resolve());
      });
      // Closing during the frame yield invalidates this invocation before any
      // rasterizer work starts; do not capture a stale, now-hidden drawer.
      if (token !== captureTokenRef.current) return;
      const captured = await captureScreenshot({
        filter: excludeReportUiFromCapture,
      });
      if (token !== captureTokenRef.current) return; // stale: drawer closed/reopened
      // Require a non-empty blob within the size cap: a zero-byte capture is a
      // failed rasterization, not a usable screenshot, and must not attach.
      const accepted =
        captured && captured.size > 0 && captured.size <= MAX_SCREENSHOT_BYTES
          ? captured
          : null;
      setScreenshot(accepted);
      setScreenshotUrl((previous) => {
        if (previous) URL.revokeObjectURL(previous);
        return accepted ? URL.createObjectURL(accepted) : null;
      });
    } catch (error) {
      if (token !== captureTokenRef.current) return;
      console.warn("Platform bug report: screenshot capture failed", error);
    } finally {
      if (token === captureTokenRef.current) setCapturing(false);
    }
  }, [enabled, open]);

  useEffect(() => {
    if (!enabled) return undefined;
    const handler = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (modifier && event.shiftKey && event.key.toLowerCase() === "b") {
        event.preventDefault();
        void openReporter();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [enabled, openReporter]);

  // Focus returns to the affordance captured at open time. `openReporter`
  // records the invoker before the drawer mounts, so once the drawer closes
  // focus lands back on the trigger regardless of the shared lifecycle's own
  // invoker-restore.
  useEffect(() => {
    if (open) return;
    const restore = restoreFocusRef.current;
    restoreFocusRef.current = null;
    if (restore?.isConnected) restore.focus();
  }, [open]);

  useEffect(
    () => () => {
      if (screenshotUrl) URL.revokeObjectURL(screenshotUrl);
    },
    [screenshotUrl],
  );

  // Submission waits for capture to settle so the mandatory preview-and-remove
  // privacy decision is always available before the report is sent.
  const canSubmit = useMemo(
    () => title.trim().length > 0 && !capturing && !report.isLoading,
    [title, capturing, report.isLoading],
  );

  const onSubmit = useCallback(async () => {
    if (!canSubmit) return;
    try {
      const result = await report.mutateAsync({
        title: title.trim(),
        body,
        severity,
        context: buildContext(appVersion, isWorkspaceRoute),
        screenshot,
      });
      setSubmittedId(result.submissionId);
    } catch {
      // The hook's typed error remains visible in the drawer.
    }
  }, [
    appVersion,
    body,
    canSubmit,
    isWorkspaceRoute,
    report,
    screenshot,
    severity,
    title,
  ]);

  const registerChromeTrigger = useCallback(() => {
    setChromeTriggers((count) => count + 1);
    return () => setChromeTriggers((count) => count - 1);
  }, []);

  const controller = useMemo<PlatformIssueReportController>(
    () => ({ enabled, openReporter, registerChromeTrigger }),
    [enabled, openReporter, registerChromeTrigger],
  );

  const floatingTriggerVisible =
    showShellTrigger && (chromeTriggers === 0 || coveringOverlayOpen);

  const drawer =
    open && enabled
      ? createPortal(
          <DrawerFocusScope panelRef={drawerRef} onClose={requestClose}>
            <div
              className="fixed inset-0 z-[1000] flex justify-end"
              role="dialog"
              aria-modal="true"
              aria-label="Report a bug about G5"
              data-testid="platform-bug-report-drawer"
              data-screenshot-exclude=""
            >
              <div
                className="absolute inset-0"
                style={{ background: "rgba(0,0,0,0.4)" }}
                onClick={requestClose}
              />
              <div
                ref={drawerRef}
                className="relative flex h-full w-full max-w-md flex-col overflow-y-auto border-l shadow-xl"
                style={{
                  background: "var(--g5-bg0)",
                  borderColor: "var(--g5-stroke)",
                }}
              >
                <div
                  className="flex items-center justify-between border-b px-4 py-3"
                  style={{ borderColor: "var(--g5-stroke)" }}
                >
                  <div
                    className="flex items-center gap-2 text-sm font-semibold"
                    style={{ color: "var(--g5-text0)" }}
                  >
                    <Bug className="h-4 w-4" />
                    Report a bug about G5
                  </div>
                  <IconButton title="Close" onClick={requestClose}>
                    <X className="h-4 w-4" />
                  </IconButton>
                </div>

                {submittedId ? (
                  <div className="flex flex-1 flex-col gap-4 p-4">
                    <p className="text-sm" style={{ color: "var(--g5-text0)" }}>
                      Thanks — your report was received.
                    </p>
                    <p className="text-xs" style={{ color: "var(--g5-text2)" }}>
                      Submission id:{" "}
                      <span data-testid="platform-bug-report-submission-id">
                        {submittedId}
                      </span>
                    </p>
                    <div>
                      <Button kind="secondary" onClick={requestClose}>
                        Done
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="flex flex-1 flex-col gap-4 p-4">
                    <label className="flex flex-col gap-1">
                      <span
                        className="text-xs font-medium"
                        style={{ color: "var(--g5-text1)" }}
                      >
                        Title
                      </span>
                      <input
                        data-testid="platform-bug-report-title"
                        value={title}
                        maxLength={500}
                        onChange={(event) => setTitle(event.target.value)}
                        placeholder="Short summary of the bug"
                        className="rounded-[var(--g5-radius-sm)] border px-2 py-1.5 text-sm"
                        style={{
                          background: "var(--g5-bg1)",
                          borderColor: "var(--g5-stroke)",
                          color: "var(--g5-text0)",
                        }}
                      />
                    </label>

                    <label className="flex flex-col gap-1">
                      <span
                        className="text-xs font-medium"
                        style={{ color: "var(--g5-text1)" }}
                      >
                        What happened?
                      </span>
                      <textarea
                        data-testid="platform-bug-report-body"
                        value={body}
                        rows={5}
                        maxLength={20_000}
                        onChange={(event) => setBody(event.target.value)}
                        placeholder="Steps to reproduce, what you expected, what happened"
                        className="rounded-[var(--g5-radius-sm)] border px-2 py-1.5 text-sm"
                        style={{
                          background: "var(--g5-bg1)",
                          borderColor: "var(--g5-stroke)",
                          color: "var(--g5-text0)",
                        }}
                      />
                    </label>

                    <label className="flex flex-col gap-1">
                      <span
                        className="text-xs font-medium"
                        style={{ color: "var(--g5-text1)" }}
                      >
                        Severity
                      </span>
                      <select
                        data-testid="platform-bug-report-severity"
                        value={severity}
                        onChange={(event) =>
                          setSeverity(event.target.value as Severity)
                        }
                        className="rounded-[var(--g5-radius-sm)] border px-2 py-1.5 text-sm"
                        style={{
                          background: "var(--g5-bg1)",
                          borderColor: "var(--g5-stroke)",
                          color: "var(--g5-text0)",
                        }}
                      >
                        {SEVERITIES.map((value) => (
                          <option key={value} value={value}>
                            {value}
                          </option>
                        ))}
                      </select>
                    </label>

                    <div className="flex flex-col gap-1">
                      <span
                        className="text-xs font-medium"
                        style={{ color: "var(--g5-text1)" }}
                      >
                        Screenshot
                      </span>
                      {capturing ? (
                        <span
                          data-testid="platform-bug-report-screenshot-capturing"
                          className="inline-flex items-center gap-1 text-xs"
                          style={{ color: "var(--g5-text2)" }}
                        >
                          <Camera className="h-3.5 w-3.5" />
                          Capturing screenshot…
                        </span>
                      ) : screenshotUrl ? (
                        <div className="flex flex-col gap-2">
                          <img
                            data-testid="platform-bug-report-screenshot-preview"
                            src={screenshotUrl}
                            alt="Captured screenshot preview"
                            className="max-h-48 w-full rounded border object-contain"
                            style={{ borderColor: "var(--g5-stroke)" }}
                          />
                          <button
                            type="button"
                            data-testid="platform-bug-report-remove-screenshot"
                            onClick={clearScreenshot}
                            className="self-start text-xs underline"
                            style={{ color: "var(--g5-text2)" }}
                          >
                            Remove screenshot
                          </button>
                        </div>
                      ) : (
                        <span
                          data-testid="platform-bug-report-no-screenshot"
                          className="inline-flex items-center gap-1 text-xs"
                          style={{ color: "var(--g5-text2)" }}
                        >
                          <Camera className="h-3.5 w-3.5" />
                          No screenshot attached
                        </span>
                      )}
                    </div>

                    <p className="text-xs" style={{ color: "var(--g5-text2)" }}>
                      Environment details (page path, app version, browser,
                      viewport) are attached automatically.
                    </p>

                    {report.error ? (
                      <p
                        className="text-xs"
                        role="alert"
                        style={{ color: "var(--g5-blocked)" }}
                      >
                        Could not submit the report. Please try again.
                      </p>
                    ) : null}

                    <div className="mt-auto flex items-center justify-end gap-2 pt-2">
                      <Button kind="ghost" onClick={reset}>
                        Cancel
                      </Button>
                      <Button
                        kind="primary"
                        disabled={!canSubmit}
                        data-testid="platform-bug-report-submit"
                        onClick={() => void onSubmit()}
                      >
                        {report.isLoading ? "Submitting…" : "Submit"}
                      </Button>
                    </div>
                  </div>
                )}
              </div>
            </div>
          </DrawerFocusScope>,
          document.body,
        )
      : null;

  return (
    <PlatformIssueReportContext.Provider value={controller}>
      {children}
      {floatingTriggerVisible ? (
        <PlatformIssueReportTrigger placement="shell" />
      ) : null}
      {drawer}
    </PlatformIssueReportContext.Provider>
  );
}

export interface PlatformIssueReportTriggerProps {
  placement?: Placement;
}

/** Stateless trigger for the single shell-owned report controller. */
export function PlatformIssueReportTrigger({
  placement = "shell",
}: PlatformIssueReportTriggerProps) {
  const controller = useContext(PlatformIssueReportContext);
  const enabled = controller?.enabled === true;
  const registerChromeTrigger = controller?.registerChromeTrigger;

  // A mounted chrome trigger tells the provider its floating fallback is
  // redundant until an overlay covers the chrome. Registered before paint so
  // the fallback never shows for a frame next to the chrome.
  useLayoutEffect(() => {
    if (placement !== "topbar" || !enabled || !registerChromeTrigger) return;
    return registerChromeTrigger();
  }, [enabled, placement, registerChromeTrigger]);

  if (!controller?.enabled) return null;

  if (placement === "shell") {
    return (
      <CornerButton
        corner="left"
        label={REPORT_LABEL}
        title={REPORT_TITLE}
        onClick={() => void controller.openReporter()}
        testId="platform-bug-report-button"
      >
        <Bug className="h-5 w-5" />
      </CornerButton>
    );
  }

  return (
    <button
      type="button"
      data-testid="platform-bug-report-topbar-button"
      // Kept out of the background capture by node filter, not by hiding, so the
      // drawer can open immediately without the trigger flickering.
      data-screenshot-exclude=""
      title={REPORT_TITLE}
      aria-label={REPORT_LABEL}
      onClick={() => void controller.openReporter()}
      className="inline-flex h-8 w-8 items-center justify-center rounded-[var(--g5-radius-sm)] text-[var(--g5-text1)] transition-colors hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
      style={{ color: "var(--g5-text1)" }}
    >
      <Bug className="h-4 w-4" />
    </button>
  );
}

/** Compatibility host for isolated tests and non-shell consumers. */
export interface PlatformIssueReportButtonProps {
  placement?: Placement;
  showTrigger?: boolean;
  isWorkspaceRoute?: boolean;
}

export function PlatformIssueReportButton({
  placement = "shell",
  showTrigger = true,
  isWorkspaceRoute = false,
}: PlatformIssueReportButtonProps) {
  return (
    <PlatformIssueReportProvider
      showShellTrigger={false}
      isWorkspaceRoute={isWorkspaceRoute}
    >
      {showTrigger ? (
        <PlatformIssueReportTrigger placement={placement} />
      ) : null}
    </PlatformIssueReportProvider>
  );
}
