// =============================================================================
// G5 Workbench — the review and re-verification prompts (ui
// g5.workbench.review-prompts)
// =============================================================================
// Code review (every Review Workspace entry point) and Re-run Verification
// seed a fresh chat with these prompts; the seed's header (the loop option or
// the cost notice) is rendered by `components/chat/ComposerSeedHeader.tsx`.
// A Base re-verification run's workspace lands on its run prompt under the
// same cost notice (ontology_and_harness g5.reverification.workspace).
// The wording is the owner's to tune.
// =============================================================================

import type { ReverificationHead, ReverificationRun } from "./types";

/** Iterations the review loop option offers (ui g5.workbench.review-prompts). */
export const REVIEW_LOOP_DEFAULT_ITERATIONS = 1;
export const REVIEW_LOOP_MAX_ITERATIONS = 10;

/** The iteration count clamped to what the option offers. */
export function clampReviewIterations(value: number): number {
  if (!Number.isFinite(value)) return REVIEW_LOOP_DEFAULT_ITERATIONS;
  return Math.min(REVIEW_LOOP_MAX_ITERATIONS, Math.max(1, Math.trunc(value)));
}

function reviewLoopPhrase(iterations: number): string {
  return `review and fix loop of ${iterations} ${iterations === 1 ? "iteration" : "iterations"}`;
}

const REVIEW_LOOP_PATTERN = /review and fix loop of \d+ iterations?/;

/**
 * Exact prefill used by every Review Workspace entry point
 * (ui g5.workbench.review-prompts).
 */
export function reviewWorkspaceChatPrompt(
  iterations: number = REVIEW_LOOP_DEFAULT_ITERATIONS,
): string {
  return `Please do the code review for the current PR. You should identify all requirements of this PR (e.g. initial requirements in project docs, design and plan, the ontology edits and relevant ontology blocks). You should fix everything that you find against the scope of this PR. Note: you do not need to fix any behavior that are not concrete bug fix, or not in the requirements and ontology and are not reasonably inferable. You will do an ${reviewLoopPhrase(clampReviewIterations(iterations))}, each iteration you will start from a fresh session to do the review and fix. In the end, create a very concise review report in the project docs (if there exists one, update it) for what you have fixed and optionally what uncertainty you have.`;
}

/**
 * The draft with its review loop count changed, keeping every other edit;
 * a draft whose loop phrase was edited away is returned unchanged.
 */
export function withReviewIterations(
  draft: string,
  iterations: number,
): string {
  return draft.replace(
    REVIEW_LOOP_PATTERN,
    reviewLoopPhrase(clampReviewIterations(iterations)),
  );
}

/** Prefill of Re-run Verification (ui g5.workbench.review-prompts). */
export function verificationChatPrompt(): string {
  return "Please identify all requirements and relevant ontology of this PR, read `$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md`, and follow it to complete the full workflow for this PR's incremental re-verification. Continue to the completion gate. When expected behavior is unclear, use a reasonable assumption and record it in the final report’s Human decisions section. Ask the owner only when a missing decision or dependency blocks required work and no reasonable assumption lets it proceed. Finish any independent work.";
}

/** The bold notice above a seeded Re-run Verification draft. */
export const VERIFICATION_NOTICE =
  "This task will build new or update existing formal state machines and run formal verification to check your PR's changes against its requirement and existing codebase's behavior. It will prove strong guarantee for the code behavior and fix hard-to-discover issues on security vulnerability, PR's feature completeness, integration issues of the PR, and any regression risks caused by this PR. Warning: this is very expensive process.";

/**
 * What a composer seed shows at the top of its fresh chat while the seeded
 * draft is pending (ui g5.workbench.review-prompts).
 */
export type ComposerSeedHeader =
  | { kind: "review-loop"; iterations: number }
  | { kind: "verification" }
  /** A Base re-verification run's landing: the same notice, about Base. */
  | { kind: "base-verification" };

