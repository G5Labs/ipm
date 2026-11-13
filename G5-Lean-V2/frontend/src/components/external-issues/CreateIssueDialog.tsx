// =============================================================================
// Create issue — in the organization's creation target (GitHub or G5)
// =============================================================================
// Opened from Home's Issues and tasks tile (with Create & Fork Workspace) and
// from the workspace rail's quick-add (without), and meant for meeting
// decision cards through `initialTitle` / `initialBody`
// (g5.workbench.issues.create). The issue goes to the organization's default
// creation target under the person's own GitHub account, through the comment
// guards (g5.external-issues.creation.admission): an untrusted domain shows
// the exact title, description, assignees and destination before creating; a
// trusted one creates straight from the form. Every submission carries an
// idempotency key that G5 turns into a hidden reference line, so a timeout is
// resolved by looking for that line — never by sending again on G5's own
// (g5.external-issues.creation.recovery). An optional parent issue in the
// destination repository makes the new issue its GitHub sub-issue; when the
// issue exists but is not under the parent, the dialog says so and offers to
// retry the same submission or continue without the parent. When the
// creation target is the native G5 tracker, the issue is created in G5 under
// the person's G5 account: no GitHub account, hidden line or review step;
// assignees are organization members and the parent is a native issue such
// as `ACME-3` (g5.issue.native.writes). Images pasted, dropped or attached
// for a G5 issue wait in this browser behind a visible placeholder; once the
// issue exists each is uploaded to it and the description is edited, at the
// version just read, to show them. One chosen with Attach image before the
// person has clicked or typed in the description waits in a list below it
// instead and is only attached to the issue. An image that fails leaves the issue
// created, with a note in its place, and the dialog says which were not
// added (g5.issue.native.attachments). GitHub issues take no uploads. Labels
// come from the destination domain's catalog through the label picker and
// stay bound to that domain: a changed creation target starts with none
// (g5.issue.labels.surfaces). GitHub silently drops labels the author may not
// set; the issue still exists, and the dialog lists the dropped labels before
// continuing (g5.issue.labels.github).
// =============================================================================

