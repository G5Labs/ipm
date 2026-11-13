// =============================================================================
// G5 Center Panel — CodeEditor (Syntax Highlighted Code Viewer)
// =============================================================================
// A code viewing component for source files (non-spec, non-doc files).
// Currently a custom viewer with plans to integrate Monaco editor.
//
// Features: line numbers, comment gutter, breadcrumb bar, syntax
// highlighting, connected user cursors, comment margin column, minimap toggle,
// status bar.
// =============================================================================

import React, {
  useState,
  useMemo,
  useCallback,
  useRef,
  useEffect,
} from "react";
import {
  AlertTriangle,
  ChevronRight,
  MessageCircle,
  Plus,
  Map as MapIcon,
  Save,
  Undo2,
} from "lucide-react";
import { classNames, IconButton } from "../ui/UI";
import { setEditorDirty } from "./dirtyEditors";
import { defineG5Themes, useThemeMode } from "./monacoTheme";
import { SourceBadge } from "./SourceBadge";
import { textMeasureColumns, wrapsAtTextMeasure } from "./textMeasure";
import { loadMonacoReact } from "./loadMonaco";
import { FileCompareDialog } from "../files/FileCompareDialog";
import {
  getFileDraft,
  beginFileSave,
  fileModelUri,
  retainFileModel,
  fileModelViewState,
  saveFileModelViewState,
  newFileDraftId,
  createFileDraft,
  fileVersionConflict,
  setFileDraft,
  useFileDraft,
  type FileDraft,
  type FileDraftBaseline,
  type FileDraftRole,
} from "../files/fileDrafts";
import { CommentColumn } from "../comments/CommentColumn";
import { placedLine } from "../comments/CommentThread";
import type { CommentThreadAction } from "../comments/CommentThread";
import { CommentDraftCard } from "../comments/CommentDraftCard";
import {
  COMMENT_DRAFT_CLOSED,
  commentDraftSubject,
  useCommentDraft,
} from "../comments/commentDraft";
import { SelectionCommentBubble } from "../comments/SelectionCommentBubble";
import {
  commentThreadFromDTO,
  orderBesideView,
  useCommentAnchorResolutions,
  type AddCommentHandler,
} from "../comments/commentThreadModel";
import {
  liveGateIsWaiting,
  liveGateReason,
  GatedControl,
  useWorkspaceLiveGate,
} from "../../lib/workspaceLiveGate";
import { formatCommentTime } from "../comments/time";
import { highlightCode } from "../../lib/utils";
import type { CommentDTO } from "../../lib/types";
import {
  FallbackSuggestionList,
  MonacoSuggestionZones,
  ReadOnlySuggestionZone,
  resolveDisplayAnchor,
  SuggestionGutterMarker,
  SuggestionZoneContent,
  splitHighlightedLines,
  suggestionZonesByLine,
  suggestionsByLine,
  type SuggestionOverlayProps,
} from "../workspace/ReviewSuggestionOverlay";
import { useSuggestionCardNodes } from "../../hooks/useSuggestionCardNodes";
import { useReviewSourceDigest } from "../../hooks/useReviewSourceDigest";
import { SuggestionCardPortals } from "../workspace/SuggestionCardPlaces";

function updateCodeEditorDebugRegistry(
  filePath: string,
  next: {
    modelValue?: string;
    modelUri?: string;
    yTextValue?: string;
  },
): void {
  if (typeof window === "undefined" || !filePath) return;
  const globalWindow = window as typeof window & {
    __g5CodeEditorDebug?: Record<
      string,
      {
        modelValue?: string;
        modelUri?: string;
        yTextValue?: string;
      }
    >;
  };
  globalWindow.__g5CodeEditorDebug ??= {};
  globalWindow.__g5CodeEditorDebug[filePath] = {
    ...(globalWindow.__g5CodeEditorDebug[filePath] ?? {}),
    ...next,
  };
}

function clearCodeEditorDebugRegistry(filePath: string): void {
  if (typeof window === "undefined" || !filePath) return;
  const globalWindow = window as typeof window & {
    __g5CodeEditorDebug?: Record<string, unknown>;
  };
  if (!globalWindow.__g5CodeEditorDebug) return;
  delete globalWindow.__g5CodeEditorDebug[filePath];
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface CodeEditorProps {
  /** Stable person/workspace/repository/path identity, also in read mode. */
  sessionKey?: string;
  /** Shared read-only presentation; toolbar and recovery stay available. */
  preview?: React.ReactNode;
  /** File path for language detection and breadcrumbs */
  filePath: string;
  /** Source code content */
  content: string;
  /** Detected language (e.g., 'rust', 'typescript', 'python') */
  language: string;
  /** Whether editing is enabled */
  editable: boolean;
  /** Hash of the exact file version loaded into the local model. */
  contentHash?: string | null;
  /** Authority source used for this read. */
  source?: "live" | "flush" | "committed";
  /** Git object id when the source is durable. */
  repoOid?: string | null;
  /** Observation time for a flush or committed snapshot. */
  snapshotAt?: string | null;
  /** VM that supplied the editor's baseline bytes. */
  observedVmId?: string | null;
  /** Explicit persistence callback. Editors never boot or commit a workspace. */
  onSave?: (
    content: string,
    expectedHash: string | null,
    expectedVmId?: string | null,
  ) => Promise<{ hash: string }>;
  /** Fetch the currently authorized source after a stale-write conflict. */
  onReload?: () => Promise<{
    content: string;
    hash: string;
    vmId?: string | null;
  }>;
  /** Whether to show the comment column */
  showComments: boolean;
  /** Toggle comment column callback */
  onToggleComments?: () => void;
  /** Comment threads for this file */
  comments: CommentDTO[];
  /**
   * The file a new-thread draft belongs to in the page's composer owner
   * (`useComposerStore`); the path when not given.
   */
  commentDraftKey?: string;
  /**
   * Comment actions, each passed only when the viewer may perform it
   * (`g5.documents.comments.controls`): without `onAddComment` no Add
   * comment control is offered.
   */
  onAddComment?: AddCommentHandler;
  onResolveComment?: CommentThreadAction<[threadId: string]>;
  onUnresolveComment?: CommentThreadAction<[threadId: string]>;
  onReplyToComment?: CommentThreadAction<[threadId: string, content: string]>;
  onDeleteComment?: CommentThreadAction<[threadId: string]>;
  /** Why the threads shown may be missing or out of date. */
  commentsNotice?: string | null;
  /** Intent record label to scroll to (e.g. "I1", "D3") */
  scrollToIntentRecord?: string | null;
  /** Sequence counter to force re-scroll even when label is unchanged */
  scrollToIntentRecordSeq?: number;
  /** 1-based line to reveal + briefly highlight when the file opens, used for
   *  citation navigation into a cited line range (chat_service D54 /
   *  message_rendering D7). Null/undefined = no scroll. */
  scrollToLine?: number | null;
  /** Distinguishes repeated jumps to the same `scrollToLine`. */
  scrollToLineSeq?: number;
  /** Navigation origin for back-button */
  navigationOrigin?: {
    tabType: string;
    nodeId: string;
    name: string;
    intentRecordNumber: string;
    filePath?: string;
  } | null;
  /** Callback to navigate back to origin */
  onNavigateBack?: () => void;
  /**
   * Open review suggestions on this file, shown in place: view zones in
   * Monaco, gutter markers plus a rail in the read-only viewer.
   */
  suggestionOverlay?: SuggestionOverlayProps | null;
  /**
   * On a review copy: the 1-based inclusive line ranges of this file that
   * differ from the source (what the next Push review would send), marked
   * in the gutter and tinted.
   */
  changedLines?: { start: number; end: number }[];
  /**
   * FILES: the unsaved buffer lives in the tab's draft store under this key,
   * so it outlives this component (`g5.documents.files-view.drafts`). The
   * editor restores it on mount and mirrors its buffer there; whoever owns
   * the store, not the editor, lists it as an unsaved editor.
   */
  draft?: { key: string; role: FileDraftRole } | null;
  /** Reads the current version for Compare when a conflict carries no text. */
  onFetchCurrent?: () => Promise<{
    content: string;
    hash: string;
    vmId?: string | null;
  }>;
  /**
   * FILES: keep showing the unsaved buffer but take no typing, while what
   * was submitted is in flight (a Push review).
   */
  locked?: boolean;
  /**
   * FILES: runs before a Save. It may write the shared draft first (Rich
   * text writes its pending edits) and returns false to cancel the Save
   * (`g5.documents.preview.rich-editing`).
   */
  onBeforeSave?: () => boolean;
  /**
   * FILES: Rich text holds edits it has not written yet, so Save stays
   * available to write and save them even while this buffer is clean.
   */
  pendingEdit?: boolean;
  /** FILES: each increment asks for a Save, as the Save button does (Rich text's Mod+S). */
  saveRequest?: number;
  /**
   * FILES: Save is offered on a read-only buffer that mirrors a draft the
   * person has not decided on yet: Done returned to reading with the draft
   * applied (`g5.documents.preview.rich-editing`).
   */
  saveAvailable?: boolean;
  /**
   * FILES: the file row's controls, rendered at its right with the editor's
   * own Save where the caller places it; the comments toggle follows
   * (`g5.documents.editor.modes`).
   */
  headerActions?: (save: React.ReactNode) => React.ReactNode;
  /** FILES: notices and panels shown under the file row. */
  notices?: React.ReactNode;
}

/**
 * Whole-line decorations for the copy's changed lines. A collection is
 * replaced when the ranges or the editor instance change and cleared on
 * unmount; Monaco keeps them across edits by shifting them with the text.
 */
const MonacoChangedLines: React.FC<{
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  editor: any;
  mountVersion: number;
  ranges: { start: number; end: number }[];
}> = ({ editor, mountVersion, ranges }) => {
  const rangesKey = ranges
    .map((range) => `${range.start}-${range.end}`)
    .join(",");
  useEffect(() => {
    if (!editor || typeof editor.createDecorationsCollection !== "function") {
      return;
    }
    const collection = editor.createDecorationsCollection(
      ranges.map((range) => ({
        range: {
          startLineNumber: range.start,
          startColumn: 1,
          endLineNumber: range.end,
          endColumn: 1,
        },
        options: {
          isWholeLine: true,
          className: "g5-review-changed-line-bg",
          linesDecorationsClassName: "g5-review-changed-line",
        },
      })),
    );
    return () => {
      collection?.clear?.();
    };
    // `rangesKey` stands for `ranges`: a new array with the same lines
    // must not redraw.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, mountVersion, rangesKey]);
  return null;
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** The read-only viewer's row a node sits in, by its `data-line-number`. */
function lineNumberOf(node: Node | null): number | null {
  const element =
    node instanceof Element ? node : (node?.parentElement ?? null);
  const row = element?.closest<HTMLElement>("[data-line-number]");
  const value = row ? Number.parseInt(row.dataset.lineNumber ?? "", 10) : NaN;
  return Number.isFinite(value) ? value : null;
}

const LINE_HEIGHT = 20; // px per line
/** Width of the marker column left of the line numbers: room for a
 *  suggestion marker beside the comment control. */
const MARKER_GUTTER_PX = 32;
// ---------------------------------------------------------------------------
// BreadcrumbBar — File path segments as clickable breadcrumbs
// ---------------------------------------------------------------------------

interface BreadcrumbBarProps {
  filePath: string;
}

const BreadcrumbBar: React.FC<BreadcrumbBarProps> = ({ filePath }) => {
  const segments = filePath.split("/").filter(Boolean);

  return (
    <div
      data-testid="code-editor-breadcrumb"
      className="flex min-w-0 shrink items-center gap-1 overflow-x-auto py-0.5 text-xs text-[var(--g5-text1)]"
    >
      {segments.map((seg, i) => (
        <React.Fragment key={i}>
          {i > 0 && (
            <ChevronRight className="w-3 h-3 text-[var(--g5-text2)] shrink-0" />
          )}
          <span
            className={classNames(
              "shrink-0",
              i === segments.length - 1
                ? "text-[var(--g5-text0)] font-medium"
                : "text-[var(--g5-text1)] hover:text-[var(--g5-text0)] cursor-pointer",
            )}
          >
            {seg}
          </span>
        </React.Fragment>
      ))}
    </div>
  );
};

// ---------------------------------------------------------------------------
// Minimap — Small code overview on the right edge
// ---------------------------------------------------------------------------

interface MinimapProps {
  lines: string[];
  totalHeight: number;
  scrollTop: number;
  visibleHeight: number;
  onScrollTo: (scrollTop: number) => void;
}

const Minimap: React.FC<MinimapProps> = ({
  lines,
  totalHeight,
  scrollTop,
  visibleHeight,
  onScrollTo,
}) => {
  const minimapHeight = 200;
  const scale = totalHeight > 0 ? minimapHeight / totalHeight : 1;
  const viewportTop = scrollTop * scale;
  const viewportHeight = Math.max(visibleHeight * scale, 10);
  const draggingRef = useRef(false);
  const minimapRef = useRef<HTMLDivElement>(null);

  /** Convert a minimap Y-coordinate to a scroll position, centering the viewport */
  const yToScroll = useCallback(
    (clientY: number) => {
      const rect = minimapRef.current?.getBoundingClientRect();
      if (!rect) return;
      const y = clientY - rect.top;
      // Map minimap y to content scroll position, centering viewport
      const maxScroll = totalHeight - visibleHeight;
      if (maxScroll <= 0) return;
      const ratio = y / minimapHeight;
      const targetScroll = Math.max(0, Math.min(maxScroll, ratio * maxScroll));
      onScrollTo(targetScroll);
    },
    [totalHeight, visibleHeight, onScrollTo, minimapHeight],
  );

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      e.preventDefault();
      draggingRef.current = true;
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
      document.body.style.userSelect = "none";
      yToScroll(e.clientY);
    },
    [yToScroll],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!draggingRef.current) return;
      yToScroll(e.clientY);
    },
    [yToScroll],
  );

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    draggingRef.current = false;
    (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    document.body.style.userSelect = "";
  }, []);

  return (
    <div
      ref={minimapRef}
      className={classNames(
        "w-[60px] shrink-0 relative cursor-pointer",
        "bg-[var(--g5-bg0)] border-l border-[var(--g5-stroke)]",
        "overflow-hidden",
      )}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    >
      {/* Minimap lines (simplified) */}
      <div className="pt-1 px-1" style={{ height: minimapHeight }}>
        {lines.slice(0, 200).map((line, i) => (
          <div
            key={i}
            className="h-[1px] mb-[0.5px]"
            style={{
              width: `${Math.min(line.length * 0.5, 50)}px`,
              backgroundColor: "var(--g5-text2)",
              opacity: 0.3,
            }}
          />
        ))}
      </div>

      {/* Viewport indicator */}
      <div
        className="absolute left-0 right-0 bg-[var(--g5-info)] opacity-10 rounded-sm pointer-events-none"
        style={{
          top: viewportTop,
          height: viewportHeight,
        }}
      />
    </div>
  );
};

