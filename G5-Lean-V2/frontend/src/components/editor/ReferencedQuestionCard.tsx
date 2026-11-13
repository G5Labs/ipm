// =============================================================================
// ReferencedQuestionCard — a `g5-question-ref` fence rendered as its set entry
// =============================================================================
// `g5.documents.question.referenced`: the fence names a set path and a
// question id; the card shows that entry from the question set service, the
// same entry every other surface shows. The attached owner answers in place:
// the Answer press is the owner's explicit Save of the set file, reported as
// saved only after the push, and as "applied, not published" (with Retry)
// when the write landed but publication failed
// (`g5.documents.question.answer-checkpoint`). Readers and a stopped
// workspace see the answer read-only; nothing here starts compute. Everyone
// who can read the workspace votes from the same card, and the card shows
// the tally per option with the voters' names (`g5.documents.question.votes`);
// a vote is a platform record and never a decision. A missing or invalid set
// renders a plain notice and never blocks the document. On a meeting decision
// card (a `meeting-` set with the template's options) the owner decides in
// one press and no votes are shown. A task option creates the tracker issue
// first (directly for a trusted destination, through the shared dialog for an
// untrusted one), records the decision with the issue's reference in the
// comment, and for "attach" links the issue to this workspace as related; a
// decision whose comment already names an issue creates no second one. The
// earlier four-option sets keep Create issue for Delay and Out of scope
// (`g5.workbench.meeting`).
// =============================================================================

import React, { useEffect, useMemo, useRef, useState } from "react";
import { Chip, classNames } from "../ui/UI";
import { IdentityAvatar } from "../shared/IdentityAvatar";
import {
  SEVERITY_LABEL,
  SEVERITY_STATE,
  questionUrgencyStyle,
} from "../../lib/severity";
import { randomUuid } from "../../lib/randomUuid";
import {
  answerRefusal,
  answeredAgainstEarlierRevision,
  findEntry,
  parseQuestionRef,
  optionTallyLabel,
  optionVoterNames,
  tallyVotes,
  type AnswerRequest,
  type QuestionEntry,
  type QuestionSetView,
  type VoteTally,
} from "../../lib/questionSet";
import {
  useQuestionSet,
  useQuestionVotes,
  useQuestionVoteWrite,
  useQuestionWrite,
} from "../../hooks/useQuestionSets";
import {
  useCreateIssue,
  useExternalIssuesFeature,
  useIssueCreateTarget,
  useLinkWorkspaceIssue,
  useRouteOrgMembership,
  useWorkspaceIssueLinks,
} from "../../hooks/useExternalIssues";
import {
  EXTERNAL_ISSUES_OFF_MESSAGE,
  createTargetBlockedReason,
  describeCreateIssueError,
} from "../../lib/issueCommentErrors";
import {
  appendIssueReference,
  meetingCardShape,
  meetingDecisionIssueOption,
  meetingIssueBody,
  meetingTaskKind,
  recordedIssueReference,
  type MeetingTaskKind,
} from "../../lib/meetingDecisions";
import type { SetQuestionOption } from "../../lib/questionSet";
import type { CreatedIssueDTO } from "../../lib/types";
import { CreateIssueDialog } from "../external-issues/CreateIssueDialog";

/** The task options wait for the links so attach can name its parent. */
export const MEETING_TASK_LINKS_LOADING_REASON = "Loading linked issues…";
export const MEETING_TASK_LINKS_UNREADABLE_REASON =
  "Linked issues could not be read; retry from Issues & tasks";
export const MEETING_TASK_TRACKER_CHECKING_REASON =
  "Checking the issue tracker…";
export const MEETING_TASK_TRACKER_UNAVAILABLE_REASON =
  "The issue tracker could not be checked";

/** An issue a task press created or reuses: enough to reference and link. */
type TaskIssue = Pick<CreatedIssueDTO, "displayKey" | "url">;

type TaskPhase =
  | { kind: "idle" }
  | { kind: "creating"; task: MeetingTaskKind }
  | { kind: "linking" }
  | {
      /** Direct creation gave no definite answer; check with the same key. */
      kind: "unclear";
      task: MeetingTaskKind;
      key: string;
      parent: string | undefined;
      revision: number;
      message: string;
    }
  | { kind: "error"; message: string };

export interface ReferencedQuestionCardProps {
  /** Body of the `g5-question-ref` fence. */
  content: string;
  workspaceId: string | null;
}

type SavePhase =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "saved" }
  | {
      kind: "applied";
      request: AnswerRequest;
      mode: "answer" | "clear";
      hash: string;
      message: string;
    }
  | { kind: "notice"; message: string; tone: "warn" | "blocked" };

type VotePhase =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "voted" }
  | { kind: "withdrawn" }
  | { kind: "error"; message: string };

function Notice({
  testId,
  children,
}: {
  testId: string;
  children: React.ReactNode;
}) {
  return (
    <div
      data-testid={testId}
      className="my-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-xs text-[var(--g5-text2)]"
    >
      {children}
    </div>
  );
}

