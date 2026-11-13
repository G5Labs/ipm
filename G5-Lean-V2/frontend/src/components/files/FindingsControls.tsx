// =============================================================================
// FindingsControls — decisions on a verification run's findings
// =============================================================================
// `g5.documents.findings`: in a findings file's Preview, each entry's status
// line becomes a control row with its severity and a chat action. An
// actionable entry also shows its decision and Need fix / Defer / Irrelevant,
// which the owner of a running workspace presses once
// (`g5.documents.findings.decision`).
//
// A press changes the tracker first, then rewrites the entry's decision line
// in the live file, without a commit of its own:
// - Need fix and Defer file one `[High]` or `[Low]` issue, found again by the
//   entry's creation key.
// - A later press retitles, closes or reopens that same issue.
//
// Each entry reads as a card edged in its severity's colour. A toolbar
// filters entries by severity and, when entries name the run that recorded
// them, by run; a filtered document also hides the description around the
// entries, keeping the headings that introduce them.
// =============================================================================

import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useInRouterContext, useLocation } from "react-router-dom";
import { Check, ListFilter, MessageCircle } from "lucide-react";
import {
  creationKey,
  findingDiscussPrompt,
  findingRuns,
  issueBody,
  prioritizedTitle,
  setDecision,
  SEVERITIES,
  type DecidedKind,
  type FindingEntry,
  type FindingsFile,
  type IssueRef,
  type Severity,
  workspaceFileUrl,
} from "../../lib/findings";
import {
  fileIssue,
  findFiledIssue,
  issueRefOf,
  readIssue,
  refreshIssueViews,
  setIssuePriority,
  setIssueState,
  UnclearOutcome,
  type NewFindingIssue,
} from "../../lib/findingIssues";
import {
  useExternalIssuesFeature,
  useIssueCreateTarget,
  useRouteOrgMembership,
} from "../../hooks/useExternalIssues";
import {
  createTargetBlockedReason,
  describeCreateIssueError,
  EXTERNAL_ISSUES_OFF_MESSAGE,
} from "../../lib/issueCommentErrors";
import { CreateIssueDialog } from "../external-issues/CreateIssueDialog";
import { RunTable } from "../editor/VerificationPanel";
import { resolveRepoLink } from "./repoLinks";
import {
  compareStates,
  concernTitle,
  receiptRun,
} from "../../lib/verificationArtifacts";
import { useRunIndex, type Read } from "../../hooks/useVerificationEvidence";
import { ApiError } from "../../lib/api";
import { NATIVE_ISSUE_REPOSITORY_ID } from "../../lib/issuePage";
import type { CreatedIssueDTO, ExternalIssueSummaryDTO } from "../../lib/types";

/** What the file view supplies for a findings file. */
export interface FindingsHost {
  workspaceId: string;
  repoId: string;
  filePath: string;
  file: FindingsFile;
  /** The displayed content the entries were parsed from. */
  content: string;
  entries: readonly FindingEntry[];
  /** Why the owner cannot decide here now, or `null` when they can. */
  blockedReason: string | null;
  /** Write the file live, without a commit, if it still holds `expected`. */
  write?: (expected: string, next: string) => Promise<void>;
  /** Seed the chat composer; absent while it cannot be seeded. */
  onDiscuss?: (text: string) => void;
  /** Opens a repository file, such as a run's receipt. */
  onOpenFile?: (path: string) => void;
}

type Phase =
  | { kind: "working"; label: string }
  | { kind: "confirm"; message: string; onConfirm: () => void }
  | { kind: "error"; message: string; retry?: () => void; retryLabel?: string }
  | { kind: "done"; message: string };

interface FindingsContextValue {
  entryOf: (index: number) => FindingEntry | undefined;
  row: (index: number) => React.ReactNode;
  host: Pick<
    FindingsHost,
    "workspaceId" | "repoId" | "filePath" | "onOpenFile"
  >;
}

const FindingsContext = createContext<FindingsContextValue | null>(null);
/** The severities the filter hides; empty while nothing is filtered. */
const HiddenSeverities = createContext<ReadonlySet<Severity>>(new Set());

/** The run filter's choices: all runs, the entries naming none, or one run. */
const ALL_RUNS = "all";
const NO_RUN = "none";
const runChoice = (label: string) => `run:${label}`;
/** The run filter's choice; all runs while it filters nothing. */
const ShownRun = createContext<string>(ALL_RUNS);

