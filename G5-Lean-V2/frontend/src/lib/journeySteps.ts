// =============================================================================
// G5 Client — Guided journey steps (workbench g5.workbench.journey.steps,
// g5.workbench.journey.state)
// =============================================================================
// The fixed catalog of the Guided panel: per phase, ordered steps bound to the
// ids of the ribbon actions that are their buttons, and the evidence rule that
// completes each step without the owner's mark. The catalog is data; the
// panel renders it and hands every click to the ribbon's dispatcher.
//
// A step is done when its latest mark says so, or when the owner of its
// outcome shows the outcome (g5.data.outcome-evidence): the open-questions
// document (`open-questions.md`, or a document under `<docs>/questions/`),
// a document under `<docs>/mocks/`, a workspace explainer, any other Project
// Doc, a pull request from this workspace, a merged one. Question SET files
// are not Project Docs (the docs inventory lists `.md`, `.jsonl` and HTML),
// so the document that references them is the evidence. A started
// conversation or task is activity, not completion.
// =============================================================================

import {
  docIsUnderFolder,
  isWorkspaceExplainerDoc,
  sanitizePathForDisplay,
} from "../components/tree/TreeNode";
import type {
  DocNode,
  JourneyMarkDTO,
  WorkbenchAction,
  WorkbenchActions,
  WorkspacePhase,
  WorkspacePullRequestSummary,
} from "./types";
import { WORKSPACE_PHASES } from "./workbenchDefaults";

/** What completes a step besides the owner's mark. */
export type JourneyEvidence =
  | { kind: "marks-only" }
  | { kind: "docs"; folder: "questions" | "mocks" | "explainers" | "intent" }
  | { kind: "pull-request"; state: "any" | "merged" };

export interface JourneyStep {
  /** Stable id (the marks record keys on it): `<phase>-<slug>`. */
  id: string;
  phase: WorkspacePhase;
  label: string;
  /** One line under the label: what the step means in this workspace. */
  note?: string;
  /** Ribbon action ids of the step's buttons; the first is primary. */
  actions: readonly string[];
  evidence: JourneyEvidence;
}

/** The catalog, in journey order (g5.workbench.journey.steps). */
export const JOURNEY_STEPS: readonly JourneyStep[] = [
  {
    id: "be-intent",
    phase: "be",
    label: "Capture what the business wants",
    note: "A requirement document: write one or upload what you have.",
    actions: ["new-doc", "upload-file"],
    evidence: { kind: "docs", folder: "intent" },
  },
  {
    id: "be-open-questions",
    phase: "be",
    label: "Surface the open questions",
    note: "open-questions.md, with the question sets the team answers in place.",
    actions: ["open-questions"],
    evidence: { kind: "docs", folder: "questions" },
  },
  {
    id: "be-mock",
    phase: "be",
    label: "Show it as a mock",
    note: "Clickable screens under Project Docs, in mocks/.",
    actions: ["build-mock"],
    evidence: { kind: "docs", folder: "mocks" },
  },
  {
    id: "se-plan",
    phase: "se",
    label: "Plan the implementation",
    note: "The design with its ontology changes.",
    actions: ["create-implementation-plan"],
    evidence: { kind: "marks-only" },
  },
  {
    id: "se-explainer",
    phase: "se",
    label: "Explain the design",
    note: "One page a reviewer reads in a minute.",
    actions: ["explainer"],
    evidence: { kind: "docs", folder: "explainers" },
  },
  {
    id: "se-diff-review",
    phase: "se",
    label: "Check what changed",
    note: "The Diff Graph of the ontology changes.",
    actions: ["view-diff-graph"],
    evidence: { kind: "marks-only" },
  },
  {
    id: "se-code",
    phase: "se",
    label: "Build the code",
    note: "Generate the code from the design.",
    actions: ["generate-code"],
    evidence: { kind: "marks-only" },
  },
  {
    id: "se-preview",
    phase: "se",
    label: "See it running",
    note: "A preview of this workspace.",
    actions: ["preview"],
    evidence: { kind: "marks-only" },
  },
  {
    id: "se-code-review",
    phase: "se",
    label: "Get the code reviewed",
    note: "The review reads the change and reports as a conversation.",
    actions: ["code-review"],
    evidence: { kind: "marks-only" },
  },
  {
    id: "se-rebase",
    phase: "se",
    label: "Catch up with the parent",
    note: "Rebase this workspace on its parent before landing.",
    actions: ["rebase"],
    evidence: { kind: "marks-only" },
  },
  {
    id: "ie-pull-request",
    phase: "ie",
    label: "Open the pull request",
    note: "One pull request from this workspace into the parent.",
    actions: ["create-pr"],
    evidence: { kind: "pull-request", state: "any" },
  },
  {
    // Approval state never completes a step (g5.workbench.journey.gates).
    id: "ie-approvals",
    phase: "ie",
    label: "Get it approved",
    note: "Request the reviews the merge needs and fix what they find.",
    actions: ["get-approvals"],
    evidence: { kind: "marks-only" },
  },
  {
    id: "ie-merge",
    phase: "ie",
    label: "Land it",
    note: "Merge the pull request into the parent.",
    actions: ["merge"],
    evidence: { kind: "pull-request", state: "merged" },
  },
];

