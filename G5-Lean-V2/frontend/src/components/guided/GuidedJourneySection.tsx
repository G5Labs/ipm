// =============================================================================
// G5 Workbench — GuidedJourneySection (ui g5.ui.guided-panel; workbench
// g5.workbench.journey, .journey.steps, .journey.state, .journey.gates,
// .journey.artifacts; ui g5.ui.journey-evidence)
// =============================================================================
// The Guided sidebar section under the alignment layout: a summary that
// names the one next thing, then the three phases as headings over step
// rows. The workspace's phase lists every step; only the next step is open
// with its note and buttons, the others open on click. The finished and
// upcoming phases fold behind their heading with a count. Between phases
// sits the finished phase's approver roster; the Integration Engineering
// roster sits before the merge step.
//
// A row's state is a word (Done, Next, Running, Skipped, Saving, Not saved,
// Unknown), never colour alone; done rows are muted, not struck. The ring is
// the owner's mark control (20px, a 40px hit area, `aria-pressed`): a press
// marks the step done, a second press undoes it, "Skip" is the third mark.
// Running is what the page knows about a session it started for the step's
// action; it is activity, never completion (g5.ui.journey-evidence). An
// unknown fact (marks or pull requests failed to load) reads as Unknown,
// never as to do; the summary carries the error with a Retry. A reader sees
// no controls and is told who runs each step. Mark failures stay on the row
// with a Retry; changes are announced in a polite live region.
//
// The section owns no behaviour: every button is a ribbon action rendered
// through the ribbon's enablement and dispatched to the page; marks go to
// the marks mutation; the phase move is the chevrons' move.
// =============================================================================

import React, {
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AlertCircle, Check, ChevronRight, Loader2, Lock } from "lucide-react";

import {
  JOURNEY_STEPS,
  findWorkbenchAction,
  journeyNextAction,
  journeyPhaseProgress,
  journeyStates,
  journeyStepsOf,
  type JourneyFacts,
  type JourneyStep,
  type JourneyStepState,
} from "../../lib/journeySteps";
import type {
  DocNode,
  WorkbenchAction,
  WorkbenchActions,
  WorkspaceApprovalsStateDTO,
  WorkspacePhase,
} from "../../lib/types";
import {
  WORKBENCH_PHASES,
  WORKSPACE_PHASES,
  phaseIndex,
} from "../../lib/workbenchDefaults";
import { CitationChips } from "../chat/ChatMessage";
import { classNames } from "../ui/UI";
import {
  type RibbonActionState,
  type RibbonButtonShape,
  type RibbonConfigStatus,
  RibbonButton,
  RibbonShapedButton,
  resolveRibbonActionState,
} from "../workbench/ActionRibbon";

/** The owner's mark on a step: done, skipped, or neither (to do). */
export interface JourneyMark {
  done: boolean;
  skipped: boolean;
}

/** A mark the page could not save, kept on its row until retried. */
export interface JourneyMarkFailure {
  stepId: string;
  mark: JourneyMark;
  message: string;
}

export interface GuidedJourneySectionProps {
  /** The workspace's current phase (DTO `phase`). */
  phase: WorkspacePhase;
  /** Resolved ribbon actions (served configuration or the client fallback). */
  actions: WorkbenchActions;
  configStatus: RibbonConfigStatus;
  /** The page's per-action enablement, merged over the ribbon's own rules. */
  actionState?: (action: WorkbenchAction) => RibbonActionState | undefined;
  /** Every executable click lands here (the ribbon's dispatcher). */
  onAction: (action: WorkbenchAction) => void;
  /** Replace a button entirely (preview renders its own control). */
  renderAction?: (
    action: WorkbenchAction,
    button: RibbonButtonShape,
    idPrefix?: string,
  ) => ReactNode | undefined;
  /** The action whose dispatch is in flight (the ribbon's pending state). */
  pendingActionId?: string | null;
  /**
   * Actions with a session the page started for them that still has a
   * running task: their step reads Running (never done).
   */
  runningActionIds?: ReadonlySet<string>;
  /** The facts the step states derive from. */
  facts: JourneyFacts;
  marksStatus: "loading" | "ok" | "error";
  /** Refetch the marks after a failed load. */
  onRetryMarks?: () => void;
  /**
   * Whether the source pull requests (the pull-request and merge steps'
   * evidence) loaded; a failure makes those steps read Unknown, never a
   * silent to-do.
   */
  pullRequestsStatus?: "loading" | "ok" | "error";
  onRetryPullRequests?: () => void;
  /** The approvals projection the chevron meters read; null while unavailable. */
  approvals: WorkspaceApprovalsStateDTO | null;
  /**
   * Whether that projection is known: `ok` renders rosters (or "no approvals
   * required"), `loading` and `error` say so — an unknown requirement set is
   * never reported as none (g5.workbench.journey.gates).
   */
  approvalsStatus?: "ok" | "loading" | "error";
  /** Human status previews require a submitted PR; null is an unknown read. */
  reviewSubmitted?: boolean | null;
  /** Open the shared approvals destination. */
  onReviewApprovals?: (phase: WorkspacePhase) => void;
  /**
   * The viewer owns the workspace: the rows carry controls. A reader (false)
   * sees who runs each step instead of disabled buttons.
   */
  isOwner?: boolean;
  /** Named to a reader as the person who runs the steps. */
  ownerDisplayName?: string | null;
  /** The owner's mark; absent while the owner cannot write (see the reason). */
  onMark?: (stepId: string, mark: JourneyMark) => void;
  /** Why marking is unavailable to the owner (Base, read-only, freezing). */
  markBlockedReason?: string | null;
  markPendingStepId?: string | null;
  /** The last mark that failed to save; its row offers Retry. */
  markFailure?: JourneyMarkFailure | null;
  /** The owner's phase move; absent for a reader or Base. */
  onMovePhase?: (phase: WorkspacePhase) => void;
  movePending?: boolean;
  /** Open a document (the chips). */
  onOpenDoc: (doc: DocNode) => void;
}

