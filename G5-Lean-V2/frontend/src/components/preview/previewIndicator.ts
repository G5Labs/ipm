/** The preview view fields the trigger indicator reads. */
export interface PreviewRunView {
  status: string;
  ready: boolean;
  closing: boolean;
  /**
   * How a cancelled preview closed: "clean", "failed", "killed", or "lost"
   * (the VM went away before cleanup finished).
   */
  close: string | null;
  port: number;
  /** The selected profile, or null for the workspace default. */
  profile?: string | null;
}

const TERMINAL = new Set(["succeeded", "failed", "cancelled"]);

/**
 * What the trigger's glyph shows about the workspace's preview without the
 * dialog: nothing, a preview coming up, one serving, one closing, or a last
 * run that failed (an app exit, or a close whose cleanup failed) until the
 * next start replaces it.
 */
export type PreviewIndicatorState =
  | "none"
  | "starting"
  | "serving"
  | "closing"
  | "failed";

export function previewIndicatorState(
  view: PreviewRunView | null | undefined,
): PreviewIndicatorState {
  if (!view) return "none";
  if (!TERMINAL.has(view.status)) {
    if (view.closing) return "closing";
    return view.ready ? "serving" : "starting";
  }
  if (view.status === "failed") return "failed";
  if (view.status === "cancelled" && view.close && view.close !== "clean") {
    return "failed";
  }
  return "none";
}

/** The state in words, for the trigger's tooltip and accessible name. */
export function previewIndicatorText(
  state: PreviewIndicatorState,
  view: PreviewRunView | null | undefined,
): string | null {
  switch (state) {
    case "starting":
      return "starting…";
    case "serving":
      return `serving on port ${view?.port ?? "?"}`;
    case "closing":
      return "closing…";
    case "failed":
      return view?.status === "cancelled" && view.close === "lost"
        ? "the last close was cut short by a lost VM; open for details"
        : "the last run failed; open for its log";
    default:
      return null;
  }
}
