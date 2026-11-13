// =============================================================================
// journeySteps — step catalog, evidence and state (workbench
// g5.workbench.journey.steps, g5.workbench.journey.state)
// =============================================================================

import { describe, expect, it } from "vitest";

import { WORKBENCH_ACTIONS } from "./workbenchDefaults";
import {
  JOURNEY_STEPS,
  findWorkbenchAction,
  journeyNextAction,
  journeyPhaseProgress,
  journeyStates,
  journeyStepById,
  journeyStepState,
  journeyStepsOf,
  type JourneyFacts,
} from "./journeySteps";
import { createMockDocNode } from "../test/fixtures";
import type { DocNode, WorkspacePullRequestSummary } from "./types";

const DOCS_FOLDER = "docs/ws-1";

function doc(filePath: string, docType = "doc"): DocNode {
  return createMockDocNode({ filePath, docType });
}

function pullRequest(status: "open" | "merged"): WorkspacePullRequestSummary {
  return {
    id: `pr-${status}`,
    status,
    mergedAt: status === "merged" ? "2026-10-02T00:00:00Z" : null,
  } as unknown as WorkspacePullRequestSummary;
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

describe("journey catalog", () => {
  it("binds every step to actions that exist in its phase's built-in ribbon", () => {
    for (const step of JOURNEY_STEPS) {
      expect(step.actions.length).toBeGreaterThan(0);
      for (const actionId of step.actions) {
        expect(
          findWorkbenchAction(WORKBENCH_ACTIONS[step.phase], actionId)?.id,
          `${step.id} → ${actionId}`,
        ).toBe(actionId);
      }
    }
  });

  it("keeps stable, server-shaped ids in phase order", () => {
    const ids = JOURNEY_STEPS.map((step) => step.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(id).toMatch(/^[a-z0-9][a-z0-9-]{0,63}$/);
    expect(journeyStepsOf("be").map((s) => s.id)).toEqual([
      "be-intent",
      "be-open-questions",
      "be-mock",
    ]);
    expect(journeyStepsOf("se").map((s) => s.id)).toEqual([
      "se-plan",
      "se-explainer",
      "se-diff-review",
      "se-code",
      "se-preview",
      "se-code-review",
      "se-rebase",
    ]);
    expect(journeyStepsOf("ie").map((s) => s.id)).toEqual([
      "ie-pull-request",
      "ie-approvals",
      "ie-merge",
    ]);
    expect(journeyStepById("se-diff-review")?.phase).toBe("se");
    expect(journeyStepById("nope")).toBeUndefined();
  });

  it("finds menu items as well as top-level actions", () => {
    expect(findWorkbenchAction(WORKBENCH_ACTIONS.be, "upload-file")?.kind).toBe(
      "open",
    );
    expect(findWorkbenchAction(WORKBENCH_ACTIONS.be, "merge")).toBeUndefined();
  });
});

describe("journeyStepState", () => {
  it("is to-do without a mark or evidence, and never shows chips for the intent step", () => {
    const state = journeyStepState(journeyStepById("be-intent")!, facts());
    expect(state).toEqual({
      status: "todo",
      byMark: false,
      byEvidence: false,
      mark: null,
      evidenceDocs: [],
    });
  });

  it("completes the artifact steps from documents in their evidence folders only", () => {
    const docs = [
      doc(`${DOCS_FOLDER}/requirements.md`),
      doc(`${DOCS_FOLDER}/open-questions.md`),
      doc(`${DOCS_FOLDER}/questions/notes.md`),
      doc(`${DOCS_FOLDER}/mocks/home.html`),
      doc(`${DOCS_FOLDER}/explainers/why.html`, "explainer"),
      doc(`docs/other-ws/mocks/theirs.html`),
      doc(`${DOCS_FOLDER}/sync/report.md`),
    ];
    const states = journeyStates(facts({ docs }));
    const questions = states.get("be-open-questions")!;
    expect(questions.status).toBe("done");
    expect(questions.evidenceDocs.map((d) => d.name).sort()).toEqual([
      "notes.md",
      "open-questions.md",
    ]);
    const mock = states.get("be-mock")!;
    expect(mock.evidenceDocs.map((d) => d.name)).toEqual(["home.html"]);
    const explainer = states.get("se-explainer")!;
    expect(explainer.evidenceDocs.map((d) => d.name)).toEqual(["why.html"]);
    // The intent step counts requirements.md but not the evidence folders
    // or sync reports, and shows no chips.
    const intent = states.get("be-intent")!;
    expect(intent.status).toBe("done");
    expect(intent.evidenceDocs).toEqual([]);
    expect(
      journeyStepState(
        journeyStepById("be-intent")!,
        facts({
          docs: [
            doc(`${DOCS_FOLDER}/mocks/home.html`),
            doc(`${DOCS_FOLDER}/sync/report.md`),
          ],
        }),
      ).status,
    ).toBe("todo");
  });

  it("does not let the open-questions document complete the intent step", () => {
    const states = journeyStates(
      facts({ docs: [doc(`${DOCS_FOLDER}/open-questions.md`)] }),
    );
    expect(states.get("be-open-questions")?.status).toBe("done");
    expect(states.get("be-intent")?.status).toBe("todo");
  });

  it("reads no document evidence without a known docs folder", () => {
    const docs = [doc(`${DOCS_FOLDER}/mocks/home.html`)];
    expect(
      journeyStepState(
        journeyStepById("be-mock")!,
        facts({ docs, docsFolderPath: null }),
      ).status,
    ).toBe("todo");
  });

  it("completes the pull-request steps from the workspace's pull requests", () => {
    const open = journeyStates(facts({ pullRequests: [pullRequest("open")] }));
    expect(open.get("ie-pull-request")?.status).toBe("done");
    expect(open.get("ie-merge")?.status).toBe("todo");
    const merged = journeyStates(
      facts({ pullRequests: [pullRequest("merged")] }),
    );
    expect(merged.get("ie-merge")?.status).toBe("done");
    // Approval state never completes the approvals step; only a mark does
    // (g5.workbench.journey.gates), and the merge still completes the journey.
    expect(merged.get("ie-approvals")?.status).toBe("todo");
    expect(journeyNextAction("ie", merged)).toEqual({ kind: "complete" });
    expect(journeyNextAction("ie", open)).toEqual({
      kind: "step",
      step: journeyStepById("ie-approvals"),
    });
  });

  it("lets a mark settle a step and never lets a mark hide evidence", () => {
    const marked = journeyStepState(
      journeyStepById("se-diff-review")!,
      facts({
        marks: [
          {
            stepId: "se-diff-review",
            done: true,
            markedAt: "2026-10-03T00:00:00Z",
          },
        ],
      }),
    );
    expect(marked).toMatchObject({
      status: "done",
      byMark: true,
      byEvidence: false,
    });
    const unmarkedWithEvidence = journeyStepState(
      journeyStepById("be-mock")!,
      facts({
        docs: [doc(`${DOCS_FOLDER}/mocks/home.html`)],
        marks: [
          { stepId: "be-mock", done: false, markedAt: "2026-10-03T00:00:00Z" },
        ],
      }),
    );
    expect(unmarkedWithEvidence.status).toBe("done");
    expect(unmarkedWithEvidence.byEvidence).toBe(true);
  });
});

describe("journeyNextAction", () => {
  it("names the first unfinished step of the phase, then the move, then completion", () => {
    const none = journeyStates(facts());
    expect(journeyNextAction("be", none)).toEqual({
      kind: "step",
      step: journeyStepById("be-intent"),
    });
    const beDone = journeyStates(
      facts({
        docs: [
          doc(`${DOCS_FOLDER}/requirements.md`),
          doc(`${DOCS_FOLDER}/mocks/home.html`),
        ],
        marks: [
          {
            stepId: "be-open-questions",
            done: true,
            markedAt: "2026-10-03T00:00:00Z",
          },
        ],
      }),
    );
    expect(journeyNextAction("be", beDone)).toEqual({ kind: "move", to: "se" });
    expect(journeyPhaseProgress("be", beDone)).toEqual({ done: 3, total: 3 });
    // Skipped earlier steps stay unfinished: the first one is still next.
    const laterOnly = journeyStates(
      facts({ marks: [{ stepId: "be-mock", done: true, markedAt: "x" }] }),
    );
    expect(journeyNextAction("be", laterOnly)).toMatchObject({
      kind: "step",
      step: { id: "be-intent" },
    });
    // A merged pull request completes the journey even when the owner never
    // marked the rebase step; the step itself stays unmarked.
    const merged = journeyStates(
      facts({ pullRequests: [pullRequest("merged")] }),
    );
    expect(journeyNextAction("ie", merged)).toEqual({ kind: "complete" });
    expect(merged.get("se-rebase")?.status).toBe("todo");
  });
});

describe("skip marks (g5.ui.guided-panel)", () => {
  it("reads a skip as skipped, leaves it out of the count and the next step, and lets evidence win", () => {
    const skipped = journeyStepState(
      journeyStepById("se-explainer")!,
      facts({
        marks: [
          {
            stepId: "se-explainer",
            done: false,
            skipped: true,
            markedAt: "2026-10-03T00:00:00Z",
          },
        ],
      }),
    );
    expect(skipped.status).toBe("skipped");
    expect(skipped.byMark).toBe(false);
    const states = journeyStates(
      facts({
        marks: [
          {
            stepId: "se-plan",
            done: false,
            skipped: true,
            markedAt: "2026-10-03T00:00:00Z",
          },
        ],
      }),
    );
    expect(journeyNextAction("se", states)).toEqual({
      kind: "step",
      step: journeyStepById("se-explainer"),
    });
    expect(journeyPhaseProgress("se", states)).toEqual({
      done: 0,
      total: journeyStepsOf("se").length - 1,
    });
    // A document proves the step: skipped or not, it is done.
    const proven = journeyStepState(
      journeyStepById("se-explainer")!,
      facts({
        docs: [doc(`${DOCS_FOLDER}/explainers/why.html`, "explainer")],
        marks: [
          {
            stepId: "se-explainer",
            done: false,
            skipped: true,
            markedAt: "2026-10-03T00:00:00Z",
          },
        ],
      }),
    );
    expect(proven.status).toBe("done");
    expect(proven.byEvidence).toBe(true);
  });
});