/** Whether the run filter's choice shows an entry. */
function runShows(choice: string, entry: FindingEntry | undefined): boolean {
  if (choice === ALL_RUNS || !entry) return true;
  return choice === (entry.run === null ? NO_RUN : runChoice(entry.run));
}

/**
 * An entry's blocks, marked with its severity: the severity filter hides
 * severities with one attribute on the document, so toggling re-renders no
 * entry. Run labels are the file's own, so the run filter hides the entry.
 */
export function FindingEntryBlocks({
  node,
  children,
}: {
  node: { properties: { entryIndex: number } };
  children?: React.ReactNode;
}) {
  const findings = useContext(FindingsContext);
  const run = useContext(ShownRun);
  const index = Number(node.properties.entryIndex);
  const entry = findings?.entryOf(index);
  return (
    <div
      className="finding-entry"
      data-finding-entry={index}
      // Links to `#<entry id>` reach the entry (`g5.documents.findings.files`).
      data-finding-id={entry?.id}
      data-severity={entry?.severity.toLowerCase()}
      hidden={!runShows(run, entry)}
    >
      {children}
    </div>
  );
}

/** Description around the entries, which a filtered document hides. */
export function FindingsContextBlocks({
  children,
}: {
  children?: React.ReactNode;
}) {
  return <div className="findings-context">{children}</div>;
}

/**
 * The heading that introduces a group of entries. A filtered document keeps it
 * while any of its section's entries is shown.
 */
export function FindingsSectionHeading({
  node,
  children,
}: {
  node: { properties: { entries?: string } };
  children?: React.ReactNode;
}) {
  const findings = useContext(FindingsContext);
  const hidden = useContext(HiddenSeverities);
  const run = useContext(ShownRun);
  const entries = String(node.properties.entries ?? "")
    .split(" ")
    .filter(Boolean)
    .map((index) => findings?.entryOf(Number(index)));
  const empty =
    (hidden.size > 0 || run !== ALL_RUNS) &&
    entries.every(
      (entry) => !entry || hidden.has(entry.severity) || !runShows(run, entry),
    );
  return (
    <div className="findings-section-heading" hidden={empty}>
      {children}
    </div>
  );
}

interface EvidenceLink {
  href: string;
  label: string;
}

const parseLinks = (value: unknown): EvidenceLink[] => {
  try {
    const parsed = JSON.parse(String(value || "[]"));
    return Array.isArray(parsed) ? parsed : [parsed];
  } catch {
    return [];
  }
};

/** A run a receipt link names, found in the run index of its folder. */
function useLinkedRuns(links: readonly EvidenceLink[]) {
  const findings = useContext(FindingsContext);
  const host = findings?.host;
  const receipts = links.map((link) => {
    const target = host ? resolveRepoLink(host.filePath, link.href) : null;
    return { link, run: target ? receiptRun(target.path) : null };
  });
  const root = receipts.find((r) => r.run)?.run?.root ?? null;
  const index = useRunIndex(host?.workspaceId ?? "", host?.repoId ?? "", root);
  const rows = index.read.status === "ok" ? index.read.data.rows : null;
  return {
    host,
    index: index.read,
    retry: () => void index.refetch(),
    receipts: receipts.map(({ link, run }) => ({
      link,
      run,
      row: run
        ? (rows?.find(
            (r) => r.runId === run.runId && r.concern === run.concern,
          ) ?? null)
        : null,
    })),
  };
}

function EvidenceHead({ title, note }: { title: string; note: string }) {
  return (
    <p className="finding-evidence-head">
      <strong>{title}</strong>
      <span className="finding-evidence-note">{note}</span>
    </p>
  );
}

function IndexNote({
  read,
  onRetry,
}: {
  read: Read<unknown>;
  onRetry: () => void;
}) {
  if (read.status === "loading")
    return <p className="finding-evidence-note">Reading the run index…</p>;
  if (read.status === "ok") return null;
  return (
    <p className="finding-evidence-note" role="alert">
      {read.status === "absent"
        ? "The run index was not found."
        : `The run index could not be read: ${read.message}`}{" "}
      {read.status === "error" && (
        <button
          type="button"
          className="finding-link-button"
          data-testid="finding-evidence-retry"
          onClick={onRetry}
        >
          Retry
        </button>
      )}
    </p>
  );
}