export const ReferencedQuestionCard: React.FC<ReferencedQuestionCardProps> = ({
  content,
  workspaceId,
}) => {
  const ref = useMemo(() => parseQuestionRef(content), [content]);
  const path = ref.ok ? ref.path : null;
  const setQuery = useQuestionSet(workspaceId, path);
  if (!ref.ok) {
    return (
      <Notice testId="referenced-question-invalid-ref">
        Invalid question reference ({ref.detail}).
      </Notice>
    );
  }
  if (!workspaceId) {
    return (
      <Notice testId="referenced-question-unresolved">
        Question {ref.id} in {ref.path} cannot be shown here.
      </Notice>
    );
  }
  if (setQuery.isPending) {
    return (
      <Notice testId="referenced-question-loading">Loading question…</Notice>
    );
  }
  if (setQuery.isError || !setQuery.data) {
    return (
      <Notice testId="referenced-question-missing">
        Question set {ref.path} was not found.
      </Notice>
    );
  }
  const set = setQuery.data;
  if (set.invalid) {
    return (
      <Notice testId="referenced-question-set-invalid">
        {ref.path} is not a valid question set ({set.invalid.code}); its
        questions cannot be answered until the file is repaired.
      </Notice>
    );
  }
  const entry = findEntry(set, ref.id);
  if (!entry) {
    return (
      <Notice testId="referenced-question-missing">
        Question {ref.id} is not in {ref.path}.
      </Notice>
    );
  }
  return (
    <ReferencedQuestionEntryCard
      workspaceId={workspaceId}
      set={set}
      entry={entry}
    />
  );
};