/** The review prompt seed with its loop option. */
export function reviewWorkspaceSeed(): {
  text: string;
  header: ComposerSeedHeader;
} {
  return {
    text: reviewWorkspaceChatPrompt(REVIEW_LOOP_DEFAULT_ITERATIONS),
    header: { kind: "review-loop", iterations: REVIEW_LOOP_DEFAULT_ITERATIONS },
  };
}

/** The Re-run Verification seed with its cost notice. */
export function verificationSeed(): {
  text: string;
  header: ComposerSeedHeader;
} {
  return { text: verificationChatPrompt(), header: { kind: "verification" } };
}

// -----------------------------------------------------------------------------
// Base re-verification run (ontology_and_harness g5.reverification.workspace)
// -----------------------------------------------------------------------------

/** Base's commits as `rootPath@commit`, one per code repository. */
function reverificationHeads(heads: ReverificationHead[] | null): string {
  if (!heads || heads.length === 0) return "(no recorded commits)";
  return heads
    .map((head) => `${head.rootPath || head.repoId}@${head.commit}`)
    .join(", ");
}

/**
 * The run's prompt: its kind, Base commits and findings label, then the work
 * as required, optional and out of scope (g5.reverification.scope), with a
 * Project Doc written first as the run's checkpoint and report.
 */
export function baseReverificationChatPrompt(run: ReverificationRun): string {
  const forkedAt = reverificationHeads(run.baseHeads);
  const label = `Label every finding from this run with \`Run: ${run.label}\`.`;
  const incremental = run.kind === "incremental";
  const since = reverificationHeads(run.sinceHeads);
  const opening = incremental
    ? `Base re-verification (incremental) for this workspace, which was forked from Base at ${forkedAt}. Re-verify what Base gained from ${since} to ${forkedAt}. ${label}`
    : `Base re-verification (full) for this workspace, which was forked from Base at ${forkedAt}. Make every model current with the code at ${forkedAt}, add models for important behaviour that has none, and run them all. ${label}`;
  const firstRequired = incremental
    ? `Follow \`$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md\`, treating Base at ${since} as the pre-PR product and Base at ${forkedAt} as the current product. Commit and push your progress as you go.`
    : `Reconcile every existing model and its correspondence with the code at ${forkedAt}, using \`$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md\`'s obligation ledger and evidence rules where they apply. Commit and push your progress as you go.`;
  return [
    opening,
    "First, write this run's scope and the work below into a Project Doc named base-reverification.md, and keep it as your checkpoint and final report. Include the Human decisions section described in `$HOME/.agents/guidance/TLC/model-checking.md`.",
    "Where this prompt and `$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md` differ, this prompt wins: commit and push, read and extend the findings earlier runs recorded, and report product-code bugs instead of fixing them. A reported bug does not by itself keep the run from completing.",
    [
      "Required:",
      `- ${firstRequired}`,
      "- Report bugs and design simplifications in the existing findings files. Follow “Prepare material for the viewer” in `$HOME/.agents/guidance/TLC/setup_foundation.md` and run its format checks until clean.",
      "- Finish with every model parsing and running.",
      "- Rebase onto Base before opening the pull request.",
    ].join("\n"),
    [
      "Optional:",
      "- Repair models, and keep checking past a bug with a repaired copy of a model. Only as-built runs may tag a statement design-verified.",
    ].join("\n"),
    [
      "Out of scope:",
      "- Fixing product code, and committing failing tests (put reproductions in the finding instead).",
      `- Re-verifying pull requests merged into Base after ${forkedAt}.`,
    ].join("\n"),
  ].join("\n\n");
}

/** The run's landing seed: its prompt under the verification cost notice. */
export function baseReverificationSeed(run: ReverificationRun): {
  text: string;
  header: ComposerSeedHeader;
} {
  return {
    text: baseReverificationChatPrompt(run),
    header: { kind: "base-verification" },
  };
}

/** The bold cost notice in Home's Re-run verification dialog. */
export const BASE_REVERIFICATION_NOTICE =
  "This builds new or updates existing formal state machines and runs formal verification to check Base's behaviour against its intent. Warning: this is a very expensive process.";