// ---------------------------------------------------------------------------
// CodeEditor Component
// ---------------------------------------------------------------------------

export const CodeEditor: React.FC<CodeEditorProps> = ({
  filePath,
  content,
  language,
  editable,
  contentHash = null,
  source,
  repoOid,
  snapshotAt,
  observedVmId = null,
  onSave,
  onReload,
  showComments,
  onToggleComments,
  comments,
  commentDraftKey,
  onAddComment,
  onResolveComment,
  onUnresolveComment,
  onReplyToComment,
  onDeleteComment,
  commentsNotice = null,
  scrollToIntentRecord,
  scrollToIntentRecordSeq,
  scrollToLine,
  scrollToLineSeq,
  navigationOrigin,
  onNavigateBack,
  suggestionOverlay: publishedSuggestionOverlay = null,
  changedLines,
  draft = null,
  sessionKey,
  preview,
  onFetchCurrent,
  locked = false,
  onBeforeSave,
  pendingEdit = false,
  saveRequest = 0,
  saveAvailable = false,
  headerActions,
  notices,
}) => {
  const [showMinimap, setShowMinimap] = useState(false);
  // Plain text wraps at the 8-inch measure, at the column count the mounted
  // font gives (`g5.documents.text-measure`); HTML and JSONL do not wrap.
  const wrapsText = wrapsAtTextMeasure(language, filePath);
  const [wrapColumn, setWrapColumn] = useState<number | null>(null);
  // A read-only editor says why, with the header's Start when the owner is
  // only waiting on the VM (`g5.workspace.ui.unavailable`); Save then stays
  // visible, disabled, with the same reason.
  const liveGate = useWorkspaceLiveGate();
  const saveWaiting =
    !editable && liveGate !== null && liveGateIsWaiting(liveGate.kind);
  const saveWaitingReason =
    saveWaiting && liveGate
      ? liveGateReason(liveGate.kind, "save", {
          workspaceNoun: liveGate.workspaceNoun,
        })
      : null;
  // Save acts on an editable buffer, or on the undecided draft a read-only
  // buffer mirrors after Done.
  const canSave = editable || saveAvailable;
  // The viewer rows the copy changed (Monaco marks its own lines).
  const changedLineSet = useMemo(() => {
    const set = new Set<number>();
    for (const range of changedLines ?? []) {
      for (let line = range.start; line <= range.end; line += 1) set.add(line);
    }
    return set;
  }, [changedLines]);
  const [hoveredLine, setHoveredLine] = useState<number | null>(null);
  const [cursorLine, setCursorLine] = useState(1);
  const [cursorCol, setCursorCol] = useState(1);
  const [expandedThreadLines, setExpandedThreadLines] = useState<Set<number>>(
    new Set(),
  );
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  /** The comment margin: its own scroll container, listing the threads. */
  const marginRef = useRef<HTMLDivElement | null>(null);
  // Once shown, the margin stays mounted while hidden: a reply or draft being
  // typed or sent, and its error, are there when it is shown again.
  const [marginMounted, setMarginMounted] = useState(showComments);
  if (showComments && !marginMounted) setMarginMounted(true);
  const [visibleHeight, setVisibleHeight] = useState(0);
  const themeMode = useThemeMode();
  const codeEditorRef = useRef<HTMLDivElement>(null);

  // Line highlighted by scroll-to-intent navigation (1-based)
  const [highlightedLine, setHighlightedLine] = useState<number | null>(null);
  const [backButtonLine, setBackButtonLine] = useState<number | null>(null);

  // A new-thread draft: the line chosen in the text on screen, so the
  // thread is anchored there even if the text changes before it is sent.
  // The draft belongs to the page's owner under the file's subject, shared
  // with the rendered document's list (`g5.documents.comments.drafts`).
  const draftSubject = commentDraftSubject(commentDraftKey ?? filePath);
  const commentDraft = useCommentDraft(draftSubject);
  const openCommentDraft = commentDraft.open;
  /** The text on screen, for handlers defined before it is derived. */
  const displayedTextRef = useRef(content);
  const openDraft = useCallback(
    (lineNumber: number, quotedText: string) =>
      openCommentDraft(lineNumber, quotedText, displayedTextRef.current),
    [openCommentDraft],
  );

  /**
   * FILES Monaco's glyph-margin Add comment: opens a draft on the clicked
   * line when the viewer may start a thread, and does nothing otherwise.
   */
  const addCommentRef = useRef<((lineNumber: number) => void) | null>(null);
  /** Monaco's context key enabling its Add Comment action. */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const canCommentKey = useRef<any>(null);
  useEffect(() => {
    canCommentKey.current?.set(!!onAddComment);
  }, [onAddComment]);
  addCommentRef.current = onAddComment
    ? (lineNumber: number) => {
        openDraft(lineNumber, "");
        if (!showComments && onToggleComments) onToggleComments();
      }
    : null;
  /** The row right-clicked to open the context menu, when it was one. */
  const contextMenuLineRef = useRef<number | null>(null);

  // Right-click context menu state
  const [contextMenu, setContextMenu] = useState<{
    visible: boolean;
    position: { top: number; left: number };
  }>({ visible: false, position: { top: 0, left: 0 } });
  const contextMenuRef = useRef<HTMLDivElement>(null);
  const savedSelectionRef = useRef<Range | null>(null);

  // Selection popover state — comment on highlighted code
  const [selectionPopover, setSelectionPopover] = useState<{
    visible: boolean;
    position: { top: number; left: number };
    text: string;
    /** Monaco's selection start line; the HTML viewer finds its row instead. */
    line?: number;
  }>({ visible: false, position: { top: 0, left: 0 }, text: "" });

  // Monaco draws its own selection, which the browser's selection does not
  // see: its Comment button, quote and line come from Monaco's model.
  const showMonacoSelection = useRef<() => void>(() => undefined);
  showMonacoSelection.current = () => {
    const editor = monacoEditorRef.current;
    const node: HTMLElement | null | undefined = editor?.getDomNode?.();
    const selection = editor?.getSelection?.();
    const text =
      onAddComment && selection && !selection.isEmpty()
        ? (editor.getModel()?.getValueInRange(selection) ?? "").trim()
        : "";
    const at =
      text.length >= 2 && node
        ? editor.getScrolledVisiblePosition?.(selection.getStartPosition())
        : null;
    if (!at || !node) {
      setSelectionPopover((prev) =>
        prev.visible ? { ...prev, visible: false } : prev,
      );
      return;
    }
    // Centred over the selection's box, as for a selection in the HTML
    // viewer: between its two ends on one line, across the lines' full width
    // when it spans several.
    const x = (lineNumber: number, column: number) =>
      editor.getScrolledVisiblePosition?.({ lineNumber, column })?.left as
        | number
        | undefined;
    const model = editor.getModel();
    const ends: number[] = [];
    let left = at.left;
    if (selection.startLineNumber === selection.endLineNumber) {
      ends.push(x(selection.endLineNumber, selection.endColumn) ?? at.left);
    } else {
      left = x(selection.startLineNumber, 1) ?? at.left;
      for (
        let line = selection.startLineNumber;
        line <= selection.endLineNumber;
        line++
      ) {
        const end =
          line === selection.endLineNumber
            ? x(line, selection.endColumn)
            : x(line, model?.getLineMaxColumn(line) ?? 1);
        if (end !== undefined) ends.push(end);
      }
    }
    const right = Math.max(at.left, ...ends);
    const rect = node.getBoundingClientRect();
    setSelectionPopover({
      visible: true,
      position: {
        top: rect.top + at.top,
        left: rect.left + (left + right) / 2,
      },
      text,
      line: selection.startLineNumber,
    });
  };
  // Monaco's context menu Add Comment quotes Monaco's selection and anchors
  // to its first line, or to the right-clicked line without one.
  const commentOnMonacoSelection = useRef<() => void>(() => undefined);

  const handleCodeMouseUp = useCallback(() => {
    if (!onAddComment || monacoEditorRef.current) return;
    setTimeout(() => {
      const sel = window.getSelection();
      if (!sel || sel.isCollapsed || !sel.rangeCount) {
        setSelectionPopover((prev) =>
          prev.visible ? { ...prev, visible: false } : prev,
        );
        return;
      }
      const text = sel.toString().trim();
      if (!text || text.length < 2) {
        setSelectionPopover((prev) =>
          prev.visible ? { ...prev, visible: false } : prev,
        );
        return;
      }
      const range = sel.getRangeAt(0);
      if (
        codeEditorRef.current &&
        !codeEditorRef.current.contains(range.commonAncestorContainer)
      ) {
        return;
      }
      const rect = range.getBoundingClientRect();
      setSelectionPopover({
        visible: true,
        position: { top: rect.top, left: rect.left + rect.width / 2 },
        text,
      });
    }, 10);
  }, [onAddComment]);

  const handleSelectionComment = useCallback(
    (quotedText: string, line?: number) => {
      // The selection's first row is the line chosen; the caret is only a
      // fallback (the read-only viewer has no caret of its own).
      const sel = window.getSelection();
      const lineNum =
        line ??
        (sel && sel.rangeCount > 0
          ? lineNumberOf(sel.getRangeAt(0).startContainer)
          : null) ??
        cursorLine;
      // Open a draft comment input instead of immediately sending
      openDraft(lineNum, quotedText);
      // Ensure comment column is visible
      if (!showComments && onToggleComments) onToggleComments();
      setSelectionPopover({
        visible: false,
        position: { top: 0, left: 0 },
        text: "",
      });
    },
    [cursorLine, openDraft, showComments, onToggleComments],
  );
  commentOnMonacoSelection.current = () => {
    const editor = monacoEditorRef.current;
    const selection = editor?.getSelection?.();
    if (!onAddComment || !selection) return;
    const text = selection.isEmpty()
      ? ""
      : (editor.getModel()?.getValueInRange(selection) ?? "").trim();
    handleSelectionComment(text, selection.startLineNumber);
  };

  // Right-click context menu handler
  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      // The menu's only item starts a thread: without that permission the
      // browser's own menu stays. Monaco offers Add Comment in its own menu.
      if (!onAddComment || (e.target as Element).closest?.(".monaco-editor"))
        return;
      e.preventDefault();
      contextMenuLineRef.current = lineNumberOf(e.target as Node);
      // Save the current selection so we can restore it when commenting
      const sel = window.getSelection();
      savedSelectionRef.current =
        sel && sel.rangeCount > 0 ? sel.getRangeAt(0).cloneRange() : null;
      setContextMenu({
        visible: true,
        position: { top: e.clientY, left: e.clientX },
      });
    },
    [onAddComment],
  );

  // Close context menu on outside click
  useEffect(() => {
    if (!contextMenu.visible) return;
    const handleClick = (e: MouseEvent) => {
      if (
        contextMenuRef.current &&
        !contextMenuRef.current.contains(e.target as Node)
      ) {
        setContextMenu({ visible: false, position: { top: 0, left: 0 } });
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [contextMenu.visible]);

  // Handle context menu action
  const handleContextAction = useCallback(
    (action: string) => {
      setContextMenu({ visible: false, position: { top: 0, left: 0 } });
      if (action === "add-comment") {
        let quotedText = "";
        const saved = savedSelectionRef.current;
        if (saved) {
          const sel = window.getSelection();
          sel?.removeAllRanges();
          sel?.addRange(saved);
          const selected = sel?.toString().trim();
          if (selected) quotedText = selected;
        }
        // The right-clicked row, else the caret (Monaco moves its caret to
        // a right-click).
        openDraft(contextMenuLineRef.current ?? cursorLine, quotedText);
        if (!showComments && onToggleComments) onToggleComments();
      }
    },
    [cursorLine, openDraft, showComments, onToggleComments],
  );

  // ---------------------------------------------------------------------------
  // Local editor model and explicit Save
  // ---------------------------------------------------------------------------

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [MonacoEditor, setMonacoEditor] =
    useState<React.ComponentType<any> | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const monacoEditorRef = useRef<any>(null);
  const [editorMountVersion, setEditorMountVersion] = useState(0);
  // FILES: a draft from the tab's store replaces the read bytes as this
  // editor's buffer and baseline (`g5.documents.files-view.drafts`).
  const [restoredDraft] = useState(() =>
    draft ? getFileDraft(draft.key) : null,
  );
  const baselineRef = useRef(
    restoredDraft
      ? {
          filePath,
          content: restoredDraft.baseline.content,
          hash: restoredDraft.baseline.hash,
        }
      : { filePath, content, hash: contentHash },
  );
  const baselineVmRef = useRef(
    restoredDraft ? restoredDraft.baseline.vmId : observedVmId,
  );
  const draftIdRef = useRef<string | null>(restoredDraft?.id ?? null);
  /** The draft this editor last wrote to the store (or restored from it). */
  const writtenDraftRef = useRef<FileDraft | null>(restoredDraft);
  /** Another writer's draft the buffer adopts on its next render. */
  const adoptingDraftRef = useRef<FileDraft | null>(null);
  /** Bumped when the mirror adopts a stored baseline, to render it. */
  const [, setBaselineVersion] = useState(0);
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);
  const lastExternalVmRef = useRef(observedVmId);
  const lastExternalSnapshotRef = useRef({
    filePath,
    content,
    hash: contentHash,
  });
  // A restored draft whose file changed meanwhile starts with that change
  // pending, like an edit made while the file changed under it.
  const restoredOverChange =
    restoredDraft !== null &&
    (restoredDraft.baseline.hash !== contentHash ||
      restoredDraft.baseline.vmId !== observedVmId);
  /** FILES: whether this session's Save was in flight at the last refresh. */
  const savePendingRef = useRef(false);
  const deferredExternalSnapshotRef = useRef<typeof baselineRef.current | null>(
    restoredOverChange ? { filePath, content, hash: contentHash } : null,
  );
  const [localContent, setLocalContent] = useState(
    restoredDraft ? restoredDraft.content : content,
  );
  /** FILES: the newer version that arrived under unsaved edits. */
  const [diskChange, setDiskChange] = useState<{
    content: string;
    hash: string | null;
    vmId: string | null;
  } | null>(() =>
    // While the draft's Save is in flight the file may be that Save landing:
    // the version waits (deferred) until the Save settles.
    restoredOverChange && !restoredDraft?.saving
      ? { content, hash: contentHash, vmId: observedVmId }
      : null,
  );
  /** FILES: the Compare dialog and what it compares the buffer with. */
  const [compare, setCompare] = useState<{
    newerContent: string;
    newerHash: string | null;
    newerVmId: string | null;
    description: string;
  } | null>(null);
  const [saveState, setSaveState] = useState<
    "idle" | "saving" | "saved" | "error"
  >("idle");
  const [saveError, setSaveError] = useState<string | null>(
    restoredDraft?.saveError ?? null,
  );
  const [conflict, setConflict] = useState<{
    currentHash: string;
    currentContent?: string;
  } | null>(restoredDraft?.saveConflict ?? null);

  const isDirty = localContent !== baselineRef.current.content;

  // While dirty, the editor is on the page's list of unsaved editors, with
  // a way to drop its edits: a review copy's Discard rewrites files, and an
  // editor that kept stale edits would later save over them with an old
  // hash. Dropping returns the editor to its baseline, so the next external
  // snapshot replaces it like any clean editor's.
  const usesDraftStore = draft !== null;
  // This editor's own mark: clearing it never unlists another holder's
  // edits on the same path (FILES drafts the page lists, another repository).
  const [dirtyOwner] = useState(() => ({}));
  useEffect(() => {
    // A FILES draft is listed by the store's owner (the workspace page),
    // which outlives this editor.
    if (usesDraftStore) return;
    if (!isDirty) {
      setEditorDirty(dirtyOwner, filePath, null);
      return;
    }
    setEditorDirty(dirtyOwner, filePath, () => {
      setLocalContent(baselineRef.current.content);
    });
    return () => setEditorDirty(dirtyOwner, filePath, null);
  }, [dirtyOwner, filePath, isDirty, usesDraftStore]);

  // FILES: mirror the buffer into the tab's draft store while it differs
  // from its baseline. A draft changed there by someone else is adopted: a
  // merge from Compare, or a drop (a review copy's Discard, Discard, a new
  // principal), which drops these edits rather than writing them back.
  const storedDraft = useFileDraft(draft?.key ?? null);
  // After every render: the baseline lives in refs that Compare and Save
  // move without changing any state this could list. Each state update
  // below is guarded by a ref it updates first, so it cannot loop.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => {
    const written = writtenDraftRef.current;
    if (adoptingDraftRef.current?.content === localContent)
      adoptingDraftRef.current = null;
    // The draft ended (dropped, or pushed) or this editor stopped mirroring
    // it (FILES left Edit): the buffer becomes the file as it is now, so no
    // stale edits stay behind to look unsaved or to edit from later.
    const showCurrentFile = () => {
      writtenDraftRef.current = null;
      adoptingDraftRef.current = null;
      draftIdRef.current = null;
      deferredExternalSnapshotRef.current = null;
      baselineRef.current = { filePath, content, hash: contentHash };
      baselineVmRef.current = observedVmId;
      setLocalContent(content);
      setDiskChange(null);
      setConflict(null);
      setCompare(null);
      setSaveError(null);
      setSaveState("idle");
    };
    if (!draft) {
      // Leaving Edit ends the session even when the buffer is clean, so a
      // Save still in flight cannot reopen it.
      if (written || draftIdRef.current) showCurrentFile();
      return;
    }
    if (storedDraft !== written) {
      if (!storedDraft) {
        showCurrentFile();
        return;
      }
      setSaveError(storedDraft.saveError ?? null);
      setConflict(storedDraft.saveConflict ?? null);
      writtenDraftRef.current = storedDraft;
      draftIdRef.current = storedDraft.id;
      const before = baselineRef.current;
      baselineRef.current = {
        filePath,
        content: storedDraft.baseline.content,
        hash: storedDraft.baseline.hash,
      };
      baselineVmRef.current = storedDraft.baseline.vmId;
      // The baseline lives in a ref: render again so what is unsaved, and
      // Save, follow it.
      if (
        before.content !== storedDraft.baseline.content ||
        before.hash !== storedDraft.baseline.hash
      ) {
        setBaselineVersion((version) => version + 1);
      }
      if (
        storedDraft.id !== written?.id ||
        storedDraft.content !== written?.content
      ) {
        // Someone else's text (a merge from Compare, another draft): show it.
        adoptingDraftRef.current = storedDraft;
        setLocalContent(storedDraft.content);
        return;
      }
      // Only its baseline or flags moved (a Save settling, a push starting):
      // the buffer, which may hold newer typing, stays.
    }
    const current = writtenDraftRef.current;
    const ownStored =
      storedDraft && storedDraft.id === draftIdRef.current ? storedDraft : null;
    const saving = ownStored?.saving ?? 0;
    if (localContent === baselineRef.current.content && saving === 0) {
      if (current) {
        // Clean again (saved, or typed back): the session goes on, so a
        // Save still in flight acknowledges into it.
        writtenDraftRef.current = null;
        if (getFileDraft(draft.key) === current) setFileDraft(draft.key, null);
      }
      return;
    }
    const id = draftIdRef.current ?? newFileDraftId();
    draftIdRef.current = id;
    const baseline = baselineRef.current;
    const next: FileDraft = {
      ...createFileDraft(
        draft.role,
        localContent,
        {
          content: baseline.content,
          hash: baseline.hash ?? null,
          vmId: baselineVmRef.current ?? null,
        },
        id,
      ),
      ...(ownStored?.saveError ? { saveError: ownStored.saveError } : {}),
      ...(saving > 0 ? { saving } : {}),
      ...(ownStored?.pushing ? { pushing: true } : {}),
    };
    if (
      !current ||
      current.id !== next.id ||
      current.role !== next.role ||
      current.content !== next.content ||
      current.baseline.content !== next.baseline.content ||
      current.baseline.hash !== next.baseline.hash ||
      current.baseline.vmId !== next.baseline.vmId ||
      (current.saving ?? 0) !== saving ||
      Boolean(current.pushing) !== Boolean(next.pushing)
    ) {
      writtenDraftRef.current = next;
      setFileDraft(draft.key, next);
    }
  });

  // Refresh a clean editor when its authorized source changes. If the user has
  // unsaved edits, keep them in memory so demotion or a branch advance cannot
  // destroy text they may need to copy elsewhere.
  useEffect(() => {
    const lastExternal = lastExternalSnapshotRef.current;
    const externalChanged =
      lastExternal.filePath !== filePath ||
      lastExternal.content !== content ||
      lastExternal.hash !== contentHash;
    const vmChanged = lastExternalVmRef.current !== observedVmId;
    if (vmChanged) lastExternalVmRef.current = observedVmId;
    if (externalChanged || vmChanged) {
      lastExternalSnapshotRef.current = {
        filePath,
        content,
        hash: contentHash,
      };
    }
    const previous = baselineRef.current;
    const changedFile = previous.filePath !== filePath;
    // FILES: while a Save of this session is in flight the file is about to
    // change under the buffer (often to the saved text itself), so the
    // buffer is kept and the change waits until the Save settles.
    const savePending =
      !!storedDraft &&
      storedDraft.id === draftIdRef.current &&
      (storedDraft.saving ?? 0) > 0;
    const mayReplace =
      changedFile || (localContent === previous.content && !savePending);
    // Once that Save settles, a version that arrived meanwhile and is not
    // the one it left behind (another writer's, or anything after a refusal)
    // is a change on disk after all: a clean buffer takes it, an edited one
    // is told. The Save's own landing is no change.
    const settled = savePendingRef.current && !savePending;
    savePendingRef.current = savePending;
    const arrived = settled ? deferredExternalSnapshotRef.current : null;
    // A successful Save superseded the version it replaced.
    const lastSave = settled ? storedDraft?.lastSave : undefined;
    const superseded = (hash: string | null) =>
      !!lastSave?.saved && hash !== null && hash === lastSave.over;
    const changedAfterSave =
      !!arrived &&
      arrived.filePath === filePath &&
      (arrived.content !== previous.content ||
        arrived.hash !== previous.hash) &&
      !superseded(arrived.hash);
    if (arrived && !changedAfterSave) {
      deferredExternalSnapshotRef.current = null;
      // A notice about that version, or one the Save superseded, is over.
      setDiskChange((shown) =>
        shown && (shown.hash === arrived.hash || superseded(shown.hash))
          ? null
          : shown,
      );
    }
    if (!mayReplace) {
      if (externalChanged || vmChanged) {
        deferredExternalSnapshotRef.current = {
          filePath,
          content,
          hash: contentHash,
        };
        const differs =
          content !== previous.content ||
          contentHash !== previous.hash ||
          baselineVmRef.current !== observedVmId;
        if (!savePending && differs) {
          setDiskChange({ content, hash: contentHash, vmId: observedVmId });
        } else if (!differs) {
          // The file is the edits' baseline again: nothing changed on disk.
          setDiskChange(null);
        }
      } else if (changedAfterSave && arrived) {
        setDiskChange({
          content: arrived.content,
          hash: arrived.hash,
          vmId: observedVmId,
        });
      }
      return;
    }
    const deferred = deferredExternalSnapshotRef.current;
    if (
      !externalChanged &&
      !vmChanged &&
      (!deferred ||
        deferred.filePath !== filePath ||
        deferred.content !== content ||
        deferred.hash !== contentHash)
    ) {
      return;
    }
    deferredExternalSnapshotRef.current = null;
    baselineRef.current = { filePath, content, hash: contentHash };
    baselineVmRef.current = observedVmId;
    setLocalContent(content);
    setConflict(null);
    setDiskChange(null);
    setSaveError(null);
    setSaveState("idle");
  }, [content, contentHash, filePath, localContent, observedVmId, storedDraft]);

  const persist = useCallback(
    async (expectedHash: string | null, written?: string) => {
      if (
        !onSave ||
        !canSave ||
        locked ||
        (draft && (getFileDraft(draft.key)?.saving ?? 0) > 0)
      )
        return;
      setSaveState("saving");
      setSaveError(null);
      // FILES: text the pre-save hook just wrote to the shared draft is
      // submitted as is; this editor adopts it into its buffer afterwards.
      const submitted = written ?? localContent;
      const submittedVmId = baselineVmRef.current;
      // FILES: the editing session this Save belongs to. It ends when the
      // view closes or its draft is discarded, dropped or replaced; a Save
      // completing after that must not touch what the editor shows now.
      // FILES: the session starts here at the latest, even before the draft
      // mirror first ran (a Save right after the first keystroke).
      if (draft && !draftIdRef.current) draftIdRef.current = newFileDraftId();
      const session = draft ? { key: draft.key, id: draftIdRef.current } : null;
      const sessionEnded = () =>
        session !== null &&
        (!mountedRef.current || draftIdRef.current !== session.id);
      const settleShared =
        draft && session?.id
          ? beginFileSave(
              session.key,
              createFileDraft(
                draft.role,
                submitted,
                {
                  content: baselineRef.current.content,
                  hash: baselineRef.current.hash ?? null,
                  vmId: submittedVmId ?? null,
                },
                session.id,
              ),
              expectedHash,
            )
          : null;
      if (draft && !settleShared) {
        setSaveState("idle");
        return;
      }
      const settle = (
        savedBaseline: FileDraftBaseline | null,
        error?: string,
        conflict?: FileDraft["saveConflict"],
      ) => settleShared?.(savedBaseline, error, conflict, sessionEnded());
      try {
        const saved = await onSave(submitted, expectedHash, submittedVmId);
        const ended = sessionEnded();
        settle({
          content: submitted,
          hash: saved.hash,
          vmId: submittedVmId ?? null,
        });
        if (ended) return;
        baselineRef.current = {
          filePath,
          content: submitted,
          hash: saved.hash,
        };
        // FILES: a version that arrived during the Save is judged once the
        // Save has settled (superseded, its own landing, or a change).
        if (!session) deferredExternalSnapshotRef.current = null;
        setConflict(null);
        setDiskChange(null);
        setSaveState("saved");
      } catch (error) {
        const ended = sessionEnded();
        const savedConflict = fileVersionConflict(error);
        settle(
          null,
          error instanceof Error ? error.message : String(error),
          savedConflict,
        );
        // A refused Save of an ended session has nothing left to resolve.
        if (ended) return;
        if (savedConflict) {
          setConflict(savedConflict);
          setSaveState("error");
          return;
        }
        setSaveError(error instanceof Error ? error.message : String(error));
        setSaveState("error");
      }
    },
    [canSave, draft, filePath, localContent, locked, onSave],
  );

  const handleSave = useCallback(async () => {
    if (saveState === "saving") return;
    if (onBeforeSave?.() === false) return;
    const after = draft ? getFileDraft(draft.key) : null;
    // Another writer's text in this session's draft that the buffer does not
    // show yet (Rich text's last edits, written on blur or by the hook just
    // now): submit that text rather than the buffer, which adopts it later.
    let written: string | undefined;
    if (after && after === adoptingDraftRef.current) written = after.content;
    else if (after && after !== writtenDraftRef.current) {
      if (!writtenDraftRef.current && after.id !== draftIdRef.current) {
        // A clean buffer had no draft; the other writer started one. Adopt
        // its id and baseline now so Save reserves this draft and settles
        // against it. The mirror still sees a draft it did not write, so the
        // next render adopts the text into the buffer instead of overwriting it.
        draftIdRef.current = after.id;
        baselineRef.current = {
          filePath,
          content: after.baseline.content,
          hash: after.baseline.hash,
        };
        baselineVmRef.current = after.baseline.vmId;
        setBaselineVersion((version) => version + 1);
      }
      if (after.id === draftIdRef.current) written = after.content;
    }
    if (
      written === undefined ? !isDirty : written === baselineRef.current.content
    )
      return;
    await persist(baselineRef.current.hash, written);
  }, [draft, filePath, isDirty, onBeforeSave, persist, saveState]);
  const saveRef = useRef(handleSave);
  useEffect(() => {
    saveRef.current = handleSave;
  }, [handleSave]);
  // A Save asked for from outside the toolbar runs the same path as the
  // button, after the handler above caught up with this render.
  useEffect(() => {
    if (saveRequest > 0) void saveRef.current();
  }, [saveRequest]);

  const handleReload = useCallback(async () => {
    if (!onReload) return;
    setSaveState("saving");
    setSaveError(null);
    try {
      const current = await onReload();
      baselineRef.current = {
        filePath,
        content: current.content,
        hash: current.hash,
      };
      baselineVmRef.current = current.vmId ?? observedVmId;
      deferredExternalSnapshotRef.current = null;
      setLocalContent(current.content);
      setConflict(null);
      setSaveState("idle");
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : String(error));
      setSaveState("error");
    }
  }, [filePath, observedVmId, onReload]);

  /**
   * Make `newer` the baseline (the version Save now compares with) and
   * `text` the buffer: Reload passes the newer content itself, Compare the
   * person's merged text.
   */
  const adoptNewerVersion = useCallback(
    (
      newer: { content: string; hash: string | null; vmId: string | null },
      text: string,
    ) => {
      baselineRef.current = {
        filePath,
        content: newer.content,
        hash: newer.hash,
      };
      baselineVmRef.current = newer.vmId ?? baselineVmRef.current;
      deferredExternalSnapshotRef.current = null;
      setLocalContent(text);
      setConflict(null);
      setDiskChange(null);
      setCompare(null);
      setSaveError(null);
      setSaveState("idle");
    },
    [filePath],
  );

  const openConflictCompare = useCallback(async () => {
    if (!conflict) return;
    const description =
      "Someone saved a newer version after you opened the file. Merge what you need into your text, then Save.";
    if (typeof conflict.currentContent === "string") {
      setCompare({
        newerContent: conflict.currentContent,
        newerHash: conflict.currentHash,
        newerVmId: baselineVmRef.current,
        description,
      });
      return;
    }
    if (!onFetchCurrent) return;
    try {
      const current = await onFetchCurrent();
      setCompare({
        newerContent: current.content,
        newerHash: current.hash,
        newerVmId: current.vmId ?? baselineVmRef.current,
        description,
      });
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : String(error));
    }
  }, [conflict, onFetchCurrent]);

  useEffect(() => {
    let cancelled = false;
    // The bundled Monaco (`g5.documents.editor.assets`), loaded once.
    loadMonacoReact()
      .then((mod) => {
        if (!cancelled) setMonacoEditor(() => mod.default);
      })
      .catch((err) => {
        console.warn(
          "[CodeEditor] Monaco dynamic import failed — staying with the textarea editor:",
          err,
        );
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(
    () => () => {
      if (sessionKey && monacoEditorRef.current) {
        saveFileModelViewState(
          sessionKey,
          monacoEditorRef.current.saveViewState?.() ?? null,
        );
      }
    },
    [sessionKey],
  );

  const handleEditorMount = useCallback(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (editor: any, monaco: any) => {
      monacoEditorRef.current = editor;
      if (sessionKey && editor.getModel()) {
        retainFileModel(sessionKey, editor.getModel());
        const state = fileModelViewState(sessionKey);
        if (state) editor.restoreViewState(state);
      }
      setEditorMountVersion((v) => v + 1);
      defineG5Themes(monaco);
      monaco.editor.setTheme(themeMode === "light" ? "g5-light" : "g5-dark");
      // The wrap column follows the measured font, including after a zoom.
      const fontInfoOption = monaco.editor.EditorOption?.fontInfo;
      const measureWrapColumn = () => {
        const font =
          fontInfoOption === undefined
            ? undefined
            : editor.getOption?.(fontInfoOption);
        setWrapColumn(textMeasureColumns(font?.typicalHalfwidthCharacterWidth));
      };
      measureWrapColumn();
      editor.onDidChangeConfiguration?.(
        (event: { hasChanged: (option: number) => boolean }) => {
          if (fontInfoOption !== undefined && event.hasChanged(fontInfoOption))
            measureWrapColumn();
        },
      );
      editor.addCommand(
        monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS,
        () => void saveRef.current(),
      );
      // Add Comment starts on the caret's line, so it follows Monaco's caret.
      editor.onDidChangeCursorPosition?.(
        (event: { position: { lineNumber: number; column: number } }) => {
          setCursorLine(event.position.lineNumber);
          setCursorCol(event.position.column);
        },
      );
      // Scrolling moves the text away from the selection's Comment button.
      editor.onDidScrollChange?.(() =>
        setSelectionPopover((prev) =>
          prev.visible ? { ...prev, visible: false } : prev,
        ),
      );
      // Selecting with the mouse offers Comment on Monaco's own selection.
      editor.onMouseUp?.(() =>
        setTimeout(() => showMonacoSelection.current(), 0),
      );
      canCommentKey.current = editor.createContextKey?.(
        "g5CanComment",
        !!addCommentRef.current,
      );
      editor.addAction?.({
        id: "g5.addComment",
        label: "Add Comment",
        contextMenuGroupId: "navigation",
        contextMenuOrder: 0,
        precondition: "g5CanComment",
        run: () => commentOnMonacoSelection.current(),
      });
      // The viewer's per-line Add comment control, in Monaco's glyph
      // margin: a hovered line shows it, a click opens the draft there.
      const hover = editor.createDecorationsCollection?.();
      editor.onMouseMove?.(
        (event: { target: { position?: { lineNumber: number } | null } }) => {
          const line = event.target.position?.lineNumber;
          if (!hover) return;
          if (!line || !addCommentRef.current) {
            hover.clear();
            return;
          }
          hover.set([
            {
              range: new monaco.Range(line, 1, line, 1),
              options: {
                glyphMarginClassName: "g5-monaco-add-comment",
                glyphMarginHoverMessage: { value: "Add comment" },
              },
            },
          ]);
        },
      );
      editor.onMouseLeave?.(() => hover?.clear());
      editor.onMouseDown?.(
        (event: {
          target: {
            type: number;
            position?: { lineNumber: number } | null;
          };
        }) => {
          if (
            event.target.type !==
              monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN ||
            !event.target.position ||
            !addCommentRef.current
          ) {
            return;
          }
          addCommentRef.current(event.target.position.lineNumber);
        },
      );
    },
    [themeMode, sessionKey],
  );

  useEffect(() => {
    const editor = monacoEditorRef.current;
    const model = editor?.getModel();
    if (!model) {
      clearCodeEditorDebugRegistry(filePath);
      return;
    }
    const updateModelDebug = () => {
      updateCodeEditorDebugRegistry(filePath, {
        modelValue: model.getValue(),
        modelUri:
          model.uri?.path ??
          (typeof model.uri?.toString === "function"
            ? model.uri.toString()
            : undefined),
      });
    };
    updateModelDebug();
    const disposable = model.onDidChangeContent(updateModelDebug);
    return () => {
      disposable.dispose();
      clearCodeEditorDebugRegistry(filePath);
    };
  }, [editorMountVersion, filePath]);

  const useMonacoEditor = !!MonacoEditor;
  const useEditableFallback = !useMonacoEditor && editable;

  const updateFallbackContent = useCallback((next: string) => {
    setLocalContent(next);
    setSaveState("idle");
    setSaveError(null);
  }, []);

  const updateCursorFromTextarea = useCallback((el: HTMLTextAreaElement) => {
    const pos = el.selectionStart ?? 0;
    const before = el.value.slice(0, pos);
    const line = before.split("\n").length;
    const lastNl = before.lastIndexOf("\n");
    const col = pos - (lastNl + 1) + 1;
    setCursorLine(line);
    setCursorCol(col);
  }, []);

  const handleFallbackTextareaEvent = useCallback(
    (e: React.SyntheticEvent<HTMLTextAreaElement>) => {
      updateCursorFromTextarea(e.currentTarget);
    },
    [updateCursorFromTextarea],
  );

  // Re-apply Monaco theme when data-mode changes
  useEffect(() => {
    if (!monacoEditorRef.current) return;
    import("@monaco-editor/react")
      .then((mod) => {
        // Get the monaco namespace from the loader
        mod.loader?.init().then((monaco) => {
          defineG5Themes(monaco);
          monaco.editor.setTheme(
            themeMode === "light" ? "g5-light" : "g5-dark",
          );
        });
      })
      .catch((err) => {
        console.warn("[CodeEditor] Monaco theme re-apply failed:", err);
      });
  }, [editorMountVersion, themeMode]);

  const useReadOnlyViewer = !useMonacoEditor && !useEditableFallback;
  const activeContent = editable || draft ? localContent : content;
  const syncingModel = useRef(false);
  // The React wrapper calls setValue even for equal read-only values, which
  // destroys retained history. A session owns its model; reconcile only real
  // source changes as native edits. Explicit Discard alone clears history.
  useEffect(() => {
    const model = monacoEditorRef.current?.getModel();
    if (
      !sessionKey ||
      !model ||
      model.getValue(undefined, true) === activeContent
    )
      return;
    syncingModel.current = true;
    try {
      model.pushStackElement();
      // Monaco retains a model's EOL across replacement edits. A newly read
      // file may use a different EOL; adopting it is not user typing.
      const eol = activeContent.includes("\r\n") ? "\r\n" : "\n";
      if (model.getEOL() !== eol) model.pushEOL(eol === "\r\n" ? 1 : 0);
      model.pushEditOperations(
        [],
        [{ range: model.getFullModelRange(), text: activeContent }],
        () => null,
      );
      model.pushStackElement();
    } finally {
      syncingModel.current = false;
    }
  }, [sessionKey, activeContent, editorMountVersion]);

  const selectedReviewSource = useReviewSourceDigest(
    filePath,
    activeContent,
    (publishedSuggestionOverlay?.suggestions ?? []).some(
      (s) =>
        s.status === "open" &&
        !!s.reviewEdit &&
        /^[0-9a-f]{64}$/.test(s.contentHash ?? ""),
    ),
    baselineRef.current.filePath === filePath &&
      baselineRef.current.content === activeContent,
  );
  const suggestionOverlay = useMemo(
    () =>
      publishedSuggestionOverlay && !preview
        ? {
            ...publishedSuggestionOverlay,
            selectedSource: selectedReviewSource,
          }
        : null,
    [publishedSuggestionOverlay, selectedReviewSource, preview],
  );

  displayedTextRef.current = activeContent;

  // Threads are placed on the displayed text, unsaved edits included.
  const commentAnchorResolutions = useCommentAnchorResolutions(
    activeContent,
    comments,
  );

  const lines = useMemo(
    () => (useReadOnlyViewer ? activeContent.split("\n") : []),
    [activeContent, useReadOnlyViewer],
  );

  // Lines that have comment threads
  const commentedLines = useMemo(() => {
    const set = new Set<number>();
    for (const c of comments) {
      const resolution = commentAnchorResolutions.get(c.id);
      if (resolution?.status === "mapped") {
        set.add(resolution.lineNumber);
      }
    }
    return set;
  }, [commentAnchorResolutions, comments]);

  // Syntax-highlighted HTML, one string per line (memoized)
  const highlightedLines = useMemo(() => {
    if (!useReadOnlyViewer || !activeContent) return null;
    return splitHighlightedLines(highlightCode(activeContent, language));
  }, [activeContent, language, useReadOnlyViewer]);

  // Review suggestions shown under their lines in the read-only viewer, the
  // way Monaco shows them in view zones. Each zone's rendered height is
  // reported back so the gutter and every line-based position make room.
  const suggestionZones = useMemo(
    () =>
      useReadOnlyViewer && suggestionOverlay
        ? suggestionZonesByLine(
            suggestionOverlay.suggestions,
            activeContent,
            selectedReviewSource?.digest,
          )
        : new Map<number, never[]>(),
    [activeContent, suggestionOverlay, useReadOnlyViewer, selectedReviewSource],
  );
  // Each card lives in a node of its own, rendered once here and adopted by
  // whichever view shows it (Monaco's zones, the fallback's list or the
  // read-only viewer's rows), so a card keeps its state when the text moves
  // it to another line or the editor switches views.
  const suggestionCardNodes = useSuggestionCardNodes();
  const [zoneHeights, setZoneHeights] = useState<Map<number, number>>(
    () => new Map(),
  );
  const reportZoneHeight = useCallback((afterLine: number, height: number) => {
    setZoneHeights((previous) => {
      if ((previous.get(afterLine) ?? 0) === height) return previous;
      const next = new Map(previous);
      if (height > 0) next.set(afterLine, height);
      else next.delete(afterLine);
      return next;
    });
  }, []);
  /** Height of the zones above `line` (zones that follow lines before it). */
  const zoneOffsetBefore = useCallback(
    (line: number) => {
      let total = 0;
      for (const [afterLine, height] of zoneHeights) {
        if (afterLine < line && suggestionZones.has(afterLine)) total += height;
      }
      return total;
    },
    [suggestionZones, zoneHeights],
  );
  const zoneOffsetRef = useRef(zoneOffsetBefore);
  zoneOffsetRef.current = zoneOffsetBefore;
  const totalZoneHeight = zoneOffsetBefore(Number.MAX_SAFE_INTEGER);

  // The margin lists the threads in the text's order, the unplaced last
  // (`g5.documents.comments.identity`).
  const commentThreads = useMemo(
    () =>
      orderBesideView(
        comments.map((comment) => ({
          thread: commentThreadFromDTO(
            comment,
            commentAnchorResolutions.get(comment.id),
          ),
          createdAt: comment.createdAt,
        })),
      ).map(({ thread }) => thread),
    [commentAnchorResolutions, comments],
  );

  // Handle scroll for minimap
  const handleScroll = useCallback(() => {
    if (scrollRef.current) {
      setScrollTop(scrollRef.current.scrollTop);
      setVisibleHeight(scrollRef.current.clientHeight);
    }
  }, []);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) {
      setVisibleHeight(el.clientHeight);
    }
  }, []);

  // Scroll to intent record block when navigated from traceability panel
  useEffect(() => {
    if (!scrollToIntentRecord) return;
    // Use content directly — the `lines` memo is empty in Monaco/editable mode
    const contentLines = content ? content.split("\n") : [];
    if (contentLines.length === 0) return;
    // Search lines for intent label pattern: "// ── I1: ..." or "// I7:" etc.
    const label = scrollToIntentRecord; // e.g. "I1", "D3"
    const lineIdx = contentLines.findIndex((line) => {
      // Match patterns like "// ── I1: ..." or "// I1: ..." or "# [I1. ..."
      const re = new RegExp(
        `\\b${label.replace(/([.*+?^${}()|[\\]\\\\])/g, "\\$1")}[:.\\s]`,
      );
      return re.test(line);
    });
    if (lineIdx < 0) {
      // Intent label not found in code file — still show back button at line 1
      // when we have a navigation origin (e.g., navigated here via trace eye button)
      if (navigationOrigin) {
        setBackButtonLine(1);
      }
      return;
    }
    const lineNum = lineIdx + 1; // 1-based
    // Show persistent back button if we have a navigation origin
    if (navigationOrigin) {
      setBackButtonLine(lineNum);
    }
    const timer = setTimeout(() => {
      // Monaco editor: use its API to scroll
      if (useMonacoEditor && monacoEditorRef.current) {
        monacoEditorRef.current.revealLineInCenter(lineNum);
      }
      // Read-only viewer: use scrollRef
      if (scrollRef.current) {
        const targetTop =
          (lineNum - 1) * LINE_HEIGHT + zoneOffsetRef.current(lineNum) - 80;
        scrollRef.current.scrollTo({
          top: Math.max(0, targetTop),
          behavior: "smooth",
        });
      }
      setHighlightedLine(lineNum);
      // Clear highlight after 3 seconds
      setTimeout(() => setHighlightedLine(null), 3000);
    }, 150);
    return () => clearTimeout(timer);
  }, [
    scrollToIntentRecord,
    scrollToIntentRecordSeq,
    content,
    navigationOrigin,
    useMonacoEditor,
    editorMountVersion,
  ]);

  // Reveal + highlight a specific line when a file tab is opened at a citation
  // target (chat_service D54 / message_rendering D7). Unlike the intent-record
  // effect above this scrolls to a raw line number rather than searching for a
  // label. Re-runs when the target, loaded content or mounted editor changes.
  useEffect(() => {
    if (!scrollToLine || scrollToLine < 1) return;
    const contentLines = content ? content.split("\n") : [];
    if (contentLines.length === 0) return;
    const lineNum = Math.min(scrollToLine, contentLines.length);
    const timer = setTimeout(() => {
      if (useMonacoEditor && monacoEditorRef.current) {
        monacoEditorRef.current.revealLineInCenter(lineNum);
      }
      if (scrollRef.current) {
        const targetTop =
          (lineNum - 1) * LINE_HEIGHT + zoneOffsetRef.current(lineNum) - 80;
        scrollRef.current.scrollTo({
          top: Math.max(0, targetTop),
          behavior: "smooth",
        });
      }
      setHighlightedLine(lineNum);
      setTimeout(() => setHighlightedLine(null), 3000);
    }, 150);
    return () => clearTimeout(timer);
  }, [
    scrollToLine,
    scrollToLineSeq,
    content,
    useMonacoEditor,
    editorMountVersion,
  ]);

  /**
   * Shows `line` as the first line of the text, for a card expanded in the
   * margin (`g5.documents.comments.identity`). The editable fallback wraps
   * its lines and has no row to scroll to.
   */
  const revealLineAtTop = useCallback(
    (line: number) => {
      if (useMonacoEditor) {
        // The row's own offset: Monaco's reveal-at-top keeps room above the
        // row for sticky scroll lines, so the row would not be first.
        const editor = monacoEditorRef.current;
        if (typeof editor?.getTopForLineNumber === "function")
          editor.setScrollTop?.(editor.getTopForLineNumber(line));
      } else if (scrollRef.current) {
        const top = (line - 1) * LINE_HEIGHT + zoneOffsetRef.current(line);
        if (typeof scrollRef.current.scrollTo === "function")
          scrollRef.current.scrollTo({ top, behavior: "smooth" });
        else scrollRef.current.scrollTop = top;
      }
      setHighlightedLine(line);
      setTimeout(() => setHighlightedLine(null), 3000);
    },
    [useMonacoEditor],
  );
  const revealThreadLine = (threadId: string, expanded: boolean) => {
    if (!expanded) return;
    const thread = commentThreads.find((listed) => listed.id === threadId);
    const line = thread ? placedLine(thread) : null;
    if (line !== null) revealLineAtTop(line);
  };

  // Gutter width based on line count
  const gutterWidth = Math.max(String(lines.length).length * 9 + 16, 40);

  const suggestionLines = useMemo(
    () =>
      suggestionsByLine(
        suggestionOverlay?.suggestions ?? [],
        activeContent,
        selectedReviewSource?.digest,
      ),
    [suggestionOverlay?.suggestions, activeContent, selectedReviewSource],
  );

  const toggleThreadLine = (lineNum: number) => {
    setExpandedThreadLines((prev) => {
      const next = new Set(prev);
      if (next.has(lineNum)) next.delete(lineNum);
      else next.add(lineNum);
      return next;
    });
  };

  const totalHeight = lines.length * LINE_HEIGHT + totalZoneHeight;

  // Save, placed in the file row by the caller's `headerActions` or at the
  // right when there is none.
  const saveControl = onSave && (canSave || saveWaiting) && (
    <GatedControl
      verb="edit"
      reason={saveWaiting ? (saveWaitingReason ?? null) : null}
      testId="code-editor-start-hint"
    >
      <button
        type="button"
        data-testid="code-editor-save-button"
        onClick={() => void handleSave()}
        disabled={
          !canSave ||
          locked ||
          (!isDirty && !pendingEdit) ||
          saveState === "saving" ||
          !!storedDraft?.saving
        }
        className={classNames(
          "flex shrink-0 items-center gap-1 rounded px-1.5 py-1 text-[11px]",
          canSave && (isDirty || pendingEdit) && saveState !== "saving"
            ? "text-[var(--g5-active)] hover:bg-[var(--g5-overlay-hover)]"
            : "cursor-not-allowed text-[var(--g5-text2)] opacity-60",
        )}
        title={saveWaitingReason ?? "Save (Ctrl+S)"}
      >
        <Save className="h-3.5 w-3.5" />
        <span>
          {saveState === "saving" || storedDraft?.saving ? "Saving…" : "Save"}
        </span>
      </button>
    </GatedControl>
  );

  // Back to where a followed link or trace came from, over the rendered
  // document and the editor alike (`g5.documents.ontology.links`).
  const backTitle = navigationOrigin
    ? `Back to ${navigationOrigin.name}${navigationOrigin.intentRecordNumber ? ` → ${navigationOrigin.intentRecordNumber}` : ""}`
    : undefined;
  const floatingBack = navigationOrigin && onNavigateBack && (
    <button
      type="button"
      className="absolute top-2 left-2 z-20 flex items-center gap-1 px-2 py-1 rounded-md bg-[var(--g5-info)] text-white text-xs cursor-pointer hover:brightness-110 transition-all shadow-md"
      data-testid="navigate-back"
      onClick={() => {
        setBackButtonLine(null);
        onNavigateBack();
      }}
      title={backTitle}
    >
      <Undo2 className="w-3 h-3" />
      <span>Back</span>
    </button>
  );

  return (
    <div
      ref={codeEditorRef}
      data-testid="code-editor"
      data-file-path={filePath}
      className="relative flex flex-col h-full min-w-0 overflow-hidden bg-[var(--g5-bg1)]"
    >
      {/* The file row: its path and what the buffer shows, with every
          control at the right (`g5.documents.editor.modes`). */}
      <div
        data-testid="code-editor-file-row"
        className={classNames(
          "flex flex-wrap items-center gap-1 px-2 py-1",
          "border-b border-[var(--g5-stroke)] text-[var(--g5-text2)]",
          "shrink-0 bg-[var(--g5-bg1)]",
        )}
      >
        <BreadcrumbBar filePath={filePath} />
        {source && (
          <SourceBadge
            testId="code-editor-source-badge"
            source={source}
            repoOid={repoOid}
            snapshotAt={snapshotAt}
          />
        )}
        {isDirty && (
          <span
            data-testid="code-editor-dirty-indicator"
            className="shrink-0 text-[10px] font-medium text-[var(--g5-warn)]"
            title="This browser buffer is not in the workspace yet. Save it before forking to include these edits."
          >
            Unsaved · browser only
          </span>
        )}
        <div className="flex-1" />
        {headerActions ? headerActions(saveControl) : saveControl}
        {/* Comments toggle */}
        {onToggleComments && (
          <button
            data-testid="code-editor-toggle-comments-button"
            onClick={onToggleComments}
            className={classNames(
              "flex items-center gap-1 px-1.5 py-1 rounded hover:bg-[var(--g5-overlay-hover)] shrink-0 text-[11px]",
              showComments ? "text-[var(--g5-info)]" : "text-[var(--g5-text2)]",
            )}
            title="Toggle Comments"
          >
            <MessageCircle className="w-3.5 h-3.5" />
            <span className="hidden sm:inline">Comments</span>
          </button>
        )}
      </div>
      {notices}

      {preview && (
        <div className="relative flex min-h-0 flex-1 flex-col">
          <div
            className="min-h-0 flex-1 overflow-auto"
            data-testid="file-view-preview"
          >
            {preview}
          </div>
          {floatingBack}
        </div>
      )}
      {/* Main content area */}
      <div
        className={classNames(
          "flex-1 flex min-w-0 overflow-hidden",
          !!preview && "hidden",
        )}
        onMouseUp={handleCodeMouseUp}
        onContextMenu={handleContextMenu}
      >
        {/* Monaco editor backed by the component's local model. */}
        {useMonacoEditor ? (
          <div className="flex-1 min-w-0 overflow-hidden relative">
            <MonacoEditor
              key={`local:${filePath}`}
              height="100%"
              language={language}
              path={sessionKey ? fileModelUri(sessionKey) : filePath}
              keepCurrentModel={!!sessionKey}
              theme={themeMode === "light" ? "g5-light" : "g5-dark"}
              defaultValue={activeContent}
              value={sessionKey ? undefined : activeContent}
              options={{
                readOnly: !editable || locked,
                glyphMargin: !!onAddComment,
                minimap: { enabled: showMinimap },
                fontSize: 13,
                // The comment margin places threads by this row height.
                lineHeight: LINE_HEIGHT,
                lineNumbers: "on",
                scrollBeyondLastLine: false,
                automaticLayout: true,
                padding: { top: 8 },
                // Bounded: the measure, or the viewport when narrower.
                wordWrap: wrapsText && wrapColumn ? "bounded" : "off",
                wordWrapColumn: wrapColumn ?? 80,
              }}
              onMount={handleEditorMount}
              onChange={(value: string | undefined) => {
                if (!editable || locked || syncingModel.current) return;
                setLocalContent(
                  monacoEditorRef.current
                    ?.getModel()
                    ?.getValue(undefined, true) ??
                    value ??
                    "",
                );
                setSaveState("idle");
                setSaveError(null);
              }}
            />
            {suggestionOverlay && suggestionOverlay.suggestions.length > 0 && (
              <MonacoSuggestionZones
                editor={monacoEditorRef.current}
                mountVersion={editorMountVersion}
                overlay={suggestionOverlay}
                cardNodes={suggestionCardNodes}
              />
            )}
            {changedLines && changedLines.length > 0 && (
              <MonacoChangedLines
                editor={monacoEditorRef.current}
                mountVersion={editorMountVersion}
                ranges={changedLines}
              />
            )}
            {/* Floating back button for Monaco mode */}
            {floatingBack}
          </div>
        ) : useEditableFallback ? (
          <div className="flex-1 min-w-0 overflow-hidden flex flex-col">
            <textarea
              className={classNames(
                "w-full h-full p-3",
                "bg-[var(--g5-editor)] text-[var(--g5-text0)]",
                "font-[var(--g5-font-mono)] text-sm leading-relaxed",
                "resize-none",
              )}
              value={localContent}
              onChange={(e) => updateFallbackContent(e.target.value)}
              onKeyDown={(event) => {
                if ((event.metaKey || event.ctrlKey) && event.key === "s") {
                  event.preventDefault();
                  void handleSave();
                }
              }}
              onSelect={handleFallbackTextareaEvent}
              onClick={handleFallbackTextareaEvent}
              onKeyUp={handleFallbackTextareaEvent}
              spellCheck={false}
            />
            {/* No line rows to sit under: the cards list their lines in the
                text being edited, unsaved changes included. */}
            {suggestionOverlay && (
              <FallbackSuggestionList
                overlay={suggestionOverlay}
                content={activeContent}
                cardNodes={suggestionCardNodes}
              />
            )}
          </div>
        ) : (
          /* Fallback viewer (read-only mode) */
          <div
            ref={scrollRef}
            data-testid="code-editor-scroll"
            className="flex-1 min-w-0 overflow-auto"
            onScroll={handleScroll}
          >
            <div className="relative" style={{ minHeight: "100%" }}>
              {/* Render lines */}
              <div className="flex">
                {/* Line number gutter + Comment gutter */}
                <div
                  className="shrink-0 select-none sticky left-0 bg-[var(--g5-bg1)] z-10"
                  style={{ width: gutterWidth + MARKER_GUTTER_PX }}
                >
                  {suggestionZones.has(0) && (
                    <div
                      data-testid="suggestion-zone-gutter-0"
                      style={{ height: zoneHeights.get(0) ?? 0 }}
                    />
                  )}
                  {lines.map((_, i) => {
                    const lineNum = i + 1;
                    const hasComment = commentedLines.has(lineNum);
                    const isHovered = hoveredLine === lineNum;
                    const isHighlighted = highlightedLine === lineNum;

                    return (
                      <React.Fragment key={lineNum}>
                        <div
                          data-testid={`code-editor-gutter-line-${lineNum}`}
                          data-line-number={lineNum}
                          className={classNames(
                            "flex items-center",
                            isHighlighted && "bg-[var(--g5-info)]/15",
                          )}
                          style={{ height: LINE_HEIGHT }}
                          onMouseEnter={() => setHoveredLine(lineNum)}
                          onMouseLeave={() => setHoveredLine(null)}
                          onClick={() => {
                            setCursorLine(lineNum);
                            setCursorCol(1);
                          }}
                        >
                          {/* Marker gutter: back-button, or a suggestion
                              marker beside the comment control */}
                          <div
                            className="flex items-center justify-center gap-1 shrink-0"
                            style={{ width: MARKER_GUTTER_PX }}
                          >
                            {backButtonLine === lineNum &&
                            navigationOrigin &&
                            onNavigateBack ? (
                              <button
                                type="button"
                                className="w-4 h-4 flex items-center justify-center rounded-full bg-[var(--g5-info)] text-white cursor-pointer hover:brightness-110 transition-all"
                                data-testid="navigate-back"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  setBackButtonLine(null);
                                  onNavigateBack();
                                }}
                                title={backTitle}
                              >
                                <Undo2 className="w-2.5 h-2.5" />
                              </button>
                            ) : (
                              <>
                                {suggestionLines.has(lineNum) && (
                                  <SuggestionGutterMarker
                                    count={
                                      suggestionLines.get(lineNum)?.length ?? 0
                                    }
                                  />
                                )}
                                {hasComment ? (
                                  <button
                                    type="button"
                                    data-testid="code-editor-view-comments-button"
                                    className="w-2.5 h-2.5 rounded-full bg-[var(--g5-info)] cursor-pointer"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      toggleThreadLine(lineNum);
                                    }}
                                    title="View comments"
                                  />
                                ) : isHovered && onAddComment ? (
                                  <button
                                    type="button"
                                    data-testid="code-editor-add-comment-button"
                                    className="w-3.5 h-3.5 flex items-center justify-center rounded-full bg-[var(--g5-info)] text-white cursor-pointer opacity-60 hover:opacity-100 transition-opacity"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      openDraft(lineNum, "");
                                      if (!showComments && onToggleComments)
                                        onToggleComments();
                                    }}
                                    title="Add comment"
                                  >
                                    <Plus className="w-2.5 h-2.5" />
                                  </button>
                                ) : null}
                              </>
                            )}
                          </div>

                          {/* Line number */}
                          <div
                            className={classNames(
                              "text-right pr-3 text-xs font-[var(--g5-font-mono)]",
                              cursorLine === lineNum
                                ? "text-[var(--g5-text0)]"
                                : "text-[var(--g5-text2)]",
                            )}
                            style={{ width: gutterWidth - 6 }}
                          >
                            {lineNum}
                          </div>
                        </div>
                        {suggestionZones.has(lineNum) && (
                          <div
                            data-testid={`suggestion-zone-gutter-${lineNum}`}
                            style={{ height: zoneHeights.get(lineNum) ?? 0 }}
                          />
                        )}
                      </React.Fragment>
                    );
                  })}
                </div>

                {/* Code content: one row per line, a suggestion zone under
                    the line it follows (zone 0 sits above the first line) */}
                <div className="flex-1 overflow-x-auto">
                  <pre
                    className="text-sm font-[var(--g5-font-mono)] text-[var(--g5-text0)]"
                    style={{ lineHeight: `${LINE_HEIGHT}px` }}
                  >
                    {suggestionOverlay && suggestionZones.has(0) && (
                      <ReadOnlySuggestionZone
                        overlay={suggestionOverlay}
                        suggestions={suggestionZones.get(0) ?? []}
                        content={content}
                        afterLine={0}
                        onHeight={reportZoneHeight}
                        cardNodes={suggestionCardNodes}
                      />
                    )}
                    {lines.map((line, i) => {
                      const lineNum = i + 1;
                      const zone = suggestionZones.get(lineNum);
                      return (
                        <React.Fragment key={i}>
                          <div
                            className={classNames(
                              "relative",
                              changedLineSet.has(lineNum) &&
                                "g5-review-changed-line g5-review-changed-line-bg",
                            )}
                            style={{ height: LINE_HEIGHT }}
                            data-testid={`code-line-${lineNum}`}
                            data-line-number={lineNum}
                            data-review-changed={
                              changedLineSet.has(lineNum) ? "true" : undefined
                            }
                          >
                            {highlightedLines ? (
                              <code
                                dangerouslySetInnerHTML={{
                                  __html: highlightedLines[i] || "\n",
                                }}
                              />
                            ) : (
                              <span>{line || "\n"}</span>
                            )}
                          </div>
                          {suggestionOverlay && zone && (
                            <ReadOnlySuggestionZone
                              overlay={suggestionOverlay}
                              suggestions={zone}
                              content={content}
                              afterLine={lineNum}
                              onHeight={reportZoneHeight}
                              cardNodes={suggestionCardNodes}
                            />
                          )}
                        </React.Fragment>
                      );
                    })}
                  </pre>

                  {/* Inline expanded comment threads */}
                  {Array.from(expandedThreadLines).map((lineNum) => {
                    const lineComments = comments.filter((c) => {
                      const resolution = commentAnchorResolutions.get(c.id);
                      return (
                        resolution?.status === "mapped" &&
                        resolution.lineNumber === lineNum
                      );
                    });
                    if (lineComments.length === 0) return null;
                    return (
                      <div
                        key={`thread-${lineNum}`}
                        className={classNames(
                          "mx-4 my-1 p-3",
                          "bg-[var(--g5-bg2)] border border-[var(--g5-stroke)]",
                          "rounded-[var(--g5-radius-sm)]",
                          "text-sm text-[var(--g5-text0)]",
                        )}
                        style={{
                          position: "absolute",
                          top:
                            lineNum * LINE_HEIGHT +
                            zoneOffsetBefore(lineNum + 1),
                          left: gutterWidth + MARKER_GUTTER_PX,
                          right: 0,
                          zIndex: 20,
                        }}
                      >
                        {lineComments.map((c) => (
                          <div key={c.id} className="mb-2 last:mb-0">
                            <div className="flex items-center gap-2 mb-1">
                              <span className="font-medium text-xs">
                                {c.authorName}
                              </span>
                              <span className="text-[10px] text-[var(--g5-text2)]">
                                {formatCommentTime(c.createdAt)}
                              </span>
                            </div>
                            <div className="text-xs text-[var(--g5-text1)]">
                              {c.content}
                            </div>
                          </div>
                        ))}
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Comment margin column (right side) */}
        {marginMounted && !preview && (
          <div
            ref={marginRef}
            hidden={!showComments}
            data-testid="code-editor-comment-margin"
            className="w-[280px] shrink-0 overflow-y-auto border-l border-[var(--g5-stroke)]"
          >
            <CommentDraftCard
              subject={draftSubject}
              draft={commentDraft.draft}
              closed={onAddComment ? null : COMMENT_DRAFT_CLOSED}
              onSubmit={(content) =>
                commentDraft.send(
                  onAddComment,
                  activeContent,
                  repoOid ?? null,
                  content,
                )
              }
              onCancel={commentDraft.cancel}
            />
            <CommentColumn
              threads={commentThreads}
              notice={commentsNotice}
              onReply={onReplyToComment}
              onResolve={onResolveComment}
              onUnresolve={onUnresolveComment}
              onDelete={onDeleteComment}
              onThreadSelect={revealThreadLine}
            />
          </div>
        )}

        {/* Minimap (shown when comments are off). Only the read-only viewer
            needs this custom rail: Monaco mode renders its own built-in
            minimap from the same showMinimap flag, and the editable fallback
            has no line model to draw. */}
        {useReadOnlyViewer && !showComments && showMinimap && (
          <Minimap
            lines={lines}
            totalHeight={totalHeight}
            scrollTop={scrollTop}
            visibleHeight={visibleHeight}
            onScrollTo={(top) => {
              if (scrollRef.current) {
                scrollRef.current.scrollTop = top;
              }
            }}
          />
        )}
      </div>

      {suggestionOverlay && suggestionOverlay.suggestions.length > 0 && (
        <SuggestionCardPortals
          nodes={suggestionCardNodes}
          suggestions={suggestionOverlay.suggestions}
          renderCard={(suggestion) => (
            <SuggestionZoneContent
              overlay={suggestionOverlay}
              suggestion={suggestion}
              anchor={resolveDisplayAnchor(
                suggestion,
                activeContent,
                selectedReviewSource?.digest,
              )}
            />
          )}
        />
      )}

      {conflict && (
        <div
          role="alertdialog"
          aria-modal="true"
          aria-label="File changed on disk"
          data-testid="code-editor-conflict-dialog"
          className="absolute inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
        >
          <div className="w-full max-w-md rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4 shadow-xl">
            <div className="flex items-start gap-3">
              <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-[var(--g5-warn)]" />
              <div>
                <h3 className="text-sm font-semibold text-[var(--g5-text0)]">
                  This file changed after you opened it
                </h3>
                <p className="mt-1 text-xs text-[var(--g5-text1)]">
                  Reload the current file, or save your version against the
                  newly observed hash. Saving may replace those intervening
                  edits.
                </p>
              </div>
            </div>
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                data-testid="code-editor-conflict-compare"
                onClick={() => void openConflictCompare()}
                disabled={saveState === "saving"}
                className="rounded border border-[var(--g5-stroke)] px-3 py-1.5 text-xs text-[var(--g5-text0)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
              >
                Compare
              </button>
              <button
                type="button"
                data-testid="code-editor-conflict-reload"
                onClick={() => void handleReload()}
                disabled={!onReload || saveState === "saving"}
                className="rounded border border-[var(--g5-stroke)] px-3 py-1.5 text-xs text-[var(--g5-text0)] hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
              >
                Reload
              </button>
              <button
                type="button"
                data-testid="code-editor-conflict-overwrite"
                onClick={() => void persist(conflict.currentHash)}
                disabled={saveState === "saving"}
                className="rounded bg-[var(--g5-active)] px-3 py-1.5 text-xs font-medium text-white disabled:opacity-50"
              >
                Save my version
              </button>
            </div>
          </div>
        </div>
      )}

      {diskChange && isDirty && !conflict && (
        <div
          role="status"
          data-testid="code-editor-disk-change"
          className="flex shrink-0 flex-wrap items-center gap-2 border-t border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10 px-3 py-1.5 text-xs text-[var(--g5-text0)]"
        >
          <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-[var(--g5-warn)]" />
          <span className="flex-1">
            {draft?.role === "suggest"
              ? "This file changed while you were editing."
              : "This file changed on disk while you were editing."}
          </span>
          <button
            type="button"
            data-testid="code-editor-disk-change-compare"
            onClick={() =>
              setCompare({
                newerContent: diskChange.content,
                newerHash: diskChange.hash,
                newerVmId: diskChange.vmId,
                description:
                  draft?.role === "suggest"
                    ? "The file changed. Merge what you need into your text; Push review then suggests it against the newer version."
                    : "The file changed on disk. Merge what you need into your text; Save then writes it over the newer version.",
              })
            }
            className="rounded border border-[var(--g5-stroke)] px-2 py-1 hover:bg-[var(--g5-overlay-hover)]"
          >
            Compare
          </button>
          <button
            type="button"
            data-testid="code-editor-disk-change-reload"
            onClick={() => adoptNewerVersion(diskChange, diskChange.content)}
            className="rounded border border-[var(--g5-stroke)] px-2 py-1 hover:bg-[var(--g5-overlay-hover)]"
          >
            Reload
          </button>
          {onSave && editable && diskChange.hash && (
            <button
              type="button"
              data-testid="code-editor-disk-change-overwrite"
              onClick={() => void persist(diskChange.hash)}
              disabled={saveState === "saving" || locked}
              className="rounded border border-[var(--g5-stroke)] px-2 py-1 hover:bg-[var(--g5-overlay-hover)] disabled:opacity-50"
            >
              Save my version
            </button>
          )}
        </div>
      )}

      {compare && (
        <FileCompareDialog
          filePath={filePath}
          language={language}
          newerContent={compare.newerContent}
          ownContent={localContent}
          description={compare.description}
          applyLabel="Use my merged text"
          onCancel={() => setCompare(null)}
          onApply={(merged) =>
            adoptNewerVersion(
              {
                content: compare.newerContent,
                hash: compare.newerHash,
                vmId: compare.newerVmId,
              },
              merged,
            )
          }
        />
      )}

      {saveError && (
        <div className="shrink-0 border-t border-[var(--g5-blocked)]/40 bg-[var(--g5-blocked)]/10 px-3 py-1.5 text-xs text-[var(--g5-blocked)]">
          Save failed: {saveError}
        </div>
      )}

      {/* Status bar */}
      <div
        className={classNames(
          "flex items-center justify-between px-4 py-1",
          "border-t border-[var(--g5-stroke)] bg-[var(--g5-bg2)]",
          "text-[10px] text-[var(--g5-text2)]",
          "shrink-0",
        )}
      >
        <div className="flex items-center gap-4">
          <span>
            Ln {cursorLine}, Col {cursorCol}
          </span>
          <span className="capitalize">{language || "Plain Text"}</span>
          <span>UTF-8</span>
        </div>

        <div className="flex items-center gap-2">
          {/* Minimap toggle. Hidden in the editable fallback, which has no
              minimap to control (only Monaco mode and the read-only viewer
              render one from showMinimap). */}
          {!showComments && !useEditableFallback && (
            <IconButton
              title={showMinimap ? "Hide minimap" : "Show minimap"}
              onClick={() => setShowMinimap(!showMinimap)}
            >
              <MapIcon
                className={classNames(
                  "w-3 h-3",
                  showMinimap && "text-[var(--g5-info)]",
                )}
              />
            </IconButton>
          )}
        </div>
      </div>

      {/* Right-click context menu */}
      {contextMenu.visible && (
        <div
          ref={contextMenuRef}
          className={classNames(
            "fixed z-50",
            "bg-[var(--g5-bg1)] border border-[var(--g5-stroke)]",
            "rounded-[var(--g5-radius-md)] shadow-lg",
            "py-1 min-w-[180px]",
          )}
          style={{
            top: contextMenu.position.top,
            left: contextMenu.position.left,
          }}
        >
          <div className="px-3 py-1 text-[10px] uppercase tracking-wide text-[var(--g5-text2)]">
            Collaborate
          </div>
          <button
            type="button"
            data-testid="code-editor-context-add-comment-button"
            className={classNames(
              "w-full flex items-center gap-2 px-3 py-1.5",
              "text-sm text-[var(--g5-text0)] cursor-pointer",
              "hover:bg-[var(--g5-bg2)] transition-colors",
            )}
            onClick={() => handleContextAction("add-comment")}
          >
            <MessageCircle className="w-3.5 h-3.5" />
            <span>Add Comment</span>
          </button>
        </div>
      )}

      {selectionPopover.visible && (
        <SelectionCommentBubble
          position={selectionPopover.position}
          testId="code-editor-selection-comment-button"
          onComment={() =>
            handleSelectionComment(selectionPopover.text, selectionPopover.line)
          }
        />
      )}
    </div>
  );
};