function ReferencedQuestionEntryCard({
  workspaceId,
  set,
  entry,
}: {
  workspaceId: string;
  set: QuestionSetView;
  entry: QuestionEntry;
}) {
  const { write, install, refresh } = useQuestionWrite(workspaceId);
  // A meeting decision card decides in one press and shows no votes, so it
  // reads none (`g5.workbench.meeting`).
  const meetingShape = useMemo(
    () => meetingCardShape(set, entry),
    [set, entry],
  );
  const meetingCard = meetingShape !== null;
  const taskCard = meetingShape === "task";
  const votesQuery = useQuestionVotes(
    workspaceId,
    meetingCard ? null : set.path,
  );
  const { vote, withdraw } = useQuestionVoteWrite(workspaceId);
  const tally = useMemo(
    () => tallyVotes(entry, votesQuery.data),
    [entry, votesQuery.data],
  );
  const [selected, setSelected] = useState<string[]>(
    entry.answer.selectedOptionIds,
  );
  const [text, setText] = useState(entry.answer.text);
  const [rationale, setRationale] = useState(entry.answer.rationale);
  const [phase, setPhase] = useState<SavePhase>({ kind: "idle" });
  const [votePhase, setVotePhase] = useState<VotePhase>({ kind: "idle" });
  // A meeting decision delayed or moved out of scope may become an issue;
  // the created issue's reference goes into the comment and is recorded by
  // the Answer that creating it triggers.
  const [createdIssue, setCreatedIssue] = useState<{
    issue: CreatedIssueDTO;
    /** The reference is in a saved answer; it is no longer restored. */
    recorded: boolean;
  } | null>(null);
  const seenRevision = useRef(entry.revision);

  // A new revision (another tab, the agent, an authoring edit) replaces the
  // inputs; an unchanged revision keeps what the person is typing.
  useEffect(() => {
    if (seenRevision.current === entry.revision) return;
    seenRevision.current = entry.revision;
    setSelected(entry.answer.selectedOptionIds);
    // A created issue not yet recorded keeps its reference in the comment
    // until a saved answer carries it; after that the comment is the owner's.
    let text = entry.answer.text;
    if (createdIssue && !createdIssue.recorded) {
      if (text.includes(createdIssue.issue.url)) {
        setCreatedIssue({ ...createdIssue, recorded: true });
      } else {
        text = appendIssueReference(text, createdIssue.issue);
      }
    }
    setText(text);
    setRationale(entry.answer.rationale);
  }, [entry, createdIssue]);

  const answerable = set.answerable;
  const [taskPhase, setTaskPhase] = useState<TaskPhase>({ kind: "idle" });
  const busy =
    phase.kind === "saving" ||
    votePhase.kind === "saving" ||
    taskPhase.kind === "creating" ||
    taskPhase.kind === "linking";
  const proposed = entry.status === "proposed";
  const decided = entry.decision;
  const earlierRevision = answeredAgainstEarlierRevision(entry);
  const answeredBy = entry.answer.answeredBy;
  const answeredByLabel =
    answeredBy?.email ?? answeredBy?.id ?? answeredBy?.kind ?? "";
  const labelOf = (id: string) =>
    entry.options.find((option) => option.id === id)?.label ?? id;

  /** Saves the answer; `true` when the write landed (published or not). */
  const run = async (
    request: AnswerRequest,
    mode: "answer" | "clear",
  ): Promise<boolean> => {
    setPhase({ kind: "saving" });
    try {
      const outcome = await write(request, mode);
      if (outcome.publication.status === "notPublished") {
        setPhase({
          kind: "applied",
          request,
          mode,
          hash: outcome.hash,
          message: outcome.publication.message,
        });
      } else {
        setPhase({ kind: "saved" });
      }
      return true;
    } catch (error) {
      const refusal = answerRefusal(error);
      switch (refusal.kind) {
        case "revision-moved":
          if (refusal.current) install(set.path, refusal.current);
          else refresh(set.path);
          setPhase({
            kind: "notice",
            tone: "warn",
            message:
              "This question changed since you opened it. Review the current state and answer again.",
          });
          return false;
        case "reload-required":
          refresh(set.path);
          setPhase({ kind: "notice", tone: "warn", message: refusal.message });
          return false;
        case "key-reused":
          refresh(set.path);
          setPhase({
            kind: "notice",
            tone: "blocked",
            message:
              "This answer conflicts with an earlier request. Reload and answer again.",
          });
          return false;
        default:
          setPhase({
            kind: "notice",
            tone: "blocked",
            message: refusal.message,
          });
          return false;
      }
    }
  };

  /** The owner's Answer with these fields; identity and key are the card's. */
  const answerWith = (fields: {
    selectedOptionIds: string[];
    text: string;
    /** The revision the owner acted on, when it was captured earlier. */
    expectedRevision?: number;
  }) =>
    void run(
      {
        path: set.path,
        questionId: entry.id,
        expectedRevision: fields.expectedRevision ?? entry.revision,
        idempotencyKey: `ui:${randomUuid()}`,
        selectedOptionIds: fields.selectedOptionIds,
        text: fields.text.trim(),
        rationale: rationale.trim(),
      },
      "answer",
    );
  const submit = (mode: "answer" | "clear") =>
    mode === "answer"
      ? answerWith({ selectedOptionIds: selected, text })
      : void run(
          {
            path: set.path,
            questionId: entry.id,
            expectedRevision: entry.revision,
            idempotencyKey: `ui:${randomUuid()}`,
          },
          "clear",
        );

  const retry = () => {
    if (phase.kind !== "applied") return;
    void run({ ...phase.request, appliedHash: phase.hash }, phase.mode);
  };

  // Legacy meeting cards: Delay / Out of scope may become an issue through
  // the Create issue button.
  const issueOption = useMemo(
    () =>
      meetingShape === "legacy"
        ? meetingDecisionIssueOption(set, entry, selected)
        : null,
    [meetingShape, set, entry, selected],
  );

  // Task cards: creation and linking owners. The queries run only for a
  // task card (`enabled`), so ordinary cards cost nothing here.
  const membership = useRouteOrgMembership();
  const feature = useExternalIssuesFeature(
    taskCard ? membership?.orgId : undefined,
  );
  const orgId = membership?.orgId ?? "";
  const issuesEnabled =
    taskCard && feature.status === "enabled" && !!membership;
  const createTarget = useIssueCreateTarget(orgId, issuesEnabled);
  const createIssue = useCreateIssue(orgId);
  const links = useWorkspaceIssueLinks(workspaceId, issuesEnabled);
  const linkIssue = useLinkWorkspaceIssue(workspaceId);
  const [taskDialog, setTaskDialog] = useState<{
    task: MeetingTaskKind;
    revision: number;
    parent: string | undefined;
  } | null>(null);
  const [taskStatus, setTaskStatus] = useState("");
  const [linkFailed, setLinkFailed] = useState<TaskIssue | null>(null);
  const [attachedKey, setAttachedKey] = useState<string | null>(null);
  const destination =
    createTarget.status === "ok" ? createTarget.data.destination : null;
  const primaryLink =
    links.status === "ok"
      ? links.data.links.find((link) => link.role === "primary")
      : undefined;
  // The quick-add rule: the primary issue is the parent only when it lives in
  // the creation repository (g5.workbench.issues.create).
  const attachParent =
    primaryLink &&
    destination &&
    primaryLink.locator.repositoryId === destination.repositoryId
      ? primaryLink.displayKey
      : undefined;
  // An issue an earlier press created for this decision: no second issue.
  const existingTaskIssue: TaskIssue | null =
    recordedIssueReference(entry.answer.text) ?? createdIssue?.issue ?? null;

  /** Why a task option cannot be pressed now, or `null`. */
  const taskOptionReason = (task: MeetingTaskKind): string | null => {
    if (!membership) return EXTERNAL_ISSUES_OFF_MESSAGE;
    if (feature.status === "loading")
      return MEETING_TASK_TRACKER_CHECKING_REASON;
    if (feature.status === "error")
      return MEETING_TASK_TRACKER_UNAVAILABLE_REASON;
    if (feature.status === "disabled") return EXTERNAL_ISSUES_OFF_MESSAGE;
    if (existingTaskIssue) return null;
    const blocked = createTargetBlockedReason(createTarget);
    if (blocked) return blocked;
    if (task === "attach") {
      if (links.status === "loading") return MEETING_TASK_LINKS_LOADING_REASON;
      if (links.status === "error") return MEETING_TASK_LINKS_UNREADABLE_REASON;
    }
    return null;
  };

  const taskBody = (task: MeetingTaskKind) =>
    meetingIssueBody(
      entry,
      { id: task, label: labelOf(task) },
      text,
      set.title,
    );

  /** Step 3 of attach: link the issue; a refusal keeps issue and decision. */
  const attachIssue = async (issue: TaskIssue) => {
    setTaskPhase({ kind: "linking" });
    try {
      await linkIssue.mutateAsync({ ref: issue.displayKey, role: "related" });
      setLinkFailed(null);
      setAttachedKey(issue.displayKey);
      setTaskStatus(`Task ${issue.displayKey} attached to this workspace.`);
    } catch {
      setLinkFailed(issue);
      setTaskStatus(
        `Task ${issue.displayKey} created, but linking it to this workspace failed.`,
      );
    } finally {
      setTaskPhase({ kind: "idle" });
    }
  };

  /** Step 2 (record) and, for attach, step 3 (link) for an existing issue. */
  const finishTask = async (
    task: MeetingTaskKind,
    issue: CreatedIssueDTO,
    revision: number,
  ) => {
    setTaskPhase({ kind: "idle" });
    setCreatedIssue({ issue, recorded: false });
    const appended = appendIssueReference(text, issue);
    setText(appended);
    await run(
      {
        path: set.path,
        questionId: entry.id,
        expectedRevision: revision,
        idempotencyKey: `ui:${randomUuid()}`,
        selectedOptionIds: [task],
        text: appended.trim(),
        rationale: rationale.trim(),
      },
      "answer",
    );
    if (task === "attach") await attachIssue(issue);
    else {
      setTaskStatus(
        `Task ${issue.displayKey} created, not linked to this workspace.`,
      );
    }
  };

  /** Step 1 for a trusted destination: create from the press. */
  const createTask = async (
    task: MeetingTaskKind,
    key: string,
    parent: string | undefined,
    revision: number,
    checkOnly = false,
  ) => {
    if (!destination) return;
    setTaskPhase({ kind: "creating", task });
    try {
      const result = await createIssue.mutateAsync({
        title: entry.prompt,
        body: taskBody(task),
        assignees: [],
        idempotencyKey: key,
        domainId: destination.domainId,
        checkOnly,
        ...(parent ? { parent } : {}),
      });
      if (result.state === "created" && result.issue) {
        await finishTask(task, result.issue, revision);
        return;
      }
      setTaskPhase({
        kind: "unclear",
        task,
        key,
        parent,
        revision,
        message:
          result.state === "outcome_unknown"
            ? "GitHub gave no definite answer. Check again before pressing the option a second time."
            : "No task from this press was found in the tracker.",
      });
    } catch (error) {
      setTaskPhase({ kind: "error", message: describeCreateIssueError(error) });
    }
  };

  /** A task option press: the whole sequence, or the dialog first. */
  const decideWithTask = (task: MeetingTaskKind) => {
    const revision = entry.revision;
    setSelected([task]);
    setLinkFailed(null);
    setTaskStatus("");
    if (existingTaskIssue) {
      const issue = existingTaskIssue;
      const appended = appendIssueReference(text, issue);
      setText(appended);
      void (async () => {
        await run(
          {
            path: set.path,
            questionId: entry.id,
            expectedRevision: revision,
            idempotencyKey: `ui:${randomUuid()}`,
            selectedOptionIds: [task],
            text: appended.trim(),
            rationale: rationale.trim(),
          },
          "answer",
        );
        if (task === "attach") await attachIssue(issue);
        else setTaskStatus(`Decision recorded; task ${issue.displayKey} kept.`);
      })();
      return;
    }
    const parent = task === "attach" ? attachParent : undefined;
    if (destination && !destination.contentTrusted) {
      // The admission rule: an untrusted destination reviews the payload.
      setTaskDialog({ task, revision, parent });
      return;
    }
    void createTask(task, randomUuid(), parent, revision);
  };
  // The decision the dialog was opened for: a question that changed while
  // the dialog was open is a moved revision, refused and shown as such.
  const createIssueOpened = useRef<{
    revision: number;
    selectedOptionIds: string[];
  } | null>(null);
  const recordCreatedIssue = (issue: CreatedIssueDTO) => {
    const opened = createIssueOpened.current;
    const appended = appendIssueReference(text, issue);
    setText(appended);
    setCreatedIssue({ issue, recorded: false });
    answerWith({
      selectedOptionIds: opened?.selectedOptionIds ?? selected,
      text: appended,
      expectedRevision: opened?.revision,
    });
  };

  const voteRefused = (error: unknown) => {
    const refusal = answerRefusal(error);
    if (refusal.kind === "revision-moved") {
      if (refusal.current) install(set.path, refusal.current);
      else refresh(set.path);
      setVotePhase({
        kind: "error",
        message:
          "This question changed since you opened it. Review it and vote again.",
      });
      return;
    }
    setVotePhase({
      kind: "error",
      message: "message" in refusal ? refusal.message : "The vote was refused.",
    });
  };

  const castVote = async () => {
    setVotePhase({ kind: "saving" });
    try {
      await vote({
        path: set.path,
        questionId: entry.id,
        expectedRevision: entry.revision,
        selectedOptionIds: selected,
        text: text.trim(),
      });
      setVotePhase({ kind: "voted" });
    } catch (error) {
      voteRefused(error);
    }
  };

  const withdrawVote = async () => {
    setVotePhase({ kind: "saving" });
    try {
      await withdraw(set.path, entry.id);
      setVotePhase({ kind: "withdrawn" });
    } catch (error) {
      voteRefused(error);
    }
  };

  const toggle = (id: string) => {
    if (meetingCard && answerable) {
      // One press decides; pressing the decided option again only restores
      // it locally (after a refused change left another option selected).
      if (decided && entry.answer.selectedOptionIds.includes(id)) {
        setSelected([id]);
        return;
      }
      const task = taskCard ? meetingTaskKind(id) : null;
      if (task) {
        decideWithTask(task);
        return;
      }
      setSelected([id]);
      answerWith({ selectedOptionIds: [id], text });
      return;
    }
    setSelected((current) =>
      entry.multiSelect
        ? current.includes(id)
          ? current.filter((value) => value !== id)
          : [...current, id]
        : current.includes(id)
          ? []
          : [id],
    );
  };

  const hasAnswerInput = selected.length > 0 || text.trim().length > 0;
  const votesReady = votesQuery.isSuccess;
  const viewerVote = tally.viewerVote;
  const unchangedFromVote =
    viewerVote != null &&
    selected.join("\u0000") === viewerVote.selectedOptionIds.join("\u0000") &&
    text.trim() === viewerVote.text;
  const unchangedFromDecision =
    decided &&
    selected.join("\u0000") === entry.answer.selectedOptionIds.join("\u0000") &&
    text.trim() === entry.answer.text &&
    rationale.trim() === entry.answer.rationale;

  return (
    <div
      data-testid="referenced-question-card"
      data-question-id={entry.id}
      data-question-status={entry.status}
      data-question-revision={entry.revision}
      className="my-2 mb-3 space-y-3 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3"
      style={questionUrgencyStyle(entry.severity ?? null, decided)}
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono text-xs text-[var(--g5-text2)]">
          Q{entry.displayNumber} · {entry.id}
        </span>
        <Chip
          state={decided ? "ok" : "warn"}
          text={decided ? "Answered" : proposed ? "Proposed" : "Open"}
          data-testid="referenced-question-status-chip"
        />
        {entry.severity && (
          <Chip
            state={decided ? "muted" : SEVERITY_STATE[entry.severity]}
            text={SEVERITY_LABEL[entry.severity]}
          />
        )}
        {entry.severity === "critical" && !decided && (
          <Chip state="blocked" text="Blocking" />
        )}
        <span className="ml-auto text-[10px] text-[var(--g5-text2)]">
          {set.title ?? set.setId}
        </span>
      </div>

      <p className="text-sm text-[var(--g5-text0)]">{entry.prompt}</p>
      {entry.context && (
        <p className="text-xs text-[var(--g5-text1)]">{entry.context}</p>
      )}

      {entry.options.length > 0 && (
        <div
          className="space-y-2"
          role={entry.multiSelect ? "group" : "radiogroup"}
          aria-label="Options"
        >
          {entry.options.map((option, index) => {
            const isSelected = selected.includes(option.id);
            const task = taskCard ? meetingTaskKind(option.id) : null;
            const taskReason =
              task && answerable ? taskOptionReason(task) : null;
            const taskPending =
              task &&
              ((taskPhase.kind === "creating" && taskPhase.task === task) ||
                (taskPhase.kind === "linking" && task === "attach"));
            return (
              <button
                key={option.id}
                type="button"
                role={entry.multiSelect ? "checkbox" : "radio"}
                aria-checked={isSelected}
                data-testid="referenced-question-option"
                data-option-id={option.id}
                disabled={
                  busy || (meetingCard && !answerable) || taskReason !== null
                }
                title={taskReason ?? undefined}
                onClick={() => toggle(option.id)}
                className={classNames(
                  "block w-full rounded-[var(--g5-radius-sm)] border px-3 py-2 text-left",
                  isSelected
                    ? "border-[var(--g5-info)] bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)]"
                    : option.recommended
                      ? "border-[var(--g5-ok)] bg-[color-mix(in_srgb,var(--g5-ok)_5%,transparent)]"
                      : "border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
                  "disabled:cursor-default",
                )}
              >
                <span className="flex items-center gap-2">
                  <span className="text-xs font-mono text-[var(--g5-text2)]">
                    #{index + 1}
                  </span>
                  <span className="text-sm font-medium text-[var(--g5-text0)]">
                    {option.label}
                  </span>
                  {taskPending && (
                    <span
                      data-testid="referenced-question-task-pending"
                      className="text-xs text-[var(--g5-text2)]"
                    >
                      {taskPhase.kind === "linking"
                        ? "Attaching task…"
                        : "Creating task…"}
                    </span>
                  )}
                  {option.recommended && (
                    <span className="text-[10px] font-semibold uppercase tracking-wide text-[var(--g5-ok)]">
                      Recommended
                    </span>
                  )}
                </span>
                {option.description && (
                  <span className="mt-1 block text-xs text-[var(--g5-text1)]">
                    {option.description}
                  </span>
                )}
                {option.tradeoffs && (
                  <span className="mt-1 block text-xs text-[var(--g5-text2)]">
                    Tradeoffs: {option.tradeoffs}
                  </span>
                )}
                {option.risks && (
                  <span className="mt-1 block text-xs text-[var(--g5-text2)]">
                    Risks: {option.risks}
                  </span>
                )}
                <OptionTally tally={tally} optionId={option.id} />
              </button>
            );
          })}
        </div>
      )}

      {taskCard && (
        <span
          role="status"
          aria-live="polite"
          data-testid="referenced-question-task-status"
          className="sr-only"
        >
          {taskStatus}
        </span>
      )}
      {taskPhase.kind === "unclear" && (
        <div
          role="alert"
          data-testid="referenced-question-task-unclear"
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-2 text-xs text-[var(--g5-text0)]"
        >
          <p>{taskPhase.message}</p>
          <div className="mt-1 flex gap-2">
            <button
              type="button"
              data-testid="referenced-question-task-check"
              onClick={() =>
                void createTask(
                  taskPhase.task,
                  taskPhase.key,
                  taskPhase.parent,
                  taskPhase.revision,
                  true,
                )
              }
              className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-0.5"
            >
              Check again
            </button>
            <button
              type="button"
              data-testid="referenced-question-task-unclear-dismiss"
              onClick={() => setTaskPhase({ kind: "idle" })}
              className="text-[var(--g5-text2)] underline"
            >
              Dismiss
            </button>
          </div>
        </div>
      )}
      {taskPhase.kind === "error" && (
        <div
          role="alert"
          data-testid="referenced-question-task-error"
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-2 text-xs text-[var(--g5-blocked)]"
        >
          {taskPhase.message}
        </div>
      )}
      {linkFailed && (
        <div
          role="alert"
          data-testid="referenced-question-link-failed"
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-3 py-2 text-xs text-[var(--g5-text0)]"
        >
          Created{" "}
          <a
            href={linkFailed.url}
            target="_blank"
            rel="noopener noreferrer"
            className="text-[var(--g5-info)] hover:underline"
          >
            {linkFailed.displayKey}
          </a>{" "}
          but could not link it to this workspace. Link it from Issues &amp;
          tasks, or press the option again.
        </div>
      )}
      {taskDialog && membership && (
        <CreateIssueDialog
          orgId={membership.orgId}
          open
          onClose={() => setTaskDialog(null)}
          onCreated={(issue) => {
            const opened = taskDialog;
            setTaskDialog(null);
            void finishTask(opened.task, issue, opened.revision);
          }}
          initialTitle={entry.prompt}
          initialBody={taskBody(taskDialog.task)}
          initialParent={taskDialog.parent}
        />
      )}

      {!meetingCard && tally.withText.length > 0 && (
        <ul
          data-testid="referenced-question-vote-texts"
          className="space-y-1 text-xs text-[var(--g5-text1)]"
        >
          {tally.withText.map((entryVote) => (
            <li key={entryVote.userId}>
              <span className="font-medium">{entryVote.displayName}:</span>{" "}
              {entryVote.text}
            </li>
          ))}
        </ul>
      )}

      {entry.status !== "open" && (
        <div
          data-testid="referenced-question-answer-summary"
          className="space-y-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-xs"
        >
          {proposed && (
            <p
              data-testid="referenced-question-proposal"
              className="font-medium text-[var(--g5-warn)]"
            >
              Proposed by an agent — not a decision until you confirm it.
            </p>
          )}
          {entry.answer.selectedOptionIds.length > 0 && (
            <p className="text-[var(--g5-text0)]">
              {entry.answer.selectedOptionIds.map(labelOf).join(", ")}
            </p>
          )}
          {entry.answer.text && (
            <p className="text-[var(--g5-text0)]">{entry.answer.text}</p>
          )}
          {entry.answer.rationale && (
            <p className="text-[var(--g5-text1)]">
              <span className="font-medium">Rationale:</span>{" "}
              {entry.answer.rationale}
            </p>
          )}
          <div
            data-testid="referenced-question-answered-by"
            className="flex items-center gap-1.5 text-[10px] text-[var(--g5-text2)]"
          >
            {answeredByLabel ? (
              <IdentityAvatar
                identity={answeredByLabel}
                avatarUrl={answeredBy?.avatarUrl}
                size={14}
              />
            ) : null}
            <span>
              {proposed ? "Proposed" : "Answered"}
              {answeredBy?.kind === "agent"
                ? " by the agent"
                : answeredByLabel
                  ? ` by ${answeredByLabel}`
                  : ""}
              {entry.answer.answeredAt ? ` at ${entry.answer.answeredAt}` : ""}
            </span>
          </div>
          {earlierRevision != null && (
            <p
              data-testid="referenced-question-earlier-revision"
              className="text-[10px] text-[var(--g5-warn)]"
            >
              Answered against revision {earlierRevision}; the question has
              since been reworded (revision {entry.revision}).
            </p>
          )}
        </div>
      )}

      {entry.allowFreeText && (!meetingCard || answerable) && (
        <textarea
          data-testid="referenced-question-text"
          aria-label={
            meetingCard ? "Comment" : answerable ? "Answer text" : "Vote text"
          }
          value={text}
          disabled={busy}
          onChange={(event) => setText(event.target.value)}
          rows={2}
          placeholder={
            meetingCard
              ? "Add a comment (optional)"
              : answerable
                ? "Answer or vote in your own words (optional)"
                : "Vote in your own words (optional)"
          }
          className="w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-xs text-[var(--g5-text0)]"
        />
      )}

      {!meetingCard && (
        <div
          data-testid="referenced-question-vote-controls"
          className="flex flex-wrap items-center gap-2"
        >
          <button
            type="button"
            data-testid="referenced-question-vote"
            disabled={
              busy || !votesReady || !hasAnswerInput || unchangedFromVote
            }
            onClick={() => void castVote()}
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 text-xs font-medium text-[var(--g5-text0)] disabled:opacity-50"
          >
            {viewerVote ? "Change vote" : "Vote"}
          </button>
          {viewerVote && (
            <button
              type="button"
              data-testid="referenced-question-withdraw-vote"
              disabled={busy}
              onClick={() => void withdrawVote()}
              className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-1 text-xs text-[var(--g5-text2)] disabled:opacity-50"
            >
              Withdraw vote
            </button>
          )}
          <span
            data-testid="referenced-question-vote-count"
            data-votes-state={votesQuery.status}
            className="text-[10px] text-[var(--g5-text2)]"
          >
            {votesQuery.isError ? (
              <>
                Votes unavailable.{" "}
                <button
                  type="button"
                  data-testid="referenced-question-votes-retry"
                  onClick={() => void votesQuery.refetch()}
                  className="underline"
                >
                  Retry
                </button>
              </>
            ) : votesQuery.isPending ? (
              "Loading votes…"
            ) : (
              <>
                {tally.total === 0
                  ? "No votes yet"
                  : `${tally.total} ${tally.total === 1 ? "vote" : "votes"}`}
                {viewerVote ? " · you voted" : ""}
              </>
            )}
          </span>
          <VoteState phase={votePhase} />
        </div>
      )}

      {answerable ? (
        <div className="space-y-2">
          <input
            data-testid="referenced-question-rationale"
            aria-label="Rationale"
            value={rationale}
            disabled={busy}
            onChange={(event) => setRationale(event.target.value)}
            placeholder="Rationale (optional)"
            className="w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-1 text-xs text-[var(--g5-text0)]"
          />
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              data-testid="referenced-question-answer"
              disabled={busy || !hasAnswerInput || unchangedFromDecision}
              onClick={() => submit("answer")}
              className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-2 py-1 text-xs font-medium text-[var(--g5-text0)] disabled:opacity-50"
            >
              {proposed
                ? "Confirm answer"
                : decided
                  ? "Change answer"
                  : "Answer"}
            </button>
            {entry.status !== "open" && (
              <button
                type="button"
                data-testid="referenced-question-clear"
                disabled={busy}
                onClick={() => submit("clear")}
                className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-1 text-xs text-[var(--g5-text2)] disabled:opacity-50"
              >
                {proposed ? "Reject proposal" : "Clear answer"}
              </button>
            )}
            {meetingShape === "legacy" && (
              <MeetingDecisionIssue
                entry={entry}
                option={issueOption}
                comment={text}
                meetingTitle={set.title}
                busy={busy}
                onOpen={() => {
                  createIssueOpened.current = {
                    revision: entry.revision,
                    selectedOptionIds: selected,
                  };
                }}
                onCreated={recordCreatedIssue}
              />
            )}
            <SaveState phase={phase} onRetry={retry} />
          </div>
          {createdIssue && (
            <p
              data-testid="referenced-question-created-issue"
              className="text-[10px] text-[var(--g5-text1)]"
            >
              Created{" "}
              <a
                href={createdIssue.issue.url}
                target="_blank"
                rel="noopener noreferrer"
                className="text-[var(--g5-info)] hover:underline"
              >
                {createdIssue.issue.displayKey}
              </a>
              ; its reference is in the comment
              {createdIssue.recorded ? "." : " — save the answer to record it."}
              {taskCard && attachedKey === createdIssue.issue.displayKey
                ? " Attached to this workspace."
                : taskCard && !linkFailed
                  ? " Not linked to this workspace."
                  : ""}
            </p>
          )}
        </div>
      ) : (
        <p
          data-testid="referenced-question-read-only"
          className="text-[10px] text-[var(--g5-text2)]"
        >
          {meetingCard
            ? "The workspace owner decides here while the workspace is running."
            : "Votes inform the decision; the workspace owner answers here while the workspace is running."}
        </p>
      )}
    </div>
  );
}

