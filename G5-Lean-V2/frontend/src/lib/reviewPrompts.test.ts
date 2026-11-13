import { describe, expect, it } from "vitest";
import type { ReverificationRun } from "./types";
import {
  baseReverificationChatPrompt,
  baseReverificationSeed,
  clampReviewIterations,
  reviewWorkspaceChatPrompt,
  reviewWorkspaceSeed,
  verificationChatPrompt,
  verificationSeed,
  withReviewIterations,
} from "./reviewPrompts";

// ui g5.workbench.review-prompts
describe("review prompts", () => {
  it("fills the loop count into the review prompt, one iteration by default", () => {
    expect(reviewWorkspaceChatPrompt()).toContain(
      "You will do an review and fix loop of 1 iteration, each iteration you will start from a fresh session",
    );
    expect(reviewWorkspaceChatPrompt(3)).toContain(
      "review and fix loop of 3 iterations,",
    );
    expect(reviewWorkspaceChatPrompt()).toMatch(
      /^Please do the code review for the current PR\. You should identify all requirements of this PR/,
    );
  });

  it("clamps the count to the offered range", () => {
    expect(clampReviewIterations(0)).toBe(1);
    expect(clampReviewIterations(4.7)).toBe(4);
    expect(clampReviewIterations(99)).toBe(10);
    expect(clampReviewIterations(Number.NaN)).toBe(1);
  });

  it("rewrites only the count in an edited draft", () => {
    const edited = `Focus on the API first. ${reviewWorkspaceChatPrompt()}`;
    const rewritten = withReviewIterations(edited, 5);
    expect(rewritten).toBe(
      edited.replace(
        "review and fix loop of 1 iteration",
        "review and fix loop of 5 iterations",
      ),
    );
    expect(withReviewIterations(rewritten, 1)).toBe(edited);
    // A draft whose loop phrase was edited away stays as the person left it.
    expect(withReviewIterations("Just review the docs.", 4)).toBe(
      "Just review the docs.",
    );
  });

  it("seeds each prompt with its header", () => {
    expect(reviewWorkspaceSeed()).toEqual({
      text: reviewWorkspaceChatPrompt(1),
      header: { kind: "review-loop", iterations: 1 },
    });
    expect(verificationSeed()).toEqual({
      text: verificationChatPrompt(),
      header: { kind: "verification" },
    });
    expect(verificationChatPrompt()).toContain(
      "$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md",
    );
  });
});

// ontology_and_harness g5.reverification.workspace
describe("Base re-verification prompt", () => {
  const F = [
    { repoId: "repo-api", rootPath: "api", commit: "f".repeat(40) },
    { repoId: "repo-web", rootPath: "web", commit: "e".repeat(40) },
  ];
  const S = [{ repoId: "repo-api", rootPath: "api", commit: "a".repeat(40) }];
  const run = (overrides: Partial<ReverificationRun>): ReverificationRun => ({
    id: "run-1",
    kind: "incremental",
    state: "in_progress",
    workspaceId: "ws-1",
    workspaceName: "base-reverification-2026-11-06",
    workspaceDeleted: false,
    ownerUserId: "user-1",
    startedBy: "user-1",
    startedAt: "2026-11-06T09:00:00Z",
    completedAt: null,
    label: "Base re-verification 2026-11-06 (incremental)",
    baseHeads: F,
    sinceHeads: S,
    ...overrides,
  });
  const fText = `api@${"f".repeat(40)}, web@${"e".repeat(40)}`;
  const sText = `api@${"a".repeat(40)}`;

  it("names an incremental run's kind, S, F and label, and lists its work", () => {
    const seed = baseReverificationSeed(run({}));
    expect(seed.header).toEqual({ kind: "base-verification" });
    expect(seed.text).toBe(
      [
        `Base re-verification (incremental) for this workspace, which was forked from Base at ${fText}. Re-verify what Base gained from ${sText} to ${fText}. Label every finding from this run with \`Run: Base re-verification 2026-11-06 (incremental)\`.`,
        "First, write this run's scope and the work below into a Project Doc named base-reverification.md, and keep it as your checkpoint and final report. Include the Human decisions section described in `$HOME/.agents/guidance/TLC/model-checking.md`.",
        "Where this prompt and `$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md` differ, this prompt wins: commit and push, read and extend the findings earlier runs recorded, and report product-code bugs instead of fixing them. A reported bug does not by itself keep the run from completing.",
        [
          "Required:",
          `- Follow \`$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md\`, treating Base at ${sText} as the pre-PR product and Base at ${fText} as the current product. Commit and push your progress as you go.`,
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
          `- Re-verifying pull requests merged into Base after ${fText}.`,
        ].join("\n"),
      ].join("\n\n"),
    );
    expect(seed.text).toBe(baseReverificationChatPrompt(run({})));
  });

  it("asks a full run to make every model current at F, with no S", () => {
    const text = baseReverificationChatPrompt(
      run({
        kind: "full",
        sinceHeads: null,
        label: "Base re-verification 2026-11-06 (full)",
      }),
    );
    expect(
      text.startsWith(
        `Base re-verification (full) for this workspace, which was forked from Base at ${fText}. Make every model current with the code at ${fText}, add models for important behaviour that has none, and run them all. Label every finding from this run with \`Run: Base re-verification 2026-11-06 (full)\`.\n\n`,
      ),
    ).toBe(true);
    expect(text).toContain(
      `Required:\n- Reconcile every existing model and its correspondence with the code at ${fText}, using \`$HOME/.agents/guidance/TLC/pr_incremental_TLA_reverification.md\`'s obligation ledger and evidence rules where they apply. Commit and push your progress as you go.\n- Report bugs`,
    );
    expect(text).not.toContain("pre-PR product");
    expect(text).toContain(
      `- Re-verifying pull requests merged into Base after ${fText}.`,
    );
  });

  it("names a repository by its id when it has no root path", () => {
    const text = baseReverificationChatPrompt(
      run({
        kind: "full",
        sinceHeads: null,
        baseHeads: [{ repoId: "repo-solo", rootPath: "", commit: "c0ffee" }],
      }),
    );
    expect(text).toContain("forked from Base at repo-solo@c0ffee.");
  });
});
