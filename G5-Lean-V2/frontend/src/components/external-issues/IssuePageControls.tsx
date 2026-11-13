// =============================================================================
// Issue page controls (g5.issue.native.page, g5.external-issues.edit)
// =============================================================================
// An issue's own page lets members with write permission comment, edit their
// own comments, close or reopen it, edit its title, description and
// assignees, and add or remove its labels — on a native G5 issue in G5, on a
// GitHub issue as their own connected GitHub account. Milestones and
// listeners are not edited here.
//
// A GitHub issue's write target says whether writes are possible now (a
// connected account, an issue domain opened for G5 comments, a repository
// GitHub reports as non-public); otherwise the controls stay disabled with
// the reason. On an untrusted domain every edit is reviewed first, showing
// exactly what is sent and as which account.
//
// Nobody overwrites a change they did not see: a native edit carries the
// version the editor read (`expectedUpdatedAt`); a GitHub edit carries what
// the form showed for each field it changes (`seen`), because GitHub bumps
// an issue's version on every comment. A comment edit carries the comment's
// `updatedAt`. A refused stale write asks the person to reload. An edit GitHub
// did not confirm, or applied without every assignee change, is said so,
// never shown as saved; saving the same values again is harmless.
//
// On a G5 issue the description and comment editors take images (paste,
// drop or Attach image), uploaded to the issue as they are added
// (g5.issue.native.attachments); GitHub's editors take none.
//
// Labels change by deltas from what the label editor started from (names
// added and removed, compared without regard to case), so a label someone
// else changed meanwhile is left alone (g5.issue.labels.contract). Each
// change reports its own outcome; a partial or unknown result lists them.
// =============================================================================

import { useId, useRef, useState, type ReactNode, type RefObject } from "react";
import { LoaderCircle } from "lucide-react";

import { Button, classNames } from "../ui/UI";
import {
  useChangeIssueLabels,
  useChangeIssuePageState,
  useEditGithubIssue,
  useEditIssuePageComment,
  useEditNativeIssue,
} from "../../hooks/useExternalIssues";
import { useOrgMembers } from "../../hooks/useOrg";
import { githubConnectionPath } from "../../lib/githubConnection";
import {
  COMMENT_EDIT_UNCONFIRMED_MESSAGE,
  ISSUE_WRITE_BLOCKED_REASONS,
  describeIssueStateError,
  describeIssueWriteError,
  isStaleIssueRefusal,
} from "../../lib/issueCommentErrors";
import {
  MAX_LABELS_PER_CHANGE,
  describeLabelError,
  labelDelta,
} from "../../lib/issueLabels";
import { isNativeIssue, type IssuePageWriter } from "../../lib/issuePage";
import { ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE } from "../../lib/issueState";
import { issueUserLabel } from "../../lib/nativeIssues";
import type {
  ExternalIssueCommentDTO,
  ExternalIssueDetailDTO,
  GithubIssueFields,
  IssueEditDTO,
  IssueLabelChangeDTO,
  IssueWriteTargetDTO,
  LabelChangeOutcomeDTO,
} from "../../lib/types";
import { G5_REFERENCE_AT_END } from "./ActivityCommentEditor";
import { MAX_ISSUE_TITLE_CHARS } from "./CreateIssueDialog";
import { useIssueImageUploads } from "./NativeAttachments";
import { IssueLabelPicker } from "./IssueLabelPicker";

const FIELD_CLASSES =
  "w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] disabled:opacity-60";

const PREVIEW_CLASSES =
  "max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)]";

function WriteError({
  testId,
  message,
  onReload,
}: {
  testId: string;
  message: string;
  onReload?: () => void;
}) {
  return (
    <div
      role="alert"
      data-testid={testId}
      className="mt-1.5 flex flex-wrap items-center gap-2 text-xs text-[var(--g5-blocked)]"
    >
      <p className="min-w-0 flex-1">{message}</p>
      {onReload && (
        <Button
          kind="secondary"
          data-testid={`${testId}-reload`}
          onClick={onReload}
          className="px-2.5 py-1 text-xs"
        >
          Reload
        </Button>
      )}
    </div>
  );
}

/** A write GitHub did not confirm; repeating it is harmless. */
function Unconfirmed({ testId, message }: { testId: string; message: string }) {
  return (
    <p
      role="status"
      data-testid={testId}
      className="mt-1.5 text-xs text-[var(--g5-warn)]"
    >
      {message}
    </p>
  );
}

