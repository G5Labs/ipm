// =============================================================================
// Approval request selection (g5.governance.approval.selection)
// =============================================================================
// The request dialog keeps only mode and checkbox state. Which reviewers a
// requirement still needs comes from the server's requirement evaluator
// (`requestNeeds`); this module maps it to requirement → recipient pairs,
// resolves the one random human the server leaves open, and explains each
// automatic choice and skip. It never derives requirement status or quorum.
// =============================================================================

import type {
  ApprovalRecipientNeedsDTO,
  ApprovalRequestNeedsDTO,
  WorkspaceOutgoingRequirementDTO,
} from "./types";

export type RequestMode = "all" | "agents" | "pick";

/** Recipient id of a requirement's configured agent. */
export const AGENT_RECIPIENT = "agent";

export interface ApprovalRecipient {
  /** `AGENT_RECIPIENT`, or the configured human's user id. */
  id: string;
  label: string;
  kind: "human" | "agent";
  /** An active (pending, advanceable) request already addresses this slot. */
  active: boolean;
}

export interface ApprovalPick {
  definitionId: string;
  recipientId: string;
}

export interface ApprovalSelectionPair {
  requirement: WorkspaceOutgoingRequirementDTO;
  recipient: ApprovalRecipient;
  /** Why this recipient was selected (Selected approvals info). */
  reason: string;
}

export interface ApprovalSelectionSkip {
  requirement: WorkspaceOutgoingRequirementDTO;
  reason: string;
}

export interface ApprovalSelection {
  pairs: ApprovalSelectionPair[];
  skipped: ApprovalSelectionSkip[];
}

export function pickKey(definitionId: string, recipientId: string): string {
  return `${definitionId}|${recipientId}`;
}

export function sortRequirements(
  requirements: WorkspaceOutgoingRequirementDTO[],
): WorkspaceOutgoingRequirementDTO[] {
  return [...requirements].sort((a, b) =>
    a.definition.name.localeCompare(b.definition.name),
  );
}

export function agentRecipientLabel(definitionName: string): string {
  return `${definitionName} agent`;
}

/** Uniform random choice through the platform CSPRNG. */
export function chooseRandom<T>(items: T[]): T | undefined {
  if (items.length === 0) return undefined;
  const values = new Uint32Array(1);
  crypto.getRandomValues(values);
  return items[Math.floor((values[0] / 2 ** 32) * items.length)];
}

function slotActive(
  requirement: WorkspaceOutgoingRequirementDTO,
  kind: "human" | "agent",
  userId: string | null,
): boolean {
  return (requirement.requests ?? []).some(
    (request) =>
      request.state === "pending" &&
      request.actorType === kind &&
      (kind === "agent" || request.actorUserId === userId) &&
      // A pending agent slot whose latest run failed or was cancelled can be
      // requested again explicitly; the server reuses or replaces it.
      !(
        kind === "agent" &&
        ["failed", "cancelled"].includes(
          request.agentReviews?.at(-1)?.state ?? "",
        )
      ),
  );
}

/**
 * Every configured recipient the owner may address for a requirement: its
 * agent, then eligible humans (active members; the owner only when self
 * approval is allowed), alphabetically.
 */
export function requestCandidates(
  requirement: WorkspaceOutgoingRequirementDTO,
  ownerId: string,
): ApprovalRecipient[] {
  const d = requirement.definition;
  const humans = d.humans
    .filter(
      (h) =>
        h.stillMember !== false &&
        !h.deactivatedAt &&
        (d.allowSelfApproval || h.userId !== ownerId),
    )
    .map(
      (h): ApprovalRecipient => ({
        id: h.userId,
        label: h.displayName,
        kind: "human",
        active: slotActive(requirement, "human", h.userId),
      }),
    )
    .sort((a, b) => a.label.localeCompare(b.label));
  return [
    ...(d.agentType === "simple"
      ? [
          {
            id: AGENT_RECIPIENT,
            label: agentRecipientLabel(d.name),
            kind: "agent" as const,
            active: slotActive(requirement, "agent", null),
          },
        ]
      : []),
    ...humans,
  ];
}

