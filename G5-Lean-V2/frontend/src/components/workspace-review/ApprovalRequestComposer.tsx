import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Info, X } from "lucide-react";
import {
  useRequestApprovals,
  type ApprovalRequestResult,
  type ApprovalRequestTarget,
} from "../../hooks/useWorkspaceApprovals";
import { useWorkspaceReviewFlow } from "../../hooks/useWorkspaceReviewFlow";
import {
  bulkSelection,
  chooseRandom,
  pickKey,
  pickSelection,
  requestCandidates,
  sortRequirements,
  type ApprovalPick,
  type ApprovalSelection,
  type RequestMode,
} from "../../lib/approvalSelection";
import { requirementPhase } from "../../lib/approvalPhases";
import { randomUuid } from "../../lib/randomUuid";
import type {
  ApprovalRequestPolicyDTO,
  WorkspaceOutgoingRequirementDTO,
} from "../../lib/types";
import { AnchoredPopup } from "../ui/AnchoredPopup";
import { ThemedSelect } from "../ui/UI";
import { Modal } from "../ui/Modal";
import { reviewButton, reviewInput } from "./ReviewDialog";

/** One batch of request operations; stored so unfinished ones keep their identities. */
type Submission = {
  targets: ApprovalRequestTarget[];
  results: ApprovalRequestResult[];
};

/** A prefilled entry (Request again / Request outstanding approvals). */
export interface ApprovalRequestPrefill {
  /** Distinguishes successive entries so each opens a new dialog. */
  token: number;
  picks: ApprovalPick[];
}

function fulfilledKeys(submission: Submission): Set<string> {
  return new Set(
    submission.results
      .filter((result) => result.status === "fulfilled")
      .map((result) => result.target.key),
  );
}

function unfinished(submission: Submission | null): submission is Submission {
  if (!submission) return false;
  const done = fulfilledKeys(submission);
  return submission.targets.some((target) => !done.has(target.key));
}

function readStored(storageKey: string): Submission | null {
  try {
    const value = JSON.parse(
      sessionStorage.getItem(storageKey) ?? "null",
    ) as Submission | null;
    return value && Array.isArray(value.targets) && Array.isArray(value.results)
      ? value
      : null;
  } catch {
    return null;
  }
}

function writeStored(storageKey: string, value: Submission | null) {
  if (value) sessionStorage.setItem(storageKey, JSON.stringify(value));
  else sessionStorage.removeItem(storageKey);
}

/**
 * An info action whose explanation opens as an anchored popup above the
 * dialog: never clipped by the dialog's scroll area, and dismissed with ×,
 * an outside click or Escape like every approval popup.
 */
function InfoPopup({
  label,
  title,
  testId,
  children,
}: {
  label: string;
  title: string;
  testId: string;
  children: ReactNode;
}) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  return (
    <>
      <button
        type="button"
        data-testid={testId}
        aria-label={label}
        title={label}
        aria-haspopup="dialog"
        className="inline-flex shrink-0 items-center rounded border border-[var(--g5-stroke)] p-1 text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
        onClick={(event) => setAnchor(event.currentTarget)}
      >
        <Info size={14} aria-hidden="true" />
      </button>
      {anchor && (
        <AnchoredPopup
          anchor={anchor}
          title={title}
          ariaLabel={title}
          widthClass="w-[min(26rem,calc(100vw-16px))]"
          testId={`${testId}-popup`}
          onClose={() => setAnchor(null)}
        >
          <div className="space-y-2 text-xs leading-relaxed text-[var(--g5-text1)]">
            {children}
          </div>
        </AnchoredPopup>
      )}
    </>
  );
}

