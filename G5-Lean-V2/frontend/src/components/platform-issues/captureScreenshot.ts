// =============================================================================
// DOM-rasterization screenshot capture for platform bug reports.
// =============================================================================
// Captures the app's own DOM to a PNG Blob entirely in-page (no permission
// prompt). Portaled modals/overlays are captured because they live in the DOM.
// This is the "good, not pixel-perfect" default path (design D-3 / §4.3); a
// getDisplayMedia upgrade is a later follow-up.

const CAPTURE_TIMEOUT_MS = 8_000;

export interface CaptureScreenshotOptions {
  /**
   * Node filter forwarded to `modern-screenshot`'s `filter` option. Returning
   * `false` for a node omits that node and its whole subtree from the captured
   * image. The report flow uses this to keep the drawer, backdrop, and floating
   * trigger out of a capture taken while the report UI is already in the DOM.
   */
  filter?: (node: Node) => boolean;
}

/**
 * Capture a PNG screenshot of the current document via DOM rasterization.
 *
 * Returns a PNG `Blob`, or `null` when capture fails — the report flow then
 * continues without a screenshot rather than blocking the user. An optional
 * `filter` drops flagged report-UI subtrees so they never appear in the image.
 */
export async function captureScreenshot(
  options: CaptureScreenshotOptions = {},
): Promise<Blob | null> {
  try {
    const { domToBlob } = await import("modern-screenshot");
    // Cap the device pixel ratio so the captured image stays within the
    // server-side size cap on large/hi-dpi displays.
    const scale = Math.min(window.devicePixelRatio || 1, 2);
    const backgroundColor =
      getComputedStyle(document.body).backgroundColor || "#ffffff";
    return await new Promise<Blob | null>((resolve, reject) => {
      const timer = window.setTimeout(() => resolve(null), CAPTURE_TIMEOUT_MS);
      domToBlob(document.body, {
        type: "image/png",
        scale,
        backgroundColor,
        filter: options.filter,
      }).then(
        (blob) => {
          window.clearTimeout(timer);
          resolve(blob);
        },
        (error) => {
          window.clearTimeout(timer);
          reject(error);
        },
      );
    });
  } catch (err) {
    // Non-fatal: fall back to a text-only report.
    console.warn("Platform bug report: screenshot capture failed", err);
    return null;
  }
}
