// =============================================================================
// Tracker operations for a findings decision (`g5.documents.findings.decision`)
// =============================================================================
// The issue a Need fix or Defer press files, and the changes a later press
// makes to it, through the organization's shared issue routes: creation with
// its admission and keyed lookup (`g5.external-issues.creation`), resolve,
// edit (`g5.external-issues.edit`) and state (`g5.external-issues.transition`).
// Nothing is stored here; the entry's decision line records the outcome.
// =============================================================================

import type { QueryClient } from "@tanstack/react-query";
import { api } from "./api";
import { routes } from "./routes";
import { externalIssueKeys } from "../hooks/useExternalIssues";
import { NATIVE_ISSUE_REPOSITORY_ID } from "./issuePage";
import type {
  CreatedIssueDTO,
  ExternalIssueSummaryDTO,
  IssueCreationDTO,
  IssueEditDTO,
  IssueStateChangeDTO,
} from "./types";
import { prioritizedTitle, type IssueRef } from "./findings";

/** A tracker step that has no definite answer yet: check before acting again. */
export class UnclearOutcome extends Error {}

export interface NewFindingIssue {
  title: string;
  body: string;
  /** The entry's creation key (`creationKey`). */
  key: string;
  domainId: string;
}

/** The issue an earlier press under this key filed, or `null`. */
export async function findFiledIssue(
  orgId: string,
  issue: NewFindingIssue,
): Promise<CreatedIssueDTO | null> {
  const check = await api.post<IssueCreationDTO>(routes.orgs.issues(orgId), {
    title: issue.title,
    body: issue.body,
    assignees: [],
    idempotencyKey: issue.key,
    domainId: issue.domainId,
    checkOnly: true,
  });
  if (check.state === "created" && check.issue) return check.issue;
  if (check.state === "outcome_unknown")
    throw new UnclearOutcome(
      check.message ?? "The tracker gave no definite answer. Try again.",
    );
  return null;
}

/** File the entry's issue: no assignee, parent or workspace link. */
export async function fileIssue(
  orgId: string,
  issue: NewFindingIssue,
): Promise<CreatedIssueDTO> {
  const created = await api.post<IssueCreationDTO>(routes.orgs.issues(orgId), {
    title: issue.title,
    body: issue.body,
    assignees: [],
    idempotencyKey: issue.key,
    domainId: issue.domainId,
  });
  if (created.state === "created" && created.issue) return created.issue;
  throw new UnclearOutcome(
    created.message ??
      "The tracker gave no definite answer. Press again to check before anything is sent again.",
  );
}

/** The recorded issue as its tracker reports it now. */
export function readIssue(
  orgId: string,
  issue: IssueRef,
): Promise<ExternalIssueSummaryDTO> {
  return api.get<ExternalIssueSummaryDTO>(routes.orgs.issueResolve(orgId), {
    params: { ref: issue.key },
  });
}

/**
 * Give the issue the priority of `kind`, keeping its other wording. Returns
 * the issue as answered; nothing is sent when the title already has it.
 */
export async function setIssuePriority(
  orgId: string,
  issue: ExternalIssueSummaryDTO,
  kind: "Need fix" | "Defer",
): Promise<ExternalIssueSummaryDTO> {
  const title = prioritizedTitle(issue.title, kind);
  if (title === issue.title) return issue;
  if (issue.locator.repositoryId === NATIVE_ISSUE_REPOSITORY_ID) {
    return api.patch<ExternalIssueSummaryDTO>(
      routes.orgs.nativeIssue(orgId, issue.locator.number),
      { title, expectedUpdatedAt: issue.updatedAt },
    );
  }
  const edited = await api.patch<IssueEditDTO>(
    routes.orgs.issue(orgId, issue.locator),
    { title, seen: { title: issue.title } },
  );
  if (edited.outcome === "unknown" || !edited.issue)
    throw new UnclearOutcome(
      edited.message ??
        "GitHub gave no definite answer about the new title. Press again.",
    );
  return edited.issue;
}

/** Close or reopen the issue; nothing is sent when it is already so. */
export async function setIssueState(
  orgId: string,
  issue: ExternalIssueSummaryDTO,
  state: "open" | "closed",
): Promise<void> {
  if (issue.state === state) return;
  const changed = await api.post<IssueStateChangeDTO>(
    routes.orgs.issueState(orgId, issue.locator),
    { state },
  );
  if (changed.outcome === "unknown")
    throw new UnclearOutcome(
      changed.message ?? "The tracker gave no definite answer. Press again.",
    );
}

/** Issue lists, pages and link summaries show the change. */
export function refreshIssueViews(client: QueryClient): void {
  void client.invalidateQueries({ queryKey: externalIssueKeys.searchAll });
  void client.invalidateQueries({ queryKey: externalIssueKeys.detailAll });
  void client.invalidateQueries({ queryKey: externalIssueKeys.linksAll });
}

/** The reference a decision line keeps for an issue. */
export function issueRefOf(
  issue: CreatedIssueDTO | ExternalIssueSummaryDTO,
): IssueRef {
  return "url" in issue
    ? { key: issue.displayKey, url: issue.url }
    : { key: issue.displayKey, url: issue.canonicalUrl };
}