/**
 * Why a GitHub issue cannot be changed from its page now, with the way to
 * fix it where there is one; who changes are made as when it can.
 */
export function IssueWriteTargetNotice({
  target,
}: {
  target: NonNullable<IssuePageWriter["target"]>;
}) {
  let content: ReactNode;
  if (target.status === "loading") {
    content = (
      <p role="status" className="text-[var(--g5-text2)]">
        Checking whether you can change this issue on GitHub…
      </p>
    );
  } else if (target.status === "error") {
    content = (
      <div role="alert" className="flex flex-wrap items-center gap-2">
        <p className="min-w-0 flex-1 text-[var(--g5-blocked)]">
          Could not check whether you can change this issue on GitHub:{" "}
          {target.error.message}
        </p>
        <Button
          kind="secondary"
          data-testid="issue-page-write-target-retry"
          onClick={target.refetch}
          className="px-2.5 py-1 text-xs"
        >
          Retry
        </Button>
      </div>
    );
  } else if (target.data.state === "ready") {
    content = (
      <p className="text-[var(--g5-text2)]">
        Edits, state changes and comments here are made on GitHub as @
        {target.data.githubLogin}.
      </p>
    );
  } else if (target.data.state === "connection_required") {
    content = (
      <div className="flex flex-wrap items-center gap-2">
        <p className="min-w-0 flex-1 text-[var(--g5-text1)]">
          Connect your GitHub account to edit, close or comment on this issue as
          yourself.
        </p>
        <a
          href={githubConnectionPath(
            `${window.location.pathname}${window.location.search}`,
          )}
          data-testid="issue-page-connect"
          className="rounded-[var(--g5-radius-sm)] bg-[var(--g5-info)] px-2.5 py-1 text-xs font-medium text-white no-underline hover:brightness-110"
        >
          Manage GitHub connection
        </a>
      </div>
    );
  } else {
    const detail =
      target.data.state === "unavailable" ? target.data.error?.message : null;
    content = (
      <p className="text-[var(--g5-text1)]">
        {ISSUE_WRITE_BLOCKED_REASONS[target.data.state]}
        {detail ? ` (${detail})` : ""}
      </p>
    );
  }
  return (
    <div
      data-testid="issue-page-write-target"
      data-state={target.status === "ok" ? target.data.state : target.status}
      className="mt-3 text-xs"
    >
      {content}
    </div>
  );
}

/** Close (as completed) or reopen the issue; repeating it is harmless. */
export function IssueStateButton({
  orgId,
  issue,
  disabled = false,
}: {
  orgId: string;
  issue: ExternalIssueDetailDTO;
  disabled?: boolean;
}) {
  const changeState = useChangeIssuePageState(orgId, issue.locator);
  const native = isNativeIssue(issue);
  const next = issue.state === "closed" ? "open" : "closed";
  return (
    <div className="flex flex-col items-end">
      <Button
        kind="secondary"
        data-testid="issue-page-state-button"
        disabled={disabled || changeState.isLoading}
        onClick={() => {
          changeState.reset();
          changeState.mutate(next);
        }}
      >
        {changeState.isLoading && (
          <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
        )}
        {next === "closed" ? "Close issue" : "Reopen issue"}
      </Button>
      {changeState.data?.outcome === "unknown" && (
        <Unconfirmed
          testId="issue-page-state-unconfirmed"
          message={ISSUE_STATE_UNKNOWN_OUTCOME_MESSAGE}
        />
      )}
      {changeState.error && (
        <WriteError
          testId="issue-page-state-error"
          message={
            native
              ? describeIssueWriteError(
                  changeState.error,
                  next === "closed"
                    ? "The issue could not be closed."
                    : "The issue could not be reopened.",
                )
              : describeIssueStateError(changeState.error, next)
          }
        />
      )}
    </div>
  );
}

