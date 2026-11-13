// =============================================================================
// G5 Files — FileView
// =============================================================================
// A file opened from FILES, shown the way GitHub shows a repository file
// (`g5.documents.files-view`): read first, Markdown as a rendered document
// and other text in read-only Monaco, and Edit as an explicit step that
// edits the rendered document in place (Rich text) or its plain text. The
// editor's file row carries every control at its right: the switch between
// the two modes, reading or editing, Edit, Push review, Save, Done and
// Discard (`g5.documents.editor.modes`); Done returns to reading with the
// unsaved draft applied.
//
// Who changes the file decides how:
// - the owner edits through the running workspace and Saves, which commits
//   and pushes the file (`g5.documents.files-view.owner-edit`); a stopped
//   workspace offers "Start workspace to edit";
// - anyone else who can read the workspace Suggests changes and pushes them
//   to the owner as review suggestions, whatever the VM's state
//   (`g5.governance.file-suggestions`).
//
// Unsaved text lives in the tab's draft store, so it survives every view,
// tab and page change (`g5.documents.files-view.drafts`). Only text files
// open in the editor; a PDF or raster image opens in its read-only viewer,
// an SVG's rendered mode is its image, and any other binary file gets a card
// with Download (`g5.documents.viewers`).
// =============================================================================

import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { AlertTriangle, Pencil, Play, Send } from "lucide-react";
import { CodeEditor, type CodeEditorProps } from "../editor/CodeEditor";
import { SideThreads } from "../comments/SideThreads";
import {
  COMMENT_DRAFT_CLOSED,
  commentDraftSubject,
  useCommentDraft,
} from "../comments/commentDraft";
import { classNames } from "../ui/UI";
import { downloadBlob } from "../../lib/utils";
import { FileNoticeCard } from "./FileNoticeCard";
import { ImageFileView } from "./ImageFileView";
import { PdfFileView } from "./PdfFileView";
import { fileBlob, fileBytes, formatByteSize } from "./fileBytes";
import { ApiError } from "../../lib/api";
import type { WorkspaceFileContent } from "../../hooks/useFiles";
import { usePushFileSuggestions } from "../../hooks/useWorkspaceReview";
import {
  DocumentPreview,
  type DocumentPreviewProps,
  type PreviewEditRequest,
  type PreviewReadingState,
} from "./DocumentPreview";
import {
  RichMarkdownEditor,
  type RichEditorHandle,
  type RichFocus,
} from "./RichMarkdownEditor";
import { isCurrentOntologyPath } from "../../lib/currentOntology";
import { findingsFileOf } from "../../lib/findings";
import {
  fileSuggestionFits,
  fileViewerKind,
  imageMimeForPath,
  isHtmlPath,
  isMarkdownPath,
  isSvgPath,
  type FileTextEligibility,
} from "./fileText";
import { FileCompareDialog } from "./FileCompareDialog";
import {
  beginFileSave,
  draftPerson,
  createFileDraft,
  fileVersionConflict,
  writeFileDraft,
  resetFileModel,
  fileDraftKey,
  getFileDraft,
  fileEditing,
  fileViewMode,
  saveFileEditing,
  saveFileViewMode,
  type FileViewMode,
  setFileDraft,
  updateFileDraft,
  useFileDraft,
  type FileDraftRole,
} from "./fileDrafts";
import {
  liveGateReason,
  useWorkspaceLiveGate,
  type LiveGateKind,
} from "../../lib/workspaceLiveGate";

/**
 * What the owner's Start button says for the VM state. Without a page gate
 * the local request flag decides, as before.
 */
export function fileViewStartLabel(
  kind: LiveGateKind | null,
  startRequested: boolean,
  workspaceNoun = "the workspace",
): { label: string; title: string | undefined } {
  if (kind === null) {
    return {
      label: startRequested ? "Starting workspace…" : "Start workspace to edit",
      title: undefined,
    };
  }
  switch (kind) {
    case "starting":
    case "queued":
      return {
        label: "Starting workspace…",
        title: liveGateReason(kind, "edit", { workspaceNoun }) ?? undefined,
      };
    case "stopping":
      return {
        label: "Workspace is stopping…",
        title: liveGateReason(kind, "edit", { workspaceNoun }) ?? undefined,
      };
    case "failed":
      return {
        label: "Start workspace again to edit",
        title: liveGateReason(kind, "edit", { workspaceNoun }) ?? undefined,
      };
    default:
      return {
        label: "Start workspace to edit",
        title: liveGateReason(kind, "edit", { workspaceNoun }) ?? undefined,
      };
  }
}

/** What FILES passes straight through to the editor. */
export type FileViewEditorProps = Pick<
  CodeEditorProps,
  | "showComments"
  | "onToggleComments"
  | "comments"
  | "commentDraftKey"
  | "onAddComment"
  | "onResolveComment"
  | "onUnresolveComment"
  | "onReplyToComment"
  | "onDeleteComment"
  | "commentsNotice"
  | "scrollToIntentRecord"
  | "scrollToIntentRecordSeq"
  | "navigationOrigin"
  | "onNavigateBack"
  | "suggestionOverlay"
  | "changedLines"
>;

