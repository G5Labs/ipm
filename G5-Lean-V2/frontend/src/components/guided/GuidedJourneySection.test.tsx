// =============================================================================
// GuidedJourneySection — the Guided panel (ui g5.ui.guided-panel; workbench
// g5.workbench.journey, .journey.steps, .journey.state, .journey.gates,
// .journey.artifacts; ui g5.ui.journey-evidence)
// =============================================================================

import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import type { JourneyFacts } from "../../lib/journeySteps";
import type {
  DocNode,
  JourneyMarkDTO,
  WorkbenchAction,
  WorkspacePullRequestSummary,
} from "../../lib/types";
import { WORKBENCH_ACTIONS } from "../../lib/workbenchDefaults";
import {
  approvalDefinition,
  approvalRequirement,
  approvalsState,
  approver,
} from "../../test/approvalFixtures";
import { createMockDocNode } from "../../test/fixtures";
import { renderApp } from "../../test/render";
import { GuidedJourneySection, relativeTime } from "./GuidedJourneySection";

const DOCS_FOLDER = "docs/ws-1";

function doc(filePath: string, docType = "doc"): DocNode {
  return createMockDocNode({ filePath, docType });
}

function facts(overrides: Partial<JourneyFacts> = {}): JourneyFacts {
  return {
    docs: [],
    docsFolderPath: DOCS_FOLDER,
    pullRequests: [],
    marks: [],
    ...overrides,
  };
}

function mark(
  stepId: string,
  overrides: Partial<JourneyMarkDTO> = {},
): JourneyMarkDTO {
  return {
    stepId,
    done: true,
    skipped: false,
    markedAt: new Date().toISOString(),
    ...overrides,
  };
}

const beRequirement = (state: "approved" | "pending") =>
  approvalRequirement({
    definition: approvalDefinition({
      id: "def-ux",
      name: "UX sign-off",
      phases: ["be"],
    }),
    phase: "be",
    state,
    approvers: [approver(state)],
  });

function renderSection(
  overrides: Partial<React.ComponentProps<typeof GuidedJourneySection>> = {},
) {
  const onAction = vi.fn<(action: WorkbenchAction) => void>();
  const onMark = vi.fn();
  const onMovePhase = vi.fn();
  const onOpenDoc = vi.fn();
  const onReviewApprovals = vi.fn();
  const { unmount } = renderApp(
    <GuidedJourneySection
      phase="be"
      actions={WORKBENCH_ACTIONS}
      configStatus="builtin"
      onAction={onAction}
      facts={facts()}
      marksStatus="ok"
      approvals={null}
      onMark={onMark}
      onMovePhase={onMovePhase}
      onOpenDoc={onOpenDoc}
      onReviewApprovals={onReviewApprovals}
      {...overrides}
    />,
    { route: "/org-g5labs/test-workspace" },
  );
  return {
    onAction,
    onMark,
    onMovePhase,
    onOpenDoc,
    onReviewApprovals,
    unmount,
  };
}