/** GitHub applied an edit but not every requested assignee change. */
export function IssueEditPartialNotice({ result }: { result: IssueEditDTO }) {
  const logins = (values: string[] | undefined) =>
    (values ?? []).map((login) => `@${login}`).join(", ");
  return (
    <div
      role="alert"
      data-testid="issue-page-edit-partial"
      className="mt-3 rounded-lg border border-[rgb(242,162,73,0.3)] bg-[rgb(242,162,73,0.06)] px-3.5 py-2.5 text-xs leading-relaxed text-[var(--g5-text1)]"
    >
      GitHub saved the edit, but its answer is missing assignee changes.
      {result.notAssigned?.length
        ? ` Not assigned: ${logins(result.notAssigned)}.`
        : ""}
      {result.notUnassigned?.length
        ? ` Still assigned: ${logins(result.notUnassigned)}.`
        : ""}
      {result.message ? ` ${result.message}` : ""}
    </div>
  );
}

/** Title, description and assignee fields shared by both trackers' forms. */
function IssueEditFields({
  title,
  onTitle,
  body,
  onBody,
  candidates,
  assignees,
  onToggleAssignee,
  disabled,
  emptyCandidates,
  bodyImages,
}: {
  title: string;
  onTitle: (value: string) => void;
  body: string;
  onBody: (value: string) => void;
  /** A G5 issue's description takes images. */
  bodyImages?: ReturnType<typeof useIssueImageUploads> & {
    textareaRef: RefObject<HTMLTextAreaElement | null>;
  };
  /** Assignable people by login, with their labels. */
  candidates: Map<string, string>;
  assignees: string[];
  onToggleAssignee: (login: string) => void;
  disabled: boolean;
  emptyCandidates: string;
}) {
  const titleId = useId();
  const bodyId = useId();
  return (
    <>
      <div>
        <label
          htmlFor={titleId}
          className="mb-1 block text-sm font-medium text-[var(--g5-text1)]"
        >
          Title
        </label>
        <input
          id={titleId}
          type="text"
          data-testid="issue-page-edit-title"
          maxLength={MAX_ISSUE_TITLE_CHARS}
          value={title}
          onChange={(event) =>
            onTitle(event.target.value.replace(/[\r\n]+/g, " "))
          }
          className={FIELD_CLASSES}
        />
      </div>
      <div>
        <label
          htmlFor={bodyId}
          className="mb-1 block text-sm font-medium text-[var(--g5-text1)]"
        >
          Description{" "}
          <span className="font-normal text-[var(--g5-text2)]">(Markdown)</span>
        </label>
        <textarea
          ref={bodyImages?.textareaRef}
          id={bodyId}
          data-testid="issue-page-edit-body"
          rows={8}
          value={body}
          onChange={(event) => onBody(event.target.value)}
          {...bodyImages?.handlers}
          className={classNames(FIELD_CLASSES, "font-[var(--g5-font-mono)]")}
        />
        {bodyImages?.controls}
      </div>
      <fieldset>
        <legend className="mb-1 text-sm font-medium text-[var(--g5-text1)]">
          Assignees
        </legend>
        <ul className="max-h-40 space-y-1 overflow-y-auto rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2">
          {[...candidates].map(([login, label]) => (
            <li key={login}>
              <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text0)]">
                <input
                  type="checkbox"
                  data-testid={`issue-page-edit-assignee-${login}`}
                  checked={assignees.includes(login)}
                  disabled={disabled}
                  onChange={() => onToggleAssignee(login)}
                  className="h-4 w-4 accent-[var(--g5-info)]"
                />
                {label}
              </label>
            </li>
          ))}
          {candidates.size === 0 && (
            <li className="text-xs text-[var(--g5-text2)]">
              {emptyCandidates}
            </li>
          )}
        </ul>
      </fieldset>
    </>
  );
}

function toggled(current: string[], login: string): string[] {
  return current.includes(login)
    ? current.filter((value) => value !== login)
    : [...current, login];
}

function sameSet(left: string[], right: string[]): boolean {
  return (
    left.length === right.length && left.every((value) => right.includes(value))
  );
}

/**
 * Edit the title, description and assignees: on a native issue at the
 * version the editor saw, on GitHub as the connected account.
 */
export function IssueEditForm({
  orgId,
  issue,
  target,
  onDone,
  onReload,
}: {
  orgId: string;
  issue: ExternalIssueDetailDTO;
  /** The GitHub write target when editing began; `null` on a native issue. */
  target: IssueWriteTargetDTO | null;
  /** Closes the form; a partially applied GitHub edit is passed along. */
  onDone: (result?: IssueEditDTO) => void;
  onReload: () => void;
}) {
  return target && !isNativeIssue(issue) ? (
    <GithubIssueEditForm
      orgId={orgId}
      issue={issue}
      target={target}
      onDone={onDone}
      onReload={onReload}
    />
  ) : (
    <NativeIssueEditForm
      orgId={orgId}
      issue={issue}
      onDone={onDone}
      onReload={onReload}
    />
  );
}