export interface FileViewProps {
  previewProps?: Pick<
    DocumentPreviewProps,
    | "onNavigate"
    | "onDiscussQuestion"
    | "onDiscussFinding"
    | "currentUserId"
    | "currentUserEmail"
    | "currentUserAvatarUrl"
  >;
  /** Specialized HTML/JSONL presentation, fed the same draft as Source. */
  renderPreview?: (
    content: string,
    actions: { canSave: boolean; save: (next: string) => Promise<void> },
  ) => React.ReactNode;
  workspaceId: string;
  repoId: string;
  filePath: string;
  language: string;
  file: WorkspaceFileContent;
  /**
   * The principal this page was rendered for: drafts are kept for its
   * person, and Push review is never sent under another principal's token.
   */
  principal: string | null;
  /** The line a citation or trace jump asked for, when one did. */
  lineTarget: number | null;
  fragment?: string;
  viewerOwnsWorkspace: boolean;
  ownerVmRunning: boolean;
  /** The workspace accepts writes: not Base, read-only or freezing. */
  workspaceWritable: boolean;
  onSave: (
    content: string,
    expectedHash: string | null,
    expectedVmId?: string | null,
    options?: { commitAndPush: boolean },
  ) => Promise<{ hash: string }>;
  onReload: NonNullable<CodeEditorProps["onReload"]>;
  /** Reads the current version for Compare, without touching the view. */
  onFetchCurrent: () => Promise<{
    content: string;
    hash: string;
    vmId?: string | null;
  }>;
  onOpenWorkspaceFile?: (
    filePath: string,
    repoRef?: string,
    line?: number,
    from?: string,
  ) => void;
  /** Starts the stopped workspace (the owner's explicit consent). */
  onStartWorkspace: () => void;
  editorProps: FileViewEditorProps;
}

export const FileView: React.FC<FileViewProps> = (props) => {
  // The extension sends a PDF or raster image to its viewer whatever the
  // bytes hold; an SVG that is text keeps the editor with its image as the
  // rendered mode (`g5.documents.viewers`).
  const kind = fileViewerKind(props.filePath, props.file);
  switch (kind) {
    case "pdf":
      return <PdfFileTab filePath={props.filePath} file={props.file} />;
    case "image":
      return <ImageFileTab filePath={props.filePath} file={props.file} />;
    case "text": {
      const svgPreview = isSvgPath(props.filePath)
        ? (content: string) => (
            <SvgPreview content={content} filePath={props.filePath} />
          )
        : undefined;
      return (
        <TextFileView
          {...props}
          renderPreview={props.renderPreview ?? svgPreview}
        />
      );
    }
    default:
      return (
        <FileUnavailableCard
          filePath={props.filePath}
          file={props.file}
          eligibility={kind}
        />
      );
  }
};

/** A PDF's bytes, decoded once per version, in the PDF viewer. */
const PdfFileTab: React.FC<{
  filePath: string;
  file: WorkspaceFileContent;
}> = ({ filePath, file }) => {
  const { content, encoding } = file;
  const bytes = useMemo(
    () => fileBytes({ content, encoding }),
    [content, encoding],
  );
  const blob = useMemo(
    () => new Blob([bytes], { type: "application/pdf" }),
    [bytes],
  );
  return (
    <PdfFileView
      key={file.hash}
      filePath={filePath}
      bytes={bytes}
      blob={blob}
      byteSize={file.size}
    />
  );
};

/** An image's bytes, typed by its extension, in the image viewer. */
const ImageFileTab: React.FC<{
  filePath: string;
  file: WorkspaceFileContent;
}> = ({ filePath, file }) => {
  const mime = imageMimeForPath(filePath) ?? "application/octet-stream";
  const { content, encoding } = file;
  const bytes = useMemo(
    () => fileBytes({ content, encoding }),
    [content, encoding],
  );
  const blob = useMemo(() => new Blob([bytes], { type: mime }), [bytes, mime]);
  return (
    <ImageFileView
      key={file.hash}
      filePath={filePath}
      blob={blob}
      bytes={bytes}
      byteSize={file.size}
    />
  );
};

/** An SVG draft shown as an image, in an image context where it runs nothing. */
const SvgPreview: React.FC<{ content: string; filePath: string }> = ({
  content,
  filePath,
}) => {
  const blob = useMemo(
    () => new Blob([content], { type: "image/svg+xml" }),
    [content],
  );
  return (
    <ImageFileView
      filePath={filePath}
      blob={blob}
      byteSize={blob.size}
      embedded
    />
  );
};

const FileUnavailableCard: React.FC<{
  filePath: string;
  file: WorkspaceFileContent;
  eligibility: Exclude<FileTextEligibility, "text">;
}> = ({ filePath, file, eligibility }) => {
  const name = filePath.split("/").pop() || filePath;
  return (
    <FileNoticeCard
      testId="file-view-unavailable"
      downloadTestId="file-view-download"
      reason={eligibility}
      filePath={filePath}
      detail={
        <>
          {eligibility === "too-large"
            ? "Too large to display"
            : "Binary file, can't be shown"}
          {" · "}
          {formatByteSize(file.size)}
        </>
      }
      onDownload={() => downloadBlob(fileBlob(file), name)}
    />
  );
};

type PushState =
  | { kind: "idle" }
  | { kind: "pushing" }
  | { kind: "stale" }
  | { kind: "error"; message: string }
  | { kind: "done"; count: number };

const headerButton =
  "flex shrink-0 items-center gap-1 rounded px-2 py-1 text-xs hover:bg-[var(--g5-overlay-hover)] disabled:cursor-not-allowed disabled:opacity-50";

