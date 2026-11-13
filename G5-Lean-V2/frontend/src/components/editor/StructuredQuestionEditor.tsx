// =============================================================================
// StructuredQuestionEditor — interactive `g5-question` card (center-panel D69)
// =============================================================================
// Renders a fenced `g5-question` code block as an interactive structured
// question card: header (id + status/severity chips), prompt, HIL-language
// option cards with aria-pressed selection, "Something else" free text with
// optional rationale, and an Answer action gated on capability (`canSave` and
// an apply callback supplied by the host editor). Payload parsing goes through
// the shared questionBlock library only; an unparseable or oversized payload
// falls back to plain code rendering with an inline notice and never blocks
// the rest of the document (lenient read).
//
// Pre-answer drafts live in a module-scoped LRU keyed `${docPath}#${id}` (chat
// question-picker pattern, ChatPanel D25) so tab switches and virtualization
// remounts keep half-typed answers; a successful apply clears the draft.
// =============================================================================

import React, {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { ChevronDown, ChevronRight, PencilLine, Plus, X } from "lucide-react";
import { Button, classNames } from "../ui/UI";
import type { EscalationOption, State } from "../../lib/types";
import type { Block } from "../../lib/utils";
import { OptionCard } from "../shared/OptionCard";
import { IdentityAvatar } from "../shared/IdentityAvatar";
import {
  GatedControl,
  useLiveGateReason,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";

const DISCUSS_TITLE =
  "Not sure yet? This drafts a chat message about this question and drops it in the composer — you can edit it, and nothing is sent until you press Send.";
import {
  hasDuplicateOptionLabels,
  parseQuestionPayload,
  type AnswerInput,
  type ParsedQuestion,
  type QuestionOption,
  type QuestionSeverity,
} from "../../lib/questionBlock";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

/** Outcome the host maps an applied answer to (live workspace file splice result, D69). On
 * `applied` the host may hand back a `watch` registration: the card supplies
 * its late-conflict callback and receives a cancel function it owns (called
 * on unmount and before any newer apply). */
export type QuestionApplyResult =
  | { status: "applied"; watch?: (onConflict: () => void) => () => void }
  | { status: "conflict"; message?: string };

export interface StructuredQuestionEditorProps {
  /** The fenced code block with language `g5-question`; body is `content`. */
  block: Extract<Block, { type: "code" }>;
  /** Host document identity (DocumentPreview file identity) keying the draft LRU. */
  docPath: string;
  /** Doc-derived default id: enclosing `[Q<N>.]` label, else index-derived. */
  fallbackId: string;
  /** Editor-level can-edit signal; false hides the Answer action entirely. */
  canSave: boolean;
  /** Authenticated answerer identity, persisted into `answer.answered_by`
   * (D69) so the record names a real person. Sourced from the same
   * `CurrentUserDTO` the comment composer uses for its author row; absent in
   * read-only or unauthenticated contexts, where only `kind` is written. */
  currentUserId?: string;
  currentUserEmail?: string;
  currentUserAvatarUrl?: string | null;
  /** True when an earlier block in this parse claimed the same question id
   * (first parsed block wins; this card renders the invalid-payload notice). */
  duplicateId?: boolean;
  /** Applies the answer through the host's live workspace file write path (D69 direct
   * apply). Absent when no live draft exists: the card hides Answer
   * (capability gating). */
  onApplyAnswer?: (
    question: ParsedQuestion,
    input: AnswerInput,
  ) => Promise<QuestionApplyResult>;
  /** Seeds the chat composer with a Discuss template for this question. */
  onDiscuss?: (question: ParsedQuestion) => void;
  /** D69 authoring: writes the field-edited question (prompt, option
   * label/description, add/remove option, recommended flag) through the
   * host's exact-body direct-apply path. The card never lets authoring touch
   * the `answer` mapping or `status`. Absent when no live draft exists. */
  onApplyEdit?: (edited: ParsedQuestion) => Promise<QuestionApplyResult>;
  /** Returns the question to unanswered (D10 Clear answer). Same direct-apply
   * path and gate as answering; absent when no live draft exists. */
  onClearAnswer?: (question: ParsedQuestion) => Promise<QuestionApplyResult>;
  /** Bounded raw repair of an unparseable payload (D69/I999 recovery): the
   * host splices exactly this fence's body under an exact-body gate and
   * converges the label in the same transaction when the repaired payload
   * parses. Absent when no live draft exists. */
  onRepairPayload?: (
    nextBody: string,
  ) => { status: "applied" } | { status: "conflict"; message: string };
}

// ---------------------------------------------------------------------------
// Authoring form state (D69: prompt + options only — never answer/status)
// ---------------------------------------------------------------------------

interface AuthoringOptionRow {
  /** Stable render key across add/remove. */
  key: number;
  /** The original parsed option, carried so tradeoffs/risks and unknown
   * option fields survive a label/description edit verbatim. */
  original: QuestionOption | null;
  label: string;
  description: string;
  recommended: boolean;
}

interface AuthoringState {
  prompt: string;
  options: AuthoringOptionRow[];
  nextKey: number;
}

function authoringStateFromQuestion(question: ParsedQuestion): AuthoringState {
  return {
    prompt: question.prompt,
    options: question.options.map((option, index) => ({
      key: index,
      original: option,
      label: option.label,
      description: option.description,
      recommended: option.recommended === true,
    })),
    nextKey: question.options.length,
  };
}

/** Rebuild the question from the form. The spread carries schema, id,
 * status, severity, multiSelect, allowFreeText, `answer`, and unknown
 * top-level fields through untouched — authoring can never edit them. */
function questionFromAuthoring(
  question: ParsedQuestion,
  authoring: AuthoringState,
): ParsedQuestion {
  return {
    ...question,
    prompt: authoring.prompt,
    options: authoring.options.map((row) => {
      const option: QuestionOption = {
        label: row.label,
        description: row.description,
        extra: row.original?.extra ?? {},
      };
      if (row.original?.tradeoffs !== undefined) {
        option.tradeoffs = row.original.tradeoffs;
      }
      if (row.original?.risks !== undefined) {
        option.risks = row.original.risks;
      }
      if (row.recommended) option.recommended = true;
      return option;
    }),
  };
}

// ---------------------------------------------------------------------------
// Module-scoped pre-answer draft LRU (survives unmount/remount)
// ---------------------------------------------------------------------------

interface QuestionAnswerDraft {
  selected: string[];
  text: string;
  rationale: string;
  freeTextOpen: boolean;
  /** `questionAnswerRevision` of the payload this draft was started against. */
  revision: string;
}

const ANSWER_DRAFT_CAP = 50;
const answerDrafts = new Map<string, QuestionAnswerDraft>();

/**
 * Signature of the live ANSWER state a draft was started against.
 *
 * The draft key is only `docPath#id`, which survives revisions of the payload
 * it points at. Without this stamp, a draft cached while the question was open
 * would be replayed after a collaborator answered — reopening the card into an
 * apparent "Change answer" session that hides the collaborator's answer.
 */
function questionAnswerRevision(question: ParsedQuestion | null): string {
  if (!question) return "\u0000unparsed";
  const { status, answer } = question;
  return JSON.stringify([
    status,
    answer.answered_by,
    answer.answered_at,
    answer.selected,
  ]);
}

/**
 * Signature of the answer SHAPE a draft was started against — `multi_select`,
 * `allow_free_text`, and the option labels.
 *
 * Deliberately SEPARATE from `questionAnswerRevision`: a shape change is
 * reconciled (`loadAnswerDraft` drops dead labels and clamps a multi-select
 * draft to one), not discarded, so the user's own typed words survive a
 * collaborator renaming an option. Its only job is to make the resync effect
 * re-run — without it the effect returns early on the unchanged answer-state
 * token, so a MOUNTED card kept both selections after a live multi→single
 * switch and could still submit a removed label.
 */
function questionShapeSignature(question: ParsedQuestion | null): string {
  if (!question) return "\u0000unparsed";
  return JSON.stringify([
    question.multiSelect,
    question.allowFreeText,
    question.options.map((option) => option.label),
  ]);
}

const emptyAnswerDraft = (revision: string): QuestionAnswerDraft => ({
  selected: [],
  text: "",
  rationale: "",
  freeTextOpen: false,
  revision,
});

/**
 * Read a cached draft, reconciled against the live payload. Returns null when
 * no usable draft exists — the caller then renders the payload's own state.
 */
function loadAnswerDraft(
  storeKey: string,
  question: ParsedQuestion | null,
): QuestionAnswerDraft | null {
  const stored = answerDrafts.get(storeKey);
  if (!stored) return null;
  if (stored.revision !== questionAnswerRevision(question)) {
    // The answer moved underneath this draft; it describes a payload that no
    // longer exists, so it must not resurrect as an edit session.
    answerDrafts.delete(storeKey);
    return null;
  }
  if (!question) return stored;
  // A removed or renamed option must not stay selected: a stale label would
  // keep Answer enabled and could persist a choice absent from the live
  // options. Free text and rationale are the user's own words — they survive.
  const liveLabels = new Set(question.options.map((option) => option.label));
  let selected = stored.selected.filter((label) => liveLabels.has(label));
  // Answer SHAPE can change under the same id too. If A+B were cached while
  // the question was `multi_select: true` and a collaborator flipped it to
  // single-select without touching the labels, every label is still live —
  // so the filter above keeps both, `answerReady` stays true, and the apply
  // would submit two values into a single-select question. Clamp to one.
  if (!question.multiSelect && selected.length > 1) {
    selected = selected.slice(0, 1);
  }
  if (selected.length === stored.selected.length) return stored;
  const reconciled = { ...stored, selected };
  answerDrafts.set(storeKey, reconciled);
  return reconciled;
}

function storeAnswerDraft(storeKey: string, draft: QuestionAnswerDraft): void {
  // Re-inserting moves the key to the newest position so the active card is
  // never the eviction victim (Map preserves insertion order).
  answerDrafts.delete(storeKey);
  answerDrafts.set(storeKey, draft);
  if (answerDrafts.size > ANSWER_DRAFT_CAP) {
    for (const key of answerDrafts.keys()) {
      answerDrafts.delete(key);
      break;
    }
  }
}

function clearAnswerDraft(storeKey: string): void {
  answerDrafts.delete(storeKey);
}

// ---------------------------------------------------------------------------
// Display helpers
// ---------------------------------------------------------------------------

function toEscalationOption(
  option: QuestionOption,
  includeDetails: boolean,
): EscalationOption {
  return {
    label: option.label,
    description: option.description,
    tradeoffs: includeDetails ? (option.tradeoffs ?? null) : null,
    risks: includeDetails ? (option.risks ?? null) : null,
    recommended: option.recommended === true,
  };
}

const TEXTAREA_CLASS = classNames(
  "w-full px-3 py-2 text-sm",
  "rounded-[var(--g5-radius-sm)]",
  "bg-[var(--g5-bg0)] border border-[var(--g5-stroke)]",
  "text-[var(--g5-text0)] placeholder-[var(--g5-text2)]",
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
  "resize-y",
);

const INPUT_CLASS = classNames(
  "w-full min-w-0 px-2 py-1 text-sm",
  "rounded-[var(--g5-radius-sm)]",
  "bg-[var(--g5-bg0)] border border-[var(--g5-stroke)]",
  "text-[var(--g5-text0)] placeholder-[var(--g5-text2)]",
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
);

const PILL_CLASS = (pressed: boolean) =>
  classNames(
    "inline-flex items-center gap-1.5 px-3 py-1.5",
    "text-xs font-medium rounded-[var(--g5-radius-sm)]",
    "border transition-colors duration-150 cursor-pointer",
    pressed
      ? "border-[var(--g5-info)] bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)] text-[var(--g5-text0)]"
      : "border-[var(--g5-stroke)] text-[var(--g5-text1)] hover:bg-[var(--g5-bg2)]",
  );

/** Plain code fallback for invalid/oversized/duplicate payloads — keeps the
 * fence content visible and the rest of the document rendering. */
function QuestionFallbackCode({
  content,
  notice,
  noticeTestId,
  onRecover,
  onRepair,
}: {
  content: string;
  notice: string;
  noticeTestId: string;
  /** Acknowledge and re-read the live parse. Present when the payload became
   * invalid UNDER the user — a remote merge can corrupt the YAML after an
   * apply, and this branch renders before the late-conflict banner, so without
   * it the conflict has no recovery affordance at all (I5/I999). */
  onRecover?: () => void;
  /** Bounded raw repair: edit exactly this fence's payload in place. Without
   * it, an unparseable payload is a dead end — the question dispatch runs
   * before the generic code editor, so no other surface can fix the YAML the
   * human is being asked to repair. */
  onRepair?: (
    nextBody: string,
  ) => { status: "applied" } | { status: "conflict"; message: string };
}) {
  const [repairDraft, setRepairDraft] = useState<string | null>(null);
  const [repairError, setRepairError] = useState<string | null>(null);
  return (
    <div
      data-testid="structured-question-fallback"
      className="relative my-2 overflow-hidden rounded-lg border border-[var(--g5-stroke)]/50 bg-[var(--g5-bg0)]"
    >
      <div className="flex items-center justify-between border-b border-[var(--g5-stroke)] bg-[var(--g5-bg2)] px-3 py-1">
        <span className="font-mono text-[10px] uppercase text-[var(--g5-text2)]">
          g5-question
        </span>
      </div>
      <div
        data-testid={noticeTestId}
        className="flex flex-wrap items-center gap-2 border-b border-[var(--g5-warn)]/40 bg-[color-mix(in_srgb,var(--g5-warn)_8%,transparent)] px-3 py-1.5 text-xs text-[var(--g5-text1)]"
      >
        <span className="min-w-0 flex-1">{notice}</span>
        {onRepair && repairDraft === null && (
          <Button
            kind="secondary"
            data-testid="question-repair-open"
            onClick={() => {
              setRepairError(null);
              setRepairDraft(content);
            }}
          >
            Repair payload
          </Button>
        )}
        {onRecover && (
          <Button
            kind="secondary"
            data-testid="question-invalid-reload"
            onClick={onRecover}
          >
            Reload
          </Button>
        )}
      </div>
      {repairDraft !== null && onRepair && (
        <div className="space-y-2 border-b border-[var(--g5-stroke)] p-3">
          <textarea
            data-testid="question-repair-editor"
            value={repairDraft}
            onChange={(event) => setRepairDraft(event.target.value)}
            rows={Math.min(20, repairDraft.split("\n").length + 2)}
            className="w-full rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 font-mono text-xs text-[var(--g5-text0)]"
          />
          {repairError && (
            <p
              role="alert"
              data-testid="question-repair-error"
              className="text-xs text-[var(--g5-blocked)]"
            >
              {repairError}
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button
              kind="primary"
              data-testid="question-repair-save"
              onClick={() => {
                const result = onRepair(repairDraft);
                if (result.status === "applied") {
                  setRepairDraft(null);
                  setRepairError(null);
                } else {
                  setRepairError(result.message);
                }
              }}
            >
              Save repaired payload
            </Button>
            <Button
              kind="ghost"
              data-testid="question-repair-cancel"
              onClick={() => {
                setRepairDraft(null);
                setRepairError(null);
              }}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
      <pre className="max-h-[600px] overflow-x-auto overflow-y-auto p-3 text-sm font-[var(--g5-font-mono)] text-[var(--g5-text0)]">
        <code>{content}</code>
      </pre>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const StructuredQuestionEditor =
  React.memo<StructuredQuestionEditorProps>(function StructuredQuestionEditor({
    block,
    docPath,
    fallbackId,
    canSave,
    currentUserId,
    currentUserEmail,
    currentUserAvatarUrl,
    duplicateId = false,
    onApplyAnswer,
    onDiscuss,
    onApplyEdit,
    onClearAnswer,
    onRepairPayload,
  }) {
    const parsed = useMemo(
      () => parseQuestionPayload(block.content, { fallbackId }),
      [block.content, fallbackId],
    );
    // Discuss prefills the composer: while the owner's VM is stopped or
    // failed the composer offers Start and send, so Discuss stays open; in
    // the other waiting states it is greyed with the reason
    // (g5.chat.start-and-send, g5.ui.unavailable-actions).
    const discussGate = useWorkspaceLiveGate();
    const discussReason = useLiveGateReason("discuss it in chat");
    const discussBlocked = discussGate?.canStart ? null : discussReason;
    const question = parsed.ok ? parsed.question : null;
    const draftKey = `${docPath}#${question ? question.id : fallbackId}`;

    const revision = questionAnswerRevision(question);
    const shapeSignature = questionShapeSignature(question);

    // Stable, unique per card: several question cards can share one document,
    // so the label/control associations below must not collide.
    const fieldIdBase = useId();
    const promptFieldId = `${fieldIdBase}-prompt`;
    const freeTextFieldId = `${fieldIdBase}-free-text`;
    const rationaleFieldId = `${fieldIdBase}-rationale`;

    const [form, setForm] = useState<QuestionAnswerDraft>(
      () => loadAnswerDraft(draftKey, question) ?? emptyAnswerDraft(revision),
    );
    // For an answered question a surviving draft means the user was mid
    // "Change answer" before a remount; reopen the form. A draft the payload
    // has moved past is dropped by `loadAnswerDraft`, so a collaborator's
    // answer is never hidden behind a stale edit session.
    const [editing, setEditing] = useState<boolean>(
      () => loadAnswerDraft(draftKey, question) !== null,
    );

    const [expandedDetails, setExpandedDetails] = useState<ReadonlySet<string>>(
      () => new Set<string>(),
    );
    const [applyState, setApplyState] = useState<{
      phase: "idle" | "pending" | "conflict";
      message?: string;
    }>({ phase: "idle" });

    // `form`/`editing` initialize on FIRST mount only, but this instance
    // outlives the question it renders: DocumentPreview keys the row by block
    // index, not question id, so a payload `id` change (or a reparse that
    // changes `fallbackId`) swaps `draftKey` underneath a mounted card.
    // Without this resync the previous question's half-typed answer would be
    // rewritten under the NEW key by `persistDraft`.
    // Declared AFTER every setter it calls: an effect placed above them
    // reads the bindings before initialization, which the hooks lint rejects.
    const syncTokenRef = useRef(
      `${draftKey}\u0000${revision}\u0000${shapeSignature}`,
    );
    useEffect(() => {
      const token = `${draftKey}\u0000${revision}\u0000${shapeSignature}`;
      if (syncTokenRef.current === token) return;
      syncTokenRef.current = token;
      const draft = loadAnswerDraft(draftKey, question);
      setForm(draft ?? emptyAnswerDraft(revision));
      // A surviving draft means the user WAS mid-edit (drafts are only
      // persisted when the form is touched), so reopening is right here too.
      setEditing(draft !== null);
      setApplyState({ phase: "idle" });
    }, [draftKey, revision, shapeSignature, question]);
    // Post-apply re-parse watch (D69, 15 s): the host's applied result hands
    // back a watch registration; the card owns the cancel across unmount and
    // newer applies, and renders the late-conflict banner when it fires.
    const [lateConflict, setLateConflict] = useState(false);
    // D69 authoring: null while not editing the question itself. The form is
    // local-only until Save; Cancel discards it and the card re-renders the
    // parsed state.
    const [authoring, setAuthoring] = useState<AuthoringState | null>(null);
    const [editState, setEditState] = useState<{
      phase: "idle" | "pending" | "conflict";
      message?: string;
    }>({ phase: "idle" });
    const watchCancelRef = useRef<(() => void) | null>(null);
    useEffect(
      () => () => {
        watchCancelRef.current?.();
        watchCancelRef.current = null;
      },
      [],
    );

    /** Persist a draft stamped with the payload revision it was made against. */
    const persistDraft = useCallback(
      (draft: QuestionAnswerDraft) => {
        storeAnswerDraft(draftKey, { ...draft, revision });
      },
      [draftKey, revision],
    );

    const updateForm = useCallback(
      (partial: Partial<QuestionAnswerDraft>) => {
        setForm((previous) => {
          const next = { ...previous, ...partial };
          persistDraft(next);
          return next;
        });
      },
      [persistDraft],
    );

    const multiSelect = question?.multiSelect === true;

    const handleOptionToggle = useCallback(
      (label: string) => {
        if (multiSelect) {
          setForm((previous) => {
            const selected = previous.selected.includes(label)
              ? previous.selected.filter((entry) => entry !== label)
              : [...previous.selected, label];
            const next = {
              ...previous,
              selected,
              // Choosing an offered option means the user is no longer
              // answering with their own words (D69). The typed text stays in
              // the draft so re-opening the pill restores it; it just stops
              // being part of the answer.
              freeTextOpen: selected.length > 0 ? false : previous.freeTextOpen,
            };
            persistDraft(next);
            return next;
          });
          return;
        }
        setForm((previous) => {
          const wasSelected =
            previous.selected.length === 1 && previous.selected[0] === label;
          const selected = wasSelected ? [] : [label];
          // "Something else" is an ALTERNATIVE to the options, not a
          // supplement (D69): picking one closes it, so the recorded answer
          // never carries both an option the user moved away from and the
          // words they replaced it with.
          const next = {
            ...previous,
            selected,
            freeTextOpen: selected.length > 0 ? false : previous.freeTextOpen,
          };
          persistDraft(next);
          return next;
        });
      },
      [persistDraft, multiSelect],
    );

    const handleSomethingElse = useCallback(() => {
      setForm((previous) => {
        const opening = !previous.freeTextOpen;
        // Answering in your own words REPLACES the offered options (D69).
        // `rationale` is the field for "I chose X because Y", so leaving a
        // selection behind here would record a decision the user moved away
        // from — visible in the answered summary and acted on by any agent
        // reading `selected`.
        const next = {
          ...previous,
          freeTextOpen: opening,
          selected: opening ? [] : previous.selected,
        };
        persistDraft(next);
        return next;
      });
    }, [persistDraft]);

    if (duplicateId) {
      return (
        <QuestionFallbackCode
          content={block.content}
          notice={
            "Invalid question payload — duplicate question id in this " +
            "document (the first question with this id wins)."
          }
          noticeTestId="question-invalid-notice"
        />
      );
    }
    if (!parsed.ok || question == null) {
      if (!parsed.ok && parsed.error === "too-large") {
        return (
          <QuestionFallbackCode
            content={block.content}
            notice="Question payload exceeds the 32 KiB cap — rendering read-only."
            noticeTestId="question-oversized-notice"
          />
        );
      }
      const detail = !parsed.ok && parsed.detail ? ` (${parsed.detail})` : "";
      return (
        <QuestionFallbackCode
          content={block.content}
          notice={
            lateConflict
              ? `This question changed after your answer was applied and its payload no longer parses${detail} — the answer you saved is not readable here.`
              : `Invalid question payload${detail} — rendering as plain code.`
          }
          noticeTestId="question-invalid-notice"
          // Only when the corruption landed under this card. A document that
          // was already invalid on arrival has nothing to recover TO.
          onRecover={lateConflict ? () => setLateConflict(false) : undefined}
          // Repair is offered whenever a live draft exists: it is the bounded
          // raw-edit path D69/I999's recovery requires, and the question
          // dispatch runs before the generic code editor, so nothing else can
          // fix this YAML.
          onRepair={canSave ? onRepairPayload : undefined}
        />
      );
    }

    const hasOptions = question.options.length > 0;
    // `allow_free_text` defaults to true only when omitted, so an author can
    // write `options: []` with `allow_free_text: false` — a question with no
    // way at all to answer it. `answerReady` could never become true, leaving a
    // permanently disabled Answer button with no explanation. Say so instead.
    const unanswerable = !hasOptions && !question.allowFreeText;
    // Labels are the answer identity, so a payload whose options collide
    // cannot record which option was chosen. Read-only until an author fixes
    // the labels — Answer and Change answer are both withheld, but the
    // authoring form stays available because editing is the fix.
    const duplicateLabels = hasDuplicateOptionLabels(question);
    const showFreeText =
      question.allowFreeText && (!hasOptions || form.freeTextOpen);
    // Selections must exist in the LIVE options. Authoring can rename or remove
    // an option while an answer is settled, leaving `answer.selected` pointing
    // at a label that no longer renders — Change answer would then copy the
    // dead label and this gate would call it sufficient to Save, recording a
    // choice no option offers and none shown as selected.
    // Plain construction, not `useMemo`: this sits AFTER the invalid/oversized
    // early returns, so a hook here would be called conditionally. Building a
    // small Set per render costs nothing.
    const liveOptionLabels = new Set(
      question.options.map((option) => option.label),
    );
    const liveSelected = form.selected.filter((label) =>
      liveOptionLabels.has(label),
    );
    const answerReady =
      !duplicateLabels &&
      (liveSelected.length > 0 ||
        (showFreeText && form.text.trim().length > 0));
    const showAnswerAction = canSave && onApplyAnswer != null;
    // Attribution shown on the answered summary. Prefer the most
    // human-readable identity the payload carries; a legacy scalar payload
    // normalizes to kind-only, so this falls back to the role.
    const answeredBy = question.answer.answered_by;
    const answeredByLabel =
      answeredBy?.email ?? answeredBy?.id ?? answeredBy?.kind ?? "";
    const answeredByAvatarUrl = answeredBy?.avatar_url ?? null;
    const answered = question.status === "answered";
    const showForm = !answered || editing;

    const handleAnswer = async () => {
      if (!onApplyAnswer) return;
      const input: AnswerInput = {
        selected: [...liveSelected],
        // Only VISIBLE free text submits; text drafted behind a closed
        // "Something else" pill stays in the LRU and submits as "".
        text: showFreeText ? form.text.trim() : "",
        // The input is gone, but an answer that already carries a rationale
        // keeps it: re-answering must not silently delete recorded reasoning.
        rationale: question.answer.rationale,
        answeredBy: {
          kind: "user",
          ...(currentUserId ? { id: currentUserId } : {}),
          ...(currentUserEmail ? { email: currentUserEmail } : {}),
          ...(currentUserAvatarUrl ? { avatar_url: currentUserAvatarUrl } : {}),
          extra: {},
        },
        answeredAt: new Date().toISOString(),
      };
      // A newer apply supersedes the previous watch and its banner.
      watchCancelRef.current?.();
      watchCancelRef.current = null;
      setLateConflict(false);
      setApplyState({ phase: "pending" });
      try {
        const result = await onApplyAnswer(question, input);
        if (result.status === "applied") {
          clearAnswerDraft(draftKey);
          setEditing(false);
          setApplyState({ phase: "idle" });
          watchCancelRef.current =
            result.watch?.(() => setLateConflict(true)) ?? null;
        } else {
          setApplyState({ phase: "conflict", message: result.message });
        }
      } catch (error) {
        setApplyState({
          phase: "conflict",
          message: error instanceof Error ? error.message : String(error),
        });
      }
    };

    const handleChangeAnswer = () => {
      // "Something else" is an ALTERNATIVE to the options, not a supplement.
      // Answers written under the PREVIOUS contract can legitimately carry
      // both, and copying the pair through would re-submit it on the next
      // Save — silently preserving the exact contradictory decision (an option
      // recorded alongside the answer the user actually moved to) that the
      // mutual exclusion exists to eliminate. Free text present means the user
      // answered in their own words, so that branch wins and the stale
      // selection is dropped; rationale is their words too and survives either
      // way.
      const hasOwnWords = question.answer.text !== "";
      const next: QuestionAnswerDraft = {
        // Dead labels are dropped here too, so reopening a decision whose
        // option was renamed or removed starts from what actually exists.
        selected: hasOwnWords
          ? []
          : question.answer.selected.filter((label) =>
              liveOptionLabels.has(label),
            ),
        text: question.answer.text,
        rationale: question.answer.rationale,
        freeTextOpen: hasOwnWords,
        revision,
      };
      persistDraft(next);
      setForm(next);
      setApplyState({ phase: "idle" });
      setEditing(true);
    };

    /** Return the question to unanswered (D10). Same write path and gate as
     * answering — the answer state is what moves, so it conflicts on the same
     * terms. Any half-typed draft goes too: it described an answer session for
     * a question that no longer has an answer. */
    const handleClearAnswer = async () => {
      if (!onClearAnswer) return;
      watchCancelRef.current?.();
      watchCancelRef.current = null;
      setLateConflict(false);
      setApplyState({ phase: "pending" });
      try {
        const result = await onClearAnswer(question);
        if (result.status === "applied") {
          clearAnswerDraft(draftKey);
          setEditing(false);
          setForm(emptyAnswerDraft(revision));
          setApplyState({ phase: "idle" });
          watchCancelRef.current =
            result.watch?.(() => setLateConflict(true)) ?? null;
        } else {
          setApplyState({ phase: "conflict", message: result.message });
        }
      } catch (error) {
        setApplyState({
          phase: "conflict",
          message: error instanceof Error ? error.message : String(error),
        });
      }
    };

    const handleCancelChange = () => {
      clearAnswerDraft(draftKey);
      setApplyState({ phase: "idle" });
      setEditing(false);
    };

    // D69 authoring (minimal structured fields; never answer/status).
    const showEditQuestion = canSave && onApplyEdit != null;

    const handleOpenEdit = () => {
      setEditState({ phase: "idle" });
      setAuthoring(authoringStateFromQuestion(question));
    };

    const handleCancelEdit = () => {
      setAuthoring(null);
      setEditState({ phase: "idle" });
    };

    const updateAuthoring = (
      updater: (previous: AuthoringState) => AuthoringState,
    ) => {
      setAuthoring((previous) => (previous ? updater(previous) : previous));
    };

    const handleEditSave = async () => {
      if (!onApplyEdit || !authoring) return;
      const edited = questionFromAuthoring(question, authoring);
      // A newer write supersedes the previous watch and its banner.
      watchCancelRef.current?.();
      watchCancelRef.current = null;
      setLateConflict(false);
      setEditState({ phase: "pending" });
      try {
        const result = await onApplyEdit(edited);
        if (result.status === "applied") {
          setAuthoring(null);
          setEditState({ phase: "idle" });
          watchCancelRef.current =
            result.watch?.(() => setLateConflict(true)) ?? null;
        } else {
          setEditState({ phase: "conflict", message: result.message });
        }
      } catch (error) {
        setEditState({
          phase: "conflict",
          message: error instanceof Error ? error.message : String(error),
        });
      }
    };

    // Labels identify an option everywhere downstream — `form.selected` stores
    // labels, `expandedDetails` is a Set of labels, and the answered summary
    // renders `key={label}`. Saving a collision would make selecting one option
    // select both, expand both details, hand React duplicate keys, and persist
    // an ambiguous answer. Empty labels are equally unusable.
    const authoringLabels =
      authoring?.options.map((row) => row.label.trim()) ?? [];
    const duplicateAuthoringLabel = authoringLabels.some(
      (label, index) =>
        label !== "" && authoringLabels.indexOf(label) !== index,
    );
    // A settled USER answer freezes the options it names (D10): removing or
    // renaming a selected option would leave an authoritative decision
    // pointing at a choice that no longer exists — the summary shows a label
    // no option offers, and a consuming agent acting on `selected` applies a
    // decision the person never made in the reshaped question. Clear answer
    // first is the path to reshaping those options. Options the answer does
    // not name stay freely editable.
    const settledUserAnswer =
      question.status === "answered" &&
      question.answer.answered_by?.kind === "user";
    const orphanedSelectedLabels = settledUserAnswer
      ? question.answer.selected.filter(
          (label) => !authoringLabels.includes(label),
        )
      : [];
    const removesSelectedOption =
      authoring != null && orphanedSelectedLabels.length > 0;
    const editSaveBlocked =
      authoring != null &&
      (authoringLabels.some((label) => label.length === 0) ||
        duplicateAuthoringLabel ||
        removesSelectedOption);

    return (
      <div
        data-testid="structured-question-editor"
        className="my-2 space-y-3 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-3"
      >
        {/* Header: id, and the authoring affordance parked top-right */}
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-xs text-[var(--g5-text2)]">
            {question.id}
          </span>
          {showEditQuestion && !authoring && (
            <button
              type="button"
              data-testid="question-edit-button"
              onClick={handleOpenEdit}
              aria-label="Edit question"
              title="Edit the question itself — its prompt and options. Your answer is never touched."
              className={classNames(
                "ml-auto shrink-0 rounded p-1 text-[var(--g5-text2)]",
                "hover:bg-[var(--g5-overlay-hover)] hover:text-[var(--g5-text0)]",
              )}
            >
              <PencilLine size={13} />
            </button>
          )}
          {/* Status and severity are reported ONCE, as block tags on the
              enclosing [Q<N>.] record (D69). Repeating them inside the fence
              said the same thing twice in two visual languages. */}
        </div>

        {/* Prompt (the authoring form edits it in place instead) */}
        {question.prompt && !authoring && (
          <p className="text-sm text-[var(--g5-text0)]">{question.prompt}</p>
        )}

        {/* Post-apply late-conflict banner (D69 re-parse watch) — rendered in
            both form and summary views; Reload acknowledges and shows the
            live parse the card already renders. */}
        {lateConflict && (
          <div
            data-testid="question-late-conflict"
            className="flex flex-wrap items-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-blocked)]/50 bg-[color-mix(in_srgb,var(--g5-blocked)_8%,transparent)] px-3 py-2 text-xs text-[var(--g5-text1)]"
          >
            <span className="min-w-0 flex-1">
              This question changed after your answer was applied — the current
              state may differ from what you saved.
            </span>
            <Button
              kind="secondary"
              data-testid="question-reload"
              onClick={() => setLateConflict(false)}
            >
              Reload
            </Button>
          </div>
        )}

        {authoring ? (
          /* D69 authoring form: prompt + options only. It never renders or
             edits answer fields, and `status` stays untouched on save. */
          <div data-testid="question-edit-form" className="space-y-3">
            <div>
              <label
                htmlFor={promptFieldId}
                className="mb-1 block text-xs text-[var(--g5-text2)]"
              >
                Prompt
              </label>
              <textarea
                id={promptFieldId}
                data-testid="question-edit-prompt"
                value={authoring.prompt}
                onChange={(event) =>
                  updateAuthoring((previous) => ({
                    ...previous,
                    prompt: event.target.value,
                  }))
                }
                rows={2}
                className={TEXTAREA_CLASS}
              />
            </div>
            <div className="space-y-2">
              {authoring.options.map((row) => (
                <div
                  key={row.key}
                  data-testid="question-edit-option"
                  className="space-y-1.5 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2"
                >
                  <div className="flex items-center gap-2">
                    <input
                      data-testid="question-edit-option-label"
                      aria-label="Option label"
                      placeholder="Label"
                      value={row.label}
                      onChange={(event) =>
                        updateAuthoring((previous) => ({
                          ...previous,
                          options: previous.options.map((entry) =>
                            entry.key === row.key
                              ? { ...entry, label: event.target.value }
                              : entry,
                          ),
                        }))
                      }
                      className={INPUT_CLASS}
                    />
                    <label className="flex shrink-0 cursor-pointer items-center gap-1 text-xs text-[var(--g5-text2)]">
                      <input
                        type="radio"
                        data-testid="question-edit-option-recommended"
                        name={`question-recommended-${draftKey}`}
                        checked={row.recommended}
                        readOnly
                        onClick={() =>
                          // Radio-like with a toggle-off: at most one option
                          // ever carries the recommended flag.
                          updateAuthoring((previous) => ({
                            ...previous,
                            options: previous.options.map((entry) => ({
                              ...entry,
                              recommended:
                                entry.key === row.key
                                  ? !entry.recommended
                                  : false,
                            })),
                          }))
                        }
                      />
                      Recommended
                    </label>
                    <button
                      type="button"
                      data-testid="question-edit-option-remove"
                      aria-label="Remove option"
                      onClick={() =>
                        updateAuthoring((previous) => ({
                          ...previous,
                          options: previous.options.filter(
                            (entry) => entry.key !== row.key,
                          ),
                        }))
                      }
                      className="shrink-0 cursor-pointer rounded p-1 text-[var(--g5-text2)] hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text0)]"
                    >
                      <X size={14} />
                    </button>
                  </div>
                  <input
                    data-testid="question-edit-option-description"
                    aria-label="Option description"
                    placeholder="Description"
                    value={row.description}
                    onChange={(event) =>
                      updateAuthoring((previous) => ({
                        ...previous,
                        options: previous.options.map((entry) =>
                          entry.key === row.key
                            ? { ...entry, description: event.target.value }
                            : entry,
                        ),
                      }))
                    }
                    className={INPUT_CLASS}
                  />
                </div>
              ))}
              <button
                type="button"
                data-testid="question-edit-add-option"
                onClick={() =>
                  updateAuthoring((previous) => ({
                    ...previous,
                    options: [
                      ...previous.options,
                      {
                        key: previous.nextKey,
                        original: null,
                        label: "",
                        description: "",
                        recommended: false,
                      },
                    ],
                    nextKey: previous.nextKey + 1,
                  }))
                }
                className={PILL_CLASS(false)}
              >
                <Plus size={12} className="text-[var(--g5-text2)]" />
                Add option
              </button>
            </div>
            {editState.phase === "conflict" && (
              <p
                data-testid="question-edit-error"
                className="text-xs text-[var(--g5-blocked)]"
              >
                {editState.message
                  ? `Could not save the question: ${editState.message}`
                  : "Could not save the question — the document changed underneath it."}
              </p>
            )}
            {duplicateAuthoringLabel && (
              <p
                data-testid="question-edit-duplicate-label"
                role="alert"
                className="text-xs text-[var(--g5-blocked)]"
              >
                Two options share the same label — labels identify an option in
                the saved answer, so each must be unique.
              </p>
            )}
            {removesSelectedOption && (
              <p
                data-testid="question-edit-selected-removed"
                role="alert"
                className="text-xs text-[var(--g5-blocked)]"
              >
                {`The recorded answer selects ${orphanedSelectedLabels
                  .map((label) => `“${label}”`)
                  .join(", ")} — removing or renaming it would rewrite what was
                decided. Clear the answer first to reshape these options.`}
              </p>
            )}
            <div className="flex flex-wrap items-center gap-2">
              <Button
                kind="primary"
                data-testid="question-edit-save"
                disabled={editState.phase === "pending" || editSaveBlocked}
                onClick={() => void handleEditSave()}
              >
                {editState.phase === "pending" ? "Saving…" : "Save question"}
              </Button>
              <Button
                kind="ghost"
                data-testid="question-edit-cancel"
                onClick={handleCancelEdit}
              >
                Cancel
              </Button>
            </div>
          </div>
        ) : showForm ? (
          <>
            {/* Options in the HIL visual language */}
            {hasOptions && (
              <div className="space-y-2">
                {question.options.map((option, index) => {
                  const selected = form.selected.includes(option.label);
                  const hasDetails =
                    option.tradeoffs != null || option.risks != null;
                  const detailsOpen = expandedDetails.has(option.label);
                  return (
                    <div key={`${option.label}-${index}`} className="space-y-1">
                      <button
                        type="button"
                        data-testid="question-option-toggle"
                        aria-pressed={selected}
                        onClick={() => handleOptionToggle(option.label)}
                        className={classNames(
                          "block w-full cursor-pointer rounded-[var(--g5-radius-sm)] text-left",
                          "transition-shadow duration-150",
                          selected && "ring-2 ring-[var(--g5-info)]",
                        )}
                      >
                        <OptionCard
                          option={toEscalationOption(option, detailsOpen)}
                          index={index}
                        />
                      </button>
                      {hasDetails && (
                        <button
                          type="button"
                          data-testid="question-option-details-toggle"
                          aria-expanded={detailsOpen}
                          onClick={() =>
                            setExpandedDetails((previous) => {
                              const next = new Set(previous);
                              if (next.has(option.label)) {
                                next.delete(option.label);
                              } else {
                                next.add(option.label);
                              }
                              return next;
                            })
                          }
                          className="flex cursor-pointer items-center gap-1 text-xs text-[var(--g5-text2)] hover:text-[var(--g5-text1)]"
                        >
                          {detailsOpen ? (
                            <ChevronDown size={12} />
                          ) : (
                            <ChevronRight size={12} />
                          )}
                          Tradeoffs &amp; risks
                        </button>
                      )}
                    </div>
                  );
                })}
                {question.allowFreeText && (
                  <button
                    type="button"
                    data-testid="question-something-else"
                    aria-pressed={form.freeTextOpen}
                    onClick={handleSomethingElse}
                    className={PILL_CLASS(form.freeTextOpen)}
                    title="Type a custom answer"
                  >
                    <PencilLine size={12} className="text-[var(--g5-text2)]" />
                    Something else
                  </button>
                )}
              </div>
            )}

            {/* Own-answer text; the rationale below is deliberately OUTSIDE
                this gate. Rationale explains ANY answer — including a chosen
                option — and hiding it behind the "Something else" pill made
                "I chose X because Y" reachable only by abandoning the option
                (D69). */}
            {showFreeText && (
              <div className="space-y-2">
                <div>
                  <label
                    htmlFor={freeTextFieldId}
                    className="mb-1 block text-xs text-[var(--g5-text2)]"
                  >
                    Your answer
                  </label>
                  <textarea
                    id={freeTextFieldId}
                    data-testid="question-free-text"
                    value={form.text}
                    onChange={(event) =>
                      updateForm({ text: event.target.value })
                    }
                    placeholder="Type your answer…"
                    rows={3}
                    className={TEXTAREA_CLASS}
                  />
                </div>
              </div>
            )}

            {duplicateLabels && (
              <p
                data-testid="question-duplicate-label-notice"
                role="alert"
                className="text-xs text-[var(--g5-warn)]"
              >
                Two options share the same label, so an answer could not record
                which one was chosen — edit the question to give each option a
                unique label before answering.
              </p>
            )}

            {unanswerable && (
              <p
                data-testid="question-unanswerable-notice"
                role="alert"
                className="text-xs text-[var(--g5-warn)]"
              >
                This question offers no options and disables free text, so it
                cannot be answered — it needs options or `allow_free_text:
                true`.
              </p>
            )}

            {applyState.phase === "conflict" && (
              <p
                data-testid="question-apply-conflict"
                className="text-xs text-[var(--g5-blocked)]"
              >
                {applyState.message
                  ? `Could not apply the answer: ${applyState.message}`
                  : "Could not apply the answer — the document changed underneath this question."}
              </p>
            )}

            {(showAnswerAction ||
              onDiscuss ||
              showEditQuestion ||
              (answered && editing)) && (
              <div className="flex flex-wrap items-center gap-2">
                {showAnswerAction && (
                  <Button
                    kind="primary"
                    data-testid="question-answer-button"
                    disabled={!answerReady || applyState.phase === "pending"}
                    onClick={() => void handleAnswer()}
                  >
                    {applyState.phase === "pending" ? "Saving…" : "Save answer"}
                  </Button>
                )}
                {answered && editing && (
                  <Button
                    kind="ghost"
                    data-testid="question-cancel-change"
                    onClick={handleCancelChange}
                  >
                    Cancel
                  </Button>
                )}
                {onDiscuss && (
                  <GatedControl
                    verb="discuss it in chat"
                    reason={discussBlocked}
                    testId="question-discuss-hint"
                  >
                    <Button
                      kind="secondary"
                      data-testid="question-discuss-button"
                      onClick={() => onDiscuss(question)}
                      disabled={discussBlocked !== null}
                      title={discussBlocked ?? DISCUSS_TITLE}
                    >
                      Discuss with agent
                    </Button>
                  </GatedControl>
                )}
              </div>
            )}
          </>
        ) : (
          <>
            {/* Answered summary */}
            <div
              data-testid="question-answered-summary"
              className="space-y-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-3 py-2"
            >
              {question.answer.selected.length > 0 && (
                <div className="flex flex-wrap items-center gap-1.5">
                  {question.answer.selected.map((label) => (
                    <span
                      key={label}
                      className="inline-flex items-center rounded-[var(--g5-radius-sm)] border border-[var(--g5-info)] bg-[color-mix(in_srgb,var(--g5-info)_10%,transparent)] px-2 py-0.5 text-xs font-medium text-[var(--g5-text0)]"
                    >
                      {label}
                    </span>
                  ))}
                </div>
              )}
              {question.answer.text && (
                <p className="text-sm text-[var(--g5-text0)]">
                  {question.answer.text}
                </p>
              )}
              {question.answer.rationale && (
                <p className="text-xs text-[var(--g5-text1)]">
                  <span className="font-medium">Rationale:</span>{" "}
                  {question.answer.rationale}
                </p>
              )}
              {/* Attribution row (D69): the persisted `answered_by` identity —
                  avatar plus the most human-readable label available. A legacy
                  payload carries only `kind`, so this degrades to the role. */}
              <div
                data-testid="question-answered-by"
                className="flex items-center gap-1.5 text-[10px] text-[var(--g5-text2)]"
              >
                {answeredByLabel ? (
                  <IdentityAvatar
                    identity={answeredByLabel}
                    avatarUrl={answeredByAvatarUrl}
                    size={14}
                  />
                ) : null}
                <span>
                  Answered
                  {answeredByLabel ? ` by ${answeredByLabel}` : ""}
                  {question.answer.answered_at
                    ? ` at ${question.answer.answered_at}`
                    : ""}
                </span>
              </div>
            </div>
            {/* Clear answer writes from the ANSWERED card, so its failure has
                to surface here — the form's conflict banner is on the other
                branch and would never be mounted to show it. */}
            {applyState.phase === "conflict" && (
              <p
                role="alert"
                data-testid="question-apply-conflict"
                className="text-xs text-[var(--g5-blocked)]"
              >
                {applyState.message
                  ? `Could not apply — ${applyState.message}.`
                  : "Could not apply — the document changed underneath this question."}
              </p>
            )}
            <div className="flex flex-wrap items-center gap-2">
              {showAnswerAction && (
                <Button
                  kind="secondary"
                  data-testid="question-change-answer"
                  disabled={duplicateLabels}
                  onClick={handleChangeAnswer}
                >
                  Change answer
                </Button>
              )}
              {canSave && onClearAnswer && (
                <Button
                  kind="secondary"
                  data-testid="question-clear-answer"
                  disabled={applyState.phase === "pending"}
                  title="Remove the recorded answer and reopen this question"
                  onClick={() => void handleClearAnswer()}
                >
                  Clear answer
                </Button>
              )}
              {onDiscuss && (
                <GatedControl
                  verb="discuss it in chat"
                  reason={discussBlocked}
                  testId="question-discuss-hint"
                >
                  <Button
                    kind="secondary"
                    data-testid="question-discuss-button"
                    onClick={() => onDiscuss(question)}
                    disabled={discussBlocked !== null}
                    title={discussBlocked ?? DISCUSS_TITLE}
                  >
                    Discuss with agent
                  </Button>
                </GatedControl>
              )}
            </div>
          </>
        )}
      </div>
    );
  });