import { useEffect, useId, useRef, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { LoaderCircle, X } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";

import { Button, Modal, classNames } from "../ui/UI";
import { useAuth } from "../../hooks/useAuth";
import {
  externalIssueKeys,
  useCreateIssue,
  useIssueCreateTarget,
  useResolveExternalIssue,
} from "../../hooks/useExternalIssues";
import {
  uploadNativeAttachment,
  type NativeAttachmentUpload,
} from "../../hooks/useNativeAttachments";
import { ApiError, api } from "../../lib/api";
import { describeIssueRequestError } from "../../lib/externalIssueErrors";
import { githubConnectionPath } from "../../lib/githubConnection";
import {
  createTargetBlockedReason,
  describeCreateIssueError,
  isParentRefusal,
} from "../../lib/issueCommentErrors";
import { describeIssueWriteError } from "../../lib/issueCommentErrors";
import {
  attachmentFileName,
  describeUploadError,
  imageFileRefusal,
  imageNotAddedNote,
  insertAtCaret,
  pendingImagePlaceholder,
} from "../../lib/nativeAttachments";
import { MAX_LABELS_PER_CHANGE } from "../../lib/issueLabels";
import { randomUuid } from "../../lib/randomUuid";
import { routes } from "../../lib/routes";
import type {
  CreatedIssueDTO,
  CreationParentDTO,
  ExternalIssueDetailDTO,
  IssueCreateTargetDTO,
  IssueCreationDTO,
  IssueCreationDestinationDTO,
} from "../../lib/types";
import {
  AttachImageButton,
  useCaretPlacement,
  useImageTransferHandlers,
} from "./NativeAttachments";
import { IssueLabelPicker } from "./IssueLabelPicker";

/** GitHub's own title limit, which the server enforces too. */
export const MAX_ISSUE_TITLE_CHARS = 256;
/** Most assignees GitHub accepts on one issue. */
const MAX_ASSIGNEES = 10;
/** The caller's own account, resolved by the server as `gh` does. */
const ASSIGN_SELF = "@me";
const MARKER_PREVIEW = "<!-- g5-issue (reference id) -->";
const PARENT_LOOKUP_DEBOUNCE_MS = 400;
/** The parent states a resend with the same key can still attach. */
const RETRYABLE_PARENT_STATES: ReadonlyArray<CreationParentDTO["state"]> = [
  "not_attached",
  "outcome_unknown",
];

const FIELD_CLASSES =
  "w-full rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)] placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] disabled:opacity-60";

/**
 * An image waiting for its issue, shown in the text by `placeholder`, or
 * only to be attached to the issue when `null`.
 */
interface PendingImage extends NativeAttachmentUpload {
  id: string;
  placeholder: string | null;
}

/** What adding a new issue's images left undone. */
interface ImageOutcome {
  issue: CreatedIssueDTO;
  fork: boolean;
  /** The requested parent's outcome, shown after this one when not attached. */
  parent: CreationParentDTO | null;
  /** Why each image that was not added failed. */
  failures: string[];
  /** The description could not be edited to show the images. */
  descriptionError: string | null;
}

/**
 * Upload `images` to the new native issue and edit its description to show
 * them, at the version read just before; each image that failed is
 * replaced by a note. A description changed meanwhile is read once more.
 */
async function addPendingImages(
  orgId: string,
  number: number,
  images: PendingImage[],
): Promise<Pick<ImageOutcome, "failures" | "descriptionError">> {
  const replacements: [string, string][] = [];
  const failures: string[] = [];
  for (const image of images) {
    const { placeholder } = image;
    try {
      const attachment = await uploadNativeAttachment(orgId, number, image);
      if (placeholder) replacements.push([placeholder, attachment.markdown]);
    } catch (error) {
      failures.push(describeUploadError(error, image.fileName));
      if (placeholder) {
        replacements.push([placeholder, imageNotAddedNote(image.fileName)]);
      }
    }
  }
  if (replacements.length === 0) return { failures, descriptionError: null };
  let descriptionError: string | null = null;
  for (let attempt = 0; attempt < 2; attempt += 1) {
    try {
      const issue = await api.get<ExternalIssueDetailDTO>(
        routes.orgs.nativeIssue(orgId, number),
      );
      const before = issue.body ?? "";
      const body = replacements.reduce(
        (text, [placeholder, shown]) => text.split(placeholder).join(shown),
        before,
      );
      if (body !== before) {
        await api.patch(routes.orgs.nativeIssue(orgId, number), {
          body,
          expectedUpdatedAt: issue.updatedAt,
        });
      }
      // Someone took a stand-in out of the description meanwhile: its
      // image stays listed on the issue's page, and the person is told.
      const missing = replacements.filter(
        ([placeholder]) => !before.includes(placeholder),
      ).length;
      descriptionError =
        missing === 0
          ? null
          : `someone changed it first, removing where ${
              missing === 1 ? "an image was" : `${missing} images were`
            } to go`;
      break;
    } catch (error) {
      descriptionError = describeIssueWriteError(
        error,
        "The description could not be updated.",
      );
      const changed =
        error instanceof ApiError && error.details.code === "issue_changed";
      if (!changed) break;
    }
  }
  return { failures, descriptionError };
}

export interface CreateIssueDialogProps {
  orgId: string;
  open: boolean;
  onClose: () => void;
  /** The issue exists; `fork` when the person chose Create & Fork Workspace. */
  onCreated: (issue: CreatedIssueDTO, options: { fork: boolean }) => void;
  /** Offer Create & Fork Workspace (Home only). */
  allowFork?: boolean;
  initialTitle?: string;
  initialBody?: string;
  /** A parent to start with, which the person may change or clear. */
  initialParent?: string;
  /**
   * One creation key for every submission, for a caller that files at most
   * one issue per item (a findings entry, `g5.documents.findings.decision`).
   * Omitted, each distinct submission gets its own.
   */
  idempotencyKey?: string;
  /** Title and description only: no assignees and no parent. */
  titleAndDescriptionOnly?: boolean;
}

export function CreateIssueDialog({ open, ...props }: CreateIssueDialogProps) {
  // Mounted per opening, so a closed dialog forgets its draft and key.
  if (!open) return null;
  return <CreateIssueForm {...props} />;
}

/** A submission's identity: the same fields reuse the same key. */
function fingerprintOf(
  title: string,
  body: string,
  assignees: string[],
  parent: string,
  domainId: string,
  labels: string[],
): string {
  return JSON.stringify([
    title.trim(),
    body.trimEnd(),
    [...assignees].sort(),
    parent.trim(),
    domainId,
    labels.map((name) => name.toLowerCase()).sort(),
  ]);
}

/** The destination is the native G5 tracker. */
function isNativeDestination(
  destination: IssueCreationDestinationDTO | null,
): boolean {
  return (
    destination?.provider === "g5" ||
    (destination?.provider === undefined && destination?.repositoryId === "g5")
  );
}

/**
 * The reference to look a parent entry up by: a bare `#N` or `N` names an
 * issue in the destination (`owner/name#N`, or `ACME-N` natively; empty
 * until the destination is known).
 */
function parentLookupRef(
  entry: string,
  destination: IssueCreationDestinationDTO | null,
): string {
  const bare = /^#?(\d+)$/.exec(entry);
  if (!bare) return entry;
  if (!destination) return "";
  return isNativeDestination(destination)
    ? `${destination.displayKey}-${bare[1]}`
    : `${destination.displayKey}#${bare[1]}`;
}

function displayAssignee(login: string, githubLogin: string | null): string {
  return login === ASSIGN_SELF ? `@${githubLogin ?? "me"} (you)` : `@${login}`;
}

/**
 * Native assignees: the organization's members by name, the caller first
 * as "Assign to me" (sent as `@me`).
 */
function NativeAssigneeOptions({
  target,
  selected,
  disabled,
  onToggle,
}: {
  target: IssueCreateTargetDTO | null;
  selected: string[];
  disabled: boolean;
  onToggle: (login: string) => void;
}) {
  const { user } = useAuth();
  const users = target?.assignableUsers ?? [];
  const self = users.find(
    (candidate) => !!user?.id && candidate.userId === user.id,
  );
  const others = users.filter((candidate) => candidate !== self);
  const full = selected.length >= MAX_ASSIGNEES;
  const option = (value: string, label: string, testId: string) => {
    const checked = selected.includes(value);
    return (
      <li key={value}>
        <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text0)]">
          <input
            type="checkbox"
            data-testid={testId}
            checked={checked}
            disabled={disabled || (!checked && full)}
            onChange={() => onToggle(value)}
            className="h-4 w-4 accent-[var(--g5-info)]"
          />
          {label}
        </label>
      </li>
    );
  };
  return (
    <>
      {self && option(ASSIGN_SELF, "Assign to me", "create-issue-assign-me")}
      {others.map((candidate) =>
        option(
          candidate.login,
          candidate.displayName
            ? `${candidate.displayName} (${candidate.login})`
            : candidate.login,
          `create-issue-assignee-${candidate.login}`,
        ),
      )}
      {!self && others.length === 0 && (
        <li className="text-xs text-[var(--g5-text2)]">
          No one to assign yet.
        </li>
      )}
    </>
  );
}

