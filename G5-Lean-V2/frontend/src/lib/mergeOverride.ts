// Merge with override (g5.workspace.merge.admin-override.formal,
// g5.git.provider-override): presentation helpers shared by the override
// dialog, Prepare to Ship and the pull-request list. Who may override is the
// server's `authority` alone; nothing here reads organization roles, so a
// change to who may override stays server-only.

import type { MergeOverridePreview, MergeOverrideRecordDTO } from "./types";

/** The confirmation a person ticks before merging with override. */
export const MERGE_OVERRIDE_CONFIRMATION =
  "I understand the requirements and rules listed above will be waived for this merge, and this override will be recorded with my name and reason.";

/** Which groups of blockers the preview carries. */
export function mergeOverrideGroups(preview: MergeOverridePreview): {
  g5: boolean;
  github: boolean;
} {
  return {
    g5:
      preview.blockers.approvals.length > 0 ||
      preview.blockers.checks.length > 0,
    github: preview.blockers.github.length > 0,
  };
}

export type MergeOverrideOffer =
  | { kind: "none" }
  | { kind: "offered" }
  /** Blockers exist but the server denies a group the caller would need. */
  | { kind: "denied"; reason: string | null };

/**
 * Whether to offer Merge with override: something overridable blocks and the
 * server grants the caller every group present.
 */
export function mergeOverrideOffer(
  preview: MergeOverridePreview | null | undefined,
): MergeOverrideOffer {
  if (!preview) return { kind: "none" };
  const groups = mergeOverrideGroups(preview);
  if (!groups.g5 && !groups.github) return { kind: "none" };
  const denied = [
    groups.g5 && !preview.authority.g5.allowed ? preview.authority.g5 : null,
    groups.github && !preview.authority.github.allowed
      ? preview.authority.github
      : null,
  ].filter((entry) => entry !== null);
  if (denied.length > 0) {
    const reasons = denied
      .map((entry) => entry.reason?.trim())
      .filter((reason): reason is string => !!reason);
    return { kind: "denied", reason: reasons.join(" ") || null };
  }
  return { kind: "offered" };
}

/** "Waiting" / "Rejected" for an unmet approval requirement's state. */
export function mergeOverrideApprovalStateLabel(state: string): string {
  switch (state) {
    case "pending":
      return "Waiting";
    case "rejected":
      return "Rejected";
    case "not_requested":
      return "Not requested";
    case "no_eligible_reviewer":
      return "No eligible reviewer";
    default:
      return state.replace(/_/g, " ");
  }
}

/** A blocking check's bucket, as a person reads it. */
export function mergeOverrideCheckBucketLabel(bucket: string): string {
  switch (bucket) {
    case "fail":
      return "Failing";
    case "cancel":
      return "Cancelled";
    case "pending":
      return "Pending";
    default:
      return bucket;
  }
}

/** What a recorded override waived: G5's requirements, GitHub's rules or both. */
export function mergeOverrideWaivedKinds(
  record: MergeOverrideRecordDTO,
): string {
  const g5 =
    (record.waivedApprovals?.length ?? 0) > 0 ||
    (record.waivedChecks?.length ?? 0) > 0;
  const github = (record.githubRepositories?.length ?? 0) > 0;
  if (g5 && github) return "G5 requirements and GitHub rules waived";
  if (github) return "GitHub rules waived";
  if (g5) return "G5 requirements waived";
  return "";
}

/** "Merged with override by X: reason" (+ "(self-override)"). */
export function mergeOverrideSummary(record: MergeOverrideRecordDTO): string {
  const actor = record.actorName?.trim() || "Unknown user";
  return `Merged with override by ${actor}: ${record.reason}${
    record.selfOverride ? " (self-override)" : ""
  }`;
}

/** The summary with what was waived, for a badge's tooltip. */
export function mergeOverrideTitle(record: MergeOverrideRecordDTO): string {
  const kinds = mergeOverrideWaivedKinds(record);
  return kinds
    ? `${mergeOverrideSummary(record)}\n${kinds}.`
    : mergeOverrideSummary(record);
}