/**
 * An entry's evidence line, rendered with the recorded runs it links
 * (`g5.documents.findings.files`): the runs that found a bug, what reproduced
 * it, or a simplification's comparison of the current and simplified models.
 */
export function FindingEvidence({
  node,
  children,
}: {
  node: { properties: Record<string, unknown> };
  children?: React.ReactNode;
}) {
  const kind = String(node.properties.kind);
  if (kind === "found-by") return <FoundBy node={node}>{children}</FoundBy>;
  if (kind === "comparison")
    return <Comparison node={node}>{children}</Comparison>;
  const none = React.Children.toArray(children).every(
    (child) => typeof child === "string" && /^\s*(none\.?)?\s*$/i.test(child),
  );
  // Without a reproduction the heading stands alone, in bold.
  return (
    <div
      className={`finding-evidence${none ? " finding-evidence-missing" : ""}`}
      data-testid="finding-evidence"
      data-kind="reproduced-by"
      data-empty={none ? "true" : undefined}
    >
      <EvidenceHead
        title="Reproduced by"
        note="A test or probe that made it happen in the code"
      />
      {none ? (
        <span className="sr-only">None recorded.</span>
      ) : (
        <p className="finding-evidence-body">{children}</p>
      )}
    </div>
  );
}

function FoundBy({
  node,
  children,
}: {
  node: { properties: Record<string, unknown> };
  children?: React.ReactNode;
}) {
  const links = parseLinks(node.properties.links);
  const findings = useContext(FindingsContext);
  // Receipt links only; other text says how the bug was found.
  const receipts = links.filter((link) => {
    const target = findings
      ? resolveRepoLink(findings.host.filePath, link.href)
      : null;
    return !!(target && receiptRun(target.path));
  });
  const machines = new Set(
    receipts.map((link) => link.href.split("/").slice(-3, -2)[0]),
  );
  const [open, setOpen] = useState(false);
  if (receipts.length === 0)
    return (
      <div
        className="finding-evidence"
        data-testid="finding-evidence"
        data-kind="found-by"
      >
        <EvidenceHead
          title="Found by"
          note="How the problem was found, as recorded"
        />
        <p className="finding-evidence-body">{children}</p>
      </div>
    );
  // Closed until asked for, as the mock keeps model evidence: a long file
  // lays out one table per bug only when it is opened.
  return (
    <details
      className="finding-evidence finding-evidence-details"
      data-testid="finding-evidence"
      data-kind="found-by"
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="finding-evidence-head">
        <strong>Found by</strong>
        <span className="finding-evidence-note">
          {receipts.length} recorded {receipts.length === 1 ? "run" : "runs"} of{" "}
          {machines.size} state {machines.size === 1 ? "machine" : "machines"}{" "}
          that showed this problem
        </span>
      </summary>
      {open && <FoundByRuns links={receipts} />}
    </details>
  );
}

function FoundByRuns({ links }: { links: readonly EvidenceLink[] }) {
  const { host, index, retry, receipts } = useLinkedRuns(links);
  return (
    <>
      <IndexNote read={index} onRetry={retry} />
      <RunTable
        compact
        onOpenFile={host?.onOpenFile}
        rows={receipts.map(({ link, run, row }, i) => ({
          key: `${i}`,
          label: row?.label ?? link.label,
          row,
          kind: row?.kind ?? null,
          variant: row?.variant ?? null,
          expect: row?.expect ?? null,
          root: run!.root,
          concern: run!.concern,
          machine: concernTitle(run!.concern),
        }))}
      />
    </>
  );
}

