// =============================================================================
// Compact approver roster for one named requirement
// (g5.governance.approval.request-status)
// =============================================================================
// Presentation only: lists the configured approvers who actually received a
// request (or left a retained decision), agent first then humans
// alphabetically, each with its latest request and, beneath an unsettled one,
// its latest earlier settled response. Overall requirement status, currentness
// and the path to approval come from the server; nothing here tallies
// reviewers into a requirement status.
// =============================================================================

import type {
  ApprovalDecisionLogEntryDTO,
  ApprovalPathDTO,
  ApprovalRequestDTO,
  ApprovalRequirementCurrentnessDTO,
  ApprovalRequirementStatus,
  WorkspaceOutgoingRequirementDTO,
} from "./types";

export type ApprovalRosterPrimary =
  | {
      type: "decision";
      decision: ApprovalDecisionLogEntryDTO;
      request: ApprovalRequestDTO | null;
    }
  | { type: "request"; request: ApprovalRequestDTO };

export interface ApprovalRequirementRosterRow {
  /** `agent`, or `user:<id>`. */
  slot: string;
  kind: "agent" | "human";
  userId: string | null;
  name: string;
  primary: ApprovalRosterPrimary;
  /** Latest earlier settled response beneath an unsettled latest request. */
  prior: ApprovalDecisionLogEntryDTO | null;
}

function decisionSlot(entry: ApprovalDecisionLogEntryDTO): string {
  return entry.actorType === "agent"
    ? "agent"
    : `user:${entry.actor?.userId ?? "historical-unknown"}`;
}

function requestSlot(request: ApprovalRequestDTO): string {
  return request.actorType === "agent"
    ? "agent"
    : `user:${request.actorUserId ?? "unknown"}`;
}

const newestFirst = (a: string, b: string) =>
  new Date(b).getTime() - new Date(a).getTime();

export function buildRequirementRoster(
  requirement: WorkspaceOutgoingRequirementDTO,
): ApprovalRequirementRosterRow[] {
  const d = requirement.definition;
  const slots = new Map<
    string,
    {
      requests: ApprovalRequestDTO[];
      decisions: ApprovalDecisionLogEntryDTO[];
    }
  >();
  const slot = (key: string) => {
    let value = slots.get(key);
    if (!value) {
      value = { requests: [], decisions: [] };
      slots.set(key, value);
    }
    return value;
  };
  for (const request of requirement.requests ?? [])
    slot(requestSlot(request)).requests.push(request);
  for (const decision of requirement.decisionLog)
    slot(decisionSlot(decision)).decisions.push(decision);

  const rows: ApprovalRequirementRosterRow[] = [];
  for (const [key, { requests, decisions }] of slots) {
    // Stable sorts keep the server's order among equal timestamps.
    requests.sort((a, b) => newestFirst(a.requestedAt, b.requestedAt));
    decisions.sort((a, b) => newestFirst(a.createdAt, b.createdAt));
    const latestRequest = requests[0] ?? null;
    const latestDecision = decisions[0] ?? null;
    const agent = key === "agent";
    let primary: ApprovalRosterPrimary;
    let prior: ApprovalDecisionLogEntryDTO | null = null;
    if (latestRequest) {
      const decision = decisions.find(
        (entry) => entry.requestId === latestRequest.requestId,
      );
      if (decision)
        primary = { type: "decision", decision, request: latestRequest };
      else {
        primary = { type: "request", request: latestRequest };
        prior = latestDecision;
      }
    } else if (latestDecision) {
      primary = { type: "decision", decision: latestDecision, request: null };
    } else continue;
    const configured = d.humans.find(
      (h) => `user:${h.userId}` === key,
    )?.displayName;
    rows.push({
      slot: key,
      kind: agent ? "agent" : "human",
      userId: agent ? null : key.slice("user:".length),
      name: agent
        ? `${d.name} agent`
        : (latestRequest?.actor?.displayName ??
          latestDecision?.actor?.displayName ??
          configured ??
          "Unknown reviewer"),
      primary,
      prior,
    });
  }
  return rows.sort((a, b) =>
    a.kind !== b.kind
      ? a.kind === "agent"
        ? -1
        : 1
      : a.name.localeCompare(b.name),
  );
}

export type ApprovalBadgeTone = "ok" | "blocked" | "warn" | "neutral";

export function decisionLabel(entry: ApprovalDecisionLogEntryDTO): {
  label: "Approved" | "Rejected" | "Withdrawn";
  tone: ApprovalBadgeTone;
} {
  return entry.outcome === "approved"
    ? { label: "Approved", tone: "ok" }
    : entry.outcome === "rejected"
      ? { label: "Rejected", tone: "blocked" }
      : { label: "Withdrawn", tone: "neutral" };
}

/** Request lifecycle label for a latest request without a decision. */
export function requestLabel(request: ApprovalRequestDTO): {
  label: string;
  tone: ApprovalBadgeTone;
} {
  switch (request.state) {
    case "pending":
      return { label: "Waiting", tone: "warn" };
    case "bypassed":
      return { label: "Bypassed by override", tone: "warn" };
    default:
      return {
        label: request.state.charAt(0).toUpperCase() + request.state.slice(1),
        tone: "neutral",
      };
  }
}

export function statusLabel(state: ApprovalRequirementStatus): {
  label: "Approved" | "Rejected" | "Pending";
  tone: ApprovalBadgeTone;
} {
  return state === "approved"
    ? { label: "Approved", tone: "ok" }
    : state === "rejected"
      ? { label: "Rejected", tone: "blocked" }
      : { label: "Pending", tone: "warn" };
}

