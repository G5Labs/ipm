import type { PreviewIndicatorState } from "./previewIndicator";

const STATE_COLOR: Record<PreviewIndicatorState, string> = {
  none: "currentColor",
  starting: "var(--g5-warn)",
  serving: "var(--g5-ok)",
  closing: "var(--g5-warn)",
  failed: "var(--g5-blocked)",
};

/**
 * The preview trigger's glyph: lucide's Monitor, whose screen lights in the
 * state's colour while a preview runs (the computer is "on"). Starting and
 * closing breathe the fill; reduced motion keeps it steady. Colour is never
 * the only cue: the trigger's tooltip and accessible name carry the state.
 */
export function PreviewMonitorIcon({
  size,
  state,
  className,
}: {
  size: number;
  state: PreviewIndicatorState;
  className?: string;
}) {
  const color = STATE_COLOR[state];
  const breathing = state === "starting" || state === "closing";
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color}
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      data-preview-state={state}
      className={className}
    >
      {state !== "none" ? (
        <rect
          width="20"
          height="14"
          x="2"
          y="3"
          rx="2"
          fill={color}
          fillOpacity={0.4}
          stroke="none"
          className={
            breathing ? "animate-pulse motion-reduce:animate-none" : undefined
          }
        />
      ) : null}
      <rect width="20" height="14" x="2" y="3" rx="2" />
      <line x1="8" x2="16" y1="21" y2="21" />
      <line x1="12" x2="12" y1="17" y2="21" />
    </svg>
  );
}