function AssigneePicker({
  target,
  selected,
  disabled,
  onToggle,
}: {
  target: IssueCreateTargetDTO | null;
  selected: string[];
  disabled: boolean;
  onToggle: (login: string) => void;
}) {
  const native = isNativeDestination(target?.destination ?? null);
  const githubLogin = target?.githubLogin ?? null;
  const others = (target?.assignableUsers ?? []).filter(
    (user) => user.login.toLowerCase() !== githubLogin?.toLowerCase(),
  );
  const full = selected.length >= MAX_ASSIGNEES;
  const option = (login: string, label: string, testId: string) => {
    const checked = selected.includes(login);
    return (
      <li key={login}>
        <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text0)]">
          <input
            type="checkbox"
            data-testid={testId}
            checked={checked}
            disabled={disabled || (!checked && full)}
            onChange={() => onToggle(login)}
            className="h-4 w-4 accent-[var(--g5-info)]"
          />
          {label}
        </label>
      </li>
    );
  };
  return (
    <fieldset>
      <legend className="mb-1 text-sm font-medium text-[var(--g5-text1)]">
        Assignees{" "}
        <span className="font-normal text-[var(--g5-text2)]">
          (optional, at most {MAX_ASSIGNEES})
        </span>
      </legend>
      <ul className="max-h-40 space-y-1 overflow-y-auto rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2">
        {native && (
          <NativeAssigneeOptions
            target={target}
            selected={selected}
            disabled={disabled}
            onToggle={onToggle}
          />
        )}
        {!native &&
          githubLogin &&
          option(
            ASSIGN_SELF,
            `Assign to me (@${githubLogin})`,
            "create-issue-assign-me",
          )}
        {!native &&
          others.map((user) =>
            option(
              user.login,
              `@${user.login}`,
              `create-issue-assignee-${user.login}`,
            ),
          )}
        {!native && !githubLogin && others.length === 0 && (
          <li className="text-xs text-[var(--g5-text2)]">
            {target?.state === "ready"
              ? "GitHub did not list who can be assigned; the issue starts unassigned."
              : "No one to assign yet."}
          </li>
        )}
      </ul>
    </fieldset>
  );
}

