// =============================================================================
// G5 Client — Activity timeline filters
// =============================================================================
// The workspace timeline shows every durable activity kind; a filter row above
// the feed chooses which groups a person sees (workbench g5.workbench.landing).
// The choice is per-person browser state like the open-tab list, never a
// platform record. Keep the existing hidden-group preference for compatibility;
// an empty set represents All, which is highlighted independently of categories.
// Filtering is client-side over the loaded
// pages: the activity API has no kind filter, so a hidden kind is still loaded
// and never re-queried. Phase changes are recorded and served but never shown
// (amendment of 2026-10-15), so they belong to no group.
// =============================================================================

import { readStoredList, writeStoredList } from "./storedList";
import type { ActivityEntryDTO } from "./types";

export type ActivityEntryKind = ActivityEntryDTO["kind"];

/** Kinds the API serves that the timeline never shows. */
export type UnshownActivityKind = "phase_changed";

/** An entry the timeline shows: every served kind but the unshown ones. */
export type ShownActivityEntry = Exclude<
  ActivityEntryDTO,
  { kind: UnshownActivityKind }
>;
export type ShownActivityKind = ShownActivityEntry["kind"];

export function isShownActivityEntry(
  entry: ActivityEntryDTO,
): entry is ShownActivityEntry {
  return entry.kind !== "phase_changed";
}

export type ActivityFilterGroupId =
  | "issue_discussion"
  | "conversations"
  | "tasks"
  | "commits"
  | "approvals"
  | "pull_requests"
  | "guided_steps";

export interface ActivityFilterGroup {
  id: ActivityFilterGroupId;
  label: string;
  kinds: readonly ShownActivityKind[];
}

/** Every shown entry kind belongs to exactly one group, in the row's order. */
export const ACTIVITY_FILTER_GROUPS: readonly ActivityFilterGroup[] = [
  {
    id: "issue_discussion",
    label: "Issue discussion",
    kinds: ["issue_opened", "issue_comment", "issue_event"],
  },
  {
    id: "conversations",
    label: "Conversations",
    kinds: ["chat", "workspace_comment", "workspace_reply"],
  },
  {
    id: "tasks",
    label: "Tasks",
    kinds: ["task", "workspace_created"],
  },
  { id: "commits", label: "Commits", kinds: ["commit"] },
  {
    id: "approvals",
    label: "Approvals",
    kinds: ["approval_decision", "approval_requested"],
  },
  { id: "pull_requests", label: "Pull requests", kinds: ["pull_request"] },
  { id: "guided_steps", label: "Guided steps", kinds: ["journey_step"] },
];

const GROUP_BY_KIND: Record<ShownActivityKind, ActivityFilterGroupId> = {
  issue_opened: "issue_discussion",
  issue_comment: "issue_discussion",
  issue_event: "issue_discussion",
  chat: "conversations",
  workspace_comment: "conversations",
  workspace_reply: "conversations",
  workspace_created: "tasks",
  approval_requested: "approvals",
  task: "tasks",
  commit: "commits",
  approval_decision: "approvals",
  pull_request: "pull_requests",
  journey_step: "guided_steps",
};

export function activityFilterGroupOf(
  kind: ShownActivityKind,
): ActivityFilterGroupId {
  return GROUP_BY_KIND[kind];
}

const GROUP_IDS = new Set<string>(ACTIVITY_FILTER_GROUPS.map((g) => g.id));

export function isActivityFilterGroupId(
  value: unknown,
): value is ActivityFilterGroupId {
  return typeof value === "string" && GROUP_IDS.has(value);
}

/** Per-user localStorage key for the hidden group list. */
export function activityFiltersKey(userId: string): string {
  return `g5:activityFilters:${userId}`;
}

/** Hidden groups for this person; empty (everything shown) when unset or unreadable. */
export function readHiddenActivityGroups(
  userId: string,
): ActivityFilterGroupId[] {
  if (!userId) return [];
  return normalizeHiddenGroups(
    readStoredList(activityFiltersKey(userId), isActivityFilterGroupId),
  );
}

function normalizeHiddenGroups(hidden: readonly ActivityFilterGroupId[]) {
  const groups = [...new Set(hidden)];
  return groups.length === GROUP_IDS.size ? [] : groups;
}

/** All → one category; then add/remove categories, with no empty selection. */
export function toggleActivityGroup(
  hidden: ReadonlySet<ActivityFilterGroupId>,
  id: ActivityFilterGroupId,
): Set<ActivityFilterGroupId> {
  if (hidden.size === 0) {
    return new Set(
      ACTIVITY_FILTER_GROUPS.map((group) => group.id).filter(
        (group) => group !== id,
      ),
    );
  }
  const next = new Set(hidden);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return new Set(normalizeHiddenGroups([...next]));
}

export function writeHiddenActivityGroups(
  userId: string,
  hidden: readonly ActivityFilterGroupId[],
): void {
  if (!userId) return;
  writeStoredList(activityFiltersKey(userId), normalizeHiddenGroups(hidden));
}