/** Only changed fields are sent, with the issue's `updatedAt` as the version the editor saw. */
function NativeIssueEditForm({
  orgId,
  issue,
  onDone,
  onReload,
}: {
  orgId: string;
  issue: ExternalIssueDetailDTO;
  onDone: () => void;
  onReload: () => void;
}) {
  const editIssue = useEditNativeIssue(orgId, issue.locator.number);
  const members = useOrgMembers(orgId);
  const [title, setTitle] = useState(issue.title);
  const [body, setBody] = useState(issue.body ?? "");
  const initialAssignees = issue.assignees.map((user) => user.login);
  const [assignees, setAssignees] = useState<string[]>(initialAssignees);
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const images = useIssueImageUploads({
    target: { orgId, number: issue.locator.number },
    textareaRef: bodyRef,
    value: body,
    onChange: setBody,
    disabled: editIssue.isLoading,
    unplacedChoice: "attach",
    testId: "issue-page-edit-image",
  });

  // Members who can write may be assigned; current assignees stay listed
  // even if they no longer qualify, so they can be removed.
  const candidates = new Map<string, string>();
  for (const user of issue.assignees) {
    candidates.set(user.login, issueUserLabel(user) ?? user.login);
  }
  if (members.status === "ok") {
    for (const member of members.data) {
      if (member.deactivatedAt || member.role === "viewer") continue;
      if (!candidates.has(member.email)) {
        candidates.set(member.email, member.displayName || member.email);
      }
    }
  }

  const titleValid =
    title.trim() !== "" && title.trim().length <= MAX_ISSUE_TITLE_CHARS;
  const sameAssignees = sameSet(assignees, initialAssignees);
  const changed =
    title.trim() !== issue.title ||
    body !== (issue.body ?? "") ||
    !sameAssignees;

  const save = async () => {
    try {
      await editIssue.mutateAsync({
        ...(title.trim() !== issue.title ? { title: title.trim() } : {}),
        ...(body !== (issue.body ?? "") ? { body } : {}),
        ...(!sameAssignees ? { assignees } : {}),
        expectedUpdatedAt: issue.updatedAt,
      });
      onDone();
    } catch {
      // Explained below; the edit is kept.
    }
  };

  return (
    <form
      data-testid="issue-page-edit-form"
      aria-label="Edit issue"
      className="space-y-3"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <IssueEditFields
        title={title}
        onTitle={setTitle}
        body={body}
        onBody={setBody}
        bodyImages={{ ...images, textareaRef: bodyRef }}
        candidates={candidates}
        assignees={assignees}
        onToggleAssignee={(login) =>
          setAssignees((current) => toggled(current, login))
        }
        disabled={editIssue.isLoading}
        emptyCandidates={
          members.status === "loading"
            ? "Loading members…"
            : "No one to assign."
        }
      />
      {editIssue.error && (
        <WriteError
          testId="issue-page-edit-error"
          message={describeIssueWriteError(
            editIssue.error,
            "The issue could not be saved. Your edit is kept.",
          )}
          onReload={isStaleIssueRefusal(editIssue.error) ? onReload : undefined}
        />
      )}
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          kind="ghost"
          data-testid="issue-page-edit-cancel"
          disabled={editIssue.isLoading}
          onClick={() => onDone()}
        >
          Cancel
        </Button>
        <Button
          kind="primary"
          type="submit"
          data-testid="issue-page-edit-save"
          disabled={
            !titleValid || !changed || editIssue.isLoading || images.uploading
          }
        >
          {editIssue.isLoading && (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          )}
          Save
        </Button>
      </div>
    </form>
  );
}

/**
 * A GitHub description without G5's trailing creation marker, which the
 * server keeps on its own (g5.external-issues.creation.recovery).
 */
function splitCreationMarker(body: string): { text: string; marker: boolean } {
  const lines = body.trimEnd().split("\n");
  const last = lines[lines.length - 1]?.trim() ?? "";
  if (last.startsWith("<!-- g5-issue ") && last.endsWith(" -->")) {
    return { text: lines.slice(0, -1).join("\n").trimEnd(), marker: true };
  }
  return { text: body, marker: false };
}