const STEP_BY_ID: ReadonlyMap<string, JourneyStep> = new Map(
  JOURNEY_STEPS.map((step) => [step.id, step]),
);

export function journeyStepById(stepId: string): JourneyStep | undefined {
  return STEP_BY_ID.get(stepId);
}

export function journeyStepsOf(phase: WorkspacePhase): JourneyStep[] {
  return JOURNEY_STEPS.filter((step) => step.phase === phase);
}

/** The facts a step's evidence rule reads (g5.workbench.journey.state). */
export interface JourneyFacts {
  /** The workspace docs inventory the sidebar already holds. */
  docs: readonly DocNode[];
  /** `WorkspaceDetail.docsFolderPath`; unknown → no document evidence. */
  docsFolderPath: string | null | undefined;
  /** Pull requests this workspace is the SOURCE of, in any state. */
  pullRequests: readonly WorkspacePullRequestSummary[];
  /** Latest mark per step. */
  marks: readonly JourneyMarkDTO[];
}

/**
 * `skipped` is the owner's third mark (ui g5.ui.guided-panel): not done,
 * left out of the phase count, undone like a mark. Evidence still wins: a
 * document or pull request makes a skipped step done.
 */
export type JourneyStepStatus = "todo" | "done" | "skipped";

export interface JourneyStepState {
  status: JourneyStepStatus;
  /** Why it is done: the owner's mark, the outcome, or both. */
  byMark: boolean;
  byEvidence: boolean;
  /** The latest mark, when the step has one (its time and actor). */
  mark: JourneyMarkDTO | null;
  /** The documents that are the step's outcome evidence (chips). */
  evidenceDocs: DocNode[];
}

const EVIDENCE_FOLDERS = ["questions", "mocks"] as const;

function docFilePath(doc: DocNode): string {
  return doc.filePath ?? "";
}

/** The documents of this workspace's Project Docs folder that count for `folder`. */
export function journeyEvidenceDocs(
  folder: Extract<JourneyEvidence, { kind: "docs" }>["folder"],
  docs: readonly DocNode[],
  docsFolderPath: string | null | undefined,
): DocNode[] {
  const workspaceFolder = sanitizePathForDisplay(docsFolderPath ?? null);
  if (!workspaceFolder) return [];
  if (folder === "explainers") {
    return docs.filter((doc) => isWorkspaceExplainerDoc(doc, docsFolderPath));
  }
  if (folder === "questions") {
    return docs.filter((doc) => isQuestionEvidence(doc, workspaceFolder));
  }
  if (folder === "mocks") {
    return docs.filter((doc) =>
      docIsUnderFolder(doc, `${workspaceFolder}/mocks`),
    );
  }
  // intent: any Project Doc of this workspace that is not another step's
  // evidence (questions, mocks, explainers) and not a sync report.
  return docs.filter(
    (doc) =>
      docIsUnderFolder(doc, workspaceFolder) &&
      !isWorkspaceExplainerDoc(doc, docsFolderPath) &&
      !isQuestionEvidence(doc, workspaceFolder) &&
      !docIsUnderFolder(doc, `${workspaceFolder}/sync`) &&
      !EVIDENCE_FOLDERS.some((name) =>
        docIsUnderFolder(doc, `${workspaceFolder}/${name}`),
      ),
  );
}