/**
 * The server's request needs. An older server omits them; its compatibility
 * stand-in only proposes unaddressed slots (the agent first for Any-of,
 * otherwise the humans) and never claims an agent approval.
 */
export function requestNeedsOf(
  requirement: WorkspaceOutgoingRequirementDTO,
  ownerId: string,
): ApprovalRequestNeedsDTO {
  if (requirement.requestNeeds) return requirement.requestNeeds;
  const candidates = requestCandidates(requirement, ownerId);
  const agent = candidates.find((c) => c.kind === "agent");
  const humans = candidates.filter((c) => c.kind === "human");
  const idle = humans.filter((c) => !c.active).map((c) => c.id);
  const all = requirement.definition.requireAllReviewers !== false;
  const agentRequestable = !!agent && !agent.active;
  const anyActive = candidates.some((c) => c.active);
  const needs: ApprovalRecipientNeedsDTO = all
    ? { agent: agentRequestable, humans: idle, randomHumanFrom: [] }
    : anyActive
      ? { agent: false, humans: [], randomHumanFrom: [] }
      : agent
        ? { agent: agentRequestable, humans: [], randomHumanFrom: [] }
        : { agent: false, humans: [], randomHumanFrom: idle };
  return {
    outstanding: needs,
    repeat: needs,
    agentRequestable,
    agentApproved: false,
  };
}

/** Whether `Request outstanding approvals` belongs on this requirement's title row. */
export function hasOutstandingNeeds(
  requirement: WorkspaceOutgoingRequirementDTO,
  ownerId: string,
): boolean {
  return (
    requirement.state !== "approved" &&
    outstandingPicks(requirement, ownerId, () => undefined).length > 0
  );
}

/**
 * The exact recipients `Request outstanding approvals` prechecks, with the
 * random human resolved once by `choose`.
 */
export function outstandingPicks(
  requirement: WorkspaceOutgoingRequirementDTO,
  ownerId: string,
  choose: (ids: string[]) => string | undefined = chooseRandom,
): ApprovalPick[] {
  if (requirement.state === "approved") return [];
  return resolveNeeds(
    requirement,
    ownerId,
    requestNeedsOf(requirement, ownerId).outstanding,
    choose,
    // The title-row check only asks whether a recipient exists.
    true,
  ).map(({ recipient }) => ({
    definitionId: requirement.definition.id,
    recipientId: recipient.id,
  }));
}

type Resolved = {
  recipient: ApprovalRecipient;
  origin: "agent" | "human" | "random";
};

function resolveNeeds(
  requirement: WorkspaceOutgoingRequirementDTO,
  ownerId: string,
  needs: ApprovalRecipientNeedsDTO,
  choose: (ids: string[]) => string | undefined,
  keepUnchosenRandom = false,
): Resolved[] {
  // Only configured, eligible recipients without an active request are sent.
  const candidates = requestCandidates(requirement, ownerId).filter(
    (c) => !c.active,
  );
  const byId = new Map(candidates.map((c) => [c.id, c]));
  const out: Resolved[] = [];
  const agent = byId.get(AGENT_RECIPIENT);
  if (needs.agent && agent) out.push({ recipient: agent, origin: "agent" });
  for (const id of needs.humans) {
    const human = byId.get(id);
    if (human?.kind === "human")
      out.push({ recipient: human, origin: "human" });
  }
  const pool = needs.randomHumanFrom.filter(
    (id) =>
      byId.get(id)?.kind === "human" && !out.some((r) => r.recipient.id === id),
  );
  if (pool.length > 0) {
    const chosen = choose(pool) ?? (keepUnchosenRandom ? pool[0] : undefined);
    const human = chosen ? byId.get(chosen) : undefined;
    if (human) out.push({ recipient: human, origin: "random" });
  }
  return out;
}

