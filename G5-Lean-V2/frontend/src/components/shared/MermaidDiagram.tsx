// =============================================================================
// G5 Shared Components — MermaidDiagram
// =============================================================================
// A theme-aware mermaid diagram renderer. Renders mermaid diagram code as SVG.
// Integrates with the g5 theme system by reading CSS custom properties and
// configuring the mermaid theme accordingly.
//
// Re-renders when theme changes (listens for data-theme and data-mode
// attribute changes on <html>).
// =============================================================================

import React, { useEffect, useRef, useState, useCallback } from "react";
import { classNames } from "../ui/UI";
import {
  isChunkLoadError,
  requestPageReload,
} from "../../lib/chunk-reload-guard";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface MermaidDiagramProps {
  code: string;
  className?: string;
}

// ---------------------------------------------------------------------------
// Zoom (shared-components D8b, issue #1184)
// ---------------------------------------------------------------------------

const ZOOM_STEP = 1.25;
const ZOOM_MIN = 0.5;
const ZOOM_MAX = 6;

// Mermaid emits `useMaxWidth`-form SVGs: `width: 100%` plus a pixel
// `max-width` cap. A percentage width re-fits the panel under CSS `zoom`, so
// the diagram would stay capped to the box at every zoom level (D8b, chat
// 8cb31169). The natural pixel width comes from that max-width style (or the
// viewBox as fallback) so the SVG can be pinned to it.
const PX_VALUE_RE = /^\d+(\.\d+)?px$/;

function readNaturalWidth(svg: SVGElement): number | null {
  if (PX_VALUE_RE.test(svg.style.maxWidth)) {
    const px = parseFloat(svg.style.maxWidth);
    if (px > 0) return px;
  }
  const viewBox = svg
    .getAttribute("viewBox")
    ?.trim()
    .split(/[\s,]+/);
  if (viewBox?.length === 4) {
    const px = parseFloat(viewBox[2]);
    if (Number.isFinite(px) && px > 0) return px;
  }
  return null;
}

const ZOOM_BUTTON_CLASS = classNames(
  "px-1.5 py-1 rounded-[var(--g5-radius-sm)]",
  "text-xs font-medium leading-none",
  "bg-[var(--g5-bg2)] text-[var(--g5-text2)]",
  "border border-[var(--g5-stroke)]",
  "hover:bg-[var(--g5-bg1)] hover:text-[var(--g5-text0)]",
  "disabled:opacity-40 disabled:pointer-events-none",
);

// ---------------------------------------------------------------------------
// Helper: Read CSS custom property value
// ---------------------------------------------------------------------------

function getCSSVar(name: string): string {
  return getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
}

// ---------------------------------------------------------------------------
// Helper: Get current theme mode
// ---------------------------------------------------------------------------

function getThemeMode(): "dark" | "light" {
  return (document.documentElement.dataset.mode as "dark" | "light") || "dark";
}

// ---------------------------------------------------------------------------
// MermaidDiagram Component
// ---------------------------------------------------------------------------

/**
 * Theme-aware mermaid diagram renderer.
 *
 * Behavior:
 * - Accepts mermaid code string and renders it as an inline SVG.
 * - Integrates with the g5 theme system: reads CSS custom properties and
 *   configures the mermaid theme accordingly.
 * - Supports light and dark modes automatically via `data-mode` attribute.
 * - Error handling: If mermaid parsing fails, displays the raw code in a
 *   code block with an error message.
 * - Loading state: Shows a placeholder while mermaid renders asynchronously.
 * - Re-renders when theme changes (listens for data-theme and data-mode
 *   attribute changes on <html>).
 * - Zoom controls: ×1.25 step in/out clamped to 50%–600%, plus a percentage
 *   readout that resets to 100%; shown only for a rendered SVG (D8b, #1184).
 *   100% fits the panel; higher steps outgrow it into a scrollable partial
 *   view via natural-width pinning + fitted-baseline zoom (D8b).
 * - Drag panning: mouse/pen drag scrolls the diagram panel; touch keeps
 *   native scrolling (D8c).
 */