function Comparison({
  node,
  children,
}: {
  node: { properties: Record<string, unknown> };
  children?: React.ReactNode;
}) {
  const [before] = parseLinks(node.properties.before || "null").filter(Boolean);
  const [after] = parseLinks(node.properties.after || "null").filter(Boolean);
  const likeForLike = node.properties.likeForLike !== "false";
  const linked = [before, after].filter(Boolean) as EvidenceLink[];
  const { host, index, retry, receipts } = useLinkedRuns(linked);
  const runOf = (link: EvidenceLink | undefined) =>
    link ? receipts.find((r) => r.link === link) : undefined;
  const beforeRun = runOf(before);
  const afterRun = runOf(after);
  const result = compareStates(beforeRun?.row, afterRun?.row, {
    linked: { before: !!before, after: !!after },
    likeForLike,
  });
  const tableRow = (
    key: string,
    fallback: string,
    link: EvidenceLink | undefined,
    found: typeof beforeRun,
  ) => ({
    key,
    label: link?.label || fallback,
    row: found?.row ?? null,
    kind: found?.row?.kind ?? null,
    variant: found?.row?.variant ?? null,
    expect: found?.row?.expect ?? null,
    root: found?.run?.root,
    concern: found?.run?.concern,
    machine: found?.run ? concernTitle(found.run.concern) : undefined,
    missing: link ? undefined : "No run linked",
  });
  return (
    <div className="finding-comparison" data-testid="finding-comparison">
      <EvidenceHead
        title="Evidence for the simplification"
        note="A run of the current design's model and of the simplified model"
      />
      {linked.length > 0 && (
        <>
          <IndexNote read={index} onRetry={retry} />
          <RunTable
            compact
            receiptLabel="Inspect run"
            onOpenFile={host?.onOpenFile}
            rows={[
              tableRow("before", "As-built", before, beforeRun),
              tableRow("after", "Proposed simplification", after, afterRun),
            ]}
          />
        </>
      )}
      {index.status === "loading" && linked.length > 0 ? null : result.kind ===
        "ratio" ? (
        <p
          className={
            result.percent > 0 && result.qualifications.length === 0
              ? "finding-reduction"
              : "finding-no-reduction"
          }
          data-testid="finding-reduction"
        >
          <strong>
            {result.percent === 0
              ? "0% change in explored states"
              : `${Math.abs(result.percent).toFixed(1)}% ${result.percent > 0 ? "fewer" : "more"} explored states`}
          </strong>
          <span>
            Recorded state-count score · Proposal / baseline:{" "}
            {result.factor.toFixed(3)}×
          </span>
          {result.qualifications.map((note) => (
            <span key={note}>{note}</span>
          ))}
        </p>
      ) : (
        <p className="finding-no-reduction" data-testid="finding-no-reduction">
          <strong>No state-count score available</strong>
          <span>{result.reason}</span>
        </p>
      )}
      {React.Children.count(children) > 0 && (
        <p className="finding-evidence-body">{children}</p>
      )}
    </div>
  );
}

/** An entry's control row, in place of its status line. */
export function FindingStatusRow({
  node,
}: {
  node: { properties: { entryIndex: number } };
}) {
  const findings = useContext(FindingsContext);
  return findings ? (
    <>{findings.row(Number(node.properties.entryIndex))}</>
  ) : null;
}

const SEVERITY_CLASS: Record<Severity, string> = {
  Critical: "finding-severity-critical",
  High: "finding-severity-high",
  Medium: "finding-severity-medium",
  Low: "finding-severity-low",
  Unranked: "finding-severity-unranked",
};

const DECISION_KINDS: readonly DecidedKind[] = [
  "Need fix",
  "Defer",
  "Irrelevant",
];

const DECISION_HELP: Record<DecidedKind, string> = {
  "Need fix": "Create an issue with high priority",
  Defer: "Create an issue with low priority",
  Irrelevant: "Not important: record it and create nothing",
};

/** The page's path: the router's when there is one. */
function useRoutePathname(): string {
  const inRouter = useInRouterContext();
  // A component is inside a router, or not, for its whole life.
  // eslint-disable-next-line react-hooks/rules-of-hooks
  return inRouter ? useLocation().pathname : window.location.pathname;
}

/** Why an entry's decision cannot be pressed, from its parse. */
function malformed(entry: FindingEntry): string | null {
  if (entry.duplicate)
    return `Another entry in this file also has the ID ${entry.id}.`;
  if (entry.decision === "?")
    return "Its decision line is not one G5 writes; fix it in plain text.";
  if (
    (entry.decision === "Need fix" || entry.decision === "Defer") &&
    !entry.issue
  )
    return "Its decision line names no issue; fix it in plain text.";
  return null;
}

function describeError(error: unknown): string {
  if (!(error instanceof ApiError))
    return error instanceof Error ? error.message : String(error);
  const message = describeCreateIssueError(error);
  return message.replace(/ Your draft is kept\.$/, "");
}

/**
 * The findings presentation for one file: the context the control rows read,
 * the severity filter, and the dialog an untrusted destination needs.
 */
