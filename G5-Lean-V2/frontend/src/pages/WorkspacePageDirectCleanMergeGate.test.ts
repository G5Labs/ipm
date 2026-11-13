// Regression coverage for the direct-merge modal's submission gate
// (CodeRabbit 2026-08-22): while a direct-clean, workspace-rebase, or
// merge-review submission is in flight, the modal is pinned open — cancel
// no-ops and the preview-attach affordance is withheld. The prior gate missed
// the `merge-review` launch key, letting cancel close the modal (and attach
// open a chat behind it) mid-approval.
import { describe, expect, it } from "vitest";

import { isDirectCleanMergeSubmitting } from "./WorkspacePage";

describe("isDirectCleanMergeSubmitting", () => {
  it.each(["direct-clean-full", "workspace-rebase", "merge-review"])(
    "pins the modal while the %s launch key is held",
    (key) => {
      expect(isDirectCleanMergeSubmitting(new Set([key]), false)).toBe(true);
    },
  );

  it("pins the modal while any merge mutation is loading", () => {
    expect(isDirectCleanMergeSubmitting(new Set(), true)).toBe(true);
  });

  it("releases the modal when nothing is in flight", () => {
    expect(isDirectCleanMergeSubmitting(new Set(), false)).toBe(false);
  });

  it("ignores unrelated launch keys", () => {
    expect(isDirectCleanMergeSubmitting(new Set(["chat-launch"]), false)).toBe(
      false,
    );
  });
});