export function ApprovalRequestComposer({
  workspaceId,
  ownerId,
  requirements,
  requestPolicy,
  disabled,
  prefill = null,
  onPrefillClose,
}: {
  workspaceId: string;
  ownerId: string;
  requirements: WorkspaceOutgoingRequirementDTO[];
  /** Server request policy; a missing value means no PR is required. */
  requestPolicy?: ApprovalRequestPolicyDTO | null;
  disabled: boolean;
  /** Opens the dialog in Pick mode with exactly these recipients checked. */
  prefill?: ApprovalRequestPrefill | null;
  onPrefillClose?: () => void;
}) {
  const flow = useWorkspaceReviewFlow(workspaceId);
  const prRequired = requestPolicy?.prRequired ?? false;
  const [freshOpen, setFreshOpen] = useState<number | null>(null);
  const opens = useRef(0);
  const dialogKey = prefill
    ? `prefill:${prefill.token}`
    : freshOpen !== null
      ? `fresh:${freshOpen}`
      : null;
  return (
    <>
      <button
        className={reviewButton}
        data-testid="workspace-request-review"
        disabled={disabled || (prRequired && flow.isPending)}
        aria-haspopup="dialog"
        onClick={() => {
          opens.current += 1;
          setFreshOpen(opens.current);
        }}
      >
        Request ▾
      </button>
      {dialogKey && (
        <ApprovalRequestDialog
          key={dialogKey}
          workspaceId={workspaceId}
          ownerId={ownerId}
          requirements={requirements}
          prRequired={prRequired}
          disabled={disabled}
          initialPicks={prefill?.picks ?? null}
          onClose={() => {
            setFreshOpen(null);
            if (prefill) onPrefillClose?.();
          }}
        />
      )}
    </>
  );
}