/**
 * Only changed fields are sent, each with what the form started from as
 * `seen`; an untrusted domain reviews exactly what is sent first.
 */
function GithubIssueEditForm({
  orgId,
  issue,
  target,
  onDone,
  onReload,
}: {
  orgId: string;
  issue: ExternalIssueDetailDTO;
  target: IssueWriteTargetDTO;
  onDone: (result?: IssueEditDTO) => void;
  onReload: () => void;
}) {
  const editIssue = useEditGithubIssue(orgId, issue.locator);
  // What the form started from: the `seen` of every changed field. Later
  // reads of the issue (a new comment bumps its version) leave it alone.
  const [initial] = useState(() => {
    const { text, marker } = splitCreationMarker(issue.body ?? "");
    return {
      title: issue.title,
      body: text,
      marker,
      assignees: issue.assignees.map((user) => user.login),
    };
  });
  const [title, setTitle] = useState(initial.title);
  const [body, setBody] = useState(initial.body);
  const [assignees, setAssignees] = useState<string[]>(initial.assignees);
  const [reviewing, setReviewing] = useState(false);
  const [unconfirmed, setUnconfirmed] = useState(false);

  // The repository's assignable accounts; current assignees stay listed so
  // they can be removed.
  const candidates = new Map<string, string>();
  for (const user of [...issue.assignees, ...target.assignableUsers]) {
    if (!candidates.has(user.login)) {
      candidates.set(user.login, `@${user.login}`);
    }
  }

  const changes: GithubIssueFields = {};
  const seen: GithubIssueFields = {};
  if (title.trim() !== initial.title) {
    changes.title = title.trim();
    seen.title = initial.title;
  }
  if (body !== initial.body) {
    changes.body = body;
    seen.body = initial.body;
  }
  if (!sameSet(assignees, initial.assignees)) {
    changes.assignees = assignees;
    seen.assignees = initial.assignees;
  }
  const titleValid =
    title.trim() !== "" && title.trim().length <= MAX_ISSUE_TITLE_CHARS;
  const changed = Object.keys(changes).length > 0;
  const busy = editIssue.isLoading;

  const edit = (apply: () => void) => {
    apply();
    setReviewing(false);
    setUnconfirmed(false);
    editIssue.reset();
  };

  const save = async () => {
    setUnconfirmed(false);
    try {
      const result = await editIssue.mutateAsync({ ...changes, seen });
      if (result.outcome === "unknown") {
        // Setting the same values again is harmless: the form stays.
        setUnconfirmed(true);
        return;
      }
      onDone(result.outcome === "partial" ? result : undefined);
    } catch {
      // Explained below; the edit is kept.
    }
  };

  const reviewFirst = !target.contentTrusted;
  return (
    <form
      data-testid="issue-page-edit-form"
      aria-label="Edit issue"
      className="space-y-3"
      onSubmit={(event) => {
        event.preventDefault();
        if (reviewFirst && !reviewing) setReviewing(true);
        else void save();
      }}
    >
      {reviewing ? (
        <div data-testid="issue-page-edit-review" className="space-y-2">
          <p className="text-xs text-[var(--g5-text2)]">
            These changes will be saved to {target.displayKey} on GitHub as @
            {target.githubLogin}.
          </p>
          {changes.title !== undefined && (
            <div>
              <p className="mb-1 text-xs font-medium text-[var(--g5-text1)]">
                Title
              </p>
              <pre className={PREVIEW_CLASSES}>{changes.title}</pre>
            </div>
          )}
          {changes.body !== undefined && (
            <div>
              <p className="mb-1 text-xs font-medium text-[var(--g5-text1)]">
                Description
                {initial.marker &&
                  " (G5 keeps its hidden reference line at the end)"}
              </p>
              <pre className={PREVIEW_CLASSES}>
                {changes.body.trimEnd() || "(empty)"}
              </pre>
            </div>
          )}
          {changes.assignees !== undefined && (
            <div>
              <p className="mb-1 text-xs font-medium text-[var(--g5-text1)]">
                Assignees
              </p>
              <pre className={PREVIEW_CLASSES}>
                {changes.assignees.length
                  ? changes.assignees.map((login) => `@${login}`).join(", ")
                  : "(none)"}
              </pre>
            </div>
          )}
        </div>
      ) : (
        <IssueEditFields
          title={title}
          onTitle={(value) => edit(() => setTitle(value))}
          body={body}
          onBody={(value) => edit(() => setBody(value))}
          candidates={candidates}
          assignees={assignees}
          onToggleAssignee={(login) =>
            edit(() => setAssignees((current) => toggled(current, login)))
          }
          disabled={busy}
          emptyCandidates="GitHub lists no one to assign."
        />
      )}
      {unconfirmed && (
        <Unconfirmed
          testId="issue-page-edit-unconfirmed"
          message="GitHub did not confirm the edit. Your edit is kept; saving again is safe."
        />
      )}
      {editIssue.error && (
        <WriteError
          testId="issue-page-edit-error"
          message={describeIssueWriteError(
            editIssue.error,
            "The issue could not be saved. Your edit is kept.",
          )}
          onReload={isStaleIssueRefusal(editIssue.error) ? onReload : undefined}
        />
      )}
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          kind="ghost"
          data-testid="issue-page-edit-cancel"
          disabled={busy}
          onClick={() => onDone()}
        >
          Cancel
        </Button>
        {reviewing && (
          <Button
            kind="secondary"
            data-testid="issue-page-edit-back"
            disabled={busy}
            onClick={() => setReviewing(false)}
          >
            Edit
          </Button>
        )}
        <Button
          kind="primary"
          type="submit"
          data-testid={
            reviewFirst && !reviewing
              ? "issue-page-edit-review-button"
              : "issue-page-edit-save"
          }
          disabled={!titleValid || !changed || busy}
        >
          {busy && <LoaderCircle className="h-3.5 w-3.5 animate-spin" />}
          {reviewFirst && !reviewing ? "Review changes" : "Save to GitHub"}
        </Button>
      </div>
    </form>
  );
}

