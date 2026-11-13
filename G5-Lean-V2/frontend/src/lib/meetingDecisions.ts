// =============================================================================
// Meeting decisions that become tasks (g5.workbench.meeting)
// =============================================================================
// The "extract decisions" template records a meeting as a question set whose
// id starts with `meeting-`. The current template offers exactly three
// options: attach (a task linked to this workspace), detach (a task with no
// link) and reject. Sets extracted before 2026-10-22 carry the earlier four
// (Accept, Delay, Out of scope, Reject); those cards keep their behaviour,
// where a delayed or out-of-scope decision may become a tracker issue through
// the Create issue button. These helpers recognise both shapes, pre-fill the
// issue from the decision, find a reference an earlier press recorded, and
// append the created issue's reference to the owner's comment.
// =============================================================================

import type { QuestionEntry, SetQuestionOption } from "./questionSet";

export const MEETING_SET_PREFIX = "meeting-";

/** The current template's three fixed option ids. */
export const MEETING_TASK_OPTION_IDS = ["attach", "detach", "reject"] as const;

/** The earlier template's four fixed option ids. */
export const MEETING_LEGACY_OPTION_IDS = [
  "accept",
  "delay",
  "out-of-scope",
  "reject",
] as const;

/** The legacy option ids whose decisions may go to the tracker. */
export const MEETING_ISSUE_OPTION_IDS = ["delay", "out-of-scope"] as const;

export type MeetingCardShape = "task" | "legacy";
export type MeetingTaskKind = "attach" | "detach";

function offersAll(
  entry: Pick<QuestionEntry, "options">,
  ids: readonly string[],
): boolean {
  return ids.every((id) => entry.options.some((option) => option.id === id));
}

/**
 * Which meeting card shape a `meeting-` set entry has, or `null` for an
 * ordinary question (another set, multi-select, or other options).
 */
export function meetingCardShape(
  set: { setId: string },
  entry: Pick<QuestionEntry, "options" | "multiSelect">,
): MeetingCardShape | null {
  if (!set.setId.startsWith(MEETING_SET_PREFIX) || entry.multiSelect) {
    return null;
  }
  if (offersAll(entry, MEETING_TASK_OPTION_IDS)) return "task";
  if (offersAll(entry, MEETING_LEGACY_OPTION_IDS)) return "legacy";
  return null;
}

/** A meeting decision card of either shape: its options decide in one press. */
export function isMeetingDecisionCard(
  set: { setId: string },
  entry: Pick<QuestionEntry, "options" | "multiSelect">,
): boolean {
  return meetingCardShape(set, entry) !== null;
}

/** The task an option id names, or `null` for Reject and other ids. */
export function meetingTaskKind(id: string): MeetingTaskKind | null {
  return id === "attach" || id === "detach" ? id : null;
}

/**
 * The selected Delay or Out of scope option of a legacy meeting decision
 * card, or `null` when the card is not that shape or neither is selected.
 */
export function meetingDecisionIssueOption(
  set: { setId: string },
  entry: Pick<QuestionEntry, "options" | "multiSelect">,
  selected: readonly string[],
): SetQuestionOption | null {
  if (meetingCardShape(set, entry) !== "legacy") return null;
  const chosen = selected.filter((id) =>
    (MEETING_ISSUE_OPTION_IDS as readonly string[]).includes(id),
  );
  if (chosen.length !== 1) return null;
  return entry.options.find((option) => option.id === chosen[0]) ?? null;
}

/**
 * The issue description a decision pre-fills: context, the decision (a
 * legacy option's label with the comment; for a task the comment alone,
 * since the task itself is the decision), and the meeting.
 */
export function meetingIssueBody(
  entry: Pick<QuestionEntry, "context">,
  option: Pick<SetQuestionOption, "id" | "label">,
  comment: string,
  meetingTitle: string | undefined,
): string {
  const parts: string[] = [];
  if (entry.context?.trim()) parts.push(entry.context.trim());
  const note = comment.trim();
  if (meetingTaskKind(option.id)) {
    if (note) parts.push(`Decision: ${note}`);
  } else {
    parts.push(
      note
        ? `Decision: ${option.label} — ${note}`
        : `Decision: ${option.label}`,
    );
  }
  if (meetingTitle?.trim()) parts.push(`From meeting: ${meetingTitle.trim()}`);
  return parts.join("\n\n");
}

/** The comment with the issue's reference appended, once. */
export function appendIssueReference(
  text: string,
  issue: { displayKey: string; url: string },
): string {
  const reference = `${issue.displayKey} ${issue.url}`;
  if (text.includes(issue.url)) return text;
  const current = text.trimEnd();
  return current ? `${current}\n${reference}` : reference;
}

/** A reference line `appendIssueReference` wrote: `owner/name#N <url>`. */
const ISSUE_REFERENCE_RE =
  /(?:^|\n)[ \t]*([\w.-]+\/[\w.-]+#\d+)[ \t]+(https?:\/\/\S+)[ \t]*(?=\n|$)/;

/**
 * The issue reference a recorded comment carries, when an earlier press
 * created a task for this decision; a later task press reuses it instead of
 * creating a second issue.
 */
export function recordedIssueReference(
  text: string,
): { displayKey: string; url: string } | null {
  const match = ISSUE_REFERENCE_RE.exec(text);
  return match ? { displayKey: match[1], url: match[2] } : null;
}