describe("GuidedJourneySection", () => {
  it("keeps approval requirements out of the phase journey", () => {
    renderSection({ approvals: approvalsState([beRequirement("pending")]) });
    for (const phase of ["be", "se", "ie"]) {
      expect(screen.queryByTestId(`guided-gate-${phase}`)).toBeNull();
      expect(screen.getByTestId(`guided-phase-${phase}`)).toBeVisible();
    }
  });

  it("names the next step in the summary, opens only that row, folds the other rows and phases", async () => {
    const user = userEvent.setup();
    renderSection();
    expect(
      screen.getByRole("navigation", { name: "Guided journey" }),
    ).toBeInTheDocument();
    // The summary is the next step's outcome and the phase's count, no label.
    const summary = screen.getByTestId("guided-summary");
    expect(summary).toHaveTextContent("Capture what the business wants");
    expect(summary).toHaveTextContent("Business Engineering, 0 of 3 done.");
    expect(summary).not.toHaveTextContent("Journey summary");
    expect(
      within(summary).getByTestId("guided-summary-action-new-doc"),
    ).toBeEnabled();
    // The active phase lists every step; only the next one carries buttons.
    const be = screen.getByTestId("guided-phase-be");
    expect(be).toHaveAttribute("data-relation", "active");
    const intent = within(be).getByTestId("guided-step-be-intent");
    expect(intent).toHaveAttribute("aria-current", "step");
    expect(intent).toHaveTextContent("Next");
    expect(
      within(intent).getByTestId("guided-step-be-intent-action-new-doc"),
    ).toBeEnabled();
    expect(
      within(intent).getByTestId("guided-step-be-intent-action-upload-file"),
    ).toBeEnabled();
    const mock = within(be).getByTestId("guided-step-be-mock");
    expect(
      within(mock).queryByTestId("guided-step-be-mock-action-build-mock"),
    ).toBeNull();
    // A folded row opens on its label.
    const toggle = within(mock).getByTestId("guided-step-be-mock-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(
      within(mock).getByTestId("guided-step-be-mock-action-build-mock"),
    ).toBeEnabled();
    expect(mock).toHaveTextContent("Clickable screens under Project Docs");
    // Later phases fold behind their heading and count; expanded, their
    // rows have no controls.
    const se = screen.getByTestId("guided-phase-se");
    expect(se).toHaveAttribute("data-relation", "upcoming");
    expect(se).toHaveTextContent("0 of 7");
    expect(within(se).queryByTestId("guided-step-se-plan")).toBeNull();
    await user.click(within(se).getByTestId("guided-phase-se-toggle"));
    expect(within(se).getByTestId("guided-step-se-plan")).toBeInTheDocument();
    expect(
      within(se).queryByTestId(
        "guided-step-se-plan-action-create-implementation-plan",
      ),
    ).toBeNull();
    expect(within(se).queryByTestId("guided-step-se-plan-mark")).toBeNull();
  });

  it("dispatches a step button to the ribbon's handler and applies the page's gates", async () => {
    const user = userEvent.setup();
    const { onAction } = renderSection({
      actionState: (action) =>
        action.id === "build-mock"
          ? { disabled: true, title: "Chat needs a writable workspace you own" }
          : undefined,
    });
    await user.click(
      screen.getByTestId("guided-step-be-intent-action-new-doc"),
    );
    expect(onAction).toHaveBeenCalledWith(
      expect.objectContaining({ id: "new-doc", kind: "open" }),
    );
    await user.click(screen.getByTestId("guided-step-be-mock-toggle"));
    const buildMock = screen.getByTestId(
      "guided-step-be-mock-action-build-mock",
    );
    expect(buildMock).toBeDisabled();
    expect(buildMock).toHaveAttribute(
      "title",
      "Chat needs a writable workspace you own",
    );
  });

  it("disables a step whose action left the phase's ribbon and keeps the rest", async () => {
    const user = userEvent.setup();
    renderSection({
      actions: {
        ...WORKBENCH_ACTIONS,
        be: WORKBENCH_ACTIONS.be.filter((group) => group.name !== "Mock"),
      },
    });
    await user.click(screen.getByTestId("guided-step-be-mock-toggle"));
    const button = screen.getByTestId("guided-step-be-mock-action-build-mock");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      '"build-mock" is not in this phase\'s ribbon',
    );
    expect(
      screen.getByTestId("guided-step-be-intent-action-new-doc"),
    ).toBeEnabled();
  });

  it("marks through the ring, undoes a fresh mark, skips, and never undoes evidence", async () => {
    const user = userEvent.setup();
    const { onMark } = renderSection({
      phase: "se",
      facts: facts({
        docs: [doc(`${DOCS_FOLDER}/explainers/why.html`, "explainer")],
        marks: [
          mark("se-plan"),
          mark("se-code", { done: false, skipped: true }),
        ],
      }),
    });
    // A marked step: pressed ring, "Done", the Undo link while fresh.
    const plan = screen.getByTestId("guided-step-se-plan");
    expect(plan).toHaveAttribute("data-state", "done");
    expect(plan).toHaveTextContent("Done");
    expect(plan).toHaveTextContent("Marked done just now");
    const planRing = within(plan).getByTestId("guided-step-se-plan-mark");
    expect(planRing).toHaveAttribute("aria-pressed", "true");
    expect(planRing).toHaveAccessibleName(
      "Marked done: Plan the implementation",
    );
    await user.click(within(plan).getByTestId("guided-step-se-plan-undo"));
    expect(onMark).toHaveBeenCalledWith("se-plan", {
      done: false,
      skipped: false,
    });
    await user.click(planRing);
    expect(onMark).toHaveBeenLastCalledWith("se-plan", {
      done: false,
      skipped: false,
    });

    // A to-do row's ring marks it; its body offers Mark done and Skip.
    const review = screen.getByTestId("guided-step-se-diff-review");
    expect(review).toHaveAttribute("data-state", "todo");
    const reviewRing = within(review).getByTestId(
      "guided-step-se-diff-review-mark",
    );
    expect(reviewRing).toHaveAttribute("aria-pressed", "false");
    await user.click(reviewRing);
    expect(onMark).toHaveBeenLastCalledWith("se-diff-review", {
      done: true,
      skipped: false,
    });
    // The next row is open already: no toggle, its text buttons visible.
    expect(
      within(review).queryByTestId("guided-step-se-diff-review-toggle"),
    ).toBeNull();
    await user.click(
      within(review).getByTestId("guided-step-se-diff-review-skip"),
    );
    expect(onMark).toHaveBeenLastCalledWith("se-diff-review", {
      done: false,
      skipped: true,
    });

    // A skipped step reads Skipped, leaves the count, and undoes.
    const code = screen.getByTestId("guided-step-se-code");
    expect(code).toHaveAttribute("data-state", "skipped");
    expect(code).toHaveTextContent("Skipped, not counted");
    expect(screen.getByTestId("guided-phase-se")).toHaveTextContent("2 of 6");
    await user.click(within(code).getByTestId("guided-step-se-code-undo"));
    expect(onMark).toHaveBeenLastCalledWith("se-code", {
      done: false,
      skipped: false,
    });

    // The explainer step is done by its document: no control at all.
    const explainer = screen.getByTestId("guided-step-se-explainer");
    expect(explainer).toHaveAttribute("data-state", "done");
    expect(
      within(explainer).queryByTestId("guided-step-se-explainer-mark"),
    ).toBeNull();
    expect(
      within(explainer).queryByTestId("guided-step-se-explainer-undo"),
    ).toBeNull();

    // The summary's Skip skips the next step.
    await user.click(screen.getByTestId("guided-summary-skip"));
    expect(onMark).toHaveBeenLastCalledWith("se-diff-review", {
      done: false,
      skipped: true,
    });
  });

  it("gives a reader no controls and names who runs the step; a blocked owner keeps the reason", () => {
    const { unmount } = renderSection({
      isOwner: false,
      ownerDisplayName: "Alex Chen",
      onMark: undefined,
      onMovePhase: undefined,
    });
    const intent = screen.getByTestId("guided-step-be-intent");
    expect(
      within(intent).queryByTestId("guided-step-be-intent-mark"),
    ).toBeNull();
    expect(
      within(intent).queryByTestId("guided-step-be-intent-action-new-doc"),
    ).toBeNull();
    expect(
      within(intent).getByTestId("guided-step-be-intent-owner"),
    ).toHaveTextContent("Alex Chen runs this step");
    expect(screen.getByTestId("guided-summary")).toHaveTextContent(
      "Alex Chen owns this workspace.",
    );
    expect(screen.queryByTestId("guided-summary-action-new-doc")).toBeNull();
    unmount();
    // The owner of a read-only workspace keeps the controls, disabled, with
    // the reason in words.
    renderSection({
      onMark: undefined,
      markBlockedReason: "This workspace is read-only",
    });
    const ring = screen.getByTestId("guided-step-be-intent-mark");
    expect(ring).toBeDisabled();
    expect(ring).toHaveAttribute("title", "This workspace is read-only");
    expect(screen.getByTestId("guided-step-be-intent-skip")).toBeDisabled();
  });

  it("renders evidence documents as citation chips that open the document, in a folded phase too", async () => {
    const user = userEvent.setup();
    const mock = doc(`${DOCS_FOLDER}/mocks/home.html`);
    const { onOpenDoc } = renderSection({
      phase: "se",
      facts: facts({ docs: [mock, doc(`${DOCS_FOLDER}/requirements.md`)] }),
    });
    const be = screen.getByTestId("guided-phase-be");
    expect(be).toHaveAttribute("data-relation", "done-phase");
    expect(be).toHaveTextContent("2 of 3");
    await user.click(within(be).getByTestId("guided-phase-be-toggle"));
    const chips = within(be).getByTestId("guided-step-be-mock-artifacts");
    const chip = within(chips).getByTestId("chat-message-citation");
    expect(chip).toHaveTextContent("home.html");
    await user.click(chip);
    expect(onOpenDoc).toHaveBeenCalledWith(mock);
    // The intent step is done by requirements.md but never shows chips.
    expect(
      within(be).queryByTestId("guided-step-be-intent-artifacts"),
    ).toBeNull();
    expect(within(be).getByTestId("guided-step-be-intent")).toHaveAttribute(
      "data-state",
      "done",
    );
  });

  it("allows the owner phase move with pending approval requirements", async () => {
    const user = userEvent.setup();
    const { onMovePhase } = renderSection({
      approvals: approvalsState([beRequirement("pending")]),
      facts: facts({
        docs: [
          doc(`${DOCS_FOLDER}/requirements.md`),
          doc(`${DOCS_FOLDER}/mocks/home.html`),
        ],
        marks: [mark("be-open-questions", { done: false, skipped: true })],
      }),
    });
    const summary = screen.getByTestId("guided-summary");
    expect(summary).toHaveTextContent("Every step is done");
    expect(summary).toHaveTextContent(
      "Business Engineering, 2 of 2 done. Next: move to System Engineering.",
    );
    await user.click(screen.getByTestId("guided-summary-move"));
    expect(onMovePhase).toHaveBeenCalledWith("se");
  });

  it("keeps the move disabled for a reader", () => {
    renderSection({
      facts: facts({
        docs: [
          doc(`${DOCS_FOLDER}/requirements.md`),
          doc(`${DOCS_FOLDER}/mocks/home.html`),
        ],
        marks: [mark("be-open-questions")],
      }),
      onMovePhase: undefined,
    });
    expect(screen.getByTestId("guided-summary-move")).toBeDisabled();
  });

  it("reads complete once the merge step is done", () => {
    renderSection({
      phase: "ie",
      facts: facts({
        pullRequests: [
          {
            id: "pr-1",
            status: "merged",
            mergedAt: "2026-10-02T00:00:00Z",
          } as unknown as WorkspacePullRequestSummary,
        ],
      }),
    });
    const summary = screen.getByTestId("guided-summary");
    expect(summary).toHaveTextContent("Journey complete");
    expect(summary).toHaveTextContent("Merged to the parent.");
    expect(screen.queryByTestId("guided-summary-move")).toBeNull();
  });

  it("reports unavailable marks and pull requests in the summary with Retry, and their steps as Unknown", async () => {
    const user = userEvent.setup();
    const onRetryMarks = vi.fn();
    const onRetryPullRequests = vi.fn();
    const { unmount } = renderSection({
      phase: "ie",
      pullRequestsStatus: "error",
      onRetryPullRequests,
    });
    const prError = screen.getByTestId("guided-pull-requests-error");
    expect(prError).toHaveTextContent("Couldn’t load pull requests.");
    await user.click(within(prError).getByRole("button", { name: "Retry" }));
    expect(onRetryPullRequests).toHaveBeenCalled();
    const pullRequest = screen.getByTestId("guided-step-ie-pull-request");
    expect(pullRequest).toHaveAttribute("data-kind", "unknown");
    expect(pullRequest).toHaveTextContent("Unknown");
    expect(pullRequest).not.toHaveTextContent("Next");
    unmount();
    renderSection({ phase: "se", marksStatus: "error", onRetryMarks });
    const marksError = screen.getByTestId("guided-marks-error");
    expect(marksError).toHaveTextContent("Couldn’t load your marks.");
    await user.click(within(marksError).getByRole("button", { name: "Retry" }));
    expect(onRetryMarks).toHaveBeenCalled();
    // A marks-only step reads Unknown; a document-proven step is still to do
    // (its evidence is known to be absent).
    expect(screen.getByTestId("guided-step-se-plan")).toHaveAttribute(
      "data-kind",
      "unknown",
    );
    expect(screen.getByTestId("guided-step-se-explainer")).toHaveAttribute(
      "data-kind",
      "todo",
    );
  });

  it("shows a skeleton while the marks load, never steps that could read as to do", () => {
    renderSection({ marksStatus: "loading" });
    const journey = screen.getByTestId("guided-journey");
    expect(journey).toHaveAttribute("data-loading", "true");
    expect(journey).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByTestId("guided-step-be-intent")).toBeNull();
  });

  it("reads a step as Running while the page knows a session for its action, and keeps it a to-do", () => {
    renderSection({
      runningActionIds: new Set(["open-questions"]),
    });
    const questions = screen.getByTestId("guided-step-be-open-questions");
    expect(questions).toHaveAttribute("data-kind", "running");
    expect(questions).toHaveAttribute("data-state", "todo");
    expect(questions).toHaveAttribute("aria-busy", "true");
    expect(questions).toHaveTextContent("Running");
    expect(questions).toHaveTextContent("Find gaps is running");
    expect(
      within(questions).queryByTestId("guided-step-be-open-questions-mark"),
    ).toBeNull();
    // The dispatch in flight reads the same way.
    expect(screen.getByTestId("guided-step-be-intent")).toHaveAttribute(
      "data-kind",
      "next",
    );
  });

  it("keeps a failed mark on its row as an alert with Retry, and says Saving while one is in flight", async () => {
    const user = userEvent.setup();
    const { onMark, unmount } = renderSection({
      markFailure: {
        stepId: "be-intent",
        mark: { done: true, skipped: false },
        message: "boom",
      },
    });
    const intent = screen.getByTestId("guided-step-be-intent");
    expect(intent).toHaveTextContent("Not saved");
    const error = within(intent).getByRole("alert");
    expect(error).toHaveTextContent("Couldn’t save your mark.");
    await user.click(within(intent).getByTestId("guided-step-be-intent-retry"));
    expect(onMark).toHaveBeenCalledWith("be-intent", {
      done: true,
      skipped: false,
    });
    expect(screen.getByTestId("guided-live")).toHaveTextContent(
      "Couldn’t save your mark for Capture what the business wants. Retry.",
    );
    unmount();
    renderSection({ markPendingStepId: "be-intent" });
    const pending = screen.getByTestId("guided-step-be-intent");
    expect(pending).toHaveTextContent("Saving");
    expect(pending).toHaveAttribute("aria-busy", "true");
    expect(
      within(pending).queryByTestId("guided-step-be-intent-mark"),
    ).toBeNull();
  });

  it("announces a state change in the live region and moves between rows with the arrow keys", async () => {
    const user = userEvent.setup();
    function Harness() {
      const [marks, setMarks] = useState<JourneyMarkDTO[]>([]);
      return (
        <>
          <button
            type="button"
            data-testid="harness-mark"
            onClick={() => setMarks([mark("be-intent")])}
          >
            mark
          </button>
          <GuidedJourneySection
            phase="be"
            actions={WORKBENCH_ACTIONS}
            configStatus="builtin"
            onAction={vi.fn()}
            facts={facts({ marks })}
            marksStatus="ok"
            approvals={null}
            onMark={vi.fn()}
            onOpenDoc={vi.fn()}
          />
        </>
      );
    }
    renderApp(<Harness />, { route: "/org-g5labs/test-workspace" });
    expect(screen.getByTestId("guided-live")).toHaveTextContent("");
    // ↓ from the next step's ring reaches the following row's control.
    const intentRing = screen.getByTestId("guided-step-be-intent-mark");
    expect(intentRing).toHaveAttribute("tabindex", "0");
    const questionsRing = screen.getByTestId(
      "guided-step-be-open-questions-mark",
    );
    expect(questionsRing).toHaveAttribute("tabindex", "-1");
    act(() => intentRing.focus());
    await user.keyboard("{ArrowDown}");
    expect(questionsRing).toHaveFocus();
    await user.click(screen.getByTestId("harness-mark"));
    expect(screen.getByTestId("guided-live")).toHaveTextContent(
      "Capture what the business wants marked done.",
    );
    expect(screen.getByTestId("guided-step-be-open-questions")).toHaveAttribute(
      "aria-current",
      "step",
    );
  });

  it("replaces only an enabled action with a custom control and keeps a disabled one as its button", async () => {
    const user = userEvent.setup();
    const renderAction = vi.fn(
      (action: WorkbenchAction, _shape: unknown, idPrefix?: string) =>
        action.id === "build-mock" ? (
          <span data-testid={`${idPrefix}-custom`}>custom</span>
        ) : undefined,
    );
    renderSection({
      renderAction,
      actionState: (action) =>
        action.id === "build-mock"
          ? { disabled: true, title: "Documents need a writable workspace" }
          : undefined,
    });
    await user.click(screen.getByTestId("guided-step-be-mock-toggle"));
    // Disabled: the ribbon-shaped button with its reason, no custom control.
    const build = screen.getByTestId("guided-step-be-mock-action-build-mock");
    expect(build).toBeDisabled();
    expect(build).toHaveAttribute(
      "title",
      "Documents need a writable workspace",
    );
    expect(renderAction).not.toHaveBeenCalledWith(
      expect.objectContaining({ id: "build-mock" }),
      expect.anything(),
      expect.anything(),
    );
  });
});

describe("relativeTime", () => {
  it("rounds to the unit a person would say", () => {
    const now = Date.parse("2026-10-20T12:00:00Z");
    expect(relativeTime("2026-10-20T11:59:40Z", now)).toBe("just now");
    expect(relativeTime("2026-10-20T11:57:00Z", now)).toBe("3 min ago");
    expect(relativeTime("2026-10-20T09:00:00Z", now)).toBe("3 h ago");
    expect(relativeTime("not a date", now)).toBe("");
  });
});