export const MermaidDiagram: React.FC<MermaidDiagramProps> = ({
  code,
  className,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [svgContent, setSvgContent] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [errorKind, setErrorKind] = useState<"parse" | "chunk-load">("parse");
  const [isLoading, setIsLoading] = useState(true);
  // Zoom survives re-renders (theme changes, code edits) so a reader's chosen
  // magnification is not lost to a debounce re-render (D8b).
  const [zoom, setZoom] = useState(1);
  const renderIdRef = useRef(0);
  // Natural-size scaling (D8b, chat 8cb31169): the injected SVG is pinned to
  // its natural pixel width and the applied CSS zoom is userZoom × fittedScale
  // so 100% still fits the panel while higher steps outgrow it.
  const scrollRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const [naturalWidth, setNaturalWidth] = useState<number | null>(null);
  const [panelWidth, setPanelWidth] = useState<number | null>(null);
  // Drag panning (D8c): mouse/pen drags scroll the panel; touch keeps native
  // overflow scrolling.
  const panStateRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    scrollLeft: number;
    scrollTop: number;
  } | null>(null);
  const [isPanning, setIsPanning] = useState(false);

  // Parse-error containment (D8d, issue #2325): mermaid.render appends a
  // temporary `div#d<id>` directly under document.body. With
  // `suppressErrorRendering` (configured below) mermaid removes it on failure
  // itself; this guard defensively removes any leftover node for an id this
  // component generated, in case a future mermaid regression leaves one
  // behind. Only direct body children are touched, so a successfully injected
  // SVG inside the component container is never affected.
  const lastDiagramIdRef = useRef<string | null>(null);
  const removeMermaidTempNodes = useCallback((diagramId: string | null) => {
    if (!diagramId) return;
    for (const nodeId of [`d${diagramId}`, diagramId]) {
      const el = document.getElementById(nodeId);
      if (el && el.parentElement === document.body) {
        el.remove();
      }
    }
  }, []);

  // Remove any leftover mermaid temp node on unmount (D8d).
  useEffect(
    () => () => removeMermaidTempNodes(lastDiagramIdRef.current),
    [removeMermaidTempNodes],
  );

  // Render mermaid diagram
  const renderDiagram = useCallback(async () => {
    if (!code?.trim()) {
      setSvgContent(null);
      setError(null);
      setIsLoading(false);
      return;
    }

    const currentRenderId = ++renderIdRef.current;
    // Generated outside the try so the catch path can clean up mermaid's
    // temporary body-level node for this exact render (D8d, issue #2325).
    const diagramId = `g5-mermaid-${currentRenderId}-${Date.now()}`;
    lastDiagramIdRef.current = diagramId;
    setIsLoading(true);
    setError(null);

    try {
      // Dynamically import mermaid to avoid SSR issues and keep it async
      const mermaid = await import("mermaid");

      // Read g5 theme tokens for mermaid configuration
      const bg0 = getCSSVar("--g5-bg0") || "#0a0c10";
      const bg1 = getCSSVar("--g5-bg1") || "#0e1118";
      const text0 = getCSSVar("--g5-text0") || "#ecf2ff";
      const active = getCSSVar("--g5-active") || "#ebcb5e";
      const info = getCSSVar("--g5-info") || "#67a4ff";
      const stroke = getCSSVar("--g5-stroke") || "rgba(255,255,255,0.08)";
      const mode = getThemeMode();

      // Configure mermaid with g5 theme colors
      mermaid.default.initialize({
        startOnLoad: false,
        theme: "base",
        themeVariables: {
          darkMode: mode === "dark",
          background: bg0,
          primaryColor: bg1,
          primaryTextColor: text0,
          primaryBorderColor: stroke,
          secondaryColor: active,
          tertiaryColor: info,
          lineColor: stroke,
          textColor: text0,
          mainBkg: bg1,
          nodeBorder: stroke,
          clusterBkg: bg0,
          clusterBorder: stroke,
          titleColor: text0,
          edgeLabelBackground: bg1,
          fontFamily: getCSSVar("--g5-font-sans") || "system-ui, sans-serif",
        },
        securityLevel: "strict",
        // Parse-error containment (D8d, issue #2325): without this, a failed
        // render draws mermaid's full-width "Syntax error in text" SVG into
        // its temporary div under document.body and throws before removing
        // it, leaking the error graphic below the entire app shell. With it,
        // render() rejects with the raw parse error (shown by the inline
        // fallback) and mermaid removes its temp nodes itself.
        suppressErrorRendering: true,
      });

      const { svg } = await mermaid.default.render(diagramId, code.trim());

      // Only update if this is still the latest render
      if (currentRenderId === renderIdRef.current) {
        setSvgContent(svg);
        setError(null);
        setIsLoading(false);
      }
    } catch (err) {
      // Defensive cleanup of mermaid's temporary body-level node for this
      // render id (D8d, issue #2325) — a no-op when suppressErrorRendering
      // already removed it.
      removeMermaidTempNodes(diagramId);
      // Only update if this is still the latest render
      if (currentRenderId === renderIdRef.current) {
        const message =
          err instanceof Error ? err.message : "Failed to render diagram";
        // A missing/unreachable lazy chunk (a deploy replaced the hashed
        // assets) is not a diagram authoring error — route it to the reload
        // affordance instead of the raw-code fallback (shared-components D8a,
        // issue #1540).
        setErrorKind(isChunkLoadError(err) ? "chunk-load" : "parse");
        setError(message);
        setSvgContent(null);
        setIsLoading(false);
      }
    }
  }, [code, removeMermaidTempNodes]);

  // Initial render and re-render on code change
  useEffect(() => {
    renderDiagram();
  }, [renderDiagram]);

  // Pin the injected SVG to its natural pixel width (D8b): mermaid's
  // percentage width would re-fit the panel under CSS zoom and cap the
  // diagram to the box.
  useEffect(() => {
    if (!svgContent) {
      setNaturalWidth(null);
      return;
    }
    const svg = viewportRef.current?.querySelector("svg");
    if (!svg) {
      setNaturalWidth(null);
      return;
    }
    const natural = readNaturalWidth(svg);
    if (natural !== null) {
      svg.style.maxWidth = "none";
      svg.style.width = `${natural}px`;
    }
    setNaturalWidth(natural);
  }, [svgContent, isLoading]);

  // Track the scroll panel's content width so the 100% baseline stays
  // "fitted" (D8b): effective zoom = userZoom × min(1, panel / natural).
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setPanelWidth(
          entry.contentRect.width > 0 ? entry.contentRect.width : null,
        );
      }
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [svgContent, isLoading]);

  // Drag-to-pan handlers (D8c). Only mouse/pen pointers pan; touch keeps
  // native overflow scrolling. Capture guards tolerate environments without
  // the Pointer Events surface (jsdom).
  const handlePanStart = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.pointerType === "touch" || e.button !== 0) return;
      const el = e.currentTarget;
      panStateRef.current = {
        pointerId: e.pointerId,
        startX: e.clientX,
        startY: e.clientY,
        scrollLeft: el.scrollLeft,
        scrollTop: el.scrollTop,
      };
      el.setPointerCapture?.(e.pointerId);
      setIsPanning(true);
      // Prevent text selection / image drag while panning.
      e.preventDefault();
    },
    [],
  );

  const handlePanMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const pan = panStateRef.current;
    if (!pan || pan.pointerId !== e.pointerId) return;
    const el = e.currentTarget;
    el.scrollLeft = pan.scrollLeft - (e.clientX - pan.startX);
    el.scrollTop = pan.scrollTop - (e.clientY - pan.startY);
  }, []);

  const handlePanEnd = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const pan = panStateRef.current;
    if (!pan || pan.pointerId !== e.pointerId) return;
    panStateRef.current = null;
    if (e.currentTarget.hasPointerCapture?.(e.pointerId)) {
      e.currentTarget.releasePointerCapture?.(e.pointerId);
    }
    setIsPanning(false);
  }, []);

  const fittedScale =
    naturalWidth !== null && panelWidth !== null
      ? Math.min(1, panelWidth / naturalWidth)
      : 1;
  const effectiveZoom = zoom * fittedScale;

  // Listen for theme changes on <html> element
  useEffect(() => {
    const htmlEl = document.documentElement;

    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (
          mutation.type === "attributes" &&
          (mutation.attributeName === "data-theme" ||
            mutation.attributeName === "data-mode")
        ) {
          renderDiagram();
          break;
        }
      }
    });

    observer.observe(htmlEl, {
      attributes: true,
      attributeFilter: ["data-theme", "data-mode"],
    });

    return () => observer.disconnect();
  }, [renderDiagram]);

  return (
    <div
      ref={containerRef}
      className={classNames(
        "rounded-[var(--g5-radius-sm)]",
        "max-w-full",
        "overflow-hidden",
        className,
      )}
    >
      {/* Loading state */}
      {isLoading && (
        <div
          className={classNames(
            "flex items-center justify-center",
            "py-8",
            "bg-[var(--g5-bg0)]",
            "text-sm text-[var(--g5-text2)]",
          )}
        >
          <div className="flex items-center gap-2">
            <svg
              className="animate-spin w-4 h-4"
              viewBox="0 0 24 24"
              fill="none"
            >
              <circle
                className="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="4"
              />
              <path
                className="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
              />
            </svg>
            Rendering diagram…
          </div>
        </div>
      )}

      {/* Rendered SVG with zoom controls (D8b) */}
      {!isLoading && svgContent && (
        <div className="relative">
          <div
            data-testid="mermaid-zoom-controls"
            className="absolute top-2 right-2 z-10 flex items-center gap-1"
          >
            <button
              type="button"
              data-testid="mermaid-zoom-out"
              aria-label="Zoom out"
              title="Zoom out"
              disabled={zoom <= ZOOM_MIN}
              onClick={() => setZoom((z) => Math.max(ZOOM_MIN, z / ZOOM_STEP))}
              className={ZOOM_BUTTON_CLASS}
            >
              −
            </button>
            <button
              type="button"
              data-testid="mermaid-zoom-reset"
              aria-label="Reset zoom"
              title="Reset zoom to 100%"
              onClick={() => setZoom(1)}
              className={ZOOM_BUTTON_CLASS}
            >
              {Math.round(zoom * 100)}%
            </button>
            <button
              type="button"
              data-testid="mermaid-zoom-in"
              aria-label="Zoom in"
              title="Zoom in"
              disabled={zoom >= ZOOM_MAX}
              onClick={() => setZoom((z) => Math.min(ZOOM_MAX, z * ZOOM_STEP))}
              className={ZOOM_BUTTON_CLASS}
            >
              +
            </button>
          </div>
          <div
            ref={scrollRef}
            data-testid="mermaid-pan-surface"
            className={classNames(
              "overflow-auto",
              "p-4",
              "bg-[var(--g5-bg0)]",
              isPanning ? "cursor-grabbing" : "cursor-grab",
            )}
            style={{ maxWidth: "100%" }}
            onPointerDown={handlePanStart}
            onPointerMove={handlePanMove}
            onPointerUp={handlePanEnd}
            onPointerCancel={handlePanEnd}
          >
            {/* Auto-margin centering (not flex justify-center) keeps the left
                edge of an overflowing zoomed diagram reachable by scroll; CSS
                zoom scales layout size, so the panel's scroll area grows with
                the diagram (D8b). The SVG is pinned to natural pixel width and
                the applied zoom folds in the fitted baseline so 100% fits the
                panel while higher steps outgrow it (D8b, chat 8cb31169). */}
            <div
              ref={viewportRef}
              data-testid="mermaid-zoom-viewport"
              data-zoom={zoom}
              style={{
                zoom: effectiveZoom,
                width: "fit-content",
                margin: "0 auto",
              }}
              dangerouslySetInnerHTML={{ __html: svgContent }}
            />
          </div>
        </div>
      )}

      {/* Chunk-load failure: the app was updated and this session's mermaid
          chunk no longer exists — offer recovery instead of dumping the raw
          import error (shared-components D8a). */}
      {!isLoading && error && errorKind === "chunk-load" && (
        <div
          data-testid="chunk-reload-affordance"
          className={classNames(
            "flex flex-col items-center justify-center gap-3",
            "py-8 px-4",
            "bg-[var(--g5-bg0)]",
            "text-sm text-[var(--g5-text2)]",
          )}
        >
          <span>G5 was updated — reload to continue</span>
          <button
            type="button"
            data-testid="mermaid-chunk-reload"
            onClick={() => requestPageReload()}
            className={classNames(
              "px-3 py-1.5 rounded-[var(--g5-radius-sm)]",
              "text-sm font-medium",
              "bg-[var(--g5-bg2)] text-[var(--g5-text0)]",
              "border border-[var(--g5-stroke)]",
              "hover:bg-[var(--g5-bg1)]",
            )}
          >
            Reload
          </button>
        </div>
      )}

      {/* Parse/authoring error state: show raw code */}
      {!isLoading && error && errorKind === "parse" && (
        <div>
          {/* Error message */}
          <div
            className={classNames("px-3 py-2", "text-xs", "bg-[var(--g5-bg2)]")}
            style={{ color: "var(--g5-blocked)" }}
          >
            ⚠ Diagram error: {error}
          </div>

          {/* Raw code fallback */}
          <pre
            className={classNames(
              "p-3 overflow-x-auto",
              "text-sm leading-relaxed",
              "bg-[var(--g5-bg0)]",
              "text-[var(--g5-text2)]",
            )}
            style={{ fontFamily: "var(--g5-font-mono)" }}
          >
            {code}
          </pre>
        </div>
      )}

      {/* Empty state */}
      {!isLoading && !svgContent && !error && (
        <div
          className={classNames(
            "flex items-center justify-center",
            "py-8",
            "bg-[var(--g5-bg0)]",
            "text-sm text-[var(--g5-text2)]",
          )}
        >
          No diagram code provided
        </div>
      )}
    </div>
  );
};
