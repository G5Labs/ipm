// =============================================================================
// IntentLogBlockInspector — inline, read-only block inspector for the intent
// lineage graph. Spec: g5/client/workspace-page/intent-log-lineage-graph [D3].
//
// Rendered inside the lineage graph region (not a route change or modal editor)
// and dismissed by an outside click. Shows the intent id + title, file path,
// intent content, an optional git diff (when resolvable; no fabrication —
// PM [D26]), and a "Go to I/D block" navigation action.
// =============================================================================

import { useEffect } from "react";
import { ArrowUpRight, FileText, GitCompareArrows, X } from "lucide-react";

export interface BlockInspectorData {
  /** id label shown in the chip (e.g. "D-3" or "fn"). */
  id: string;
  /** intent title (derived from content). */
  title: string;
  fileName: string | null;
  content: string | null;
  /** optional unified diff; omitted entirely when unresolvable (no fabrication). */
  diff?: string | null;
  colorVar: string;
  kindName?: string;
  isCenter: boolean;
  /** false → no openable I/D markdown block (chat / supervisor-decision). */
  navigable?: boolean;
}

interface IntentLogBlockInspectorProps {
  data: BlockInspectorData;
  /** half width (focus mode) vs full. */
  half?: boolean;
  onClose: () => void;
  onGoto?: () => void;
}

function DiffView({ diff }: { diff: string }) {
  const lines = diff.split("\n");
  return (
    <div
      className="overflow-hidden rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] font-mono text-[11.5px]"
      data-testid="block-inspector-diff"
    >
      {lines.map((line, index) => {
        const sign = line[0];
        const add = sign === "+";
        const del = sign === "-";
        return (
          <div
            key={index}
            className="whitespace-pre-wrap px-3 py-0.5"
            style={{
              background: add
                ? "color-mix(in srgb, var(--g5-ok) 13%, transparent)"
                : del
                  ? "color-mix(in srgb, var(--g5-blocked) 13%, transparent)"
                  : "transparent",
              color: add
                ? "var(--g5-ok)"
                : del
                  ? "var(--g5-blocked)"
                  : "var(--g5-text2)",
            }}
          >
            {line || " "}
          </div>
        );
      })}
    </div>
  );
}

export function IntentLogBlockInspector({
  data,
  half = false,
  onClose,
  onGoto,
}: IntentLogBlockInspectorProps) {
  // Escape also dismisses, matching the outside-click affordance.
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  return (
    <div
      className="absolute inset-0 z-20"
      style={{
        background: "color-mix(in srgb, var(--g5-bg0) 55%, transparent)",
      }}
      onClick={onClose}
      data-testid="block-inspector-backdrop"
    >
      <div
        role="dialog"
        aria-label={`Intent block ${data.id} ${data.title}`}
        data-testid="block-inspector"
        className={`absolute left-1/2 top-1/2 max-h-[94%] -translate-x-1/2 -translate-y-1/2 overflow-auto rounded-[var(--g5-radius-md)] shadow-xl ${half ? "w-[47%] min-w-[260px]" : "w-[94%]"}`}
        style={{
          background: "var(--g5-bg1)",
          border: `1px solid ${data.colorVar}`,
        }}
        onClick={(event) => event.stopPropagation()}
      >
        {/* header: id: title */}
        <div className="flex items-start gap-2.5 border-b border-[var(--g5-stroke)] px-3.5 py-2.5">
          <span
            className="mt-0.5 inline-flex shrink-0 items-center justify-center rounded-[var(--g5-radius-sm)] font-mono text-[13px] font-bold"
            style={{
              width: 32,
              height: 32,
              color: data.colorVar,
              background: `color-mix(in srgb, ${data.colorVar} 18%, var(--g5-bg2))`,
              border: `1.5px solid ${data.colorVar}`,
            }}
          >
            {data.id}
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2 text-sm font-semibold text-[var(--g5-text0)]">
              <span className="font-mono" style={{ color: data.colorVar }}>
                {data.id}
              </span>
              <span className="text-[var(--g5-text2)]">:</span>
              <span>{data.title}</span>
              {data.isCenter && (
                <span
                  className="rounded px-1.5 py-0.5 text-[9.5px] uppercase tracking-wide"
                  style={{ color: "var(--g5-bg0)", background: data.colorVar }}
                >
                  current intent
                </span>
              )}
            </div>
            <div className="mt-0.5 truncate font-mono text-[11px] text-[var(--g5-text2)]">
              <FileText size={11} className="-mt-0.5 mr-1 inline" />
              {data.fileName || "—"}
              {data.kindName && (
                <span className="ml-2 not-italic">· {data.kindName}</span>
              )}
            </div>
          </div>
          <button
            type="button"
            data-testid="intent-log-inspector-close-button"
            aria-label="Close inspector"
            onClick={onClose}
            className="shrink-0 rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)]"
          >
            <X size={15} />
          </button>
        </div>

        <div className="px-3.5 py-3">
          {/* intent content */}
          <div className="mb-1 text-[10.5px] uppercase tracking-wide text-[var(--g5-text2)]">
            Intent content
          </div>
          <div className="mb-3.5 whitespace-pre-wrap rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-2 text-[12.5px] text-[var(--g5-text1)]">
            {data.content || "(no content recorded for this block)"}
          </div>

          {/* git diff — only when resolvable; no fabricated diff (PM [D26]) */}
          {data.diff && (
            <>
              <div className="mb-1 flex items-center gap-1.5 text-[10.5px] uppercase tracking-wide text-[var(--g5-text2)]">
                <GitCompareArrows size={12} /> Git diff — what changed for this
                intent
              </div>
              <DiffView diff={data.diff} />
            </>
          )}

          {onGoto && (
            <div className="mt-3.5 flex flex-col items-end gap-1">
              <button
                type="button"
                data-testid="intent-log-inspector-goto-button"
                disabled={data.navigable === false}
                onClick={data.navigable === false ? undefined : onGoto}
                title={
                  data.navigable === false
                    ? "Chat and supervisor-decision records have no I/D block to open"
                    : undefined
                }
                className="inline-flex items-center gap-1.5 rounded-[var(--g5-radius-sm)] px-3 py-1.5 text-xs font-medium disabled:cursor-not-allowed"
                style={
                  data.navigable === false
                    ? {
                        background: "var(--g5-bg2)",
                        color: "var(--g5-text2)",
                        opacity: 0.55,
                      }
                    : { background: data.colorVar, color: "var(--g5-bg0)" }
                }
              >
                Go to I/D block <ArrowUpRight size={14} />
              </button>
              {data.navigable === false && (
                <span className="text-[10px] text-[var(--g5-text2)]">
                  No I/D block to open for this source
                </span>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