/**
 * The requirement badge: its server status, except that an overridable agent
 * rejection still awaiting a human reads Needs Human Escalation
 * (g5.governance.approval.overview). Counts keep reading the status.
 */
export function requirementBadge(
  requirement: WorkspaceOutgoingRequirementDTO,
): {
  label: string;
  tone: ApprovalBadgeTone;
} {
  return requirement.state === "rejected" &&
    requirement.agentEscalation?.state === "needsHumanEscalation"
    ? { label: "Needs Human Escalation", tone: "warn" }
    : statusLabel(requirement.state);
}

/**
 * A roster decision badge. Only the counted agent decision named by the
 * requirement's escalation display is relabelled; reports, Timeline and
 * earlier responses keep the recorded outcome.
 */
export function rosterDecisionLabel(
  entry: ApprovalDecisionLogEntryDTO,
  escalation: WorkspaceOutgoingRequirementDTO["agentEscalation"],
): { label: string; tone: ApprovalBadgeTone } {
  if (
    escalation?.agentReviewId &&
    entry.actorType === "agent" &&
    entry.outcome === "rejected" &&
    entry.agentReviewId === escalation.agentReviewId
  )
    return escalation.state === "needsHumanEscalation"
      ? { label: "Needs Human Escalation", tone: "warn" }
      : { label: "Overridden by Human", tone: "neutral" };
  return decisionLabel(entry);
}

/** Request again belongs only beside the latest settled Rejected/Withdrawn response. */
export function requestAgainTarget(
  row: ApprovalRequirementRosterRow,
): "primary" | "prior" | null {
  const nonApproved = (entry: ApprovalDecisionLogEntryDTO | null) =>
    !!entry && entry.outcome !== "approved";
  if (row.primary.type === "decision")
    return nonApproved(row.primary.decision) ? "primary" : null;
  // A newer waiting request keeps the earlier response without the action;
  // an operationally closed request leaves the earlier response latest.
  if (row.primary.request.state === "pending") return null;
  return nonApproved(row.prior) ? "prior" : null;
}

function joinNames(names: string[], conjunction: "and" | "or"): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} ${conjunction} ${names.at(-1)}`;
}

/** The exact path-to-approval sentence for a server path. */
export function approvalPathText(path: ApprovalPathDTO): string {
  switch (path.kind) {
    case "agentRejected":
      return path.humanOverride
        ? `Rejected by ${path.agentName}. A new agent approval, or a configured human approval with no human rejection, will meet this requirement.`
        : `Rejected by ${path.agentName}. A new agent approval will meet this requirement.`;
    case "humansRejected": {
      const names = joinNames(
        path.humans.map((h) => h.displayName),
        "and",
      );
      const verb = path.humans.length === 1 ? "has" : "have";
      return `Rejected by ${names}. ${names} ${verb} to approve (with no other human rejection) to meet this requirement.${
        path.othersRequired
          ? " Additionally, all other configured humans also have to approve to meet this requirement."
          : ""
      }`;
    }
    case "needed": {
      const names = path.approvers.map((a) => a.displayName);
      const sentences: string[] = [];
      if (names.length > 0)
        sentences.push(
          path.requireAll
            ? `Approval from ${joinNames(names, "and")} will meet this requirement.`
            : `An approval from ${joinNames(names, "or")} will meet this requirement.`,
        );
      if (path.allHumansRequired)
        sentences.push(
          "All configured humans have to approve to meet this requirement.",
        );
      return sentences.join(" ");
    }
  }
}

/** Aggregate currentness popup copy; null when there is no counted evidence. */
export function currentnessMessage(
  currentness: ApprovalRequirementCurrentnessDTO | null | undefined,
): {
  status: "matching" | "changed" | "unknown";
  label: string;
  message: string;
} | null {
  if (!currentness || currentness.state === "none") return null;
  if (currentness.state === "matching")
    return {
      status: "matching",
      label: "All approvals are current",
      message: "All approvals are current.",
    };
  if (currentness.state === "unknown")
    return {
      status: "unknown",
      label: "Source commits unknown",
      message: "Source commits unknown.",
    };
  const names = [...new Set(currentness.notCurrent.map((r) => r.displayName))];
  return {
    status: "changed",
    label: "Some approvals are not current",
    message: `Approvals from ${names.length ? names.join(", ") : "the recorded reviewers"} are not current.`,
  };
}

/**
 * Theme-aware tone for a currentness indicator: neutral blue for matching,
 * amber for changed and gray for unknown, at the aggregate and decision
 * levels alike (g5.governance.approval.evidence). The circled glyph carries
 * the meaning; the colour never suggests a decision.
 */
export function currentnessTone(
  status: "matching" | "changed" | "unknown",
): string {
  return status === "matching"
    ? "bg-[color-mix(in_srgb,var(--g5-info)_12%,transparent)] text-[var(--g5-info)]"
    : status === "changed"
      ? "bg-[color-mix(in_srgb,var(--g5-warn)_14%,transparent)] text-[var(--g5-warn)]"
      : "bg-[color-mix(in_srgb,var(--g5-text2)_12%,transparent)] text-[var(--g5-text2)]";
}

/**
 * The circle drawn around a currentness glyph, in the indicator's tone. The
 * colour is important: index.css gives every element the stroke colour.
 */
export const CURRENTNESS_CIRCLE =
  "inline-flex h-[15px] w-[15px] items-center justify-center rounded-full border-[1.5px] border-current! text-[11px] font-semibold leading-none";

export function currentnessGlyph(
  status: "matching" | "changed" | "unknown",
): string {
  return status === "matching" ? "=" : status === "changed" ? "≠" : "?";
}
