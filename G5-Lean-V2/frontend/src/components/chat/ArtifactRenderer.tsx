// =============================================================================
// ArtifactRenderer — Type-dispatching renderer for 11 artifact types
// =============================================================================

import {
  BarChart,
  Check,
  ChevronDown,
  ChevronUp,
  Code,
  Copy,
  Download,
  ExternalLink,
  FileText,
  GitBranch,
  Hash,
  Image,
  Play,
  SortAsc,
  Terminal,
  X,
} from "lucide-react";
import React, { Suspense, lazy, useCallback, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Artifact, ArtifactAction, ArtifactType } from "../../lib/types";
import { MermaidDiagram } from "../shared/MermaidDiagram";
import { classNames, useDialogFocus } from "../ui/UI";
import { highlightCode } from "../../lib/utils";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Detect MIME type from the leading bytes of a base64-encoded image. */
function detectBase64Mime(b64: string): string {
  if (b64.startsWith("/9j/")) return "image/jpeg";
  if (b64.startsWith("R0lGOD")) return "image/gif";
  if (b64.startsWith("UklGR")) return "image/webp";
  return "image/png";
}

// ---------------------------------------------------------------------------
// Dynamic imports for heavy renderers
// ---------------------------------------------------------------------------

const LazyKaTeX = lazy(async () => {
  try {
    await import("katex");
    return {
      default: ({
        math,
        displayMode,
      }: {
        math: string;
        displayMode: boolean;
      }) => {
        const katex = (window as unknown as Record<string, unknown>)[
          "katex"
        ] as
          | {
              renderToString: (
                tex: string,
                opts: Record<string, boolean>,
              ) => string;
            }
          | undefined;
        if (!katex)
          return (
            <pre className="text-[var(--g5-text1)] font-[var(--g5-font-mono)]">
              {math}
            </pre>
          );
        const html = katex.renderToString(math, {
          displayMode,
          throwOnError: false,
        });
        return <div dangerouslySetInnerHTML={{ __html: html }} />;
      },
    };
  } catch {
    return {
      default: ({ math }: { math: string }) => (
        <pre className="text-[var(--g5-text1)] font-[var(--g5-font-mono)]">
          {math}
        </pre>
      ),
    };
  }
});

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ArtifactRendererProps {
  artifact: Artifact;
  /** Actions to display in artifact header */
  actions?: ArtifactAction[];
  /** Callback when an action button is clicked */
  onAction?: (action: ArtifactAction) => void;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function typeIcon(type: ArtifactType): React.ReactNode {
  const size = 14;
  switch (type) {
    case "code":
      return <Code size={size} />;
    case "diff":
      return <GitBranch size={size} />;
    case "table":
      return <Hash size={size} />;
    case "chart":
      return <BarChart size={size} />;
    case "diagram":
      return <Code size={size} />;
    case "image":
      return <Image size={size} />;
    case "file":
      return <FileText size={size} />;
    case "terminal":
      return <Terminal size={size} />;
    case "math":
      return <Hash size={size} />;
    case "json":
      return <Code size={size} />;
    case "dataframe":
      return <SortAsc size={size} />;
    default:
      return <FileText size={size} />;
  }
}

function typeLabel(type: ArtifactType): string {
  switch (type) {
    case "code":
      return "Code";
    case "diff":
      return "Diff";
    case "table":
      return "Table";
    case "chart":
      return "Chart";
    case "diagram":
      return "Diagram";
    case "image":
      return "Image";
    case "file":
      return "File";
    case "terminal":
      return "Terminal";
    case "math":
      return "Math";
    case "json":
      return "JSON";
    case "dataframe":
      return "DataFrame";
    default:
      return "Artifact";
  }
}

function actionIcon(action: ArtifactAction): React.ReactNode {
  const size = 12;
  switch (action.action.type) {
    case "copy":
      return <Copy size={size} />;
    case "download":
      return <Download size={size} />;
    case "apply":
      return <GitBranch size={size} />;
    case "run":
      return <Play size={size} />;
  }
}

function contentAsString(content: string | object): string {
  if (typeof content === "string") return content;
  try {
    return JSON.stringify(content, null, 2);
  } catch {
    return String(content);
  }
}

const MAX_LINES_BEFORE_TRUNCATE = 1000;

// ---------------------------------------------------------------------------
// Sub-renderers
// ---------------------------------------------------------------------------

function CodeArtifact({
  content,
  language,
}: {
  content: string;
  language?: string;
}) {
  const lines = content.split("\n");
  const [showAll, setShowAll] = useState(
    lines.length <= MAX_LINES_BEFORE_TRUNCATE,
  );
  const [collapsedRanges, setCollapsedRanges] = useState<Set<number>>(
    new Set(),
  );
  const displayLines = showAll
    ? lines
    : lines.slice(0, MAX_LINES_BEFORE_TRUNCATE);

  // Syntax-highlighted HTML (memoized)
  const highlightedLines = React.useMemo(() => {
    const highlighted = highlightCode(content, language ?? "");
    return highlighted.split("\n");
  }, [content, language]);

  const gutterWidth = Math.max(String(displayLines.length).length * 8 + 12, 32);

  // Detect foldable blocks (lines ending with { or : that start an indented block)
  const foldableLines = React.useMemo(() => {
    const foldable = new Set<number>();
    for (let i = 0; i < lines.length - 1; i++) {
      const trimmed = lines[i].trimEnd();
      if (
        trimmed.endsWith("{") ||
        trimmed.endsWith(":") ||
        trimmed.endsWith("(") ||
        trimmed.endsWith("[")
      ) {
        // Check if next line is more indented
        const currIndent = lines[i].length - lines[i].trimStart().length;
        const nextIndent =
          lines[i + 1].length - lines[i + 1].trimStart().length;
        if (nextIndent > currIndent || lines[i + 1].trim() === "") {
          foldable.add(i);
        }
      }
    }
    return foldable;
  }, [lines]);

  // Find the end of a foldable block
  function findBlockEnd(startLine: number): number {
    const baseIndent =
      lines[startLine].length - lines[startLine].trimStart().length;
    for (let i = startLine + 1; i < lines.length; i++) {
      const line = lines[i];
      if (line.trim() === "") continue;
      const indent = line.length - line.trimStart().length;
      if (indent <= baseIndent) return i - 1;
    }
    return lines.length - 1;
  }

  function toggleFold(lineIdx: number) {
    setCollapsedRanges((prev) => {
      const next = new Set(prev);
      if (next.has(lineIdx)) next.delete(lineIdx);
      else next.add(lineIdx);
      return next;
    });
  }

  // Build visible lines, skipping collapsed ranges
  const visibleLines: Array<{
    originalIdx: number;
    html: string;
    isFoldHeader: boolean;
    isCollapsed: boolean;
    foldedCount: number;
  }> = React.useMemo(() => {
    const result: typeof visibleLines extends Array<infer T> ? T[] : never = [];
    let i = 0;
    while (i < displayLines.length) {
      const isFoldable = foldableLines.has(i);
      const isCollapsed = collapsedRanges.has(i);
      const blockEnd = isFoldable ? findBlockEnd(i) : i;
      result.push({
        originalIdx: i,
        html: (showAll ? highlightedLines[i] : highlightedLines[i]) ?? "",
        isFoldHeader: isFoldable,
        isCollapsed,
        foldedCount: isCollapsed ? blockEnd - i : 0,
      });
      if (isCollapsed && isFoldable) {
        i = blockEnd + 1;
      } else {
        i++;
      }
    }
    return result;
  }, [displayLines, highlightedLines, foldableLines, collapsedRanges, showAll]);

  return (
    <div className="overflow-auto relative bg-[var(--g5-bg0)]">
      {/* Language badge */}
      {language && (
        <div className="absolute top-1.5 right-2 text-[10px] text-[var(--g5-text2)] uppercase tracking-wide z-10 bg-[var(--g5-bg0)]/80 px-1.5 py-0.5 rounded-[var(--g5-radius-sm)]">
          {language}
        </div>
      )}
      <pre className="text-xs leading-[18px] font-[var(--g5-font-mono)] p-0 m-0">
        <code>
          {visibleLines.map((vl) => (
            <div
              key={vl.originalIdx}
              className="flex hover:bg-[var(--g5-overlay-hover)]"
            >
              {/* Fold gutter */}
              <span className="select-none w-5 shrink-0 flex items-center justify-center text-[var(--g5-text2)]">
                {vl.isFoldHeader ? (
                  <button
                    type="button"
                    data-testid="artifact-code-fold-toggle"
                    onClick={() => toggleFold(vl.originalIdx)}
                    className="text-[9px] hover:text-[var(--g5-text0)] cursor-pointer"
                    title={vl.isCollapsed ? "Expand" : "Collapse"}
                  >
                    {vl.isCollapsed ? "▶" : "▼"}
                  </button>
                ) : null}
              </span>
              {/* Line number */}
              <span
                className="select-none text-[var(--g5-linenum)] text-right pr-3 shrink-0"
                style={{ width: gutterWidth, minWidth: gutterWidth }}
              >
                {vl.originalIdx + 1}
              </span>
              {/* Code content — preserve indentation via pre/whitespace */}
              <span
                className="flex-1 text-[var(--g5-text0)] whitespace-pre"
                dangerouslySetInnerHTML={{ __html: vl.html }}
              />
              {/* Collapsed indicator */}
              {vl.isCollapsed && vl.foldedCount > 0 && (
                <span className="ml-1 text-[10px] text-[var(--g5-text2)] bg-[var(--g5-bg2)] px-1 rounded-[var(--g5-radius-sm)] select-none">
                  ⋯ {vl.foldedCount} lines
                </span>
              )}
            </div>
          ))}
        </code>
      </pre>
      {!showAll && (
        <button
          type="button"
          data-testid="artifact-code-show-all"
          onClick={() => setShowAll(true)}
          className="w-full py-1.5 text-xs text-[var(--g5-info)] hover:underline cursor-pointer border-t border-[var(--g5-stroke)]"
        >
          Show all {lines.length} lines
        </button>
      )}
    </div>
  );
}

function DiffArtifact({ content }: { content: string }) {
  const lines = content.split("\n");
  return (
    <div className="overflow-auto">
      <pre className="text-xs leading-relaxed font-[var(--g5-font-mono)]">
        {lines.map((line, i) => {
          let lineClass = "text-[var(--g5-text1)]";
          if (line.startsWith("+") && !line.startsWith("+++")) {
            lineClass = "text-[var(--g5-ok)] bg-[var(--g5-ok)]/10";
          } else if (line.startsWith("-") && !line.startsWith("---")) {
            lineClass = "text-[var(--g5-blocked)] bg-[var(--g5-blocked)]/10";
          } else if (line.startsWith("@@")) {
            lineClass = "text-[var(--g5-info)]";
          }
          return (
            <div key={i} className="flex">
              <span className="select-none text-[var(--g5-text2)] w-10 text-right pr-3 shrink-0">
                {i + 1}
              </span>
              <span className={lineClass}>{line}</span>
            </div>
          );
        })}
      </pre>
    </div>
  );
}

function TableArtifact({ content }: { content: string | object }) {
  // Parse markdown table or JSON array
  let headers: string[] = [];
  let rows: string[][] = [];

  if (typeof content === "string") {
    const lines = content
      .trim()
      .split("\n")
      .filter((l) => l.trim());
    if (lines.length >= 2) {
      const splitPipeCells = (line: string): string[] => {
        const cells = line.split("|").map((c) => c.trim());
        if (cells[0] === "") cells.shift();
        if (cells.length > 0 && cells[cells.length - 1] === "") cells.pop();
        return cells;
      };
      headers = splitPipeCells(lines[0]);
      // Skip separator line (line[1])
      for (let i = 2; i < lines.length; i++) {
        rows.push(splitPipeCells(lines[i]));
      }
    }
  } else if (Array.isArray(content)) {
    if (content.length > 0 && typeof content[0] === "object") {
      headers = [
        ...new Set(
          (content as Record<string, unknown>[]).flatMap((row) =>
            Object.keys(row),
          ),
        ),
      ];
      rows = content.map((row) =>
        headers.map((h) => String((row as Record<string, unknown>)[h] ?? "")),
      );
    }
  }

  if (headers.length === 0) {
    return (
      <pre className="text-xs text-[var(--g5-text1)] font-[var(--g5-font-mono)]">
        {contentAsString(content)}
      </pre>
    );
  }

  return (
    <div className="overflow-auto">
      <table className="w-full text-xs">
        <thead>
          <tr className="border-b border-[var(--g5-stroke)]">
            {headers.map((h, i) => (
              <th
                key={i}
                className="px-3 py-1.5 text-left font-semibold text-[var(--g5-text0)]"
              >
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, ri) => (
            <tr
              key={ri}
              className={classNames(
                "border-b border-[var(--g5-stroke)]/50",
                ri % 2 === 1 && "bg-[var(--g5-bg2)]/30",
              )}
            >
              {row.map((cell, ci) => (
                <td key={ci} className="px-3 py-1.5 text-[var(--g5-text1)]">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ChartArtifact({ content }: { content: string | object }) {
  // Dynamic import for react-chartjs-2 — render a placeholder if unavailable
  const data =
    typeof content === "string"
      ? (() => {
          try {
            return JSON.parse(content);
          } catch {
            return null;
          }
        })()
      : content;

  if (!data) {
    return (
      <pre className="text-xs text-[var(--g5-text2)] font-[var(--g5-font-mono)]">
        Invalid chart data
      </pre>
    );
  }

  return (
    <div className="flex items-center justify-center p-4 min-h-[200px]">
      <div className="text-xs text-[var(--g5-text2)] text-center">
        <BarChart size={32} className="mx-auto mb-2 opacity-40" />
        <div>Chart visualization</div>
        <div className="text-[10px] mt-1">react-chartjs-2 renderer</div>
      </div>
    </div>
  );
}

function DiagramArtifact({ content }: { content: string }) {
  // Use the shared MermaidDiagram component for diagram rendering
  return (
    <div className="overflow-auto p-2">
      <MermaidDiagram code={content} />
    </div>
  );
}

/**
 * Fullscreen image viewer. Not a centered panel dialog, so it keeps its own
 * edge-to-edge layout and takes only the shared dialog keyboard/focus
 * lifecycle — Escape, Tab trap, and focus restore (design-system D15).
 */
function ImageLightbox({
  src,
  title,
  onClose,
}: {
  src: string;
  title?: string;
  onClose: () => void;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  // Priority mirrors the fullscreen overlay's z-[9999].
  useDialogFocus(panelRef, { onClose, priority: 9999 });
  // Backdrop dismissal is press-AND-release, matching the Modal shell: a
  // drag that starts on the image and releases on the backdrop dispatches
  // its click to the common ancestor and must not close the viewer.
  const pressStartedOnBackdropRef = useRef(false);

  // Portaled to <body>: a detached-chat FloatingPanel is a fixed-position
  // stacking context, and no z-index can escape one from inside it.
  return createPortal(
    <div
      ref={panelRef}
      role="dialog"
      aria-modal="true"
      aria-label={title ?? "Image artifact"}
      tabIndex={-1}
      className="fixed inset-0 z-[9999] bg-black/80 flex items-center justify-center p-8"
      onMouseDown={(event) => {
        pressStartedOnBackdropRef.current =
          event.target === event.currentTarget;
      }}
      onMouseUp={(event) => {
        const startedOnBackdrop = pressStartedOnBackdropRef.current;
        pressStartedOnBackdropRef.current = false;
        if (startedOnBackdrop && event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <button
        type="button"
        data-testid="artifact-image-lightbox-close"
        onClick={onClose}
        aria-label="Close image preview"
        className="absolute top-4 right-4 text-white hover:text-gray-300"
      >
        <X size={24} />
      </button>
      <img
        src={src}
        alt={title ?? "Image artifact"}
        className="max-w-full max-h-full object-contain"
      />
    </div>,
    document.body,
  );
}

function ImageArtifact({
  content,
  title,
}: {
  content: string;
  title?: string;
}) {
  const [lightboxOpen, setLightboxOpen] = useState(false);
  const src =
    content.startsWith("data:") || content.startsWith("http")
      ? content
      : `data:${detectBase64Mime(content)};base64,${content}`;

  return (
    <>
      <div className="flex justify-center p-2">
        {/* A real button: keyboard-openable, and a focusable opener the
            lightbox's focus lifecycle can restore to on close (a bare <img>
            is not focusable, so restore would land on <body>). */}
        <button
          type="button"
          data-testid="artifact-image-open-fullscreen"
          onClick={() => setLightboxOpen(true)}
          aria-haspopup="dialog"
          aria-label={`Open ${title ?? "image artifact"} full screen`}
          className="rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
        >
          <img
            src={src}
            alt={title ?? "Image artifact"}
            className="max-w-full max-h-80 rounded cursor-pointer hover:opacity-90 transition-opacity"
          />
        </button>
      </div>
      {lightboxOpen && (
        <ImageLightbox
          src={src}
          title={title}
          onClose={() => setLightboxOpen(false)}
        />
      )}
    </>
  );
}

function FileArtifact({ content, title }: { content: string; title?: string }) {
  const sizeBytes = typeof content === "string" ? new Blob([content]).size : 0;
  const sizeLabel =
    sizeBytes < 1024 ? `${sizeBytes}B` : `${(sizeBytes / 1024).toFixed(1)}KB`;
  const preview = typeof content === "string" ? content.slice(0, 200) : "";

  return (
    <div className="p-3">
      <div className="flex items-center gap-2 mb-2">
        <FileText size={16} className="text-[var(--g5-info)]" />
        <span className="text-sm font-medium text-[var(--g5-text0)]">
          {title || "File"}
        </span>
        <span className="text-xs text-[var(--g5-text2)]">{sizeLabel}</span>
      </div>
      {preview && (
        <pre className="text-xs text-[var(--g5-text2)] font-[var(--g5-font-mono)] truncate">
          {preview}
        </pre>
      )}
    </div>
  );
}

function TerminalArtifact({ content }: { content: string }) {
  const lines = content.split("\n");
  // Try to detect command vs output: first line starting with $ or > is the command
  let command = "";
  let output = content;
  let exitCode: string | null = null;

  if (
    lines.length > 0 &&
    (lines[0].startsWith("$ ") || lines[0].startsWith("> "))
  ) {
    command = lines[0].slice(2);
    output = lines.slice(1).join("\n");
  }

  // Check for exit code in last line
  const lastLine = lines[lines.length - 1];
  if (lastLine && /^exit\s+\d+$/i.test(lastLine.trim())) {
    exitCode = lastLine.trim().split(/\s+/)[1];
    output = lines.slice(command ? 1 : 0, -1).join("\n");
  }

  return (
    <div className="bg-[var(--g5-bg0)] rounded-[var(--g5-radius-sm)] overflow-auto">
      {command && (
        <div className="px-3 py-1.5 border-b border-[var(--g5-stroke)]/50 flex items-center gap-2">
          <span className="text-[var(--g5-ok)] text-xs font-[var(--g5-font-mono)]">
            $
          </span>
          <span className="text-xs text-[var(--g5-text0)] font-[var(--g5-font-mono)]">
            {command}
          </span>
        </div>
      )}
      <pre className="px-3 py-2 text-xs text-[var(--g5-text1)] font-[var(--g5-font-mono)] leading-relaxed whitespace-pre-wrap">
        {output}
      </pre>
      {exitCode != null && (
        <div
          className={classNames(
            "px-3 py-1 border-t border-[var(--g5-stroke)]/50 text-[10px]",
            exitCode === "0"
              ? "text-[var(--g5-ok)]"
              : "text-[var(--g5-blocked)]",
          )}
        >
          exit {exitCode}
        </div>
      )}
    </div>
  );
}

function MathArtifact({ content }: { content: string }) {
  const isBlock =
    content.includes("\\\\") ||
    content.includes("\\begin") ||
    content.length > 50;
  return (
    <Suspense
      fallback={
        <pre className="text-xs text-[var(--g5-text1)] font-[var(--g5-font-mono)] p-2">
          {content}
        </pre>
      }
    >
      <div className="p-3 flex justify-center">
        <LazyKaTeX math={content} displayMode={isBlock} />
      </div>
    </Suspense>
  );
}

function JsonArtifact({ content }: { content: string | object }) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const jsonStr =
    typeof content === "string" ? content : JSON.stringify(content, null, 2);

  // Simple JSON rendering with syntax highlighting
  const highlighted = highlightCode(jsonStr, "json");

  return (
    <div className="overflow-auto">
      <pre
        className="text-xs leading-relaxed font-[var(--g5-font-mono)] p-2"
        dangerouslySetInnerHTML={{ __html: highlighted }}
      />
    </div>
  );
}

function DataFrameArtifact({ content }: { content: string | object }) {
  // Render as scrollable table with column sorting
  let data: Record<string, unknown>[] = [];

  if (typeof content === "string") {
    try {
      data = JSON.parse(content);
    } catch (err) {
      console.warn("[ArtifactRenderer] Failed to parse DataFrame JSON:", err);
    }
  } else if (Array.isArray(content)) {
    data = content as Record<string, unknown>[];
  }

  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortAsc, setSortAsc] = useState(true);

  if (!Array.isArray(data) || data.length === 0) {
    return (
      <pre className="text-xs text-[var(--g5-text2)] font-[var(--g5-font-mono)] p-2">
        No data
      </pre>
    );
  }

  const columns = [...new Set(data.flatMap((row) => Object.keys(row)))];
  const sorted = sortCol
    ? [...data].sort((a, b) => {
        const va = a[sortCol];
        const vb = b[sortCol];
        const na = Number(va);
        const nb = Number(vb);
        const cmp =
          !isNaN(na) && !isNaN(nb)
            ? na - nb
            : String(va ?? "").localeCompare(String(vb ?? ""));
        return sortAsc ? cmp : -cmp;
      })
    : data;

  function handleSort(col: string) {
    if (sortCol === col) {
      setSortAsc((prev) => !prev);
    } else {
      setSortCol(col);
      setSortAsc(true);
    }
  }

  return (
    <div className="overflow-auto">
      <div className="text-[10px] text-[var(--g5-text2)] px-3 py-1">
        {data.length} rows × {columns.length} columns
      </div>
      <table className="w-full text-xs">
        <thead>
          <tr className="border-b border-[var(--g5-stroke)]">
            {columns.map((col) => (
              <th
                key={col}
                onClick={() => handleSort(col)}
                className="px-3 py-1.5 text-left font-semibold text-[var(--g5-text0)] cursor-pointer hover:bg-[var(--g5-bg2)]/50"
              >
                <span className="flex items-center gap-1">
                  {col}
                  {sortCol === col && (
                    <SortAsc
                      size={10}
                      className={classNames(!sortAsc && "rotate-180")}
                    />
                  )}
                </span>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sorted.map((row, ri) => (
            <tr
              key={ri}
              className={classNames(
                "border-b border-[var(--g5-stroke)]/50",
                ri % 2 === 1 && "bg-[var(--g5-bg2)]/30",
              )}
            >
              {columns.map((col) => (
                <td key={col} className="px-3 py-1.5 text-[var(--g5-text1)]">
                  {String(row[col] ?? "")}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

export function ArtifactRenderer({
  artifact,
  actions,
  onAction,
}: ArtifactRendererProps) {
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [copiedAction, setCopiedAction] = useState<string | null>(null);

  const title = artifact.title || `${typeLabel(artifact.type)} artifact`;
  const contentStr = contentAsString(artifact.content);

  const handleAction = useCallback(
    (action: ArtifactAction) => {
      if (action.action.type === "copy") {
        navigator.clipboard
          ?.writeText(contentStr)
          .then(() => {
            setCopiedAction(action.label);
            const timer = setTimeout(() => setCopiedAction(null), 2000);
            return () => clearTimeout(timer);
          })
          .catch(() => {
            /* Clipboard API unavailable — ignore silently */
          });
      } else if (action.action.type === "download") {
        const blob = new Blob([contentStr], { type: "text/plain" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = action.action.filename;
        a.click();
        URL.revokeObjectURL(url);
      }
      onAction?.(action);
    },
    [contentStr, onAction],
  );

  // Default actions based on artifact type
  const defaultActions: ArtifactAction[] = [];
  defaultActions.push({
    label: "Copy",
    icon: "copy",
    action: { type: "copy" },
  });

  if (artifact.type === "code" || artifact.type === "file") {
    defaultActions.push({
      label: "Download",
      icon: "download",
      action: {
        type: "download",
        filename: artifact.filePath || `artifact.${artifact.language || "txt"}`,
      },
    });
  }

  const allActions = actions ?? artifact.actions ?? defaultActions;

  function renderContent() {
    switch (artifact.type) {
      case "code":
        return (
          <CodeArtifact content={contentStr} language={artifact.language} />
        );
      case "diff":
        return <DiffArtifact content={contentStr} />;
      case "table":
        return <TableArtifact content={artifact.content} />;
      case "chart":
        return <ChartArtifact content={artifact.content} />;
      case "diagram":
        return <DiagramArtifact content={contentStr} />;
      case "image":
        return <ImageArtifact content={contentStr} title={artifact.title} />;
      case "file":
        return <FileArtifact content={contentStr} title={artifact.title} />;
      case "terminal":
        return <TerminalArtifact content={contentStr} />;
      case "math":
        return <MathArtifact content={contentStr} />;
      case "json":
        return <JsonArtifact content={artifact.content} />;
      case "dataframe":
        return <DataFrameArtifact content={artifact.content} />;
      default:
        return (
          <pre className="text-xs text-[var(--g5-text1)] font-[var(--g5-font-mono)] p-2">
            {contentStr}
          </pre>
        );
    }
  }

  return (
    <div
      className={classNames(
        "rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)]",
        "bg-[var(--g5-bg1)] overflow-hidden my-2",
      )}
    >
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg2)]/50">
        <span className="text-[var(--g5-text2)]">
          {typeIcon(artifact.type)}
        </span>
        <span className="text-xs font-medium text-[var(--g5-text0)] flex-1 truncate">
          {title}
        </span>

        {/* Action buttons */}
        <div className="flex items-center gap-1">
          {allActions.map((action) => (
            <button
              key={action.label}
              type="button"
              data-testid="artifact-action"
              onClick={() => handleAction(action)}
              className={classNames(
                "p-1 rounded-[var(--g5-radius-sm)] text-[var(--g5-text2)]",
                "hover:text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]",
                "transition-colors cursor-pointer",
              )}
              title={action.label}
            >
              {copiedAction === action.label ? (
                <Check size={12} className="text-[var(--g5-ok)]" />
              ) : (
                actionIcon(action)
              )}
            </button>
          ))}

          {/* Collapse toggle */}
          <button
            type="button"
            data-testid="artifact-collapse-toggle"
            onClick={() => setIsCollapsed((prev) => !prev)}
            className={classNames(
              "p-1 rounded-[var(--g5-radius-sm)] text-[var(--g5-text2)]",
              "hover:text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]",
              "transition-colors cursor-pointer",
            )}
            title={isCollapsed ? "Expand" : "Collapse"}
          >
            {isCollapsed ? <ChevronDown size={12} /> : <ChevronUp size={12} />}
          </button>
        </div>
      </div>

      {/* Body */}
      {!isCollapsed && <div className="relative">{renderContent()}</div>}
    </div>
  );
}
