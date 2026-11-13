import { classNames } from "../ui/classNames";

/**
 * Theme tones for the reading view's Compare and filter toggles, after the
 * retired SpecEditor toolbar; they sit in the file row beside its buttons.
 */
const TONES = {
  info: {
    border: "border-[var(--g5-info)]/35",
    selected: "bg-[var(--g5-info)]/15 text-[var(--g5-info)]",
    count: "bg-[var(--g5-info)]",
  },
  ok: {
    border: "border-[var(--g5-ok)]/35",
    selected: "bg-[var(--g5-ok)]/15 text-[var(--g5-ok)]",
    count: "bg-[var(--g5-ok)]",
  },
  blocked: {
    border: "border-[var(--g5-blocked)]/35",
    selected: "bg-[var(--g5-blocked)]/15 text-[var(--g5-blocked)]",
    count: "bg-[var(--g5-blocked)]",
  },
} as const;

export type PreviewToolbarTone = keyof typeof TONES;

/** A toggle button whose pressed state is visible, not only announced. */
export function previewToolbarButtonClass(
  selected: boolean,
  tone: PreviewToolbarTone,
) {
  return classNames(
    "preview-toolbar-button inline-flex shrink-0 items-center gap-1 rounded border px-2 py-[3px] text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-50",
    TONES[tone].border,
    selected
      ? TONES[tone].selected
      : "text-[var(--g5-text2)] enabled:hover:bg-[var(--g5-overlay-hover)]",
  );
}

/** A count pill shown inline after a toolbar label. */
export function previewToolbarCountClass(tone: PreviewToolbarTone) {
  return classNames(
    "flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full px-1 font-mono text-[10px] font-semibold leading-none text-white",
    TONES[tone].count,
  );
}