export function FindingsLayer({
  host,
  children,
}: {
  host: FindingsHost;
  children: React.ReactNode;
}) {
  const { entries, file, filePath, repoId, blockedReason, write } = host;
  const client = useQueryClient();
  const pathname = useRoutePathname();
  const membership = useRouteOrgMembership();
  const orgId = membership?.orgId ?? "";
  const feature = useExternalIssuesFeature(membership?.orgId);
  const trackerOn = feature.status === "enabled" && !!membership;
  const createTarget = useIssueCreateTarget(orgId, trackerOn);
  const destination =
    createTarget.status === "ok" ? createTarget.data.destination : null;

  const [phases, setPhases] = useState<Record<string, Phase>>({});
  const [dialog, setDialog] = useState<{
    id: string;
    kind: DecidedKind;
    draft: NewFindingIssue;
  } | null>(null);
  // The latest content: a write after a slow tracker step rewrites what the
  // file holds then, not what it held when the press started.
  const latest = useRef(host.content);
  useEffect(() => {
    latest.current = host.content;
  }, [host.content]);
  const busy = Object.values(phases).some((p) => p.kind === "working");
  const setPhase = useCallback(
    (id: string, phase: Phase | null) =>
      setPhases((current) => {
        const next = { ...current };
        if (phase) next[id] = phase;
        else delete next[id];
        return next;
      }),
    [],
  );

  /** Why the tracker cannot be used now, or `null`. */
  const trackerReason = (): string | null => {
    if (!membership) return EXTERNAL_ISSUES_OFF_MESSAGE;
    if (feature.status === "loading") return "Checking the issue tracker…";
    if (feature.status === "error")
      return "The issue tracker could not be checked. Reload to try again.";
    if (feature.status === "disabled") return EXTERNAL_ISSUES_OFF_MESSAGE;
    return null;
  };
  /** Why creating a new issue is impossible now, or `null`. */
  const creationReason = (): string | null =>
    trackerReason() ?? createTargetBlockedReason(createTarget);

  // Retry, Check again and Confirm run later than the render that offered
  // them: they always call this render's actions, which read the current
  // entry, file, gate and tracker.
  const actions = useRef<{
    decide: (id: string, kind: DecidedKind, confirmedTitle?: string) => void;
    record: (id: string, kind: DecidedKind, issue: IssueRef | null) => void;
  }>(null!);
  const entryById = (id: string) => entries.find((e) => e.id === id);

  /** Write the decision line; a refused write keeps the issue and offers Retry. */
  const record = async (
    id: string,
    kind: DecidedKind,
    issue: IssueRef | null,
  ) => {
    if (!write) return;
    setPhase(id, { kind: "working", label: "Recording the decision…" });
    try {
      const before = latest.current;
      await write(before, setDecision(before, id, kind, issue, file));
      setPhase(id, {
        kind: "done",
        message: issue
          ? `${kind}: ${issue.key}. The decision is in this workspace's files and is committed with its next save.`
          : `${kind}. The decision is in this workspace's files and is committed with its next save.`,
      });
    } catch (error) {
      setPhase(id, {
        kind: "error",
        message: `${issue ? `${issue.key} is up to date, but the` : "The"} decision could not be written: ${error instanceof Error ? error.message : String(error)}`,
        retry: () => void actions.current.record(id, kind, issue),
        retryLabel: "Retry recording",
      });
    }
  };

  /**
   * Reopen and retitle a recorded issue for Need fix or Defer. An untrusted
   * GitHub domain first shows the exact new title; `null` while it waits.
   */
  const keepIssue = async (
    id: string,
    kind: "Need fix" | "Defer",
    current: ExternalIssueSummaryDTO,
    confirmedTitle: string | undefined,
  ): Promise<IssueRef | null> => {
    const title = prioritizedTitle(current.title, kind);
    const github = current.locator.repositoryId !== NATIVE_ISSUE_REPOSITORY_ID;
    if (
      title !== current.title &&
      github &&
      !current.contentTrusted &&
      confirmedTitle !== title
    ) {
      setPhase(id, {
        kind: "confirm",
        message: `Change the title of ${current.displayKey} on GitHub to “${title}”?`,
        onConfirm: () => void actions.current.decide(id, kind, title),
      });
      return null;
    }
    // Retitle first: a native edit applies only to the version read, and a
    // state change moves it.
    if (title !== current.title) {
      setPhase(id, {
        kind: "working",
        label: `Updating ${current.displayKey}…`,
      });
      await setIssuePriority(orgId, current, kind);
    }
    if (current.state === "closed") {
      setPhase(id, {
        kind: "working",
        label: `Reopening ${current.displayKey}…`,
      });
      await setIssueState(orgId, current, "open");
    }
    return issueRefOf(current);
  };

  /** The entry's new issue, and its creation key, from what it shows now. */
  const draftFor = (
    entry: FindingEntry,
    kind: DecidedKind,
    domainId: string,
  ): NewFindingIssue => ({
    title: prioritizedTitle(
      entry.title,
      kind === "Defer" ? "Defer" : "Need fix",
    ),
    body: issueBody(latest.current, entry, filePath, file, (path, fragment) =>
      workspaceFileUrl(pathname, repoId, path, fragment),
    ),
    key: creationKey(repoId, filePath, entry.id),
    domainId,
  });

  /** One press: the tracker first, then the decision line. */
  const decide = async (
    id: string,
    kind: DecidedKind,
    confirmedTitle?: string,
  ) => {
    const entry = entryById(id);
    if (blockedReason || !write || !entry) return;
    setPhase(id, { kind: "working", label: "Working…" });
    try {
      let issue = entry.issue;
      if (
        !issue &&
        destination &&
        !(kind === "Irrelevant" && trackerReason())
      ) {
        // An issue an earlier press filed, whose record was lost.
        setPhase(id, {
          kind: "working",
          label: "Looking for an earlier issue…",
        });
        const found = await findFiledIssue(
          orgId,
          draftFor(entry, kind, destination.domainId),
        );
        if (found) issue = issueRefOf(found);
      }
      if (kind === "Irrelevant") {
        if (issue) {
          setPhase(id, { kind: "working", label: `Closing ${issue.key}…` });
          await setIssueState(orgId, await readIssue(orgId, issue), "closed");
        }
      } else if (issue) {
        const kept = await keepIssue(
          id,
          kind,
          await readIssue(orgId, issue),
          confirmedTitle,
        );
        if (!kept) return;
      } else {
        if (!destination)
          throw new Error(creationReason() ?? "No tracker is available.");
        const draft = draftFor(entry, kind, destination.domainId);
        if (!destination.contentTrusted) {
          // An untrusted destination reviews the exact payload first.
          setPhase(id, {
            kind: "working",
            label: "Review the issue before it is created…",
          });
          setDialog({ id, kind, draft });
          return;
        }
        setPhase(id, { kind: "working", label: "Creating the issue…" });
        issue = issueRefOf(await fileIssue(orgId, draft));
      }
      refreshIssueViews(client);
      await actions.current.record(id, kind, issue);
    } catch (error) {
      refreshIssueViews(client);
      setPhase(id, {
        kind: "error",
        message: describeError(error),
        retry: () => void actions.current.decide(id, kind),
        retryLabel: error instanceof UnclearOutcome ? "Check again" : "Retry",
      });
    }
  };
  actions.current = { decide, record };

  const created = (issue: CreatedIssueDTO) => {
    const opened = dialog;
    setDialog(null);
    if (!opened) return;
    refreshIssueViews(client);
    void actions.current.record(opened.id, opened.kind, issueRefOf(issue));
  };

  const row = (index: number): React.ReactNode => {
    const entry = entries[index];
    if (!entry) return null;
    const phase = phases[entry.id];
    const problem = entry.decision !== null ? malformed(entry) : null;
    const actionable = entry.decision !== null && !problem;
    const reasonFor = (kind: DecidedKind): string | null => {
      if (blockedReason) return blockedReason;
      if (busy) return "Another decision is in progress.";
      if (entry.decision === kind) return `Already ${kind}.`;
      if (kind === "Irrelevant") {
        if (entry.issue) return trackerReason();
        // Wait for the tracker, so an issue an earlier press filed is found.
        return feature.status === "loading" ||
          (trackerOn && createTarget.status === "loading")
          ? "Checking the issue tracker…"
          : null;
      }
      return entry.issue ? trackerReason() : creationReason();
    };
    return (
      <div
        className="finding-row"
        data-testid="finding-row"
        data-entry-id={entry.id}
        data-decision={entry.decision ?? "none"}
      >
        <span
          className={`finding-severity ${SEVERITY_CLASS[entry.severity]}`}
          data-testid="finding-severity"
        >
          {entry.severity}
        </span>
        {entry.decision !== null && !problem && (
          <span
            className="finding-decision"
            data-testid="finding-decision"
            data-decision={entry.decision}
          >
            {entry.decision}
            {entry.issue && (
              <>
                {" · "}
                <a
                  href={entry.issue.url}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  {entry.issue.key}
                </a>
                {entry.decision === "Irrelevant" && " (closed)"}
              </>
            )}
          </span>
        )}
        {actionable && (
          <span
            className="finding-actions"
            role="group"
            aria-label={`Decide ${entry.title}`}
          >
            {DECISION_KINDS.map((kind) => {
              const reason = reasonFor(kind);
              return (
                <button
                  key={kind}
                  type="button"
                  className="finding-action"
                  data-testid="finding-action"
                  data-kind={kind}
                  aria-pressed={entry.decision === kind}
                  disabled={reason !== null}
                  title={reason ?? DECISION_HELP[kind]}
                  onClick={() => void decide(entry.id, kind)}
                >
                  {kind}
                </button>
              );
            })}
          </span>
        )}
        {host.onDiscuss && (
          <button
            type="button"
            className="finding-chat"
            data-testid="finding-chat"
            aria-label={`Discuss ${entry.title} in chat`}
            title="Discuss in chat"
            onClick={() =>
              host.onDiscuss!(findingDiscussPrompt(entry, file, filePath))
            }
          >
            <MessageCircle size={14} aria-hidden />
          </button>
        )}
        {problem && (
          <span
            className="finding-note"
            role="note"
            data-testid="finding-malformed"
          >
            {problem}
          </span>
        )}
        {phase && (
          <PhaseNote phase={phase} onDismiss={() => setPhase(entry.id, null)} />
        )}
      </div>
    );
  };

  const context: FindingsContextValue = {
    entryOf: (index) => entries[index],
    row,
    host,
  };
  return (
    <FindingsContext.Provider value={context}>
      <FindingsFilter entries={entries} notice={blockedReason}>
        {children}
      </FindingsFilter>
      {dialog && (
        <CreateIssueDialog
          open
          orgId={orgId}
          initialTitle={dialog.draft.title}
          initialBody={dialog.draft.body}
          idempotencyKey={dialog.draft.key}
          titleAndDescriptionOnly
          onCreated={(issue) => created(issue)}
          onClose={() => {
            setPhase(dialog.id, null);
            setDialog(null);
          }}
        />
      )}
    </FindingsContext.Provider>
  );
}