const TextFileView: React.FC<FileViewProps> = ({
  workspaceId,
  repoId,
  filePath,
  language,
  file,
  principal,
  lineTarget,
  fragment,
  viewerOwnsWorkspace,
  ownerVmRunning,
  workspaceWritable,
  onSave,
  onReload,
  onFetchCurrent,
  onOpenWorkspaceFile,
  onStartWorkspace,
  editorProps,
  previewProps,
  renderPreview,
}) => {
  const markdown = isMarkdownPath(filePath) || !!renderPreview;
  const role: FileDraftRole = viewerOwnsWorkspace ? "owner" : "suggest";
  const person = draftPerson(principal);
  const draftKey = person
    ? fileDraftKey({ person, workspaceId, repoId, path: filePath })
    : null;
  const draft = useFileDraft(draftKey);

  // The owner edits only live bytes of the running attachment
  // (`g5.documents.save.reload`); a reader suggests from what they read.
  const ownerCanEditNow =
    viewerOwnsWorkspace &&
    workspaceWritable &&
    ownerVmRunning &&
    file.source === "live" &&
    !!file.vmId;
  const ownerMustStart =
    viewerOwnsWorkspace && workspaceWritable && !ownerVmRunning;
  const suggestionBase = draft?.baseline.content ?? file.content;
  const suggestionBaseFits = useMemo(
    () => role !== "suggest" || fileSuggestionFits(suggestionBase),
    [role, suggestionBase],
  );
  const canSuggest =
    !viewerOwnsWorkspace && workspaceWritable && suggestionBaseFits;
  const roleMatches = !draft || draft.role === role;
  const mayEdit =
    roleMatches && (role === "owner" ? ownerCanEditNow : canSuggest);

  // The reading view's state and the navigation requests it handled outlive
  // its remounts while the file stays open (`g5.documents.editor.modes`).
  const readingState = useRef<PreviewReadingState>({}).current;
  // Where the reading view's Compare and filters go in the file row.
  const [readingTools, setReadingTools] = useState<HTMLElement | null>(null);
  // A statement or heading target opens the rendered reading view from either
  // mode, leaving editing as Done does, once per request
  // (`g5.documents.ontology.links`).
  const navigationTarget =
    editorProps.scrollToIntentRecord?.replace(/^#/, "") || fragment || null;
  const navigationRequest = navigationTarget
    ? `${navigationTarget}\u0000${editorProps.scrollToIntentRecordSeq ?? ""}`
    : null;
  const navigationPending =
    markdown &&
    !renderPreview &&
    navigationRequest !== null &&
    readingState.handledTarget !== navigationRequest;
  // A file reopens as it was left: editing, or reading with its draft applied
  // after Done; a draft this tab has not recorded a state for reopens in Edit
  // (`g5.documents.editor.modes`). Opened at a statement or heading target,
  // it is read with its draft applied, as after Done.
  const [editing, setEditing] = useState(
    () =>
      (!navigationPending || lineTarget != null) &&
      ((draftKey ? fileEditing(draftKey) : undefined) ?? draft !== null),
  );
  // The mode: the rendered document ("preview") or plain text ("source").
  const [view, setView] = useState<FileViewMode>(() => {
    if (lineTarget != null) return "source";
    if (navigationPending) return "preview";
    const stored = draftKey ? fileViewMode(draftKey) : undefined;
    return stored || (markdown ? "preview" : "source");
  });
  useEffect(() => {
    if (!draftKey) return;
    saveFileViewMode(draftKey, view);
    saveFileEditing(draftKey, editing);
  }, [draftKey, editing, view]);
  const [startRequested, setStartRequested] = useState(false);
  const liveGate = useWorkspaceLiveGate();
  const startLabel = fileViewStartLabel(
    liveGate?.kind ?? null,
    startRequested,
    liveGate?.workspaceNoun,
  );
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const [pushNote, setPushNote] = useState<string | null>(null);
  const [pushState, setPushState] = useState<PushState>({ kind: "idle" });
  const [compare, setCompare] = useState<{
    content: string;
    hash: string;
  } | null>(null);
  const push = usePushFileSuggestions(workspaceId);

  // Rich text, while open, is flushed before Save, Push review, Done,
  // citations, comments and leaving it; a refused write reloads it with the
  // draft as it now is (`g5.documents.preview.rich-editing`).
  const richEditor = useRef<RichEditorHandle | null>(null);
  const [richOpen, setRichOpen] = useState(false);
  // Edits typed but not yet written keep Save available to write and save them.
  const [richPending, setRichPending] = useState(false);
  const [richFocus, setRichFocus] = useState<RichFocus | null>(null);
  const registerRichEditor = useCallback((handle: RichEditorHandle | null) => {
    richEditor.current = handle;
    setRichOpen(handle !== null);
    if (handle) return;
    setRichPending(false);
    // A caret request belongs to the opening it was made for.
    setRichFocus(null);
  }, []);
  const commitRichEditor = useCallback(
    () => (richEditor.current ? richEditor.current.flush() : true),
    [],
  );
  const [sourceJump, setSourceJump] = useState<{
    line: number;
    seq: number;
  } | null>(null);
  const [saveRequest, setSaveRequest] = useState(0);
  // Markdown that DocumentPreview renders is edited as a rendered document;
  // HTML and JSONL previews keep Source editing.
  const richCapable = markdown && !renderPreview;

  // A citation or trace jump into an open file shows its line in Code view.
  const [shownLineTarget, setShownLineTarget] = useState(lineTarget);
  if (lineTarget !== shownLineTarget) {
    setShownLineTarget(lineTarget);
    if (lineTarget != null) {
      // The citation's line shows, not a jump kept from before it.
      setSourceJump(null);
      if (!richOpen) setView("source");
    }
  }
  // With Rich text open, the jump follows its flush.
  useEffect(() => {
    if (lineTarget != null && richEditor.current && commitRichEditor())
      setView("source");
  }, [commitRichEditor, lineTarget]);
  const [shownNavigation, setShownNavigation] = useState(navigationRequest);
  if (navigationRequest !== shownNavigation) {
    setShownNavigation(navigationRequest);
    if (navigationPending && !richOpen) {
      setEditing(false);
      setView("preview");
    }
  }
  useEffect(() => {
    if (navigationPending && richEditor.current && commitRichEditor()) {
      setEditing(false);
      setView("preview");
    }
    // A new request, not each render, moves the view.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [commitRichEditor, navigationRequest]);

  // The rendered mode is Rich text while the person edits and may write, and
  // the reading view otherwise (`g5.documents.preview.rich-editing`).
  const showRich = richCapable && editing && mayEdit && view === "preview";
  const showPreview = markdown && view === "preview" && !showRich;
  const displayedContent = draft?.content ?? file.content;
  const suggestionFits = useMemo(
    () =>
      role !== "suggest" ||
      (suggestionBaseFits && fileSuggestionFits(displayedContent)),
    [role, suggestionBaseFits, displayedContent],
  );
  // Markdown, JSONL and SVG share the same preview threshold; HTML is exempt.
  // This defers rendering, and Rich text, only; Source remains available
  // without truncating or rejecting editable text.
  const [previewLoaded, setPreviewLoaded] = useState(false);
  // A findings file is opened to read and decide its entries, so it renders
  // at once (`g5.documents.findings.files`).
  const deferPreview =
    displayedContent.length > 1024 * 1024 &&
    !isHtmlPath(filePath) &&
    !previewLoaded &&
    !richOpen &&
    !findingsFileOf(filePath);
  // Editing starts in the mode being read: Rich text from rendered Markdown;
  // plain text from plain text, from the HTML and JSONL previews and from a
  // deferred preview (`g5.documents.source-editing`).
  const editingView: FileViewMode =
    richCapable && view === "preview" && !deferPreview ? "preview" : "source";

  // "Start workspace to edit": editing begins once live bytes arrived.
  if (startRequested && ownerCanEditNow) {
    setStartRequested(false);
    if (!richOpen) setView(editingView);
    setEditing(true);
  }
  // A push of this draft may have started in an earlier view of the file.
  const pushing = pushState.kind === "pushing" || draft?.pushing === true;
  // Findings decisions (`g5.documents.findings.decision`) write the live
  // file, so only the owner of the running workspace makes them, and not
  // over unsaved edits of their own.
  const findingsBlockedReason = !viewerOwnsWorkspace
    ? "Only the workspace owner decides here."
    : !workspaceWritable
      ? "This workspace is read-only."
      : !ownerCanEditNow
        ? "The workspace owner decides here while the workspace is running."
        : pushing || draft?.saving
          ? "A save is in progress."
          : draft || !roleMatches
            ? "Save or discard your edits to this file first."
            : null;
  const editorDraft = useMemo(
    () => (draftKey ? { key: draftKey, role: draft?.role ?? role } : null),
    [draftKey, draft?.role, role],
  );
  // Who may start a thread here: as plain text decides it, and not a reader
  // composing a suggestion or reading it after Done, who sees unpushed text
  // only the owner may anchor to (`g5.documents.comments.authority`).
  const addComment =
    role === "suggest" && (editing || draft !== null)
      ? undefined
      : editorProps.onAddComment;
  // A thread started from a selection in the rendered document is written
  // in the margin, the page staying as it is
  // (`g5.documents.comments.identity`).
  const draftSubject = commentDraftSubject(
    editorProps.commentDraftKey ?? filePath,
  );
  const commentDraft = useCommentDraft(draftSubject);
  const startThread = (line: number, quote: string) => {
    if (!commitRichEditor()) return;
    commentDraft.open(line, quote, displayedContent);
    if (!editorProps.showComments) editorProps.onToggleComments?.();
  };
  // A comment card's line label opens plain text at its line, starting no
  // draft; expanding the card shows the line's rendering in the rendered document
  // (`g5.documents.comments.identity`).
  const openSourceLine = (line: number) => {
    if (!commitRichEditor()) return;
    setSourceJump({ line, seq: Date.now() });
    setView("source");
  };

  // Every Preview write (a question answer, an HTML answer, the Preview
  // editor) joins the viewer's draft under the same gate as Edit and Suggest
  // changes.
  const rewriteDraft = (expected: string, next: string) => {
    if (!draftKey || !mayEdit || pushing)
      throw new Error("This file is read-only.");
    const current = getFileDraft(draftKey);
    const initial = createFileDraft(
      role,
      next,
      current?.baseline ?? {
        content: file.content,
        hash: file.hash,
        vmId: file.vmId ?? null,
      },
      current?.id,
    );
    const submitted = writeFileDraft(draftKey, initial, {
      expectedContent: expected,
    });
    if (!submitted)
      throw new Error(
        "The file changed or a Save is in progress. Review it and retry.",
      );
    // The draft's presence offers Save, Push review and Discard: a domain
    // write from the reading view leaves the person on the card they answered.
    return submitted;
  };

  const savePreview = async (
    next: string,
    options: { expected?: string; forgetIfClean?: boolean } = {},
  ) => {
    const submitted = rewriteDraft(options.expected ?? displayedContent, next);
    const settle = beginFileSave(draftKey!, submitted, submitted.baseline.hash);
    if (!settle)
      throw new Error("The file session changed. Review it and retry.");
    try {
      const saved = await onSave(
        next,
        submitted.baseline.hash,
        submitted.baseline.vmId,
        { commitAndPush: false },
      );
      settle(
        { ...submitted.baseline, content: next, hash: saved.hash },
        undefined,
        undefined,
        options.forgetIfClean,
      );
    } catch (error) {
      const conflict = fileVersionConflict(error);
      settle(
        null,
        error instanceof Error ? error.message : String(error),
        conflict,
      );
      throw error;
    }
  };

  // A findings decision (`g5.documents.findings.decision`) is written whole
  // or not at all: a refused write drops the draft it made, so the reader
  // shows the file as it is and nothing else waits on it; after a version
  // conflict the file is read again, so Retry writes over the current text.
  const writeDecision = async (expected: string, next: string) => {
    try {
      await savePreview(next, { expected, forgetIfClean: true });
    } catch (error) {
      if (draftKey && getFileDraft(draftKey)?.content === next) {
        resetFileModel(draftKey, file.content);
        setFileDraft(draftKey, null);
      }
      if (fileVersionConflict(error)) await onReload().catch(() => undefined);
      throw error;
    }
  };

  const finishEditing = useCallback(() => {
    setEditing(false);
    setView(markdown ? "preview" : "source");
    setConfirmDiscard(false);
    setPushNote(null);
  }, [markdown]);

  const discard = useCallback(() => {
    if (draftKey) {
      resetFileModel(draftKey, file.content);
      setFileDraft(draftKey, null);
    }
    setPushState({ kind: "idle" });
    finishEditing();
  }, [draftKey, file.content, finishEditing]);

  const submitPush = useCallback(
    async (allowStaleBase: boolean) => {
      // Rich text's last edits join the push; a refused write cancels it.
      if (!draftKey || !commitRichEditor()) return;
      const submitted = getFileDraft(draftKey);
      if (
        !submitted ||
        submitted.pushing ||
        !canSuggest ||
        submitted.role !== "suggest" ||
        !fileSuggestionFits(submitted.baseline.content) ||
        !fileSuggestionFits(submitted.content)
      )
        return;
      // Every view of this draft takes no typing until the push settles,
      // even one opened after this view closed.
      updateFileDraft(draftKey, submitted.id, (current) => ({
        ...current,
        pushing: true,
      }));
      setPushState({ kind: "pushing" });
      try {
        const result = await push.mutateAsync({
          repoId,
          path: filePath,
          baseContent: submitted.baseline.content,
          content: submitted.content,
          note: pushNote?.trim() ? pushNote.trim() : undefined,
          allowStaleBase,
          principal,
        });
        // Only the version that was pushed is done; a draft dropped or
        // replaced meanwhile is left alone, and so is any other text.
        updateFileDraft(draftKey, submitted.id, (current) =>
          current.content === submitted.content &&
          current.baseline.content === submitted.baseline.content
            ? null
            : { ...current, pushing: false },
        );
        if (!getFileDraft(draftKey)) finishEditing();
        setPushState({ kind: "done", count: result.suggestions.length });
      } catch (error) {
        updateFileDraft(draftKey, submitted.id, (current) => ({
          ...current,
          pushing: false,
        }));
        if (error instanceof ApiError && error.is("FILE_VERSION_CONFLICT")) {
          setPushState({ kind: "stale" });
          return;
        }
        setPushState({
          kind: "error",
          message: error instanceof Error ? error.message : String(error),
        });
      }
    },
    [
      canSuggest,
      commitRichEditor,
      draftKey,
      filePath,
      finishEditing,
      principal,
      push,
      pushNote,
      repoId,
    ],
  );

  const openStaleCompare = useCallback(async () => {
    try {
      const current = await onFetchCurrent();
      setCompare({ content: current.content, hash: current.hash });
    } catch (error) {
      setPushState({
        kind: "error",
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }, [onFetchCurrent]);

  // Rich text opens focused: at a block the person double-clicked, or at
  // the start.
  const startEditing = (at?: PreviewEditRequest) => {
    if (!commitRichEditor()) return;
    if (!editing) setPushState({ kind: "idle" });
    if (editingView === "preview")
      setRichFocus({ offset: null, word: null, ...at, seq: Date.now() });
    setView(editingView);
    setEditing(true);
  };
  // One control switches between the rendered document and plain text,
  // reading or editing (`g5.documents.editor.modes`). Rich text writes its
  // edits first; a refused write keeps the mode and shows the draft as it is.
  const switchMode = () => {
    if (commitRichEditor()) setView(view === "preview" ? "source" : "preview");
  };
  const switchLabel =
    view === "preview"
      ? editing
        ? "Switch to plain text editing"
        : "Switch to plain text"
      : !richCapable
        ? "Switch to preview"
        : editing
          ? "Switch to rich text editing"
          : "Switch to rich text";
  // Double-click or `e` in the rendered reading view opens Rich text.
  const canStartEditing =
    richCapable && (editing ? mayEdit : ownerCanEditNow || canSuggest);
  // `e` in the reading view starts editing, as in Confluence.
  const rootRef = useRef<HTMLDivElement>(null);
  const startFromKey = useRef<() => void>(() => undefined);
  useEffect(() => {
    startFromKey.current = () => {
      if (canStartEditing && showPreview && !deferPreview) startEditing();
    };
  });
  // Mod+S outside the editors means the file's Save, never the browser's
  // Save page: it saves an owner's unsaved draft whenever Save is offered,
  // including in the reading view after Done, and otherwise does nothing.
  // In a comment, question or other text input it only keeps the dialog away.
  const saveFromKey = useRef<() => void>(() => undefined);
  useEffect(() => {
    saveFromKey.current = () => {
      if (role === "owner" && mayEdit && (draft || richPending))
        setSaveRequest((value) => value + 1);
    };
  });
  useEffect(() => {
    const element = rootRef.current;
    if (!element) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        (event.key === "s" || event.key === "S") &&
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        // Monaco and Rich text bind Mod+S themselves.
        !(event.target as HTMLElement).closest(
          "[contenteditable='true'], .monaco-editor",
        )
      ) {
        event.preventDefault();
        if (!(event.target as HTMLElement).closest("input, textarea, select"))
          saveFromKey.current();
        return;
      }
      if (
        event.key !== "e" ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        event.shiftKey
      )
        return;
      const target = event.target as HTMLElement;
      if (
        target.closest(
          "input, textarea, select, button, [contenteditable='true'], .monaco-editor",
        )
      )
        return;
      event.preventDefault();
      startFromKey.current();
    };
    element.addEventListener("keydown", onKeyDown);
    return () => element.removeEventListener("keydown", onKeyDown);
  }, []);

  // The file row's controls, at its right (`g5.documents.editor.modes`):
  // the role badge and mode switch, then the role's actions with the
  // editor's Save between Push review and Done.
  const headerActions = (save: React.ReactNode) => (
    <>
      {/* The reading view puts Compare and its filters here, first among the
          row's controls (`g5.documents.editor.modes`). */}
      {showPreview && (
        <div
          ref={setReadingTools}
          data-testid="file-view-reading-tools"
          className="flex items-center empty:hidden"
        />
      )}
      {(editing || draft) && role === "suggest" && (
        <span
          data-testid="file-view-suggesting"
          className="ml-1 rounded bg-[var(--g5-bg2)] px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-[var(--g5-text1)]"
          title="Your edits are sent to the workspace owner as suggestions"
        >
          Suggesting changes
        </span>
      )}
      {markdown && (
        <button
          type="button"
          data-testid="file-view-switch-mode"
          className={classNames(headerButton, "text-[var(--g5-text1)]")}
          onClick={switchMode}
        >
          {switchLabel}
        </button>
      )}
      {!editing && ownerCanEditNow && (
        <button
          type="button"
          data-testid="file-view-edit"
          className={classNames(headerButton, "text-[var(--g5-active)]")}
          onClick={() => startEditing()}
        >
          <Pencil className="h-3.5 w-3.5" />
          Edit
        </button>
      )}
      {!editing &&
        viewerOwnsWorkspace &&
        workspaceWritable &&
        ownerVmRunning &&
        !ownerCanEditNow && (
          <span
            data-testid="file-view-loading-live"
            className="text-[11px] text-[var(--g5-text2)]"
          >
            Loading the live file…
          </span>
        )}
      {ownerMustStart && (!editing || draft !== null) && (
        <button
          type="button"
          data-testid="file-view-start-to-edit"
          className={classNames(headerButton, "text-[var(--g5-active)]")}
          // The label follows the VM state when the page provides it
          // (`g5.workspace.ui.vm-state`): a failed start offers Start again
          // instead of a stuck "Starting…".
          disabled={liveGate ? !liveGate.canStart : startRequested}
          title={startLabel.title}
          onClick={() => {
            setStartRequested(true);
            onStartWorkspace();
          }}
        >
          <Play className="h-3.5 w-3.5" />
          {startLabel.label}
        </button>
      )}
      {!editing && canSuggest && (
        <button
          type="button"
          data-testid="file-view-suggest"
          className={classNames(headerButton, "text-[var(--g5-active)]")}
          onClick={() => startEditing()}
          title="Edit this file and push your changes to the owner as suggestions"
        >
          <Pencil className="h-3.5 w-3.5" />
          Suggest changes
        </button>
      )}
      {(editing || draft) && role === "suggest" && (
        <button
          type="button"
          data-testid="file-view-push-review"
          className={classNames(headerButton, "text-[var(--g5-active)]")}
          disabled={
            // Edits Rich text has not written yet count: it writes first.
            (!draft && !richPending) ||
            pushing ||
            !mayEdit ||
            !suggestionFits ||
            pushNote !== null
          }
          onClick={() => {
            if (!commitRichEditor()) return;
            setPushState({ kind: "idle" });
            setPushNote("");
          }}
        >
          <Send className="h-3.5 w-3.5" />
          Push review
        </button>
      )}
      {save}
      {editing && (
        <button
          type="button"
          data-testid="file-view-done"
          className={classNames(headerButton, "text-[var(--g5-text1)]")}
          title="Back to reading; unsaved changes stay until you Save or Discard"
          onClick={() => {
            // Rich text's last edits join the draft, which Done keeps.
            if (commitRichEditor()) finishEditing();
          }}
        >
          Done
        </button>
      )}
      {(editing || draft) && !confirmDiscard && (
        // Shown throughout editing, disabled until there is a draft: Rich
        // text writes its draft when a header button takes focus, and a
        // button appearing then would move Done or Push review from under
        // the pointer before the click lands.
        <button
          type="button"
          data-testid="file-view-discard"
          className={classNames(headerButton, "text-[var(--g5-text1)]")}
          disabled={!draft || pushing}
          title={draft ? undefined : "Nothing to discard yet"}
          onClick={() => setConfirmDiscard(true)}
        >
          Discard
        </button>
      )}
      {draft && confirmDiscard && (
        <span className="flex items-center gap-1 text-xs text-[var(--g5-text1)]">
          Discard your changes?
          <button
            type="button"
            data-testid="file-view-discard-confirm"
            className={classNames(headerButton, "text-[var(--g5-blocked)]")}
            onClick={discard}
          >
            Discard
          </button>
          <button
            type="button"
            data-testid="file-view-discard-cancel"
            className={classNames(headerButton, "text-[var(--g5-text1)]")}
            onClick={() => setConfirmDiscard(false)}
          >
            Keep editing
          </button>
        </span>
      )}
    </>
  );

  // Notices and the push panel, shown under the file row.
  const notices = (
    <>
      {role === "suggest" && !suggestionFits && (
        <Notice testId="file-view-suggestion-size" tone="warn">
          Suggestions support files and edited text up to 2 MiB of UTF-8 text.
          {suggestionBaseFits
            ? " Your draft is kept; reduce its size before pushing."
            : " You can read this file, but it is too large to suggest changes."}
        </Notice>
      )}

      {!roleMatches && (
        <Notice testId="file-view-role-changed">
          Your access changed. Your previous draft is kept for recovery; discard
          it before starting a new edit or suggestion.
        </Notice>
      )}
      {draft?.pushing && pushState.kind !== "pushing" && (
        <Notice testId="file-view-pushing">
          Pushing your suggestions to the owner…
        </Notice>
      )}

      {role === "owner" && !ownerCanEditNow && draft && (
        <Notice testId="file-view-owner-stopped">
          Your unsaved edits are kept in this browser. Start the workspace to
          keep editing and save them.
        </Notice>
      )}

      {pushNote !== null && (
        <div
          data-testid="file-view-push-panel"
          className="flex shrink-0 flex-col gap-2 border-b border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-2"
        >
          <label
            className="text-xs text-[var(--g5-text1)]"
            htmlFor="file-view-push-note"
          >
            Push your changes to the owner as suggestions. Add a note
            (optional); it starts each suggestion's discussion.
          </label>
          <textarea
            id="file-view-push-note"
            data-testid="file-view-push-note"
            value={pushNote}
            onChange={(event) => setPushNote(event.target.value)}
            rows={2}
            className="w-full resize-y rounded border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-2 text-xs text-[var(--g5-text0)]"
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              data-testid="file-view-push-cancel"
              className={classNames(headerButton, "text-[var(--g5-text1)]")}
              disabled={pushing}
              onClick={() => setPushNote(null)}
            >
              Cancel
            </button>
            <button
              type="button"
              data-testid="file-view-push-submit"
              className="flex items-center gap-1 rounded bg-[var(--g5-active)] px-3 py-1 text-xs font-medium text-white disabled:opacity-50"
              disabled={pushing || !draft || !mayEdit || !suggestionFits}
              onClick={() => void submitPush(false)}
            >
              <Send className="h-3.5 w-3.5" />
              {pushing ? "Pushing…" : "Push review"}
            </button>
          </div>
        </div>
      )}

      {pushState.kind === "stale" && (
        <Notice testId="file-view-stale" tone="warn">
          <span className="flex-1">
            The owner changed this file after you started editing. Compare to
            bring your text up to date, or push it against the version you
            started from.
          </span>
          <button
            type="button"
            data-testid="file-view-stale-compare"
            className={headerButton}
            onClick={() => void openStaleCompare()}
          >
            Compare
          </button>
          <button
            type="button"
            data-testid="file-view-stale-push-anyway"
            className={headerButton}
            disabled={pushing || !mayEdit || !suggestionFits}
            onClick={() => void submitPush(true)}
          >
            Push anyway
          </button>
          <button
            type="button"
            data-testid="file-view-stale-cancel"
            className={headerButton}
            onClick={() => setPushState({ kind: "idle" })}
          >
            Cancel
          </button>
        </Notice>
      )}

      {pushState.kind === "error" && (
        <Notice testId="file-view-push-error" tone="error">
          Push review failed: {pushState.message} Your changes are kept.
        </Notice>
      )}

      {pushState.kind === "done" && (
        <Notice testId="file-view-push-done">
          Pushed {pushState.count} suggestion
          {pushState.count === 1 ? "" : "s"} to the owner. They appear on this
          file and in Reviews.
        </Notice>
      )}
    </>
  );

  const sideDraft = {
    subject: draftSubject,
    draft: commentDraft.draft,
    closed: addComment ? null : COMMENT_DRAFT_CLOSED,
    onSubmit: (content: string) =>
      commentDraft.send(
        addComment,
        displayedContent,
        file.repoOid ?? null,
        content,
      ),
    onCancel: commentDraft.cancel,
  };
  // The rendered reading view places its cards itself; the other views
  // (Rich text editing, a custom preview, the deferred one) keep the list.
  const documentMargin = showPreview && !deferPreview && !renderPreview;
  return (
    <div
      data-testid="file-view"
      data-file-path={filePath}
      className="relative flex h-full min-h-0 flex-col"
      ref={rootRef}
    >
      <div className="relative min-h-0 flex-1">
        <CodeEditor
          {...editorProps}
          sessionKey={draftKey ?? undefined}
          preview={
            showPreview || showRich ? (
              <SideThreads
                title="Comments on this file"
                testId="file-view-preview-threads"
                // The rendered document shows its threads in its own margin,
                // level with their text (`g5.documents.comments.identity`).
                shown={editorProps.showComments && !documentMargin}
                comments={editorProps.comments}
                draft={sideDraft}
                emptyHint={
                  addComment && showPreview && !deferPreview
                    ? "Select text to comment."
                    : null
                }
                content={displayedContent}
                suggestionOverlay={
                  renderPreview ? editorProps.suggestionOverlay : null
                }
                notice={editorProps.commentsNotice ?? null}
                onReply={editorProps.onReplyToComment}
                onResolve={editorProps.onResolveComment}
                onUnresolve={editorProps.onUnresolveComment}
                onDelete={editorProps.onDeleteComment}
                onOpenLine={openSourceLine}
              >
                {deferPreview ? (
                  <div className="p-6" data-testid="file-view-deferred-preview">
                    <p>
                      Large file. Load the rendered preview when you need it.
                    </p>
                    <button
                      data-testid="file-view-load-preview"
                      type="button"
                      className={headerButton}
                      onClick={() => setPreviewLoaded(true)}
                    >
                      Load preview
                    </button>
                  </div>
                ) : showRich ? (
                  <RichMarkdownEditor
                    key={draftKey ?? "reader"}
                    content={displayedContent}
                    filePath={filePath}
                    workspaceId={workspaceId}
                    repoId={repoId}
                    ontology={isCurrentOntologyPath(filePath)}
                    editable={!pushing && !draft?.saving}
                    focusAt={richFocus}
                    onWrite={(expected, next) => {
                      try {
                        rewriteDraft(expected, next);
                        return true;
                      } catch {
                        return false;
                      }
                    }}
                    onHandle={registerRichEditor}
                    onPendingChange={setRichPending}
                    onSaveShortcut={
                      role === "owner"
                        ? () => setSaveRequest((value) => value + 1)
                        : undefined
                    }
                    onOpenSource={(line) => {
                      setSourceJump({ line, seq: Date.now() });
                      setView("source");
                    }}
                  />
                ) : renderPreview ? (
                  renderPreview(displayedContent, {
                    canSave:
                      ownerCanEditNow &&
                      roleMatches &&
                      !pushing &&
                      !draft?.saving,
                    save: (next) => savePreview(next),
                  })
                ) : (
                  <DocumentPreview
                    {...previewProps}
                    content={displayedContent}
                    sourceContent={file.content}
                    filePath={filePath}
                    workspaceId={workspaceId}
                    repoId={repoId}
                    repoOid={file.repoOid}
                    sessionKey={draftKey ?? undefined}
                    fragment={fragment}
                    target={editorProps.scrollToIntentRecord}
                    targetSeq={editorProps.scrollToIntentRecordSeq}
                    comments={{
                      shown: editorProps.showComments,
                      threads: editorProps.comments,
                      draft: sideDraft,
                      emptyHint: addComment ? "Select text to comment." : null,
                      notice: editorProps.commentsNotice ?? null,
                      onReply: editorProps.onReplyToComment,
                      onResolve: editorProps.onResolveComment,
                      onUnresolve: editorProps.onUnresolveComment,
                      onDelete: editorProps.onDeleteComment,
                      onOpenLine: openSourceLine,
                      testId: "file-view-preview-threads",
                    }}
                    onOpenWorkspaceFile={onOpenWorkspaceFile}
                    onCommentSelection={
                      addComment
                        ? (at) => startThread(at.line, at.quote)
                        : undefined
                    }
                    readingState={readingState}
                    toolbarSlot={readingTools}
                    suggestionOverlay={editorProps.suggestionOverlay}
                    canAnswer={
                      ownerCanEditNow &&
                      roleMatches &&
                      !pushing &&
                      !draft?.saving
                    }
                    onRewrite={(expected, next) => {
                      try {
                        rewriteDraft(expected, next);
                        return true;
                      } catch {
                        return false;
                      }
                    }}
                    onRequestEdit={
                      canStartEditing ? (at) => startEditing(at) : undefined
                    }
                    findings={{
                      blockedReason: findingsBlockedReason,
                      write: writeDecision,
                    }}
                  />
                )}
              </SideThreads>
            ) : undefined
          }
          onAddComment={addComment}
          filePath={filePath}
          content={file.content}
          language={language}
          editable={editing && mayEdit}
          // After Done the reading view keeps the draft: Save stays offered
          // until the person decides (`g5.documents.preview.rich-editing`).
          saveAvailable={!!draft && mayEdit}
          headerActions={headerActions}
          notices={notices}
          draft={editorDraft}
          locked={pushing || !roleMatches}
          scrollToLine={sourceJump?.line ?? lineTarget}
          scrollToLineSeq={sourceJump?.seq}
          contentHash={file.hash}
          source={file.source}
          repoOid={file.repoOid}
          snapshotAt={file.snapshotAt}
          observedVmId={file.vmId ?? null}
          onSave={role === "owner" ? onSave : undefined}
          onBeforeSave={commitRichEditor}
          // Rich text's edits keep Save available until the buffer shows
          // them: those not written yet, and a draft written but not adopted.
          pendingEdit={
            richPending ||
            (showRich && !!draft && draft.content !== draft.baseline.content)
          }
          saveRequest={saveRequest}
          onReload={onReload}
          onFetchCurrent={onFetchCurrent}
        />
      </div>

      {compare && draft && draftKey && (
        <FileCompareDialog
          filePath={filePath}
          language={language}
          newerContent={compare.content}
          ownContent={draft.content}
          description="The owner's file changed after you started. Merge what you need into your text; Push review then suggests it against the newer version."
          applyLabel="Use my merged text"
          onCancel={() => setCompare(null)}
          onApply={(merged) => {
            updateFileDraft(draftKey, draft.id, (current) => ({
              ...current,
              content: merged,
              baseline: {
                content: compare.content,
                hash: compare.hash,
                vmId: null,
              },
            }));
            setCompare(null);
            setPushState({ kind: "idle" });
          }}
        />
      )}
    </div>
  );
};

const Notice: React.FC<{
  testId: string;
  tone?: "info" | "warn" | "error";
  children: React.ReactNode;
}> = ({ testId, tone = "info", children }) => (
  <div
    role="status"
    data-testid={testId}
    className={classNames(
      "flex shrink-0 flex-wrap items-center gap-2 border-b px-3 py-1.5 text-xs text-[var(--g5-text0)]",
      tone === "warn" && "border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10",
      tone === "error" &&
        "border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10",
      tone === "info" && "border-[var(--g5-stroke)] bg-[var(--g5-bg1)]",
    )}
  >
    {tone !== "info" && (
      <AlertTriangle
        className={classNames(
          "h-3.5 w-3.5 shrink-0",
          tone === "warn"
            ? "text-[var(--g5-warn)]"
            : "text-[var(--g5-blocked)]",
        )}
      />
    )}
    {children}
  </div>
);