const FOCUS_CLASS =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--g5-bg1)]";
/** Every pressable gives press feedback; movement goes under reduced motion. */
const PRESS_CLASS =
  "transition-transform duration-[160ms] ease-[var(--g5-ease-out)] active:scale-[0.97] motion-reduce:transition-none motion-reduce:active:scale-100";
const TEXT_BUTTON_CLASS = classNames(
  "inline-flex h-8 items-center rounded-[var(--g5-radius-sm)] px-2 text-[13px] text-[var(--g5-text1)] hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]",
  "disabled:cursor-not-allowed disabled:text-[var(--g5-text2)] disabled:hover:bg-transparent",
  FOCUS_CLASS,
  PRESS_CLASS,
);
const LINK_CLASS = classNames(
  "inline-flex min-h-6 items-center rounded-[var(--g5-radius-sm)] px-0.5 text-[var(--g5-info)] hover:text-[var(--g5-text0)]",
  FOCUS_CLASS,
);
/** How long after a mark its row keeps the Undo link (then the ring undoes). */
const UNDO_WINDOW_MS = 8_000;

type RowKind =
  | "todo"
  | "next"
  | "done"
  | "skipped"
  | "running"
  | "pending"
  | "failed"
  | "unknown";

const STATUS_WORD: Record<RowKind, string> = {
  todo: "",
  next: "Next",
  done: "Done",
  skipped: "Skipped",
  running: "Running",
  pending: "Saving",
  failed: "Not saved",
  unknown: "Unknown",
};

const SR_PREFIX: Record<RowKind, string> = {
  todo: "To do: ",
  next: "Next: ",
  done: "Done: ",
  skipped: "Skipped: ",
  running: "Running: ",
  pending: "Saving: ",
  failed: "Not saved: ",
  unknown: "Unknown: ",
};

/** A step button: the ribbon's text shape at 32px, primary in the phase hue. */
function stepButtonShape(primary: boolean, hue: string): RibbonButtonShape {
  return {
    className: classNames(
      "inline-flex h-8 items-center gap-1.5 whitespace-nowrap rounded-[var(--g5-radius-sm)] border px-3 text-[13px] font-medium",
      "disabled:cursor-not-allowed disabled:border-[var(--g5-stroke)] disabled:text-[var(--g5-text2)] disabled:hover:bg-transparent",
      FOCUS_CLASS,
      PRESS_CLASS,
      primary
        ? "bg-transparent hover:bg-[var(--g5-overlay)]"
        : "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg0)]",
    ),
    style: primary ? { borderColor: hue, color: hue } : undefined,
    iconSize: 14,
    labelHidden: false,
  };
}