function ApprovalRequestDialog({
  workspaceId,
  ownerId,
  requirements,
  prRequired,
  disabled,
  initialPicks,
  onClose,
}: {
  workspaceId: string;
  ownerId: string;
  requirements: WorkspaceOutgoingRequirementDTO[];
  prRequired: boolean;
  disabled: boolean;
  initialPicks: ApprovalPick[] | null;
  onClose: () => void;
}) {
  const titleId = useId();
  const modeId = useId();
  const modeLabelId = useId();
  const flow = useWorkspaceReviewFlow(workspaceId);
  const request = useRequestApprovals(workspaceId);
  const storageKey = `g5:approval-submission:${workspaceId}:${ownerId}`;
  // Unfinished operations from an earlier batch keep their stable identities;
  // a fresh page-level open resumes them, and completed batches are cleared.
  const [stored] = useState(() => readStored(storageKey));
  const [submission, setSubmission] = useState<Submission | null>(() =>
    !initialPicks && unfinished(stored) ? stored : null,
  );
  useEffect(() => {
    if (stored && !unfinished(stored)) writeStored(storageKey, null);
  }, [stored, storageKey]);
  const [mode, setMode] = useState<RequestMode | "">(
    initialPicks ? "pick" : "",
  );
  const [includeSatisfied, setIncludeSatisfied] = useState(false);
  const [bulk, setBulk] = useState<ApprovalSelection | null>(null);
  const [picked, setPicked] = useState<Set<string>>(
    () =>
      new Set(
        (initialPicks ?? []).map((p) => pickKey(p.definitionId, p.recipientId)),
      ),
  );
  const [note, setNote] = useState("");
  const sending = useRef(false);

  const hasOpenPr = !!flow.data?.pr && !flow.error;
  const requestsAllowed = !prRequired || hasOpenPr;
  const sorted = useMemo(() => sortRequirements(requirements), [requirements]);

  function choose(nextMode: RequestMode | "", include: boolean) {
    setMode(nextMode);
    setIncludeSatisfied(include);
    // Bulk choices, including the one random human, are resolved once here
    // and displayed as fixed pairs before anything is sent.
    setBulk(
      nextMode === "all" || nextMode === "agents"
        ? bulkSelection(
            requirements,
            ownerId,
            nextMode,
            include,
            requestsAllowed,
          )
        : null,
    );
  }

  const selection: ApprovalSelection | null =
    mode === "pick"
      ? requestsAllowed
        ? pickSelection(requirements, ownerId, picked)
        : { pairs: [], skipped: [] }
      : bulk;
  const pairs = selection?.pairs ?? [];
  const showSelected = mode !== "" && (mode !== "pick" || pairs.length > 0);

  function togglePick(key: string, checked: boolean) {
    setPicked((current) => {
      const next = new Set(current);
      if (checked) next.add(key);
      else next.delete(key);
      return next;
    });
  }

  function reviewRequest() {
    // Only still-unfinished operations may be retried under their identity;
    // a fulfilled target's identity would replay its settled receipt.
    const done = unfinished(stored) ? fulfilledKeys(stored) : new Set<string>();
    const priorTargets = new Map(
      (unfinished(stored) ? stored.targets : [])
        .filter((t) => !done.has(t.key))
        .map((t) => [t.key, t]),
    );
    const humanNote = note.trim() || undefined;
    const targets = pairs.map(({ requirement, recipient }) => {
      const d = requirement.definition;
      const key = `${d.id}:${recipient.id}`;
      const targetNote = recipient.kind === "human" ? humanNote : undefined;
      const previous = priorTargets.get(key);
      // An unchanged unfinished operation is retried under its identity.
      const clientRequestId =
        previous?.clientRequestId &&
        previous.definitionVersion === d.latestVersion &&
        previous.kind === recipient.kind &&
        (previous.note ?? undefined) === targetNote
          ? previous.clientRequestId
          : randomUuid();
      const common = {
        key,
        label: `${d.name} → ${recipient.label}`,
        definitionId: d.id,
        definitionVersion: d.latestVersion,
        phase: requirementPhase(requirement),
        clientRequestId,
      };
      return recipient.kind === "agent"
        ? ({ ...common, kind: "agent" } as ApprovalRequestTarget)
        : ({
            ...common,
            kind: "human",
            actorUserId: recipient.id,
            note: targetNote,
          } as ApprovalRequestTarget);
    });
    setSubmission({ targets, results: [] });
  }

  async function send() {
    if (!submission || sending.current || disabled) return;
    sending.current = true;
    try {
      // Persist before sending so an interrupted batch keeps its identities.
      writeStored(storageKey, submission);
      const done = fulfilledKeys(submission);
      const pending = submission.targets.filter((t) => !done.has(t.key));
      const results = await request.mutateAsync(pending);
      const next: Submission = {
        ...submission,
        results: [
          ...submission.results.filter((r) => r.status === "fulfilled"),
          ...results,
        ],
      };
      if (!unfinished(next)) {
        // Full success: the roster refresh already completed in the hook.
        writeStored(storageKey, null);
        onClose();
        return;
      }
      writeStored(storageKey, next);
      setSubmission(next);
    } finally {
      sending.current = false;
    }
  }

  const busy = request.isLoading;
  const closeButton = (
    <button
      type="button"
      data-testid="approval-request-close"
      aria-label="Close"
      className="shrink-0 rounded p-1 text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)] disabled:opacity-50"
      disabled={busy}
      onClick={onClose}
    >
      <X size={16} aria-hidden="true" />
    </button>
  );

  return (
    <Modal
      labelledBy={titleId}
      size="lg"
      closeDisabled={busy}
      onClose={onClose}
      testId="approval-request-dialog"
    >
      {submission ? (
        <>
          <div className="mb-3 flex items-center gap-2 border-b border-[var(--g5-stroke)] pb-3">
            <h2 id={titleId} className="flex-1 text-lg font-semibold">
              Review request
            </h2>
            {closeButton}
          </div>
          <p className="text-xs text-[var(--g5-text2)]">
            Selected {submission.targets.length} recipient(s). Only these
            recipients will be contacted.
          </p>
          <ul className="my-2">
            {submission.targets.map((target) => {
              const result = submission.results.find(
                (r) => r.target.key === target.key,
              );
              return (
                <li
                  key={target.key}
                  data-testid="approval-request-recipient"
                  className="border-b border-[var(--g5-stroke)] py-2 text-sm"
                >
                  {target.label}
                  {result?.status === "fulfilled" && (
                    <span className="ml-2 text-xs text-[var(--g5-text2)]">
                      Request sent
                    </span>
                  )}
                  {result?.status === "rejected" && (
                    <p
                      role="alert"
                      className="mt-1 text-xs text-[var(--g5-blocked)]"
                    >
                      Failed or unconfirmed: {result.message}. Retry to send
                      this request again.
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
          <div className="mt-4 flex flex-wrap gap-3">
            {submission.results.length === 0 ? (
              <button
                type="button"
                className={`${reviewButton} font-semibold`}
                data-testid="review-request-submit"
                disabled={disabled || busy}
                onClick={() => void send()}
              >
                {busy ? "Sending…" : "Submit"}
              </button>
            ) : (
              <>
                <button
                  type="button"
                  className={`${reviewButton} font-semibold`}
                  data-testid="review-request-submit"
                  disabled={disabled || busy}
                  onClick={() => void send()}
                >
                  {busy ? "Sending…" : "Retry failed requests"}
                </button>
                <button
                  type="button"
                  data-testid="approval-request-new-selection"
                  className={reviewButton}
                  disabled={busy}
                  onClick={() => {
                    writeStored(storageKey, null);
                    setSubmission(null);
                  }}
                >
                  New selection
                </button>
              </>
            )}
          </div>
        </>
      ) : (
        <>
          <div className="mb-3 flex items-center gap-2 border-b border-[var(--g5-stroke)] pb-3">
            <h2 id={titleId} className="text-lg font-semibold">
              Request approvals
            </h2>
            <InfoPopup
              label="About request options"
              title="Request options"
              testId="approval-request-options-info"
            >
              <ul className="list-disc space-y-1 pl-4">
                <li>
                  Approved requirements are satisfied and skipped unless{" "}
                  <strong>Include satisfied approval requirements</strong> is
                  checked.
                </li>
                <li>
                  <strong>Request all</strong> selects each requirement needing
                  a request and its needed reviewers. For any-of, it starts with
                  the configured agent; when a human is needed, one configured
                  human is chosen at random.
                </li>
                <li>
                  <strong>Request agents only</strong> selects configured agents
                  for requirements needing requests.
                </li>
                <li>
                  <strong>Pick approvals and approvers</strong> lets you choose
                  them directly, including satisfied requirements.
                </li>
                <li>
                  All-of needs every configured approver. Any-of needs one
                  approval and no human rejection; a human approval supersedes
                  an agent rejection only when configured.
                </li>
                <li>
                  {prRequired
                    ? "Human and agent approvals require a submitted, open PR."
                    : "Human and agent approvals can be requested before a PR exists."}
                </li>
              </ul>
            </InfoPopup>
            <span className="flex-1" />
            {closeButton}
          </div>
          {/* The visible label names the trigger through aria-labelledby;
              a native label would replace the chosen mode in its name. */}
          <span id={modeLabelId} className="mb-1 block text-sm font-semibold">
            Request mode
          </span>
          <ThemedSelect
            id={modeId}
            testId="approval-request-mode"
            className="w-fit max-w-full"
            indicator="caret"
            labelledBy={mode ? `${modeLabelId} ${modeId}` : modeLabelId}
            placeholder="Select"
            value={mode}
            options={[
              { value: "all", label: "Request all" },
              { value: "agents", label: "Request agents only" },
              { value: "pick", label: "Pick approvals and approvers" },
            ]}
            // Like a native select, reselecting the current mode changes
            // nothing (it would otherwise redraw the random human).
            onChange={(value) => {
              if (value !== mode)
                choose(value as RequestMode, includeSatisfied);
            }}
          />
          <label
            className={`my-3 flex items-center gap-2 text-sm ${mode === "pick" ? "text-[var(--g5-text2)] opacity-60" : ""}`}
          >
            <input
              type="checkbox"
              checked={includeSatisfied}
              disabled={mode === "pick"}
              onChange={(e) => choose(mode, e.target.checked)}
            />
            Include satisfied approval requirements
          </label>
          {prRequired && flow.error && (
            <p role="alert" className="my-2 text-sm">
              Request prerequisites are unavailable. Refresh before requesting
              approvals.
            </p>
          )}
          {prRequired && !flow.error && !flow.isPending && !hasOpenPr && (
            <p role="status" className="my-2 text-sm">
              Approval requests require a submitted, open PR.
            </p>
          )}
          {mode === "pick" && (
            <section
              aria-label="Pick approvals and approvers"
              className="my-3 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3"
            >
              <h3 className="mb-1 text-sm font-semibold">
                Pick approvals and approvers
              </h3>
              {sorted.map((requirement) => {
                const d = requirement.definition;
                const candidates = requestCandidates(requirement, ownerId);
                const idleHumans = candidates.filter(
                  (c) => c.kind === "human" && !c.active,
                );
                return (
                  <fieldset
                    key={d.id}
                    className="border-b border-[var(--g5-stroke)] py-2 last:border-b-0"
                  >
                    <legend className="text-sm font-semibold">{d.name}</legend>
                    {candidates.length === 0 && (
                      <p className="text-xs text-[var(--g5-text2)]">
                        No eligible recipients.
                      </p>
                    )}
                    {candidates.map((candidate) => {
                      const key = pickKey(d.id, candidate.id);
                      return (
                        <label
                          key={candidate.id}
                          className={`mt-1.5 flex items-center gap-2 text-sm ${candidate.active || !requestsAllowed ? "text-[var(--g5-text2)]" : ""}`}
                        >
                          <input
                            type="checkbox"
                            checked={!candidate.active && picked.has(key)}
                            disabled={candidate.active || !requestsAllowed}
                            onChange={(e) => togglePick(key, e.target.checked)}
                          />
                          {candidate.label}
                          {candidate.active && (
                            <span className="text-xs"> · request waiting</span>
                          )}
                        </label>
                      );
                    })}
                    {d.requireAllReviewers === false &&
                      idleHumans.length > 0 &&
                      requestsAllowed && (
                        <button
                          type="button"
                          data-testid="approval-request-random"
                          className="mt-1.5 text-xs text-[var(--g5-info)]"
                          onClick={() => {
                            const chosen = chooseRandom(idleHumans);
                            if (!chosen) return;
                            setPicked((current) => {
                              const next = new Set(current);
                              for (const human of idleHumans)
                                next.delete(pickKey(d.id, human.id));
                              next.add(pickKey(d.id, chosen.id));
                              return next;
                            });
                          }}
                        >
                          Choose human reviewer at random
                        </button>
                      )}
                  </fieldset>
                );
              })}
            </section>
          )}
          {showSelected && selection && (
            <section
              aria-label="Selected approvals"
              data-testid="approval-request-selected"
              className="my-3 rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3"
            >
              <div className="flex items-center justify-between gap-2">
                <h3 className="text-sm font-semibold">Selected approvals</h3>
                <InfoPopup
                  label="Why these selections"
                  title="Selected approvals"
                  testId="approval-request-selected-info"
                >
                  {pairs.length > 0 && (
                    <>
                      <p className="font-semibold">Why selected</p>
                      <ul className="list-disc space-y-1 pl-4">
                        {pairs.map((pair) => (
                          <li
                            key={pickKey(
                              pair.requirement.definition.id,
                              pair.recipient.id,
                            )}
                          >
                            {pair.requirement.definition.name}: {pair.reason}
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                  {selection.skipped.length > 0 && (
                    <>
                      <p className="font-semibold">Not selected</p>
                      <ul className="list-disc space-y-1 pl-4">
                        {selection.skipped.map((skip) => (
                          <li key={skip.requirement.definition.id}>
                            {skip.requirement.definition.name}: {skip.reason}
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                </InfoPopup>
              </div>
              {pairs.length === 0 ? (
                <p role="status" className="mt-1 text-sm">
                  No new approvals to request with this selection.
                </p>
              ) : (
                <ul className="mt-1 space-y-1">
                  {pairs.map((pair) => (
                    <li
                      key={pickKey(
                        pair.requirement.definition.id,
                        pair.recipient.id,
                      )}
                      data-testid="approval-request-pair"
                      className="text-sm"
                    >
                      {pair.requirement.definition.name} →{" "}
                      {pair.recipient.label}
                    </li>
                  ))}
                </ul>
              )}
            </section>
          )}
          <label className="block text-sm font-semibold">
            Note for human reviewers (optional)
            <textarea
              aria-label="Note for reviewers (optional)"
              className={`${reviewInput} mt-1 font-normal`}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <button
            type="button"
            data-testid="approval-request-preview"
            className={`${reviewButton} mt-4`}
            disabled={pairs.length === 0 || disabled}
            onClick={reviewRequest}
          >
            Review request
          </button>
        </>
      )}
    </Modal>
  );
}
