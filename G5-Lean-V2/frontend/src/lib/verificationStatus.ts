// =============================================================================
// verificationStatus — a statement's verification status from its tag
// =============================================================================
// `g5.documents.ontology.verification.status`: the status comes from the
// statement's one verification tag; Design verified also needs a linked state
// machine with a manifest and a recorded run; a statement shows Design verified
// through a Design verified descendant unless its own status is
// Implementation verification, and never takes its parent's status.
// Freshness and applicability are not judged.
// =============================================================================

import type { VerificationTag } from "./currentOntology";

export type StatusKind = VerificationTag | "unverified" | "checking";

export const STATUS_LABEL: Record<StatusKind, string> = {
  "design-verified": "Design verified",
  "implementation-verified": "Implementation verification",
  "test-coverage": "Test Coverage",
  "manual-check-review": "Manual checking and review",
  unverified: "Unverified",
  checking: "Checking…",
};

/** Unverified and Checking… have no details to open. */
export const hasVerificationDetails = (status: StatusKind) =>
  status !== "unverified" && status !== "checking";

/**
 * What the reader found for one linked state machine. `root` is where its
 * manifest was found, when it was.
 */
export type LinkEvidence =
  | { state: "checking" }
  | { state: "found"; root: string }
  | { state: "missing"; reason: string; root?: string }
  | { state: "unreadable"; reason: string; root?: string };

export interface StatementStatus {
  /** The statement's own status from its tag and, for Design verified, evidence. */
  own: StatusKind;
  /** What its button shows, including Design verified through descendants. */
  shown: StatusKind;
  /** Its verification tags; more than one is a conflict and sets no status. */
  tags: readonly VerificationTag[];
  /** Each linked state machine's evidence, whatever the tag. */
  evidence: readonly LinkEvidence[];
  /** Descendants (record indices) whose own status is Design verified. */
  viaChildren: readonly number[];
}

export function ownStatus(
  tags: readonly VerificationTag[],
  evidence: readonly LinkEvidence[],
): StatusKind {
  if (tags.length !== 1) return "unverified";
  const [tag] = tags;
  if (tag !== "design-verified") return tag;
  if (evidence.some((e) => e.state === "found")) return "design-verified";
  if (evidence.some((e) => e.state === "checking")) return "checking";
  return "unverified";
}

/**
 * Every record's status. `parents` gives each record's parent index or -1;
 * `evidence(i)` lists the evidence for record i's linked state machines, which
 * sets the status only for a design-verified tag.
 */
export function statementStatuses(
  parents: readonly number[],
  tags: readonly (readonly VerificationTag[])[],
  evidence: (index: number) => readonly LinkEvidence[],
): StatementStatus[] {
  const linked = tags.map((_, i) => evidence(i));
  const own = tags.map((list, i) => ownStatus(list, linked[i]));
  const children = parents.map(() => [] as number[]);
  parents.forEach((parent, i) => {
    if (parent >= 0) children[parent].push(i);
  });
  const descendants = (i: number): number[] =>
    children[i].flatMap((child) => [child, ...descendants(child)]);
  return tags.map((list, i) => {
    const below = descendants(i);
    const viaChildren = below.filter((d) => own[d] === "design-verified");
    const pending = below.some((d) => own[d] === "checking");
    let shown = own[i];
    if (shown !== "implementation-verified" && shown !== "design-verified") {
      if (viaChildren.length) shown = "design-verified";
      else if (pending) shown = "checking";
    }
    return {
      own: own[i],
      shown,
      tags: list,
      evidence: linked[i],
      viaChildren,
    };
  });
}