/**
 * The severity and run filters and the document they filter. Their own state,
 * so a severity toggle re-renders neither the document nor its entries' rows,
 * only the headings that introduce entries. The run filter shows only when
 * some entry names its run: all runs, each run in the file's order, and the
 * entries that name none.
 */
function FindingsFilter({
  entries,
  notice,
  children,
}: {
  entries: readonly FindingEntry[];
  /** Why the decisions are unavailable, said once for the whole page. */
  notice: string | null;
  children: React.ReactNode;
}) {
  const [hidden, setHidden] = useState<ReadonlySet<Severity>>(new Set());
  const [chosenRun, setChosenRun] = useState(ALL_RUNS);
  const runSelect = useId();
  const counts = useMemo(() => {
    const byLevel = new Map<Severity, number>();
    for (const e of entries)
      byLevel.set(e.severity, (byLevel.get(e.severity) ?? 0) + 1);
    return byLevel;
  }, [entries]);
  const runOptions = useMemo(() => {
    const { labels, unnamed } = findingRuns(entries);
    if (labels.length === 0) return [];
    return [
      { value: ALL_RUNS, label: "All runs" },
      ...labels.map((label) => ({ value: runChoice(label), label })),
      ...(unnamed ? [{ value: NO_RUN, label: "No run named" }] : []),
    ];
  }, [entries]);
  // A run the file no longer names filters nothing.
  const run = runOptions.some((option) => option.value === chosenRun)
    ? chosenRun
    : ALL_RUNS;
  const filtered = hidden.size > 0 || run !== ALL_RUNS;
  const toggle = (severity: Severity) =>
    setHidden((current) => {
      const next = new Set(current);
      if (next.has(severity)) next.delete(severity);
      else next.add(severity);
      return next;
    });
  const total = entries.length;
  const shown = entries.filter(
    (e) => !hidden.has(e.severity) && runShows(run, e),
  ).length;
  return (
    <>
      <div className="findings-toolbar" data-testid="findings-toolbar">
        <div className="flex flex-wrap items-center gap-1.5">
          <div
            className="findings-filter"
            role="group"
            aria-label="Filter by severity"
            data-testid="findings-filter"
          >
            <span className="findings-toolbar-label">
              <ListFilter size={14} aria-hidden />
              Filter by severity
            </span>
            {SEVERITIES.map((severity) => {
              const on = !hidden.has(severity);
              return (
                <button
                  key={severity}
                  type="button"
                  className={`finding-severity finding-filter ${SEVERITY_CLASS[severity]}`}
                  data-testid="finding-filter"
                  data-severity={severity}
                  aria-pressed={on}
                  title={
                    on ? `Hide ${severity} entries` : `Show ${severity} entries`
                  }
                  onClick={() => toggle(severity)}
                >
                  <Check
                    size={12}
                    aria-hidden
                    className="finding-filter-check"
                    strokeWidth={3}
                  />
                  {severity} {counts.get(severity) ?? 0}
                </button>
              );
            })}
          </div>
          {runOptions.length > 0 && (
            <span className="ml-2 inline-flex items-center gap-1.5">
              <label className="findings-toolbar-label" htmlFor={runSelect}>
                Run
              </label>
              <select
                id={runSelect}
                data-testid="findings-run-filter"
                value={run}
                title="Show only the entries a run first recorded"
                onChange={(event) => setChosenRun(event.target.value)}
                className="max-w-[18rem] rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-1 py-0.5 text-xs text-[var(--g5-text0)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--g5-info)]"
              >
                {runOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </span>
          )}
          <button
            type="button"
            className="finding-filter-reset"
            data-testid="finding-filter-reset"
            disabled={!filtered}
            onClick={() => {
              setHidden(new Set());
              setChosenRun(ALL_RUNS);
            }}
          >
            Show all
          </button>
          <span
            className="findings-toolbar-count"
            role="status"
            data-testid="findings-shown"
          >
            {filtered
              ? `Showing ${shown} of ${total} entries`
              : `${total} ${total === 1 ? "entry" : "entries"}`}
          </span>
        </div>
        {notice && (
          <p
            className="findings-notice"
            role="note"
            data-testid="findings-notice"
          >
            {notice}
          </p>
        )}
      </div>
      <HiddenSeverities.Provider value={hidden}>
        <ShownRun.Provider value={run}>
          <div
            className="findings-document"
            data-testid="findings-document"
            data-filtered={filtered ? "" : undefined}
            {...Object.fromEntries(
              [...hidden].map((s) => [`data-hide-${s.toLowerCase()}`, ""]),
            )}
          >
            {children}
          </div>
        </ShownRun.Provider>
      </HiddenSeverities.Provider>
    </>
  );
}

function PhaseNote({
  phase,
  onDismiss,
}: {
  phase: Phase;
  onDismiss: () => void;
}) {
  if (phase.kind === "working")
    return (
      <span className="finding-note" role="status" data-testid="finding-phase">
        {phase.label}
      </span>
    );
  if (phase.kind === "confirm")
    return (
      <span className="finding-note" role="alert" data-testid="finding-phase">
        {phase.message}{" "}
        <button
          type="button"
          className="finding-link-button"
          data-testid="finding-confirm"
          onClick={phase.onConfirm}
        >
          Confirm
        </button>{" "}
        <button
          type="button"
          className="finding-link-button"
          data-testid="finding-cancel"
          onClick={onDismiss}
        >
          Cancel
        </button>
      </span>
    );
  if (phase.kind === "error")
    return (
      <span
        className="finding-note finding-note-error"
        role="alert"
        data-testid="finding-phase"
      >
        {phase.message}{" "}
        {phase.retry && (
          <button
            type="button"
            className="finding-link-button"
            data-testid="finding-retry"
            onClick={phase.retry}
          >
            {phase.retryLabel ?? "Retry"}
          </button>
        )}
      </span>
    );
  return (
    <span className="finding-note" role="status" data-testid="finding-phase">
      {phase.message}
    </span>
  );
}