/**
 * Create issue for a delayed or out-of-scope meeting decision: the
 * organization's Create issue dialog, pre-filled from the decision, offered
 * only while creation is possible and disabled with the target's reason
 * otherwise (`g5.workbench.meeting`). Absent when external issues are off.
 */
function MeetingDecisionIssue({
  entry,
  option,
  comment,
  meetingTitle,
  busy,
  onOpen,
  onCreated,
}: {
  entry: QuestionEntry;
  /** The chosen Delay or Out of scope option; `null` offers no button. */
  option: SetQuestionOption | null;
  comment: string;
  meetingTitle: string | undefined;
  busy: boolean;
  /** The dialog opens: the card captures the decision it is for. */
  onOpen: () => void;
  onCreated: (issue: CreatedIssueDTO) => void;
}) {
  const membership = useRouteOrgMembership();
  const feature = useExternalIssuesFeature(membership?.orgId);
  const enabled = feature.status === "enabled" && !!membership;
  const createTarget = useIssueCreateTarget(membership?.orgId ?? "", enabled);
  // The dialog is for the decision it was opened on: a change to the
  // question meanwhile (another tab's answer) does not unmount it, so an
  // unclear creation keeps its key and recovery until the owner closes it.
  const [opened, setOpened] = useState<{ option: SetQuestionOption } | null>(
    null,
  );
  if (!enabled) return null;
  const reason = createTargetBlockedReason(createTarget);
  return (
    <>
      {option && (
        <button
          type="button"
          data-testid="referenced-question-create-issue"
          disabled={busy || reason !== null}
          title={
            reason ??
            `Create an issue for this ${option.label.toLowerCase()} decision and record it in the comment`
          }
          onClick={() => {
            onOpen();
            setOpened({ option });
          }}
          className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-1 text-xs text-[var(--g5-text1)] disabled:opacity-50"
        >
          Create issue…
        </button>
      )}
      {opened && (
        <CreateIssueDialog
          orgId={membership.orgId}
          open
          onClose={() => setOpened(null)}
          onCreated={(issue) => {
            setOpened(null);
            onCreated(issue);
          }}
          initialTitle={entry.prompt}
          initialBody={meetingIssueBody(
            entry,
            opened.option,
            comment,
            meetingTitle,
          )}
        />
      )}
    </>
  );
}