/**
 * Edit one's own comment, only if it is unchanged since it was read. On
 * GitHub G5 keeps its hidden reference line, and an untrusted domain reviews
 * the new text first.
 */
export function IssuePageCommentEditor({
  orgId,
  issue,
  comment,
  target,
  onDone,
  onReload,
}: {
  orgId: string;
  issue: ExternalIssueDetailDTO;
  comment: ExternalIssueCommentDTO;
  /** The GitHub write target when editing began; `null` on a native issue. */
  target: IssueWriteTargetDTO | null;
  onDone: () => void;
  onReload: () => void;
}) {
  const inputId = useId();
  const editComment = useEditIssuePageComment(orgId, issue.locator);
  const keepsReference = !!target && G5_REFERENCE_AT_END.test(comment.body);
  const [original] = useState(() =>
    target ? comment.body.replace(G5_REFERENCE_AT_END, "") : comment.body,
  );
  const [text, setText] = useState(original);
  const [reviewing, setReviewing] = useState(false);
  const [unconfirmed, setUnconfirmed] = useState(false);
  const reviewFirst = !!target && !target.contentTrusted;
  const changeText = (value: string) => {
    setText(value);
    setUnconfirmed(false);
    editComment.reset();
  };
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const images = useIssueImageUploads({
    target: isNativeIssue(issue)
      ? { orgId, number: issue.locator.number }
      : null,
    textareaRef: inputRef,
    value: text,
    onChange: changeText,
    disabled: editComment.isLoading,
    testId: "issue-page-comment-edit-image",
  });

  const save = async () => {
    setUnconfirmed(false);
    try {
      const result = await editComment.mutateAsync({
        commentId: comment.id,
        body: text,
        expectedUpdatedAt: comment.updatedAt,
      });
      if (result.state === "outcome_unknown") {
        // Nothing is recorded; replacing the text again is harmless.
        setUnconfirmed(true);
        return;
      }
      onDone();
    } catch {
      // Explained below; the edit is kept.
    }
  };
  const canSave =
    text.trim() !== "" &&
    text !== original &&
    !editComment.isLoading &&
    !images.uploading;
  return (
    <div data-testid={`issue-page-comment-editor-${comment.id}`}>
      {reviewing && target ? (
        <div data-testid="issue-page-comment-edit-review">
          <p className="mb-1 text-xs text-[var(--g5-text2)]">
            This text will replace the comment on {target.displayKey} as @
            {target.githubLogin}.
            {keepsReference &&
              " G5 keeps its hidden reference line at the end."}
          </p>
          <pre className={PREVIEW_CLASSES}>{text.trimEnd()}</pre>
        </div>
      ) : (
        <>
          <label htmlFor={inputId} className="sr-only">
            Edit comment
          </label>
          <textarea
            ref={inputRef}
            id={inputId}
            data-testid="issue-page-comment-edit-input"
            rows={4}
            value={text}
            onChange={(event) => changeText(event.target.value)}
            {...images.handlers}
            className={FIELD_CLASSES}
          />
          {images.controls}
        </>
      )}
      {unconfirmed && (
        <Unconfirmed
          testId="issue-page-comment-edit-unconfirmed"
          message={COMMENT_EDIT_UNCONFIRMED_MESSAGE}
        />
      )}
      {editComment.error && (
        <WriteError
          testId="issue-page-comment-edit-error"
          message={describeIssueWriteError(
            editComment.error,
            "The comment could not be saved. Your edit is kept.",
          )}
          onReload={
            isStaleIssueRefusal(editComment.error) ? onReload : undefined
          }
        />
      )}
      <div className="mt-2 flex flex-wrap justify-end gap-2">
        <Button
          kind="ghost"
          data-testid="issue-page-comment-edit-cancel"
          disabled={editComment.isLoading}
          onClick={onDone}
        >
          Cancel
        </Button>
        {reviewing && (
          <Button
            kind="secondary"
            data-testid="issue-page-comment-edit-back"
            disabled={editComment.isLoading}
            onClick={() => setReviewing(false)}
          >
            Edit
          </Button>
        )}
        {reviewFirst && !reviewing ? (
          <Button
            kind="primary"
            data-testid="issue-page-comment-edit-review-button"
            disabled={!canSave}
            onClick={() => setReviewing(true)}
          >
            Review change
          </Button>
        ) : (
          <Button
            kind="primary"
            data-testid="issue-page-comment-edit-save"
            disabled={!canSave}
            onClick={() => void save()}
          >
            {editComment.isLoading
              ? "Saving…"
              : target
                ? "Save to GitHub"
                : "Save"}
          </Button>
        )}
      </div>
    </div>
  );
}