function CreateIssueForm({
  orgId,
  onClose,
  onCreated,
  allowFork = false,
  initialTitle = "",
  initialBody = "",
  initialParent = "",
  idempotencyKey,
  titleAndDescriptionOnly = false,
}: Omit<CreateIssueDialogProps, "open">) {
  const titleId = useId();
  const headingId = useId();
  const bodyId = useId();
  const parentId = useId();
  const parentHintId = useId();
  const location = useLocation();
  const { user } = useAuth();
  const target = useIssueCreateTarget(orgId);
  const createIssue = useCreateIssue(orgId);
  const queryClient = useQueryClient();
  const bodyRef = useRef<HTMLTextAreaElement>(null);
  const bodyPlacement = useCaretPlacement();
  // Images for a G5 issue, uploaded once it exists.
  const [pendingImages, setPendingImages] = useState<PendingImage[]>([]);
  const [imageRefusals, setImageRefusals] = useState<string[]>([]);
  const [attaching, setAttaching] = useState(false);
  // The issue exists but not every image was added to it.
  const [imageOutcome, setImageOutcome] = useState<ImageOutcome | null>(null);

  const [title, setTitle] = useState(initialTitle);
  const [body, setBody] = useState(initialBody);
  const [assignees, setAssignees] = useState<string[]>([]);
  // Optional parent issue; Home starts without one, the rail with the
  // workspace's primary issue.
  const [parent, setParent] = useState(initialParent);
  // Labels, bound to the domain they were picked from.
  const [labelSelection, setLabelSelection] = useState<{
    domainId: string;
    names: string[];
  } | null>(null);
  const [reviewing, setReviewing] = useState(false);
  // The last unclear answer (outcome unknown, or a check that found nothing)
  // and whether that submission was Create & Fork.
  const [unresolved, setUnresolved] = useState<{
    result: IssueCreationDTO;
    fork: boolean;
  } | null>(null);
  // The issue exists but is not under the requested parent (the person
  // retries the attach or continues without it), or GitHub left out labels
  // (the person continues and adds them from the issue's page).
  const [createdNotice, setCreatedNotice] = useState<{
    issue: CreatedIssueDTO;
    parent: CreationParentDTO | null;
    labelsNotApplied: string[];
    fork: boolean;
  } | null>(null);
  const parentNotice =
    createdNotice?.parent != null
      ? { ...createdNotice, parent: createdNotice.parent }
      : null;
  // One key per distinct submitted payload, reused for its checks and
  // retries; the server looks for the key's issue before every send.
  const submission = useRef<{ key: string; fingerprint: string } | null>(null);

  const targetData = target.status === "ok" ? target.data : null;
  const destination = targetData?.destination ?? null;
  const blockedReason = createTargetBlockedReason(target);
  const ready = blockedReason === null && destination !== null;
  // A native issue stays in G5: nothing to review before it is created.
  const native = isNativeDestination(destination);
  const trusted = destination?.contentTrusted === true || native;
  // A selection made for another domain is not this destination's.
  const labels =
    destination && labelSelection?.domainId === destination.domainId
      ? labelSelection.names
      : [];
  const canManageLabels =
    user?.orgs.find((membership) => membership.orgId === orgId)?.role !==
    "viewer";
  const titleValid =
    title.trim() !== "" && title.trim().length <= MAX_ISSUE_TITLE_CHARS;

  // Look the parent up (debounced) to confirm it is an issue in the
  // destination; the server checks again before sending anything.
  const trimmedParent = parent.trim();
  const parentRef = parentLookupRef(trimmedParent, destination);
  const [lookupRef, setLookupRef] = useState(parentRef);
  useEffect(() => {
    const timer = window.setTimeout(
      () => setLookupRef(parentRef),
      PARENT_LOOKUP_DEBOUNCE_MS,
    );
    return () => window.clearTimeout(timer);
  }, [parentRef]);
  const lookupCurrent = parentRef !== "" && lookupRef === parentRef;
  const parentLookup = useResolveExternalIssue(
    orgId,
    lookupCurrent ? lookupRef : "",
  );
  const parentIssue =
    lookupCurrent && parentLookup.status === "ok" ? parentLookup.data : null;
  const parentLookupError =
    lookupCurrent && parentLookup.status === "error"
      ? describeIssueRequestError(parentLookup.error).title
      : null;
  const parentOutside =
    parentIssue !== null &&
    destination !== null &&
    parentIssue.domainId !== destination.domainId;
  const parentValid =
    trimmedParent === "" || (parentIssue !== null && !parentOutside);

  const pending = createIssue.isLoading || attaching;
  // Images only to be attached, and those whose placeholder is still in the
  // text.
  const imagesToAdd = pendingImages.filter(
    (image) => image.placeholder === null || body.includes(image.placeholder),
  );
  const unplacedImages = pendingImages.filter(
    (image) => image.placeholder === null,
  );
  // Images wait for a G5 issue; a destination that became GitHub's would
  // receive their stand-ins as text.
  const imagesStranded = !native && imagesToAdd.length > 0;
  const canCreate =
    ready && titleValid && parentValid && !pending && !imagesStranded;
  const destinationChanged =
    createIssue.error instanceof ApiError &&
    createIssue.error.details.code === "destination_changed";
  // A refused parent is explained at its field while the form is shown.
  const parentRefused = isParentRefusal(createIssue.error) && !createdNotice;

  const edited = () => {
    setReviewing(false);
    setUnresolved(null);
    setCreatedNotice(null);
    createIssue.reset();
  };

  const toggleAssignee = (login: string) => {
    setAssignees((current) =>
      current.includes(login)
        ? current.filter((value) => value !== login)
        : [...current, login],
    );
    edited();
  };

  const takeImages = (files: File[], chosen: boolean) => {
    // An image chosen before the person has been in the description need
    // not appear in it.
    const shown = !chosen || bodyPlacement.placed.current;
    const refused: string[] = [];
    const added: PendingImage[] = [];
    for (const file of files) {
      const refusal = imageFileRefusal(file);
      if (refusal) {
        refused.push(refusal);
        continue;
      }
      const id = randomUuid();
      const fileName = attachmentFileName(file);
      const image = {
        id,
        file,
        fileName,
        idempotencyKey: randomUuid(),
        placeholder: shown ? pendingImagePlaceholder(id, fileName) : null,
      };
      added.push(image);
    }
    setImageRefusals(refused);
    if (added.length === 0) return;
    setPendingImages((current) => [...current, ...added]);
    if (!shown) {
      edited();
      return;
    }
    insertAtCaret(
      bodyRef.current,
      body,
      added.map((image) => image.placeholder).join("\n"),
      (text) => {
        setBody(text);
        edited();
      },
      bodyPlacement.placed.current,
    );
  };

  const bodyTransfer = useImageTransferHandlers(
    (files) => takeImages(files, false),
    pending,
  );
  const bodyImageHandlers = { ...bodyTransfer, ...bodyPlacement.handlers };

  /** The issue exists: add its images, then hand it on. */
  const finishCreated = async (
    issue: CreatedIssueDTO,
    fork: boolean,
    parentResult: CreationParentDTO | null,
    labelsNotApplied: string[],
  ) => {
    if (native && imagesToAdd.length > 0) {
      setAttaching(true);
      const outcome = await addPendingImages(orgId, issue.number, imagesToAdd);
      setAttaching(false);
      void queryClient.invalidateQueries({
        queryKey: externalIssueKeys.detailAll,
      });
      if (outcome.failures.length > 0 || outcome.descriptionError) {
        setImageOutcome({ issue, fork, parent: parentResult, ...outcome });
        return;
      }
    }
    const parentLeft =
      parentResult && parentResult.state !== "attached" ? parentResult : null;
    if (parentLeft || labelsNotApplied.length > 0) {
      setCreatedNotice({ issue, parent: parentLeft, labelsNotApplied, fork });
      return;
    }
    setCreatedNotice(null);
    onCreated(issue, { fork });
  };

  const submit = async (fork: boolean, checkOnly = false) => {
    if (!destination) return;
    const fingerprint = fingerprintOf(
      title,
      body,
      assignees,
      parent,
      destination.domainId,
      labels,
    );
    if (submission.current?.fingerprint !== fingerprint) {
      submission.current = {
        key: idempotencyKey ?? randomUuid(),
        fingerprint,
      };
    }
    try {
      const result = await createIssue.mutateAsync({
        title: title.trim(),
        body,
        assignees,
        idempotencyKey: submission.current.key,
        domainId: destination.domainId,
        checkOnly,
        ...(trimmedParent ? { parent: trimmedParent } : {}),
        ...(labels.length > 0 ? { labels } : {}),
      });
      if (result.state === "created" && result.issue) {
        setUnresolved(null);
        await finishCreated(
          result.issue,
          fork,
          result.parent,
          result.labelsNotApplied ?? [],
        );
        return;
      }
      setCreatedNotice(null);
      setUnresolved({ result, fork });
    } catch {
      // The refusal is explained below; the draft is kept.
    }
  };

  const reviewAgain = () => {
    createIssue.reset();
    setReviewing(false);
    target.refetch();
  };

  const createButtons = (
    <>
      <Button
        kind="primary"
        data-testid="create-issue-submit"
        disabled={!canCreate}
        onClick={() => void submit(false)}
      >
        {pending && <LoaderCircle className="h-4 w-4 animate-spin" />}
        Create
      </Button>
      {allowFork && (
        <Button
          kind="secondary"
          data-testid="create-issue-submit-fork"
          disabled={!canCreate}
          onClick={() => void submit(true)}
        >
          Create & Fork Workspace
        </Button>
      )}
    </>
  );

  return (
    <Modal
      onClose={onClose}
      labelledBy={headingId}
      size="md"
      closeDisabled={pending}
      testId="create-issue-dialog"
      className="flex flex-col p-0"
    >
      <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] px-5 py-4">
        <h2
          id={headingId}
          className="text-base font-semibold text-[var(--g5-text0)]"
        >
          Create issue
        </h2>
        <button
          type="button"
          data-testid="create-issue-close"
          onClick={onClose}
          disabled={pending}
          title="Close"
          aria-label="Close"
          className="inline-flex h-7 w-7 items-center justify-center rounded-[var(--g5-radius-sm)] text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
        >
          <X size={16} aria-hidden="true" />
        </button>
      </div>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
        <p
          data-testid="create-issue-destination"
          className="text-sm text-[var(--g5-text1)]"
        >
          {target.status === "loading" ? (
            "Checking where new issues go…"
          ) : destination ? (
            <>
              Creating in{" "}
              <code className="font-[var(--g5-font-mono)] text-[var(--g5-text0)]">
                {destination.displayKey}
              </code>
              {!native &&
                targetData?.githubLogin &&
                ` as @${targetData.githubLogin}`}
            </>
          ) : (
            "No repository is set up for new issues."
          )}
        </p>

        {blockedReason && target.status !== "loading" && (
          <div
            data-testid="create-issue-blocked"
            className="flex flex-wrap items-center gap-2 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-xs text-[var(--g5-text1)]"
          >
            <p className="min-w-0 flex-1">
              {blockedReason}. Your draft is kept.
            </p>
            {targetData?.state === "connection_required" && (
              <Link
                data-testid="create-issue-connect"
                to={githubConnectionPath(
                  `${location.pathname}${location.search}`,
                )}
                className="flex-none rounded border border-[var(--g5-stroke)] px-2.5 py-1 font-medium text-[var(--g5-text0)] no-underline hover:bg-[var(--g5-bg2)]"
              >
                Manage GitHub connection
              </Link>
            )}
          </div>
        )}

        {imageOutcome ? (
          <div
            role="status"
            data-testid="create-issue-image-notice"
            className="rounded-md border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2.5 text-xs text-[var(--g5-text1)]"
          >
            <p>
              Created {imageOutcome.issue.displayKey}
              {imageOutcome.failures.length > 0
                ? `, but ${imageOutcome.failures.length === 1 ? "an image was" : `${imageOutcome.failures.length} images were`} not added:`
                : "."}
            </p>
            {imageOutcome.failures.length > 0 && (
              <ul className="mt-1 list-disc pl-5">
                {imageOutcome.failures.map((failure) => (
                  <li key={failure}>{failure}</li>
                ))}
              </ul>
            )}
            {imageOutcome.descriptionError && (
              <p className="mt-1">
                Its description could not be updated to show the images (
                {imageOutcome.descriptionError}). Uploaded images are listed on
                the issue&apos;s page.
              </p>
            )}
            <div className="mt-2 flex flex-wrap gap-2">
              <Button
                kind="secondary"
                data-testid="create-issue-image-continue"
                onClick={() => {
                  const { issue, fork, parent } = imageOutcome;
                  setImageOutcome(null);
                  if (parent && parent.state !== "attached") {
                    setCreatedNotice({
                      issue,
                      parent,
                      labelsNotApplied: [],
                      fork,
                    });
                  } else {
                    onCreated(issue, { fork });
                  }
                }}
                className="px-2.5 py-1 text-xs"
              >
                Continue
              </Button>
            </div>
          </div>
        ) : createdNotice ? (
          <div className="space-y-2">
            {parentNotice && (
              <div
                role="status"
                data-testid="create-issue-parent-notice"
                className="rounded-md border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2.5 text-xs text-[var(--g5-text1)]"
              >
                <p>
                  Created {parentNotice.issue.displayKey}, but it was not added
                  under {parentNotice.parent.displayKey}.
                  {parentNotice.parent.message && (
                    <span className="text-[var(--g5-text2)]">
                      {" "}
                      ({parentNotice.parent.message})
                    </span>
                  )}
                </p>
                <div className="mt-2 flex flex-wrap gap-2">
                  {RETRYABLE_PARENT_STATES.includes(
                    parentNotice.parent.state,
                  ) && (
                    <Button
                      kind="secondary"
                      data-testid="create-issue-retry-parent"
                      disabled={pending}
                      title="Sends the same submission again; G5 finds the issue it created and adds it under the parent"
                      onClick={() => void submit(parentNotice.fork)}
                      className="px-2.5 py-1 text-xs"
                    >
                      {pending && (
                        <LoaderCircle className="h-3 w-3 animate-spin" />
                      )}
                      Retry adding parent
                    </Button>
                  )}
                  <Button
                    kind="secondary"
                    data-testid="create-issue-continue-without-parent"
                    disabled={pending}
                    onClick={() =>
                      onCreated(parentNotice.issue, {
                        fork: parentNotice.fork,
                      })
                    }
                    className="px-2.5 py-1 text-xs"
                  >
                    Continue without parent
                  </Button>
                </div>
              </div>
            )}
            {createdNotice.labelsNotApplied.length > 0 && (
              <div
                role="status"
                data-testid="create-issue-labels-not-applied"
                className="rounded-md border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2.5 text-xs text-[var(--g5-text1)]"
              >
                <p>
                  Created {createdNotice.issue.displayKey}, but GitHub did not
                  apply{" "}
                  {createdNotice.labelsNotApplied.length === 1
                    ? "the label"
                    : "the labels"}{" "}
                  {createdNotice.labelsNotApplied
                    .map((name) => `“${name}”`)
                    .join(", ")}
                  : it leaves out labels the author may not set. You can add
                  them from the issue's page.
                </p>
                {!parentNotice && (
                  <div className="mt-2 flex flex-wrap gap-2">
                    <Button
                      kind="secondary"
                      data-testid="create-issue-continue"
                      onClick={() =>
                        onCreated(createdNotice.issue, {
                          fork: createdNotice.fork,
                        })
                      }
                      className="px-2.5 py-1 text-xs"
                    >
                      Continue
                    </Button>
                  </div>
                )}
              </div>
            )}
          </div>
        ) : reviewing && destination ? (
          <div data-testid="create-issue-review" className="space-y-3">
            <p className="text-xs text-[var(--g5-text2)]">
              This exact issue will be created in {destination.displayKey} as @
              {targetData?.githubLogin}. The description ends with a hidden G5
              reference line (not rendered by GitHub) that lets G5 tell whether
              the issue was created.
            </p>
            <div>
              <p className="text-xs font-medium text-[var(--g5-text2)]">
                Title
              </p>
              <p className="mt-0.5 text-sm font-semibold text-[var(--g5-text0)]">
                {title.trim()}
              </p>
            </div>
            <div>
              <p className="text-xs font-medium text-[var(--g5-text2)]">
                Description
              </p>
              <pre className="mt-0.5 max-h-64 overflow-auto whitespace-pre-wrap rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2 text-sm text-[var(--g5-text0)]">
                {body.trim()
                  ? `${body.trimEnd()}\n\n${MARKER_PREVIEW}`
                  : MARKER_PREVIEW}
              </pre>
            </div>
            <div>
              <p className="text-xs font-medium text-[var(--g5-text2)]">
                Assignees
              </p>
              <p className="mt-0.5 text-sm text-[var(--g5-text0)]">
                {assignees.length === 0
                  ? "Unassigned"
                  : assignees
                      .map((login) =>
                        displayAssignee(login, targetData?.githubLogin ?? null),
                      )
                      .join(", ")}
              </p>
            </div>
            {labels.length > 0 && (
              <div>
                <p className="text-xs font-medium text-[var(--g5-text2)]">
                  Labels
                </p>
                <p
                  data-testid="create-issue-review-labels"
                  className="mt-0.5 text-sm text-[var(--g5-text0)]"
                >
                  {labels.join(", ")}
                </p>
              </div>
            )}
            {trimmedParent && (
              <div>
                <p className="text-xs font-medium text-[var(--g5-text2)]">
                  Parent issue
                </p>
                <p
                  data-testid="create-issue-review-parent"
                  className="mt-0.5 text-sm text-[var(--g5-text0)]"
                >
                  {parentIssue
                    ? `${parentIssue.displayKey} · ${parentIssue.title}`
                    : trimmedParent}
                </p>
              </div>
            )}
          </div>
        ) : (
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
                data-testid="create-issue-title"
                required
                maxLength={MAX_ISSUE_TITLE_CHARS}
                value={title}
                onChange={(event) => {
                  setTitle(event.target.value.replace(/[\r\n]+/g, " "));
                  edited();
                }}
                className={FIELD_CLASSES}
              />
            </div>
            <div>
              <label
                htmlFor={bodyId}
                className="mb-1 block text-sm font-medium text-[var(--g5-text1)]"
              >
                Description{" "}
                <span className="font-normal text-[var(--g5-text2)]">
                  (Markdown)
                </span>
              </label>
              <textarea
                ref={bodyRef}
                id={bodyId}
                data-testid="create-issue-body"
                rows={8}
                value={body}
                onChange={(event) => {
                  setBody(event.target.value);
                  edited();
                }}
                {...(native ? bodyImageHandlers : {})}
                className={classNames(
                  FIELD_CLASSES,
                  "font-[var(--g5-font-mono)]",
                )}
              />
              {native && (
                <div className="mt-1.5 space-y-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <AttachImageButton
                      onFiles={(files) => takeImages(files, true)}
                      disabled={pending}
                      testId="create-issue-image"
                    />
                    <span className="text-xs text-[var(--g5-text2)]">
                      {imagesToAdd.length > 0
                        ? `${imagesToAdd.length === 1 ? "1 image is" : `${imagesToAdd.length} images are`} uploaded when the issue is created.`
                        : "Paste or drop PNG, JPEG, GIF or WebP images (10 MiB each) into the description."}
                    </span>
                  </div>
                  {unplacedImages.map((image) => (
                    <div
                      key={image.id}
                      data-testid="create-issue-image-unplaced"
                      className="flex flex-wrap items-center gap-2 text-xs text-[var(--g5-text1)]"
                    >
                      <p className="min-w-0 flex-1">
                        {image.fileName} is attached to the issue, not shown in
                        the description.
                      </p>
                      <Button
                        kind="ghost"
                        data-testid="create-issue-image-unplaced-remove"
                        disabled={pending}
                        onClick={() => {
                          setPendingImages((current) =>
                            current.filter((other) => other.id !== image.id),
                          );
                          edited();
                        }}
                        className="px-2 py-0.5 text-xs"
                      >
                        Remove
                      </Button>
                    </div>
                  ))}
                  {imageRefusals.map((message) => (
                    <p
                      key={message}
                      role="alert"
                      data-testid="create-issue-image-refused"
                      className="text-xs text-[var(--g5-blocked)]"
                    >
                      {message}
                    </p>
                  ))}
                </div>
              )}
              {imagesStranded && (
                <p
                  role="alert"
                  data-testid="create-issue-image-stranded"
                  className="mt-1 text-xs text-[var(--g5-blocked)]"
                >
                  New issues now go to {destination?.displayKey ?? "GitHub"},
                  which takes no uploads from G5: remove the images to create it
                  there.
                </p>
              )}
            </div>
            {!titleAndDescriptionOnly && (
              <>
                <AssigneePicker
                  target={targetData}
                  selected={assignees}
                  disabled={pending}
                  onToggle={toggleAssignee}
                />
                {destination && (
                  <IssueLabelPicker
                    // Bound to the destination it was opened for.
                    key={destination.domainId}
                    orgId={orgId}
                    domain={{
                      domainId: destination.domainId,
                      displayKey: destination.displayKey,
                      provider: native ? "g5" : "github",
                      contentTrusted: destination.contentTrusted,
                    }}
                    selected={labels}
                    onChange={(names) => {
                      setLabelSelection({
                        domainId: destination.domainId,
                        names,
                      });
                      edited();
                    }}
                    limit={MAX_LABELS_PER_CHANGE}
                    disabled={pending}
                    canManage={canManageLabels}
                    hint={`(optional, at most ${MAX_LABELS_PER_CHANGE})`}
                  />
                )}
                <div>
                  <label
                    htmlFor={parentId}
                    className="mb-1 block text-sm font-medium text-[var(--g5-text1)]"
                  >
                    Parent issue{" "}
                    <span className="font-normal text-[var(--g5-text2)]">
                      (optional)
                    </span>
                  </label>
                  <input
                    id={parentId}
                    type="text"
                    data-testid="create-issue-parent"
                    aria-describedby={parentHintId}
                    aria-invalid={
                      parentOutside ||
                      parentLookupError !== null ||
                      parentRefused
                    }
                    value={parent}
                    onChange={(event) => {
                      setParent(event.target.value);
                      edited();
                    }}
                    placeholder={
                      native
                        ? `#3, ${destination?.displayKey ?? "ACME"}-3 or issue URL`
                        : "#123, owner/repo#123 or issue URL"
                    }
                    className={FIELD_CLASSES}
                  />
                  {trimmedParent !== "" &&
                    (parentIssue ? (
                      <p
                        data-testid="create-issue-parent-lookup"
                        className="mt-1 truncate text-xs text-[var(--g5-text1)]"
                      >
                        <span className="font-mono">
                          {parentIssue.displayKey}
                        </span>{" "}
                        · {parentIssue.title}
                      </p>
                    ) : parentLookupError === null ? (
                      <p
                        role="status"
                        data-testid="create-issue-parent-lookup"
                        className="mt-1 text-xs text-[var(--g5-text2)]"
                      >
                        Looking up issue…
                      </p>
                    ) : null)}
                  {(parentOutside || parentLookupError || parentRefused) && (
                    <p
                      role="alert"
                      data-testid="create-issue-parent-error"
                      className="mt-1 text-xs text-[var(--g5-blocked)]"
                    >
                      {parentRefused
                        ? describeCreateIssueError(createIssue.error)
                        : parentOutside
                          ? native
                            ? `The parent must be a ${destination?.displayKey} issue, where the new issue is created.`
                            : `The parent must be an issue in ${destination?.displayKey}, where the new issue is created.`
                          : parentLookupError}
                    </p>
                  )}
                  <p
                    id={parentHintId}
                    className="mt-1 text-xs text-[var(--g5-text2)]"
                  >
                    {native
                      ? `The new issue becomes a sub-issue of this one, which must be a ${destination?.displayKey ?? "G5"} issue.`
                      : "The new issue becomes a sub-issue of this one, which must be in the repository it is created in."}
                  </p>
                </div>
              </>
            )}
            {trusted && ready && !native && (
              <p className="text-xs text-[var(--g5-text2)]">
                G5 adds a hidden reference line to the description so it can
                tell whether the issue was created.
              </p>
            )}
          </>
        )}

        {attaching && (
          <p
            role="status"
            data-testid="create-issue-adding-images"
            className="flex items-center gap-1.5 text-xs text-[var(--g5-text1)]"
          >
            <LoaderCircle aria-hidden="true" className="h-3 w-3 animate-spin" />
            Adding images to the new issue…
          </p>
        )}

        {createIssue.error && !parentRefused && (
          <div
            role="alert"
            data-testid="create-issue-error"
            className="flex flex-wrap items-center gap-2 text-xs text-[var(--g5-blocked)]"
          >
            <p className="min-w-0 flex-1">
              {describeCreateIssueError(createIssue.error)}
            </p>
            {destinationChanged && (
              <Button
                kind="secondary"
                data-testid="create-issue-review-again"
                onClick={reviewAgain}
                className="px-2.5 py-1 text-xs"
              >
                Review again
              </Button>
            )}
          </div>
        )}

        {unresolved && (
          <div
            role="status"
            data-testid="create-issue-unknown"
            className="rounded-md border border-dashed border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2.5 text-xs text-[var(--g5-text1)]"
          >
            {unresolved.result.state === "not_found" ? (
              <p>
                GitHub has no issue from this submission in{" "}
                {unresolved.result.destination.displayKey}. Create anyway sends
                it; G5 looks for it once more first.
              </p>
            ) : (
              <p>
                GitHub did not answer clearly, so the issue may or may not exist
                in {unresolved.result.destination.displayKey}. G5 will not
                create it again on its own.
                {unresolved.result.message && (
                  <span className="text-[var(--g5-text2)]">
                    {" "}
                    ({unresolved.result.message})
                  </span>
                )}
              </p>
            )}
            <div className="mt-2 flex flex-wrap gap-2">
              <Button
                kind="secondary"
                data-testid="create-issue-check-again"
                disabled={pending}
                onClick={() => void submit(unresolved.fork, true)}
                className="px-2.5 py-1 text-xs"
              >
                Check again
              </Button>
              <Button
                kind="secondary"
                data-testid="create-issue-create-anyway"
                disabled={pending || !ready}
                title="Looks for the issue once more, and creates it only if it is not there"
                onClick={() => void submit(unresolved.fork)}
                className="px-2.5 py-1 text-xs"
              >
                Create anyway
              </Button>
            </div>
          </div>
        )}
      </div>

      <div className="flex flex-wrap justify-end gap-2 border-t border-[var(--g5-stroke)] px-5 py-3">
        <Button
          kind="ghost"
          data-testid="create-issue-cancel"
          disabled={pending}
          onClick={onClose}
        >
          {createdNotice || imageOutcome ? "Close" : "Cancel"}
        </Button>
        {reviewing && !createdNotice && !imageOutcome && (
          <Button
            kind="secondary"
            data-testid="create-issue-edit"
            disabled={pending}
            onClick={() => setReviewing(false)}
          >
            Edit
          </Button>
        )}
        {/* An unclear outcome offers only Check again and Create anyway; a
            parent left out offers only its own choices. */}
        {unresolved || createdNotice || imageOutcome ? null : reviewing ||
          trusted ||
          !ready ? (
          createButtons
        ) : (
          <Button
            kind="primary"
            data-testid="create-issue-review-button"
            disabled={!canCreate}
            onClick={() => {
              createIssue.reset();
              setReviewing(true);
            }}
          >
            Review issue
          </Button>
        )}
      </div>
    </Modal>
  );
}
