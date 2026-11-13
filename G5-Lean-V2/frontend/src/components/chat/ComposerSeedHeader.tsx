// =============================================================================
// ComposerSeedHeader — what a seeded fresh chat shows at its very top while
// the seeded draft is pending (ui g5.workbench.review-prompts)
// =============================================================================

import {
  type ComposerSeedHeader as ComposerSeedHeaderValue,
  REVIEW_LOOP_MAX_ITERATIONS,
  BASE_REVERIFICATION_NOTICE,
  VERIFICATION_NOTICE,
} from "../../lib/reviewPrompts";

const ITERATION_CHOICES = Array.from(
  { length: REVIEW_LOOP_MAX_ITERATIONS },
  (_, index) => index + 1,
);

export function ComposerSeedHeader({
  header,
  onIterationsChange,
}: {
  header: ComposerSeedHeaderValue;
  onIterationsChange: (iterations: number) => void;
}) {
  if (header.kind === "verification" || header.kind === "base-verification") {
    return (
      <p
        data-testid="composer-seed-verification-notice"
        role="note"
        className="mb-3 rounded-md border border-[var(--g5-warn)]/60 bg-[var(--g5-warn)]/15 px-4 py-3 text-sm font-semibold leading-6 text-[var(--g5-text0)]"
      >
        {header.kind === "verification"
          ? VERIFICATION_NOTICE
          : BASE_REVERIFICATION_NOTICE}
      </p>
    );
  }
  return (
    <div
      data-testid="composer-seed-review-loop"
      className="mb-3 rounded-md border border-[var(--g5-info)]/50 bg-[var(--g5-info)]/10 px-4 py-3"
    >
      <label className="flex flex-wrap items-center gap-2 text-sm font-semibold text-[var(--g5-text0)]">
        <span>Review in a loop with</span>
        <select
          data-testid="composer-seed-review-iterations"
          aria-label="Review iterations"
          value={header.iterations}
          onChange={(event) => onIterationsChange(Number(event.target.value))}
          className="rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2 py-1 text-sm font-semibold text-[var(--g5-text0)]"
        >
          {ITERATION_CHOICES.map((count) => (
            <option key={count} value={count}>
              {count}
            </option>
          ))}
        </select>
        <span>{header.iterations === 1 ? "iteration" : "iterations"}</span>
      </label>
      <p className="mt-1.5 text-xs leading-5 text-[var(--g5-text1)]">
        More iterations give stronger coverage, fix more issues and give more
        confidence, at the cost of more time and tokens.
      </p>
    </div>
  );
}