/** The issue's domain key, from its display key (`owner/name#12`, `ACME-12`). */
function domainKeyOf(issue: ExternalIssueDetailDTO): string {
  return isNativeIssue(issue)
    ? issue.displayKey.replace(/-\d+$/, "")
    : issue.displayKey.replace(/#\d+$/, "");
}

const LABEL_OUTCOME_TEXT: Record<LabelChangeOutcomeDTO["outcome"], string> = {
  applied: "applied",
  refused: "refused",
  unknown: "not confirmed",
  not_attempted: "not attempted",
};

/**
 * A label change that did not fully apply: what became of each requested
 * change. An unknown outcome is re-read before any retry.
 */
export function IssueLabelChangeNotice({
  result,
}: {
  result: IssueLabelChangeDTO;
}) {
  return (
    <div
      role="alert"
      data-testid="issue-page-labels-outcome"
      data-outcome={result.outcome}
      className="mt-3 rounded-lg border border-[rgb(242,162,73,0.3)] bg-[rgb(242,162,73,0.06)] px-3.5 py-2.5 text-xs leading-relaxed text-[var(--g5-text1)]"
    >
      <p>
        {result.outcome === "unknown"
          ? "GitHub did not confirm every label change, so some may or may not have applied. The labels shown were read again; check them before trying again."
          : "Only some label changes were applied."}
        {result.issue === null &&
          " The issue could not be read again; reload to see its labels."}
        {result.message ? ` ${result.message}` : ""}
      </p>
      <ul className="mt-1 space-y-0.5">
        {result.changes.map((change) => (
          <li
            key={`${change.change}:${change.label}`}
            data-testid={`issue-page-labels-outcome-${change.change}-${change.label}`}
            data-outcome={change.outcome}
          >
            {change.change === "add" ? "Add" : "Remove"} “{change.label}”:{" "}
            {LABEL_OUTCOME_TEXT[change.outcome]}
            {change.message ? ` (${change.message})` : ""}
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Add and remove the issue's labels with the label picker. Only the changes
 * from what the editor started from are sent; an untrusted GitHub domain
 * reviews exactly those first.
 */
export function IssueLabelsEditor({
  orgId,
  issue,
  target,
  onDone,
}: {
  orgId: string;
  issue: ExternalIssueDetailDTO;
  /** The GitHub write target when editing began; `null` on a native issue. */
  target: IssueWriteTargetDTO | null;
  /** Closes the editor; a partial or unknown change is passed along. */
  onDone: (result?: IssueLabelChangeDTO) => void;
}) {
  const changeLabels = useChangeIssueLabels(orgId);
  // What the editor started from; later reads of the issue leave it alone,
  // so a label someone else changed meanwhile is not sent.
  const [initial] = useState(() => ({
    names: issue.labels.map((label) => label.name),
    labels: issue.labels,
  }));
  const [selected, setSelected] = useState<string[]>(initial.names);
  const [reviewing, setReviewing] = useState(false);
  const delta = labelDelta(initial.names, selected);
  const count = delta.addLabels.length + delta.removeLabels.length;
  const tooMany = count > MAX_LABELS_PER_CHANGE;
  const busy = changeLabels.isLoading;
  const reviewFirst = !!target && !target.contentTrusted;

  const save = async () => {
    try {
      const result = await changeLabels.mutateAsync({
        locator: issue.locator,
        ...delta,
      });
      onDone(result.outcome === "applied" ? undefined : result);
    } catch {
      // Explained below; the selection is kept.
    }
  };

  return (
    <form
      data-testid="issue-page-labels-form"
      aria-label="Edit labels"
      className="space-y-3"
      onSubmit={(event) => {
        event.preventDefault();
        if (reviewFirst && !reviewing) setReviewing(true);
        else void save();
      }}
    >
      {reviewing && target ? (
        <div data-testid="issue-page-labels-review" className="space-y-2">
          <p className="text-xs text-[var(--g5-text2)]">
            These label changes will be saved to {target.displayKey} on GitHub
            as @{target.githubLogin}.
          </p>
          {delta.addLabels.length > 0 && (
            <div>
              <p className="mb-1 text-xs font-medium text-[var(--g5-text1)]">
                Add
              </p>
              <pre
                data-testid="issue-page-labels-review-add"
                className={PREVIEW_CLASSES}
              >
                {delta.addLabels.join("\n")}
              </pre>
            </div>
          )}
          {delta.removeLabels.length > 0 && (
            <div>
              <p className="mb-1 text-xs font-medium text-[var(--g5-text1)]">
                Remove
              </p>
              <pre
                data-testid="issue-page-labels-review-remove"
                className={PREVIEW_CLASSES}
              >
                {delta.removeLabels.join("\n")}
              </pre>
            </div>
          )}
        </div>
      ) : (
        <IssueLabelPicker
          orgId={orgId}
          domain={{
            domainId: issue.domainId,
            displayKey: domainKeyOf(issue),
            provider: isNativeIssue(issue) ? "g5" : "github",
            contentTrusted: target
              ? target.contentTrusted
              : issue.contentTrusted,
          }}
          selected={selected}
          onChange={(names) => {
            setSelected(names);
            setReviewing(false);
            changeLabels.reset();
          }}
          known={initial.labels}
          disabled={busy}
          canManage
        />
      )}
      {tooMany && (
        <p
          role="status"
          data-testid="issue-page-labels-too-many"
          className="text-xs text-[var(--g5-blocked)]"
        >
          One change adds and removes at most {MAX_LABELS_PER_CHANGE} labels;
          save some first.
        </p>
      )}
      {changeLabels.error && (
        <WriteError
          testId="issue-page-labels-error"
          message={describeLabelError(
            changeLabels.error,
            "The labels could not be changed. Your selection is kept.",
          )}
        />
      )}
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          kind="ghost"
          data-testid="issue-page-labels-cancel"
          disabled={busy}
          onClick={() => onDone()}
        >
          Cancel
        </Button>
        {reviewing && (
          <Button
            kind="secondary"
            data-testid="issue-page-labels-back"
            disabled={busy}
            onClick={() => setReviewing(false)}
          >
            Edit
          </Button>
        )}
        <Button
          kind="primary"
          type="submit"
          data-testid={
            reviewFirst && !reviewing
              ? "issue-page-labels-review-button"
              : "issue-page-labels-save"
          }
          disabled={count === 0 || tooMany || busy}
        >
          {busy && <LoaderCircle className="h-3.5 w-3.5 animate-spin" />}
          {reviewFirst && !reviewing
            ? "Review changes"
            : target
              ? "Save to GitHub"
              : "Save labels"}
        </Button>
      </div>
    </form>
  );
}
