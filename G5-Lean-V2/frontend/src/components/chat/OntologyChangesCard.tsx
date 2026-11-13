import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronRight, ListFilter, Pencil } from "lucide-react";
import { api, ApiError } from "../../lib/api";
import { randomUuid } from "../../lib/randomUuid";
import {
  cardCounts,
  cardFiles,
  cardGoverningFiles,
  cardStatement,
  type OntologyCard,
  type OntologyCardEdit,
  type OntologyCardEntry,
  type OntologyCardFile,
  type OntologyCardGoverning,
} from "../../lib/ontologyCard";
import {
  GatedControl,
  liveGateReason,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import {
  useClearComposer,
  useComposerField,
  useComposerRef,
} from "../../hooks/useComposerStore";
import { ComposerOwnerIfNone } from "../comments/ComposerOwner";
import { OntologyChangeRow } from "../editor/OntologyChangesView";
import {
  OntologyStatementMarkdown,
  type StatementFileLinks,
} from "../editor/OntologyStatementMarkdown";
import {
  openStatementLinks,
  statementMetadataContext,
} from "../editor/statementLinks";
import { SemanticRecordFrame } from "../editor/SemanticRecordFrame";

/** Statements a card shows before Show more (chat g5.chat.ontology-card). */
export const CARD_STATEMENTS_SHOWN = 5;

/** The card's edit box fits its text from 2 to 6 lines, then scrolls. */
const EDIT_BOX_MIN_LINES = 2;
const EDIT_BOX_MAX_LINES = 6;

const TURN_RUNNING_REASON =
  "Wait for the reply in progress to finish before editing the card.";

/**
 * The changes card that ends a workspace chat reply when the ontology's
 * statements changed between the turn's start and end
 * (chat g5.chat.ontology-card): every changed statement, whoever made it,
 * with its Before text, the unchanged statements the agent named as
 * governing, and the owner's in-place edits on the newest card.
 */
export function OntologyChangesCard({
  card,
  sessionId,
  messageId,
  newest,
  turnRunning,
  onOpenAllChanges,
  onOpenFile,
}: {
  card: OntologyCard;
  sessionId: string;
  messageId: string;
  /** Only the newest card is expanded and editable. */
  newest: boolean;
  /** A turn of this chat is queued, running or waiting. */
  turnRunning: boolean;
  onOpenAllChanges?: () => void;
  /** Opens a file a statement links to, at a line when the link names one. */
  onOpenFile?: StatementFileLinks["onOpenFile"];
}) {
  const files = useMemo(() => cardFiles(card), [card]);
  const counts = cardCounts(files);
  // A card collapses once a later card becomes the newest.
  const [open, setOpen] = useState(newest);
  const [wasNewest, setWasNewest] = useState(newest);
  if (wasNewest !== newest) {
    setWasNewest(newest);
    setOpen(newest);
  }
  const [showAll, setShowAll] = useState(false);
  const gate = useWorkspaceLiveGate();
  const editReason = !gate
    ? "Editing is unavailable here."
    : (liveGateReason(gate.kind, "edit the card", {
        workspaceNoun: gate.workspaceNoun,
      }) ?? (turnRunning ? TURN_RUNNING_REASON : null));
  const total = files.reduce((sum, file) => sum + file.entries.length, 0);
  let budget = showAll ? Number.POSITIVE_INFINITY : CARD_STATEMENTS_SHOWN;
  const governing = useMemo(() => cardGoverningFiles(card), [card]);
  const governingCount = governing.reduce(
    (sum, file) => sum + file.entries.length,
    0,
  );
  // Statement edits share the page's composer owner, or one for this card.
  return (
    <ComposerOwnerIfNone>
      <details
        data-testid="ontology-changes-card"
        data-newest={newest ? "true" : "false"}
        open={open}
        onToggle={(event) => setOpen(event.currentTarget.open)}
        className="w-full rounded-lg border border-[var(--g5-info)]/40 bg-[var(--g5-bg1)] text-[13px]"
      >
        <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-1.5 font-semibold">
          <ListFilter size={14} className="text-[var(--g5-info)]" />
          <span>Ontology changes in this reply</span>
          {!card.tooLarge && (
            <span
              className="flex gap-1 text-[11px] font-normal"
              aria-label={`${counts.Added} added, ${counts.Modified} modified, ${counts.Deleted} deleted`}
            >
              <span className="rounded bg-[var(--g5-ok)]/10 px-1 text-[var(--g5-ok)]">
                +{counts.Added}
              </span>
              <span className="rounded bg-[var(--g5-warn)]/10 px-1 text-[var(--g5-warn)]">
                ~{counts.Modified}
              </span>
              <span className="rounded bg-[var(--g5-blocked)]/10 px-1 text-[var(--g5-blocked)]">
                −{counts.Deleted}
              </span>
            </span>
          )}
        </summary>
        <div className="space-y-2 px-3 pb-2">
          {card.tooLarge ? (
            <p role="status" className="text-[var(--g5-text2)]">
              This turn changed too many statements to show here.
            </p>
          ) : (
            files.map(({ key, file, title, entries }) => {
              const visible = entries.slice(0, Math.max(0, budget));
              budget -= visible.length;
              if (!visible.length && !file.refusal) return null;
              return (
                <section
                  key={key}
                  data-testid="ontology-card-file"
                  data-repo-id={file.repoId}
                  data-path={file.path}
                  className="space-y-1"
                >
                  <CardFileHeading title={title} path={file.path} />
                  {file.refusal ? (
                    <p role="alert" className="text-[var(--g5-blocked)]">
                      {file.refusal}
                    </p>
                  ) : (
                    <div className="ontology-statement-list">
                      {visible.map((entry) =>
                        entry.current || entry.before ? (
                          <CardStatement
                            key={entry.key}
                            entry={entry}
                            file={file}
                            sessionId={sessionId}
                            messageId={messageId}
                            editable={newest && entry.status !== "Deleted"}
                            editReason={editReason}
                            viewerOwns={gate?.owner === true}
                            links={fileLinks(file, onOpenFile)}
                          />
                        ) : (
                          <UnshownStatement key={entry.key} entry={entry} />
                        ),
                      )}
                    </div>
                  )}
                </section>
              );
            })
          )}
          {!showAll && total > CARD_STATEMENTS_SHOWN && !card.tooLarge && (
            <button
              type="button"
              data-testid="ontology-card-show-more"
              onClick={() => setShowAll(true)}
              className="rounded border border-[var(--g5-stroke)] px-2 py-0.5 text-xs hover:bg-[var(--g5-bg2)]"
            >
              Show more ({total - CARD_STATEMENTS_SHOWN})
            </button>
          )}
          {governingCount > 0 && (
            <details
              data-testid="ontology-card-governing"
              className="group/governing border-t border-[var(--g5-stroke)] pt-2"
            >
              {/* A section over its file headings, at their size and edge;
                  the card's padding leaves no gutter, so the chevron trails. */}
              <summary className="flex cursor-pointer list-none items-center gap-1 font-semibold [&::-webkit-details-marker]:hidden">
                <span>Unchanged governing statements</span>
                <span
                  data-testid="ontology-card-governing-count"
                  className="ml-1 rounded bg-[var(--g5-bg2)] px-1 text-[11px] font-normal text-[var(--g5-text2)]"
                >
                  {governingCount}
                </span>
                <ChevronRight
                  size={14}
                  aria-hidden="true"
                  className="shrink-0 text-[var(--g5-text2)] transition-transform group-open/governing:rotate-90"
                />
              </summary>
              <div className="mt-1.5 space-y-2">
                {governing.map((file) => (
                  <section
                    key={file.key}
                    data-testid="ontology-card-governing-file"
                    data-path={file.path}
                    className="space-y-1"
                  >
                    <CardFileHeading title={file.title} path={file.path} />
                    {/* Laid out like the changes, so the text lines up. */}
                    <div className="ontology-statement-list space-y-2">
                      {file.entries.map((entry) => (
                        <GoverningStatement
                          key={entry.id}
                          entry={entry}
                          links={fileLinks(entry, onOpenFile)}
                        />
                      ))}
                    </div>
                  </section>
                ))}
              </div>
            </details>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2 border-t border-[var(--g5-stroke)] px-3 py-1.5 text-xs text-[var(--g5-text2)]">
          <span>Ontology changes between the start and end of this turn.</span>
          {onOpenAllChanges && (
            <button
              type="button"
              data-testid="ontology-card-view-all"
              onClick={onOpenAllChanges}
              className="ml-auto text-[var(--g5-info)] underline"
            >
              View all changes in workspace
            </button>
          )}
        </div>
      </details>
    </ComposerOwnerIfNone>
  );
}

/** A change whose stored text the editor's parser cannot rebuild: still
 * listed under its label, so the card and its counts show every change. */
function UnshownStatement({ entry }: { entry: OntologyCardEntry }) {
  return (
    <div
      data-testid="ontology-card-statement"
      data-statement-id={entry.id}
      data-status={entry.status}
      className="ontology-statement-bullet ontology-comparison-item"
    >
      <div
        role="region"
        aria-label={`${entry.status} statement ${entry.id}`}
        className="ontology-comparison-row ontology-comparison-unchanged"
      >
        <div className="ontology-comparison-label">{entry.status}</div>
        <p
          data-testid="ontology-card-statement-unshown"
          className="col-start-2 leading-6 text-[var(--g5-text2)]"
        >
          <code className="font-mono text-[11px]">{entry.id}</code> can&apos;t
          be shown here.
        </p>
      </div>
    </div>
  );
}

function fileLinks(
  file: { repoId: string; path: string },
  onOpenFile: StatementFileLinks["onOpenFile"] | undefined,
): StatementFileLinks | undefined {
  return onOpenFile
    ? { filePath: file.path, repoId: file.repoId, onOpenFile }
    : undefined;
}

function CardFileHeading({ title, path }: { title: string; path: string }) {
  return (
    <div className="flex items-baseline gap-2 font-semibold">
      <span>{title}</span>
      <span className="font-mono text-[11px] font-normal text-[var(--g5-text2)]">
        {path}
      </span>
    </div>
  );
}

/** A governing statement laid out like the card's changes, under a muted
 * Unchanged label, or its name under Not found, with the agent's reason
 * under its text. */
function GoverningStatement({
  entry,
  links,
}: {
  entry: OntologyCardGoverning;
  links?: StatementFileLinks;
}) {
  const statement = cardStatement(entry.id, entry.statement);
  return (
    <div
      data-testid="ontology-card-governing-statement"
      data-statement-id={entry.id}
      className="ontology-statement-bullet ontology-comparison-item"
      onClickCapture={openStatementLinks(links)}
    >
      <div
        role="region"
        aria-label={`${statement ? "Unchanged" : "Not found"} statement ${entry.id}`}
        className="ontology-comparison-row ontology-comparison-unchanged"
      >
        <div className="ontology-comparison-label">
          {statement ? "Unchanged" : "Not found"}
        </div>
        {statement ? (
          <SemanticRecordFrame
            record={statement.record}
            tags={[]}
            metadata={statement.metadata}
            metadataContext={statementMetadataContext(links)}
          >
            <OntologyStatementMarkdown content={statement.body} links={links} />
          </SemanticRecordFrame>
        ) : (
          <p
            role="alert"
            className="col-start-2 leading-6 text-[var(--g5-blocked)]"
          >
            No statement with the ID{" "}
            <code className="font-mono text-[11px]">{entry.id}</code> was in the
            ontology when the turn ended.
          </p>
        )}
        <p className="col-start-2 flex gap-1 pb-1 text-xs text-[var(--g5-text2)]">
          {/* About the statement above. */}
          <span aria-hidden="true">↳</span>
          <span>
            <span
              className="font-semibold"
              title="The agent's reason for naming this statement"
            >
              Why it applies:
            </span>{" "}
            {entry.reason}
          </span>
        </p>
      </div>
    </div>
  );
}

interface CardEditRequest {
  repoId: string;
  path: string;
  statementId: string;
  expectedText: string;
  text: string;
  idempotencyKey: string;
}

function useEditCardStatement(sessionId: string, messageId: string) {
  const queryClient = useQueryClient();
  return useMutation<OntologyCardEdit, Error, CardEditRequest>({
    mutationFn: (request) =>
      api.post<OntologyCardEdit>(
        `/api/v1/sessions/${sessionId}/messages/${messageId}/ontology-card/edits`,
        request,
      ),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: ["chat-messages", sessionId],
      });
    },
  });
}