/** "just now", "3 min ago", "2 h ago", else the date. */
export function relativeTime(iso: string, now = Date.now()): string {
  const at = Date.parse(iso);
  if (!Number.isFinite(at)) return "";
  const seconds = Math.max(0, Math.round((now - at) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(at).toLocaleDateString();
}

export const GuidedJourneySection: React.FC<GuidedJourneySectionProps> = ({
  phase,
  actions,
  configStatus,
  actionState,
  onAction,
  renderAction,
  pendingActionId,
  runningActionIds,
  facts,
  marksStatus,
  onRetryMarks,
  pullRequestsStatus = "ok",
  onRetryPullRequests,
  isOwner = true,
  ownerDisplayName,
  onMark,
  markBlockedReason,
  markPendingStepId,
  markFailure,
  onMovePhase,
  movePending,
  onOpenDoc,
}) => {
  const states = useMemo(() => journeyStates(facts), [facts]);
  const next = journeyNextAction(phase, states);
  const progress = journeyPhaseProgress(phase, states);
  const activeIndex = phaseIndex(phase);
  const info = WORKBENCH_PHASES[phase];
  const hue = `var(${info.token})`;

  // Rows the person opened by hand (a next step is always open) and the
  // folded phases they expanded.
  const [openSteps, setOpenSteps] = useState<ReadonlySet<string>>(new Set());
  const [openPhases, setOpenPhases] = useState<ReadonlySet<WorkspacePhase>>(
    new Set(),
  );
  // The Undo link stays on a freshly marked row for a window, then the
  // ring alone undoes it; a tick re-renders when the earliest window ends.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const expiries = JOURNEY_STEPS.flatMap((step) => {
      const state = states.get(step.id);
      if (!state?.mark || state.byEvidence) return [];
      if (state.status !== "done" && state.status !== "skipped") return [];
      const at = Date.parse(state.mark.markedAt) + UNDO_WINDOW_MS;
      return at > now ? [at] : [];
    });
    if (expiries.length === 0) return;
    const timer = setTimeout(
      () => setNow(Date.now()),
      Math.min(...expiries) - now + 50,
    );
    return () => clearTimeout(timer);
  }, [states, now]);

  // The polite live region: one sentence per state change the person can
  // hear (a mark, an undo, a skip, evidence arriving, a failed save).
  const [announcement, setAnnouncement] = useState("");
  const previousStates = useRef<ReadonlyMap<string, JourneyStepState> | null>(
    null,
  );
  useEffect(() => {
    const previous = previousStates.current;
    previousStates.current = states;
    if (!previous) return;
    for (const step of JOURNEY_STEPS) {
      const before = previous.get(step.id)?.status ?? "todo";
      const after = states.get(step.id)?.status ?? "todo";
      if (before === after) continue;
      setAnnouncement(
        after === "done"
          ? `${step.label} marked done.`
          : after === "skipped"
            ? `${step.label} skipped.`
            : `${step.label} is to do again.`,
      );
      setNow(Date.now());
      return;
    }
  }, [states]);
  useEffect(() => {
    if (!markFailure) return;
    const step = JOURNEY_STEPS.find((entry) => entry.id === markFailure.stepId);
    setAnnouncement(
      `Couldn’t save your mark for ${step?.label ?? markFailure.stepId}. Retry.`,
    );
  }, [markFailure]);

  // ↑ ↓ move between the active list's controls (roving tabindex): the Tab
  // stops stay few, the rows stay reachable.
  const listRef = useRef<HTMLOListElement | null>(null);
  const onListKeyDown = (event: KeyboardEvent<HTMLOListElement>) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const list = listRef.current;
    if (!list) return;
    const items = Array.from(
      list.querySelectorAll<HTMLElement>("[data-guided-rove]"),
    );
    const index = items.indexOf(document.activeElement as HTMLElement);
    if (index < 0) return;
    event.preventDefault();
    const target =
      items[
        (index + (event.key === "ArrowDown" ? 1 : items.length - 1)) %
          items.length
      ];
    items.forEach((item) => (item.tabIndex = -1));
    target.tabIndex = 0;
    target.focus();
  };

  // The same resolution the ribbon applies: its own rules, then the page's
  // gates (g5.workbench.journey; g5.workspace.ui.gates).
  const resolve = (step: JourneyStep, actionId: string) => {
    const action = findWorkbenchAction(actions[step.phase], actionId);
    if (!action) return null;
    return {
      action,
      state: resolveRibbonActionState(action, configStatus, actionState),
    };
  };

  const renderStepButton = (
    step: JourneyStep,
    actionId: string,
    primary: boolean,
    testIdPrefix = `guided-step-${step.id}-action-`,
  ) => {
    const resolved = resolve(step, actionId);
    const shape = stepButtonShape(
      primary,
      `var(${WORKBENCH_PHASES[step.phase].token})`,
    );
    if (!resolved) {
      // The action left the phase's ribbon (a committed actions.yaml):
      // visible, disabled, with the reason (g5.workbench.journey.steps).
      return (
        <button
          key={actionId}
          type="button"
          disabled
          data-testid={`${testIdPrefix}${actionId}`}
          title={`"${actionId}" is not in this phase's ribbon`}
          className={shape.className}
        >
          {actionId}
        </button>
      );
    }
    // Like the ribbon, a custom control replaces only an ENABLED action; a
    // disabled one keeps its visible button and reason.
    const replaced =
      !resolved.state.disabled && renderAction
        ? renderAction(
            resolved.action,
            shape,
            `${testIdPrefix}${actionId}`.replace(/-$/, ""),
          )
        : undefined;
    if (replaced !== undefined) {
      return (
        <span
          key={actionId}
          data-testid={`${testIdPrefix}${actionId}`}
          className="inline-flex"
        >
          {replaced}
        </span>
      );
    }
    return (
      <span key={actionId} className="inline-flex">
        <RibbonButton
          action={resolved.action}
          shape={shape}
          state={resolved.state}
          pending={pendingActionId === resolved.action.id}
          onClick={() => onAction(resolved.action)}
          testId={`${testIdPrefix}${actionId}`}
        />
      </span>
    );
  };

  const renderChips = (step: JourneyStep, state: JourneyStepState) => {
    if (state.evidenceDocs.length === 0) return null;
    return (
      <div
        data-testid={`guided-step-${step.id}-artifacts`}
        className="g5-guided-enter col-start-2 col-end-4"
      >
        <CitationChips
          citations={state.evidenceDocs.map((doc) => ({
            label: doc.name,
            filePath: doc.filePath ?? undefined,
            repoId: doc.repoId ?? undefined,
          }))}
          onCitationClick={(citation) => {
            const doc = state.evidenceDocs.find(
              (entry) => entry.filePath === citation.filePath,
            );
            if (doc) onOpenDoc(doc);
          }}
        />
      </div>
    );
  };

  const rowKind = (
    step: JourneyStep,
    state: JourneyStepState,
    relation: "done-phase" | "active" | "upcoming",
    isNext: boolean,
  ): RowKind => {
    if (markPendingStepId === step.id) return "pending";
    if (markFailure?.stepId === step.id) return "failed";
    if (state.status === "done") return "done";
    if (state.status === "skipped") return "skipped";
    const primary = step.actions[0];
    if (
      relation === "active" &&
      primary &&
      (runningActionIds?.has(primary) || pendingActionId === primary)
    ) {
      return "running";
    }
    if (
      (marksStatus === "error" && step.evidence.kind === "marks-only") ||
      (pullRequestsStatus === "error" && step.evidence.kind === "pull-request")
    ) {
      return "unknown";
    }
    return isNext ? "next" : "todo";
  };

  const renderRing = (
    step: JourneyStep,
    state: JourneyStepState,
    kind: RowKind,
    relation: "done-phase" | "active" | "upcoming",
    isNext: boolean,
  ) => {
    const done = kind === "done";
    const glyph =
      kind === "running" || kind === "pending" ? (
        <Loader2
          size={12}
          strokeWidth={2.5}
          aria-hidden="true"
          className="animate-spin motion-reduce:animate-none"
        />
      ) : done && state.byEvidence ? (
        <Lock size={11} strokeWidth={2.5} aria-hidden="true" />
      ) : (
        <Check
          size={12}
          strokeWidth={3}
          aria-hidden="true"
          className={classNames(
            "transition-[opacity,transform] duration-[var(--g5-d-base)] ease-[var(--g5-ease-out)] motion-reduce:transition-none motion-reduce:transform-none",
            done ? "opacity-100 scale-100" : "opacity-0 scale-90",
          )}
        />
      );
    const ringClass = classNames(
      "relative mt-[1px] inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full border-[1.5px] text-[rgb(10,12,16)]",
      "transition-[background-color,border-color] duration-[var(--g5-d-base)] ease-[var(--g5-ease-out)] motion-reduce:transition-none",
      done
        ? "border-[var(--g5-ok)] bg-[var(--g5-ok)]"
        : kind === "running"
          ? "border-2 border-[var(--g5-info)] text-[var(--g5-info)] g5-guided-breathe"
          : kind === "pending"
            ? "border-[rgb(255_255_255/0.35)] text-[var(--g5-text1)]"
            : kind === "failed"
              ? "border-[var(--g5-warn)]"
              : kind === "skipped"
                ? "border-dashed border-[rgb(255_255_255/0.35)]"
                : isNext
                  ? "border-[var(--g5-info)]"
                  : "border-[rgb(255_255_255/0.35)]",
    );
    // The ring is the owner's control on the active phase's rows, except on
    // a step its outcome proves (nothing to undo) or one that is running.
    const controllable =
      relation === "active" &&
      isOwner &&
      !(done && state.byEvidence) &&
      kind !== "running" &&
      kind !== "pending";
    if (!controllable) {
      return (
        <span aria-hidden="true" className={ringClass}>
          {glyph}
        </span>
      );
    }
    const pressed = kind === "done";
    return (
      <button
        type="button"
        data-testid={`guided-step-${step.id}-mark`}
        data-guided-rove=""
        tabIndex={isNext ? 0 : -1}
        aria-pressed={pressed}
        aria-label={`${pressed ? "Marked done" : "Mark done"}: ${step.label}`}
        disabled={!onMark}
        title={markBlockedReason ?? undefined}
        onClick={() => onMark?.(step.id, { done: !pressed, skipped: false })}
        className={classNames(
          ringClass,
          "cursor-pointer before:absolute before:-inset-[10px] before:rounded-full before:content-['']",
          "hover:bg-[var(--g5-bg2)] disabled:cursor-not-allowed",
          FOCUS_CLASS,
          "transition-transform active:scale-[0.94] motion-reduce:active:scale-100",
        )}
      >
        {glyph}
      </button>
    );
  };

  const renderStep = (
    step: JourneyStep,
    relation: "done-phase" | "active" | "upcoming",
    isNext: boolean,
  ) => {
    const state = states.get(step.id) ?? {
      status: "todo" as const,
      byMark: false,
      byEvidence: false,
      mark: null,
      evidenceDocs: [],
    };
    const kind = rowKind(step, state, relation, isNext);
    const canToggle =
      relation === "active" &&
      (kind === "todo" || kind === "unknown" || kind === "running");
    const expanded = isNext || openSteps.has(step.id);
    const showBody =
      relation === "active" &&
      expanded &&
      kind !== "done" &&
      kind !== "skipped" &&
      kind !== "pending";
    const primaryAction = step.actions[0]
      ? resolve(step, step.actions[0])
      : null;
    const recentlyMarked =
      state.mark !== null &&
      !state.byEvidence &&
      Date.parse(state.mark.markedAt) + UNDO_WINDOW_MS > now;
    const bodyId = `guided-step-${step.id}-body`;
    return (
      <li
        key={step.id}
        data-testid={`guided-step-${step.id}`}
        data-state={state.status}
        data-kind={kind}
        aria-current={isNext ? "step" : undefined}
        aria-busy={kind === "running" || kind === "pending" ? true : undefined}
        className="grid min-h-8 scroll-mt-12 grid-cols-[20px_1fr_auto] items-start gap-x-2.5 py-1.5"
      >
        <span className="sr-only">{SR_PREFIX[kind]}</span>
        {renderRing(step, state, kind, relation, isNext)}
        {canToggle ? (
          <button
            type="button"
            data-testid={`guided-step-${step.id}-toggle`}
            data-guided-rove=""
            tabIndex={-1}
            aria-expanded={expanded}
            aria-controls={bodyId}
            onClick={() =>
              setOpenSteps((current) => {
                const nextSet = new Set(current);
                if (nextSet.has(step.id)) nextSet.delete(step.id);
                else nextSet.add(step.id);
                return nextSet;
              })
            }
            className={classNames(
              "min-w-0 rounded-[var(--g5-radius-sm)] text-left text-[13px] leading-5 text-[var(--g5-text0)]",
              FOCUS_CLASS,
            )}
          >
            {step.label}
          </button>
        ) : (
          <span
            className={classNames(
              "min-w-0 text-[13px] leading-5",
              kind === "done" || kind === "skipped" || relation !== "active"
                ? "text-[var(--g5-text1)]"
                : "text-[var(--g5-text0)]",
              isNext && "font-medium",
              kind === "skipped" && "italic",
            )}
          >
            {step.label}
          </span>
        )}
        <span
          aria-hidden="true"
          className={classNames(
            "whitespace-nowrap text-[12px] leading-5 tabular-nums",
            kind === "done"
              ? "text-[var(--g5-ok)]"
              : kind === "running"
                ? "text-[var(--g5-info)]"
                : kind === "failed"
                  ? "text-[var(--g5-warn)]"
                  : kind === "next"
                    ? "text-[var(--g5-info)]"
                    : "text-[var(--g5-text2)]",
          )}
        >
          {STATUS_WORD[kind]}
        </span>
        {showBody ? (
          <div id={bodyId} className="g5-guided-enter col-start-2 col-end-4">
            {step.note ? (
              <div className="mt-0.5 text-[12px] leading-4 text-[var(--g5-text1)] text-pretty">
                {step.note}
              </div>
            ) : null}
            {isOwner ? (
              <div className="mt-2 flex flex-wrap items-center gap-2">
                {step.actions.map((actionId, index) =>
                  renderStepButton(step, actionId, index === 0 && isNext),
                )}
                {kind !== "running" ? (
                  <>
                    <button
                      type="button"
                      data-testid={`guided-step-${step.id}-mark-done`}
                      disabled={!onMark}
                      title={markBlockedReason ?? undefined}
                      onClick={() =>
                        onMark?.(step.id, { done: true, skipped: false })
                      }
                      className={TEXT_BUTTON_CLASS}
                    >
                      Mark done
                    </button>
                    <button
                      type="button"
                      data-testid={`guided-step-${step.id}-skip`}
                      disabled={!onMark}
                      title={
                        markBlockedReason ??
                        "Not needed in this workspace; left out of the count"
                      }
                      onClick={() =>
                        onMark?.(step.id, { done: false, skipped: true })
                      }
                      className={TEXT_BUTTON_CLASS}
                    >
                      Skip
                    </button>
                  </>
                ) : null}
              </div>
            ) : (
              <div
                data-testid={`guided-step-${step.id}-owner`}
                className="mt-1.5 text-[12px] leading-4 text-[var(--g5-text1)]"
              >
                {ownerDisplayName ?? "The workspace owner"} runs this step
              </div>
            )}
          </div>
        ) : null}
        {kind === "running" ? (
          <div className="col-start-2 col-end-4 mt-0.5 text-[12px] leading-4 text-[var(--g5-text1)]">
            {primaryAction?.action.label ?? step.label} is running
          </div>
        ) : null}
        {kind === "pending" ? (
          <div className="col-start-2 col-end-4 mt-0.5 text-[12px] leading-4 text-[var(--g5-text1)]">
            Saving your mark
          </div>
        ) : null}
        {kind === "failed" && markFailure ? (
          <div
            role="alert"
            data-testid={`guided-step-${step.id}-error`}
            className="col-start-2 col-end-4 mt-0.5 flex flex-wrap items-center gap-x-2 text-[12px] leading-4 text-[var(--g5-warn)]"
          >
            <span className="inline-flex items-center gap-1.5">
              <AlertCircle size={14} aria-hidden="true" />
              Couldn’t save your mark.
            </span>
            <button
              type="button"
              data-testid={`guided-step-${step.id}-retry`}
              onClick={() => onMark?.(step.id, markFailure.mark)}
              className={classNames(LINK_CLASS, "text-[var(--g5-text0)]")}
            >
              Retry
            </button>
          </div>
        ) : null}
        {kind === "done" && state.byMark && !state.byEvidence && state.mark ? (
          <div className="col-start-2 col-end-4 mt-0.5 flex flex-wrap items-center gap-x-2 text-[12px] leading-4 text-[var(--g5-text1)] tabular-nums">
            <span>Marked done {relativeTime(state.mark.markedAt, now)}</span>
            {recentlyMarked && onMark && relation === "active" ? (
              <button
                type="button"
                data-testid={`guided-step-${step.id}-undo`}
                onClick={() => onMark(step.id, { done: false, skipped: false })}
                className={LINK_CLASS}
              >
                Undo
              </button>
            ) : null}
          </div>
        ) : null}
        {kind === "skipped" ? (
          <div className="col-start-2 col-end-4 mt-0.5 flex flex-wrap items-center gap-x-2 text-[12px] leading-4 text-[var(--g5-text1)]">
            <span>Skipped, not counted</span>
            {onMark && relation === "active" ? (
              <button
                type="button"
                data-testid={`guided-step-${step.id}-undo`}
                onClick={() => onMark(step.id, { done: false, skipped: false })}
                className={LINK_CLASS}
              >
                Undo
              </button>
            ) : null}
          </div>
        ) : null}
        {renderChips(step, state)}
      </li>
    );
  };

  const renderPhase = (phaseId: WorkspacePhase) => {
    const index = phaseIndex(phaseId);
    const relation =
      index < activeIndex
        ? ("done-phase" as const)
        : index === activeIndex
          ? ("active" as const)
          : ("upcoming" as const);
    const phaseInfo = WORKBENCH_PHASES[phaseId];
    const phaseProgress = journeyPhaseProgress(phaseId, states);
    const steps = journeyStepsOf(phaseId);
    const nextStepId = next.kind === "step" ? next.step.id : null;
    const before = steps.filter((step) => step.id !== "ie-merge");
    const merge = steps.filter((step) => step.id === "ie-merge");
    const headingId = `guided-phase-${phaseId}-heading`;
    const listId = `guided-phase-${phaseId}-list`;
    const open = relation === "active" || openPhases.has(phaseId);
    const count = (
      <span className="ml-auto whitespace-nowrap text-[12px] font-normal leading-5 text-[var(--g5-text2)] tabular-nums">
        {phaseProgress.done} of {phaseProgress.total}
      </span>
    );
    const list = (
      <>
        <ol
          id={listId}
          ref={relation === "active" ? listRef : undefined}
          aria-labelledby={headingId}
          onKeyDown={relation === "active" ? onListKeyDown : undefined}
          className="mt-1"
        >
          {before.map((step) =>
            renderStep(step, relation, step.id === nextStepId),
          )}
        </ol>
        {phaseId === "ie" ? (
          <>
            <ol aria-label="Landing" className="mt-0">
              {merge.map((step) =>
                renderStep(step, relation, step.id === nextStepId),
              )}
            </ol>
          </>
        ) : null}
      </>
    );
    return (
      <React.Fragment key={phaseId}>
        <section
          data-testid={`guided-phase-${phaseId}`}
          data-relation={relation}
          className="mt-4 first:mt-3"
        >
          {relation === "active" ? (
            <>
              <h3
                id={headingId}
                className="flex items-baseline gap-2 text-[14px] font-medium leading-5 text-[var(--g5-text0)]"
              >
                {phaseInfo.name}
                {count}
              </h3>
              <div className="text-[12px] leading-4 text-[var(--g5-text1)]">
                {phaseInfo.subline}
              </div>
              {list}
            </>
          ) : (
            <>
              <h3
                id={headingId}
                className={classNames(
                  "flex items-center gap-2 text-[14px] font-medium leading-5",
                  relation === "upcoming"
                    ? "text-[var(--g5-text1)]"
                    : "text-[var(--g5-text0)]",
                )}
              >
                <button
                  type="button"
                  data-testid={`guided-phase-${phaseId}-toggle`}
                  aria-expanded={open}
                  aria-controls={listId}
                  onClick={() =>
                    setOpenPhases((current) => {
                      const nextSet = new Set(current);
                      if (nextSet.has(phaseId)) nextSet.delete(phaseId);
                      else nextSet.add(phaseId);
                      return nextSet;
                    })
                  }
                  className={classNames(
                    "-ml-1 inline-flex min-h-7 items-center gap-1.5 rounded-[var(--g5-radius-sm)] pl-1 pr-1 text-left",
                    FOCUS_CLASS,
                  )}
                >
                  <ChevronRight
                    size={12}
                    strokeWidth={2.5}
                    aria-hidden="true"
                    className={classNames(
                      "text-[var(--g5-text2)] transition-transform duration-[var(--g5-d-fast)] ease-[var(--g5-ease-out)] motion-reduce:transition-none",
                      open && "rotate-90",
                    )}
                  />
                  {phaseInfo.name}
                </button>
                {count}
              </h3>
              {open ? <div className="g5-guided-enter">{list}</div> : null}
            </>
          )}
        </section>
      </React.Fragment>
    );
  };

  // ---- summary ----
  const summary = (() => {
    const [done, total] = [progress.done, progress.total];
    const ownerSentence = isOwner
      ? ""
      : ` ${ownerDisplayName ?? "The workspace owner"} owns this workspace.`;
    if (next.kind === "complete") {
      return (
        <div
          data-testid="guided-summary"
          className="g5-guided-enter rounded-[var(--g5-radius-sm)] border border-[var(--g5-ok)] bg-[rgb(113_201_136/0.1)] px-3 py-3"
        >
          <div className="flex items-center gap-2 text-[14px] font-medium leading-5 text-[var(--g5-text0)]">
            <span
              aria-hidden="true"
              className="inline-flex h-5 w-5 items-center justify-center rounded-full bg-[var(--g5-ok)] text-[rgb(10,12,16)]"
            >
              <Check size={12} strokeWidth={3} />
            </span>
            Journey complete
          </div>
          <div className="mt-0.5 text-[12px] leading-4 text-[var(--g5-text1)]">
            Merged to the parent.{ownerSentence}
          </div>
        </div>
      );
    }
    const lead = next.kind === "step" ? next.step.label : "Every step is done";
    const sub =
      next.kind === "step"
        ? `${info.name}, ${done} of ${total} done.${ownerSentence}`
        : `${info.name}, ${done} of ${total} done. Next: move to ${WORKBENCH_PHASES[next.to].name}.${ownerSentence}`;
    const errorLine = (
      text: string,
      retry: (() => void) | undefined,
      testId: string,
    ) => (
      <div
        data-testid={testId}
        className="mt-2 text-[12px] leading-4 text-[var(--g5-text1)]"
      >
        <span className="font-medium text-[var(--g5-text0)]">{text}</span>{" "}
        {retry ? (
          <button
            type="button"
            data-testid={`${testId}-retry`}
            onClick={retry}
            className={LINK_CLASS}
          >
            Retry
          </button>
        ) : null}
      </div>
    );
    return (
      <div
        data-testid="guided-summary"
        className="rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg2)] px-3 py-3"
      >
        <div className="text-[14px] font-medium leading-5 text-[var(--g5-text0)] text-pretty">
          {lead}
        </div>
        <div className="mt-0.5 text-[12px] leading-4 text-[var(--g5-text1)] tabular-nums">
          {sub}
        </div>
        {marksStatus === "error"
          ? errorLine(
              "Couldn’t load your marks. Showing what the documents prove.",
              onRetryMarks,
              "guided-marks-error",
            )
          : null}
        {pullRequestsStatus === "error"
          ? errorLine(
              "Couldn’t load pull requests. Their steps read Unknown.",
              onRetryPullRequests,
              "guided-pull-requests-error",
            )
          : null}
        {isOwner && next.kind === "step" && next.step.actions[0] ? (
          <div className="mt-2.5 flex flex-wrap items-center gap-2">
            {renderStepButton(
              next.step,
              next.step.actions[0],
              true,
              "guided-summary-action-",
            )}
            <button
              type="button"
              data-testid="guided-summary-skip"
              disabled={!onMark}
              title={
                markBlockedReason ??
                "Not needed in this workspace; left out of the count"
              }
              onClick={() =>
                onMark?.(next.step.id, { done: false, skipped: true })
              }
              className={TEXT_BUTTON_CLASS}
            >
              Skip
            </button>
          </div>
        ) : null}
        {next.kind === "move" ? (
          <div className="mt-2.5">
            <RibbonShapedButton
              shape={stepButtonShape(
                true,
                `var(${WORKBENCH_PHASES[next.to].token})`,
              )}
              icon={
                movePending ? (
                  <Loader2
                    size={14}
                    aria-hidden="true"
                    className="animate-spin motion-reduce:animate-none"
                  />
                ) : (
                  <ChevronRight size={14} aria-hidden="true" />
                )
              }
              label={`Move to ${WORKBENCH_PHASES[next.to].name}`}
              title={
                onMovePhase
                  ? `Set the workspace phase to ${WORKBENCH_PHASES[next.to].name}`
                  : "Only the workspace owner moves the phase"
              }
              disabled={!onMovePhase || !!movePending}
              onClick={() => onMovePhase?.(next.to)}
              testId="guided-summary-move"
            />
          </div>
        ) : null}
      </div>
    );
  })();

  if (marksStatus === "loading") {
    // Flat bars at the rows' real heights, no shimmer: nothing jumps when
    // the marks land.
    const bar = (width: string, height: string, extra = "") => (
      <span
        aria-hidden="true"
        className={classNames(
          "block rounded-[3px] bg-[rgb(255_255_255/0.07)]",
          extra,
        )}
        style={{ width, height }}
      />
    );
    return (
      <div
        data-testid="guided-journey"
        data-loading="true"
        aria-busy="true"
        aria-label="Loading the guided journey"
        className="px-3 pb-4 pt-2"
        style={{ "--hue": hue } as CSSProperties}
      >
        <div className="rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg2)] px-3 py-3">
          {bar("62%", "14px", "my-[3px]")}
          {bar("44%", "10px", "mb-[3px] mt-1.5")}
        </div>
        <div className="mt-4 flex items-center gap-2">
          {bar("46%", "12px")}
          {bar("30px", "10px", "ml-auto")}
        </div>
        {["58%", "40%", "52%", "46%"].map((width) => (
          <div
            key={width}
            className="grid min-h-8 grid-cols-[20px_1fr] items-start gap-x-2.5 py-1.5"
          >
            <span
              aria-hidden="true"
              className="mt-[1px] inline-block h-5 w-5 rounded-full border-[1.5px] border-[rgb(255_255_255/0.12)]"
            />
            {bar(width, "11px", "mt-[5px]")}
          </div>
        ))}
        <span className="sr-only">Loading the guided journey</span>
      </div>
    );
  }

  return (
    <nav
      data-testid="guided-journey"
      aria-label="Guided journey"
      className="px-3 pb-4 pt-2 text-[13px]"
      style={{ "--hue": hue } as CSSProperties}
    >
      {summary}
      <div
        role="status"
        aria-live="polite"
        data-testid="guided-live"
        className="sr-only"
      >
        {announcement}
      </div>
      {WORKSPACE_PHASES.map(renderPhase)}
      <div className="sr-only">{JOURNEY_STEPS.length} steps</div>
    </nav>
  );
};