/** The votes one option received, with who cast them. */
function OptionTally({
  tally,
  optionId,
}: {
  tally: VoteTally;
  optionId: string;
}) {
  if (tally.total === 0) return null;
  const names = optionVoterNames(tally, optionId);
  const share = Math.round((names.length / tally.total) * 100);
  return (
    <span
      data-testid="referenced-question-option-votes"
      data-option-votes={names.length}
      className="mt-1 flex items-center gap-2 text-[10px] text-[var(--g5-text2)]"
    >
      <span
        aria-hidden="true"
        className="h-1 w-16 overflow-hidden rounded bg-[var(--g5-bg2)]"
      >
        <span
          className="block h-full bg-[var(--g5-info)]"
          style={{ width: `${share}%` }}
        />
      </span>
      <span>{optionTallyLabel(names)}</span>
    </span>
  );
}

function VoteState({ phase }: { phase: VotePhase }) {
  switch (phase.kind) {
    case "idle":
      return null;
    case "saving":
      return (
        <span className="text-xs text-[var(--g5-text2)]">Saving vote…</span>
      );
    case "voted":
      return (
        <span
          data-testid="referenced-question-voted"
          className="text-xs text-[var(--g5-ok)]"
        >
          Vote recorded
        </span>
      );
    case "withdrawn":
      return (
        <span
          data-testid="referenced-question-vote-withdrawn"
          className="text-xs text-[var(--g5-text2)]"
        >
          Vote withdrawn
        </span>
      );
    case "error":
      return (
        <span
          data-testid="referenced-question-vote-error"
          className="text-xs text-[var(--g5-blocked)]"
        >
          {phase.message}
        </span>
      );
  }
}

function SaveState({
  phase,
  onRetry,
}: {
  phase: SavePhase;
  onRetry: () => void;
}) {
  switch (phase.kind) {
    case "idle":
      return null;
    case "saving":
      return (
        <span
          data-testid="referenced-question-saving"
          className="text-xs text-[var(--g5-text2)]"
        >
          Saving…
        </span>
      );
    case "saved":
      return (
        <span
          data-testid="referenced-question-saved"
          className="text-xs text-[var(--g5-ok)]"
        >
          Saved
        </span>
      );
    case "applied":
      return (
        <span
          data-testid="referenced-question-not-published"
          className="flex items-center gap-2 text-xs text-[var(--g5-warn)]"
        >
          Applied, not published ({phase.message}).
          <button
            type="button"
            data-testid="referenced-question-retry"
            onClick={onRetry}
            className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] px-2 py-0.5 text-[var(--g5-text0)]"
          >
            Retry publish
          </button>
        </span>
      );
    case "notice":
      return (
        <span
          data-testid="referenced-question-notice"
          className={classNames(
            "text-xs",
            phase.tone === "blocked"
              ? "text-[var(--g5-blocked)]"
              : "text-[var(--g5-warn)]",
          )}
        >
          {phase.message}
        </span>
      );
  }
}