function humanRejected(
  requirement: WorkspaceOutgoingRequirementDTO,
  userId: string,
): boolean {
  const latest = requirement.decisionLog
    .filter(
      (entry) => entry.actorType !== "agent" && entry.actor?.userId === userId,
    )
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
  return latest?.outcome === "rejected";
}

function bulkReason(
  requirement: WorkspaceOutgoingRequirementDTO,
  resolved: Resolved,
  mode: "all" | "agents",
): string {
  const name = resolved.recipient.label;
  const prefix =
    requirement.state === "approved"
      ? "Included because satisfied approval requirements are included. "
      : "";
  if (mode === "agents")
    return `${prefix}${name} is the configured agent and has no active request.`;
  if (requirement.definition.requireAllReviewers !== false)
    return `${prefix}${name} still needs to approve this all-of requirement.`;
  if (resolved.origin === "agent")
    return `${prefix}The configured agent is the first reviewer for this any-of requirement, or a new agent review is needed.`;
  if (resolved.origin === "random")
    return `${prefix}${name} was chosen at random from configured humans because a human review is needed.`;
  return humanRejected(requirement, resolved.recipient.id)
    ? `${prefix}${name} previously rejected and must be requested again.`
    : `${prefix}${name} still needs to be requested for this requirement.`;
}

/**
 * Request all / Request agents only. Approved requirements are skipped unless
 * `includeSatisfied`; random humans are resolved here, once per call.
 * `requestsAllowed` is false only while the request PR rule blocks requests.
 */
export function bulkSelection(
  requirements: WorkspaceOutgoingRequirementDTO[],
  ownerId: string,
  mode: "all" | "agents",
  includeSatisfied: boolean,
  requestsAllowed = true,
  choose: (ids: string[]) => string | undefined = chooseRandom,
): ApprovalSelection {
  const pairs: ApprovalSelectionPair[] = [];
  const skipped: ApprovalSelectionSkip[] = [];
  for (const requirement of sortRequirements(requirements)) {
    const approved = requirement.state === "approved";
    const skip = (reason: string) => skipped.push({ requirement, reason });
    if (approved && !includeSatisfied) {
      skip("already approved");
      continue;
    }
    if (mode === "agents" && requirement.definition.agentType !== "simple") {
      skip("no agent configured");
      continue;
    }
    if (!requestsAllowed) {
      skip("a submitted, open PR is required to request approvals");
      continue;
    }
    const needs = requestNeedsOf(requirement, ownerId);
    const resolved =
      mode === "agents"
        ? needs.agentRequestable && (includeSatisfied || !needs.agentApproved)
          ? resolveNeeds(
              requirement,
              ownerId,
              { agent: true, humans: [], randomHumanFrom: [] },
              choose,
            )
          : []
        : resolveNeeds(
            requirement,
            ownerId,
            approved ? needs.repeat : needs.outstanding,
            choose,
          );
    if (resolved.length === 0) {
      skip("no new reviewer needed or an active request is pending");
      continue;
    }
    for (const entry of resolved)
      pairs.push({
        requirement,
        recipient: entry.recipient,
        reason: bulkReason(requirement, entry, mode),
      });
  }
  return { pairs, skipped };
}

/** Pick approvals and approvers: exactly the checked recipients. */
export function pickSelection(
  requirements: WorkspaceOutgoingRequirementDTO[],
  ownerId: string,
  picked: ReadonlySet<string>,
): ApprovalSelection {
  const pairs: ApprovalSelectionPair[] = [];
  for (const requirement of sortRequirements(requirements))
    for (const recipient of requestCandidates(requirement, ownerId))
      if (
        !recipient.active &&
        picked.has(pickKey(requirement.definition.id, recipient.id))
      )
        pairs.push({
          requirement,
          recipient,
          reason: `${recipient.label} was selected for this request. You can change the checkboxes before submitting.`,
        });
  return { pairs, skipped: [] };
}