/** The open-questions document, or anything filed under `questions/`. */
function isQuestionEvidence(doc: DocNode, workspaceFolder: string): boolean {
  return (
    docIsUnderFolder(doc, `${workspaceFolder}/questions`) ||
    (docIsUnderFolder(doc, workspaceFolder) &&
      /(^|\/)open-questions[^/]*\.md$/i.test(docFilePath(doc)))
  );
}

function pullRequestEvidence(
  state: "any" | "merged",
  pullRequests: readonly WorkspacePullRequestSummary[],
): boolean {
  return pullRequests.some((pr) =>
    state === "merged" ? pr.status === "merged" || pr.mergedAt !== null : true,
  );
}

/** Pure derivation of one step's state (g5.workbench.journey.state). */
export function journeyStepState(
  step: JourneyStep,
  facts: JourneyFacts,
): JourneyStepState {
  const mark = facts.marks.find((entry) => entry.stepId === step.id) ?? null;
  const byMark = mark?.done === true;
  const skipped = mark?.skipped === true && !byMark;
  let byEvidence = false;
  let evidenceDocs: DocNode[] = [];
  switch (step.evidence.kind) {
    case "docs":
      evidenceDocs = journeyEvidenceDocs(
        step.evidence.folder,
        facts.docs,
        facts.docsFolderPath,
      );
      byEvidence = evidenceDocs.length > 0;
      break;
    case "pull-request":
      byEvidence = pullRequestEvidence(step.evidence.state, facts.pullRequests);
      break;
    case "marks-only":
      break;
  }
  return {
    status: byMark || byEvidence ? "done" : skipped ? "skipped" : "todo",
    byMark,
    byEvidence,
    mark,
    // Chips are for the three artifact steps only
    // (g5.workbench.journey.artifacts); the intent step shows none.
    evidenceDocs:
      step.evidence.kind === "docs" && step.evidence.folder !== "intent"
        ? evidenceDocs
        : [],
  };
}

export function journeyStates(
  facts: JourneyFacts,
): ReadonlyMap<string, JourneyStepState> {
  return new Map(
    JOURNEY_STEPS.map((step) => [step.id, journeyStepState(step, facts)]),
  );
}

/** What the summary offers (g5.workbench.journey). */
export type JourneyNextAction =
  | { kind: "step"; step: JourneyStep }
  | { kind: "move"; to: WorkspacePhase }
  | { kind: "complete" };

/**
 * The first unfinished step of the active phase (a skipped step is not
 * unfinished); the move to the next phase once every step of the phase is
 * done or skipped; complete after the last phase.
 */
export function journeyNextAction(
  phase: WorkspacePhase,
  states: ReadonlyMap<string, JourneyStepState>,
): JourneyNextAction {
  // The journey is complete once the merge step is done, whatever earlier
  // IE steps the owner never marked (g5.workbench.journey).
  if (phase === "ie" && states.get("ie-merge")?.status === "done") {
    return { kind: "complete" };
  }
  const next = journeyStepsOf(phase).find(
    (step) => (states.get(step.id)?.status ?? "todo") === "todo",
  );
  if (next) return { kind: "step", step: next };
  const index = WORKSPACE_PHASES.indexOf(phase);
  const to = WORKSPACE_PHASES[index + 1];
  return to ? { kind: "move", to } : { kind: "complete" };
}

/** `n of m` done for a phase; a skipped step leaves `m`. */
export function journeyPhaseProgress(
  phase: WorkspacePhase,
  states: ReadonlyMap<string, JourneyStepState>,
): { done: number; total: number } {
  const steps = journeyStepsOf(phase);
  return {
    done: steps.filter((step) => states.get(step.id)?.status === "done").length,
    total: steps.filter((step) => states.get(step.id)?.status !== "skipped")
      .length,
  };
}

/** Find an action by id in a phase's groups, menus included. */
export function findWorkbenchAction(
  groups: WorkbenchActions[WorkspacePhase] | undefined,
  actionId: string,
): WorkbenchAction | undefined {
  for (const group of groups ?? []) {
    for (const action of group.actions) {
      if (action.id === actionId) return action;
      const item = action.items?.find((entry) => entry.id === actionId);
      if (item) return item;
    }
  }
  return undefined;
}