function editError(error: Error): string {
  if (error instanceof ApiError) {
    const detail = error.details?.error;
    return typeof detail === "string" ? detail : error.message;
  }
  return error.message;
}

function CardStatement({
  entry,
  file,
  sessionId,
  messageId,
  editable,
  editReason,
  viewerOwns,
  links,
}: {
  entry: OntologyCardEntry;
  file: OntologyCardFile;
  sessionId: string;
  messageId: string;
  editable: boolean;
  editReason: string | null;
  viewerOwns: boolean;
  links?: StatementFileLinks;
}) {
  const current = entry.current;
  const shownText = entry.edit ? entry.edit.text : (current?.body.trim() ?? "");
  // The open edit belongs to the page's composer owner, so rebuilding the
  // chat (a layout change, a chat switch) keeps it, and a Save completes
  // into it even if this card is gone (chat g5.chat.ontology-card.edit).
  const subject = {
    kind: "card-edit" as const,
    id: `${messageId}/${file.repoId}/${file.path}/${entry.id}`,
  };
  const [editing, setEditing] = useComposerField(subject, "editing", false);
  const [draft, setDraft] = useComposerField(subject, "draft", "");
  const [sending, setSending] = useComposerField(subject, "sending", false);
  const [error, setError] = useComposerField<string | null>(
    subject,
    "error",
    null,
  );
  // One key per edit, reused when the same text is saved again after a lost
  // reply; changed text is a new edit with a new key.
  const keyRef = useComposerRef<{ key: string; text: string } | null>(
    subject,
    "key",
    null,
  );
  const clearEdit = useClearComposer();
  const mutation = useEditCardStatement(sessionId, messageId);
  const editButton = useRef<HTMLButtonElement>(null);
  const editText = useRef<HTMLTextAreaElement>(null);
  // Fit the edit box to its text, like the composer; wrapping changes with
  // width, so refit then too (but not for the height set here).
  useLayoutEffect(() => {
    const el = editText.current;
    if (!editing || !el) return;
    let width = -1;
    const fit = () => {
      width = el.clientWidth;
      const style = getComputedStyle(el);
      const px = (value: string) => parseFloat(value) || 0;
      const line = px(style.lineHeight) || 16;
      const border = px(style.borderTopWidth) + px(style.borderBottomWidth);
      const padding = px(style.paddingTop) + px(style.paddingBottom);
      el.style.height = "auto";
      const natural = el.scrollHeight + border;
      const max = EDIT_BOX_MAX_LINES * line + padding + border;
      el.style.height = `${Math.min(natural, max)}px`;
      el.style.overflowY = natural > max ? "auto" : "hidden";
    };
    fit();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (el.clientWidth !== width) fit();
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, [draft, editing]);
  // Opening the editor focuses its text; leaving it returns focus to Edit,
  // which renders again after it.
  const refocus = useRef(false);
  useEffect(() => {
    if (editing) {
      editText.current?.focus();
      return;
    }
    if (!refocus.current) return;
    refocus.current = false;
    editButton.current?.focus();
  }, [editing]);
  const cancel = () => {
    refocus.current = true;
    clearEdit(subject);
  };
  const start = () => {
    clearEdit(subject);
    setDraft(shownText);
    setEditing(true);
  };
  const save = () => {
    if (keyRef.current?.text !== draft.trim())
      keyRef.current = { key: randomUuid(), text: draft.trim() };
    const idempotencyKey = keyRef.current.key;
    setSending(true);
    setError(null);
    mutation
      .mutateAsync({
        repoId: file.repoId,
        path: file.path,
        statementId: entry.id,
        expectedText: shownText,
        text: draft,
        idempotencyKey,
      })
      .then(
        () => {
          refocus.current = true;
          clearEdit(subject);
        },
        (failure: Error) => {
          setSending(false);
          setError(editError(failure));
        },
      );
  };
  const editedCurrent =
    entry.edit && current
      ? { ...current, body: `\n\n${entry.edit.text}\n\n` }
      : current;
  // A kept draft survives a turn starting or the VM stopping, but cannot be
  // saved until editing is available again.
  const canSave =
    !sending && !editReason && !!draft.trim() && draft.trim() !== shownText;
  // A kept edit shows only while the card can still be edited (the newest).
  const editor =
    editing && editable ? (
      <form
        className="space-y-2 pt-0.5 pb-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSave) save();
        }}
      >
        <textarea
          data-testid="ontology-card-edit-text"
          aria-label={`Edit ${entry.id}`}
          ref={editText}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              cancel();
            } else if (
              event.key === "Enter" &&
              (event.metaKey || event.ctrlKey)
            ) {
              event.preventDefault();
              event.currentTarget.form?.requestSubmit();
            }
          }}
          rows={EDIT_BOX_MIN_LINES}
          // Keeps the text where the statement's text was.
          className="-mx-2 block w-[calc(100%+1rem)] resize-none rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] px-2 py-0.5 font-mono text-xs"
        />
        {(error || editReason) && (
          <p role="alert" className="text-xs text-[var(--g5-blocked)]">
            {error ?? editReason}
          </p>
        )}
        {/* Starts at the edit box's left border; the primary action first. */}
        <div className="-mx-2 flex gap-2">
          <button
            type="submit"
            data-testid="ontology-card-edit-save"
            disabled={!canSave}
            className="rounded bg-[var(--g5-info)] px-2 py-0.5 text-xs text-white disabled:opacity-50"
          >
            {sending ? "Saving…" : "Save"}
          </button>
          <button
            type="button"
            data-testid="ontology-card-edit-cancel"
            onClick={cancel}
            className="rounded border border-[var(--g5-stroke)] px-2 py-0.5 text-xs"
          >
            Cancel
          </button>
        </div>
      </form>
    ) : undefined;
  const edited = entry.edit && (
    <span
      data-testid="ontology-card-edited"
      data-change-highlight-ignore
      title={
        viewerOwns
          ? "You edited this after the agent's turn; saved to the ontology file."
          : "The workspace owner edited this after the agent's turn; saved to the ontology file."
      }
      // Not part of the statement, so copying the text leaves it out.
      className="ml-1 select-none text-[0.8em] text-[var(--g5-text2)]"
    >
      (edited)
    </span>
  );
  const actions = editable && (
    <GatedControl verb="edit the card" reason={editReason}>
      <button
        ref={editButton}
        type="button"
        data-testid="ontology-card-edit"
        disabled={!!editReason}
        aria-label={`Edit ${entry.id}`}
        title={editReason ?? `Edit ${entry.id}`}
        onClick={start}
        className="semantic-record-hover-action rounded p-1 hover:bg-[var(--g5-bg2)] hover:text-[var(--g5-text1)]"
      >
        <Pencil className="h-4 w-4" aria-hidden="true" />
      </button>
    </GatedControl>
  );
  return (
    <div
      data-testid="ontology-card-statement"
      data-statement-id={entry.id}
      data-status={entry.status}
      onClickCapture={openStatementLinks(links)}
    >
      <OntologyChangeRow
        change={{
          status: entry.status,
          current: editedCurrent ?? undefined,
          before: entry.before ?? undefined,
        }}
        actions={actions || undefined}
        textSuffix={edited || undefined}
        editor={editor}
        links={links}
      />
    </div>
  );
}
