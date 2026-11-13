/**
 * Preview's rich Markdown editor (`g5.documents.preview.rich-editing`).
 *
 * The document is loaded from the file's current text; edits are written back
 * as Markdown through the caller's guarded draft write, debounced, on blur and
 * whenever the caller flushes (Save, Push review, Done, leaving Preview). When
 * the draft changes elsewhere (a Compare merge, Discard), the editor reloads
 * from it rather than writing over it.
 */
import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  EditorContent,
  NodeViewWrapper,
  ReactNodeViewRenderer,
  useEditor,
  useEditorState,
  type Editor,
  type NodeViewProps,
} from "@tiptap/react";
import { NodeSelection, Selection, TextSelection } from "@tiptap/pm/state";
import {
  Bold,
  Code,
  Heading1,
  Heading2,
  Heading3,
  Italic,
  Link2,
  List,
  ListChecks,
  ListOrdered,
  Minus,
  Pilcrow,
  Quote,
  Redo2,
  SquareCode,
  Strikethrough,
  Table as TableIcon,
  Undo2,
} from "lucide-react";
import { MarkdownPreview } from "./MarkdownPreview";
import {
  docToMarkdown,
  markdownToDoc,
  opaqueMarkdown,
  topLevelPositionAt,
  type RichBaseline,
} from "./markdownDocument";
import {
  RICH_LOAD_META,
  richMarkdownExtensions,
  statementLayout,
  visibleSelectionFrom,
} from "./richExtensions";

/** The open editor, as FileView commits it before Save, Push and leaving. */
export interface RichEditorHandle {
  /** Writes pending edits; false when the draft refused them (it reloads). */
  flush(): boolean;
  focus(): void;
}

/** Where to place the caret when editing starts: a block's source offset and a word in it. */
export interface RichFocus {
  offset: number | null;
  word: string | null;
  seq: number;
}

export interface RichMarkdownEditorProps {
  /** The file's current text: the draft, or the file without one. */
  content: string;
  filePath: string;
  workspaceId: string;
  repoId: string;
  ontology: boolean;
  editable: boolean;
  focusAt?: RichFocus | null;
  /** The guarded draft write: false when the draft changed or may not be written. */
  onWrite: (expected: string, next: string) => boolean;
  onHandle?: (handle: RichEditorHandle | null) => void;
  /** Whether edits wait to be written, so Save can write and save them. */
  onPendingChange?: (pending: boolean) => void;
  /** Mod+S: written first, then saved by the caller. */
  onSaveShortcut?: () => void;
  /** Opens Source at a 1-based line of the written text. */
  onOpenSource?: (line: number) => void;
}

const FLUSH_DELAY_MS = 300;

const EDITOR_PROPS = {
  attributes: {
    class: "markdown-body g5-markdown-preview g5-rich-content",
    "data-testid": "rich-editor-content",
    "aria-label": "Document editor",
    role: "textbox",
    "aria-multiline": "true",
  },
};

const OPAQUE_LABELS: Record<string, string> = {
  frontmatter: "Front matter",
  structure: "Record line",
  label: "Record label",
  html: "HTML",
  math: "Math",
  definition: "Link definition",
  footnote: "Footnote",
  alert: "Alert",
  question: "Question",
  list: "List",
  image: "Image",
  reference: "Reference",
  link: "Link",
  other: "Markdown",
};

/** Kinds shown as their source rather than rendered. */
const SOURCE_KINDS = new Set([
  "frontmatter",
  "structure",
  "label",
  "definition",
  "footnote",
]);

interface RichContextValue {
  render: (markdown: string, inline: boolean) => React.ReactNode;
  openSource: ((position: number) => void) | null;
}

const RichContext = createContext<RichContextValue | null>(null);

/** Links inside opaque content do not navigate while editing. */
const stopLinks = (event: React.MouseEvent) => {
  if ((event.target as HTMLElement).closest("a")) event.preventDefault();
};

/**
 * Opaque content rendered as in reading view; where that renders nothing
 * visible (a comment, an empty anchor), its source shows instead.
 */
function OpaqueRendering({ markdown }: { markdown: string }) {
  const context = useContext(RichContext);
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    const rendered = element?.firstElementChild;
    if (!element || !rendered) return;
    element.dataset.empty = String(
      !rendered.textContent?.trim() &&
        !rendered.querySelector("img, svg, video, iframe, table, hr, input"),
    );
  }, [markdown]);
  return (
    <div
      ref={ref}
      className="g5-rich-opaque-rendering"
      onClickCapture={stopLinks}
    >
      <div className="g5-rich-opaque-rendered">
        {context?.render(markdown, false)}
      </div>
      <pre className="g5-rich-opaque-source g5-rich-opaque-fallback">
        {markdown}
      </pre>
    </div>
  );
}

function OpaqueBlockView({ node, selected, getPos }: NodeViewProps) {
  const context = useContext(RichContext);
  const kind = String(node.attrs.kind);
  // An ontology record line Preview hides stays in the document with no box.
  if (node.attrs.hidden)
    return (
      <NodeViewWrapper
        className="g5-rich-hidden"
        data-testid="rich-hidden-line"
        data-kind={kind}
        contentEditable={false}
        aria-hidden="true"
      />
    );
  const markdown = String(node.attrs.src || opaqueMarkdown(node.attrs.mdast));
  const label = OPAQUE_LABELS[kind] ?? OPAQUE_LABELS.other;
  return (
    <NodeViewWrapper
      className={`g5-rich-opaque${selected ? " is-selected" : ""}`}
      data-testid="rich-opaque-block"
      data-kind={kind}
      contentEditable={false}
      title={`${label}: edit this in plain text`}
    >
      {context?.openSource && (
        <button
          type="button"
          className="g5-rich-opaque-source-button"
          data-testid="rich-opaque-open-source"
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => {
            const position = typeof getPos === "function" ? getPos() : null;
            if (typeof position === "number") context.openSource?.(position);
          }}
        >
          Edit in plain text
        </button>
      )}
      {SOURCE_KINDS.has(kind) ? (
        <pre className="g5-rich-opaque-source">{markdown}</pre>
      ) : (
        <OpaqueRendering markdown={markdown} />
      )}
    </NodeViewWrapper>
  );
}

function OpaqueInlineView({ node, selected }: NodeViewProps) {
  const context = useContext(RichContext);
  const kind = String(node.attrs.kind);
  const markdown = opaqueMarkdown(node.attrs.mdast, true);
  return (
    <NodeViewWrapper
      as="span"
      className={`g5-rich-opaque-inline${selected ? " is-selected" : ""}`}
      data-testid="rich-opaque-inline"
      data-kind={kind}
      contentEditable={false}
      title={`${OPAQUE_LABELS[kind] ?? OPAQUE_LABELS.other}: edit this in plain text`}
      onClickCapture={stopLinks}
    >
      {context?.render(markdown, true)}
    </NodeViewWrapper>
  );
}

/**
 * Put the caret where Rich text was asked to open: after the double-clicked
 * word, at the end of a block double-clicked beside its text, on an opaque
 * block as a whole, or at the start for Edit and `e`.
 */
function placeCaret(editor: Editor, baseline: RichBaseline, focus: RichFocus) {
  const { doc } = editor.state;
  const start =
    focus.offset === null
      ? null
      : topLevelPositionAt(doc, baseline, focus.offset);
  const block = start === null ? null : doc.nodeAt(start);
  // The first text, not a leading opaque block such as front matter.
  let selection: Selection =
    Selection.findFrom(doc.resolve(0), 1, true) ?? Selection.atStart(doc);
  // A hidden record line (a statement's marker) opens at what follows it:
  // the statement's first body block.
  if (
    start !== null &&
    statementLayout(doc).hidden[doc.resolve(start).index(0)]
  )
    selection = visibleSelectionFrom(doc, start, 1) ?? selection;
  else if (start !== null && block?.isAtom)
    selection = NodeSelection.create(doc, start);
  else if (start !== null && block) {
    let found = -1;
    if (focus.word)
      block.descendants((child, offset) => {
        if (found >= 0) return false;
        if (child.isText && child.text) {
          const at = child.text.indexOf(focus.word!);
          if (at >= 0) found = start + 1 + offset + at + focus.word!.length;
        }
        return true;
      });
    selection =
      found >= 0
        ? TextSelection.create(doc, found)
        : focus.word
          ? TextSelection.near(doc.resolve(start + 1))
          : TextSelection.near(doc.resolve(start + block.nodeSize - 1), -1);
  }
  editor.view.dispatch(
    editor.state.tr.setSelection(selection).setMeta(RICH_LOAD_META, true),
  );
  editor.commands.focus(undefined, { scrollIntoView: true });
}

interface ToolbarState {
  editable: boolean;
  bold: boolean;
  italic: boolean;
  strike: boolean;
  code: boolean;
  link: string | null;
  paragraph: boolean;
  h1: boolean;
  h2: boolean;
  h3: boolean;
  bullet: boolean;
  ordered: boolean;
  task: boolean;
  quote: boolean;
  codeBlock: boolean;
  table: boolean;
  canUndo: boolean;
  canRedo: boolean;
}

function ToolbarButton({
  label,
  testId,
  active,
  disabled,
  onClick,
  children,
}: {
  label: string;
  testId: string;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      data-testid={testId}
      aria-pressed={active === undefined ? undefined : active}
      disabled={disabled}
      // Keep the editor's selection: the command acts on it.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function LinkEditor({
  editor,
  initial,
  onClose,
}: {
  editor: Editor;
  initial: string | null;
  onClose: () => void;
}) {
  const [href, setHref] = useState(initial ?? "");
  const finish = () => {
    onClose();
    editor.commands.focus();
  };
  const apply = () => {
    const value = href.trim();
    const chain = editor.chain().focus().extendMarkRange("link");
    if (!value) chain.unsetLink().run();
    else if (editor.state.selection.empty && !editor.isActive("link"))
      chain
        .insertContent({
          type: "text",
          text: value,
          marks: [{ type: "link", attrs: { href: value } }],
        })
        .run();
    else chain.setLink({ href: value }).run();
    onClose();
  };
  return (
    <div className="g5-rich-link-editor" data-testid="rich-link-editor">
      <input
        // The address field takes focus when it opens.
        // eslint-disable-next-line jsx-a11y/no-autofocus
        autoFocus
        aria-label="Link address"
        data-testid="rich-link-input"
        placeholder="https://"
        value={href}
        onChange={(event) => setHref(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            apply();
          } else if (event.key === "Escape") {
            event.preventDefault();
            finish();
          }
        }}
      />
      <button type="button" data-testid="rich-link-apply" onClick={apply}>
        Apply
      </button>
      {initial && (
        <button
          type="button"
          data-testid="rich-link-remove"
          onClick={() => {
            editor.chain().focus().extendMarkRange("link").unsetLink().run();
            onClose();
          }}
        >
          Remove link
        </button>
      )}
      <button type="button" data-testid="rich-link-cancel" onClick={finish}>
        Cancel
      </button>
    </div>
  );
}

function RichToolbar({
  editor,
  linkOpen,
  setLinkOpen,
}: {
  editor: Editor;
  linkOpen: boolean;
  setLinkOpen: (open: boolean) => void;
}) {
  const state = useEditorState({
    editor,
    selector: ({ editor: current }): ToolbarState => ({
      editable: current.isEditable,
      bold: current.isActive("bold"),
      italic: current.isActive("italic"),
      strike: current.isActive("strike"),
      code: current.isActive("code"),
      link: current.isActive("link")
        ? String(current.getAttributes("link").href ?? "")
        : null,
      paragraph: current.isActive("paragraph"),
      h1: current.isActive("heading", { level: 1 }),
      h2: current.isActive("heading", { level: 2 }),
      h3: current.isActive("heading", { level: 3 }),
      bullet: current.isActive("bulletList"),
      ordered: current.isActive("orderedList"),
      task: current.isActive("taskList"),
      quote: current.isActive("blockquote"),
      codeBlock: current.isActive("codeBlock"),
      table: current.isActive("table"),
      canUndo: current.can().undo(),
      canRedo: current.can().redo(),
    }),
  });
  const off = !state.editable;
  const run = (
    command: (
      chain: ReturnType<Editor["chain"]>,
    ) => ReturnType<Editor["chain"]>,
  ) => command(editor.chain().focus()).run();
  return (
    <div
      className="g5-rich-toolbar"
      role="toolbar"
      aria-label="Formatting"
      data-testid="rich-toolbar"
    >
      <div className="g5-rich-toolbar-row">
        <ToolbarButton
          label="Bold"
          testId="rich-toolbar-bold"
          active={state.bold}
          disabled={off}
          onClick={() => run((c) => c.toggleBold())}
        >
          <Bold className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Italic"
          testId="rich-toolbar-italic"
          active={state.italic}
          disabled={off}
          onClick={() => run((c) => c.toggleItalic())}
        >
          <Italic className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Strikethrough"
          testId="rich-toolbar-strike"
          active={state.strike}
          disabled={off}
          onClick={() => run((c) => c.toggleStrike())}
        >
          <Strikethrough className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Code"
          testId="rich-toolbar-code"
          active={state.code}
          disabled={off}
          onClick={() => run((c) => c.toggleCode())}
        >
          <Code className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Link"
          testId="rich-toolbar-link"
          active={state.link !== null}
          disabled={off}
          onClick={() => setLinkOpen(!linkOpen)}
        >
          <Link2 className="h-3.5 w-3.5" />
        </ToolbarButton>
        <span className="g5-rich-toolbar-separator" aria-hidden="true" />
        <ToolbarButton
          label="Paragraph"
          testId="rich-toolbar-paragraph"
          active={state.paragraph}
          disabled={off}
          onClick={() => run((c) => c.setParagraph())}
        >
          <Pilcrow className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Heading 1"
          testId="rich-toolbar-h1"
          active={state.h1}
          disabled={off}
          onClick={() => run((c) => c.toggleHeading({ level: 1 }))}
        >
          <Heading1 className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Heading 2"
          testId="rich-toolbar-h2"
          active={state.h2}
          disabled={off}
          onClick={() => run((c) => c.toggleHeading({ level: 2 }))}
        >
          <Heading2 className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Heading 3"
          testId="rich-toolbar-h3"
          active={state.h3}
          disabled={off}
          onClick={() => run((c) => c.toggleHeading({ level: 3 }))}
        >
          <Heading3 className="h-3.5 w-3.5" />
        </ToolbarButton>
        <span className="g5-rich-toolbar-separator" aria-hidden="true" />
        <ToolbarButton
          label="Bulleted list"
          testId="rich-toolbar-bullet"
          active={state.bullet}
          disabled={off}
          onClick={() => run((c) => c.toggleBulletList())}
        >
          <List className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Numbered list"
          testId="rich-toolbar-ordered"
          active={state.ordered}
          disabled={off}
          onClick={() => run((c) => c.toggleOrderedList())}
        >
          <ListOrdered className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Task list"
          testId="rich-toolbar-task"
          active={state.task}
          disabled={off}
          onClick={() => run((c) => c.toggleTaskList())}
        >
          <ListChecks className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Quote"
          testId="rich-toolbar-quote"
          active={state.quote}
          disabled={off}
          onClick={() => run((c) => c.toggleBlockquote())}
        >
          <Quote className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Code block"
          testId="rich-toolbar-code-block"
          active={state.codeBlock}
          disabled={off}
          onClick={() => run((c) => c.toggleCodeBlock())}
        >
          <SquareCode className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Insert table"
          testId="rich-toolbar-table"
          disabled={off || state.table}
          onClick={() =>
            run((c) => c.insertTable({ rows: 3, cols: 3, withHeaderRow: true }))
          }
        >
          <TableIcon className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Divider"
          testId="rich-toolbar-rule"
          disabled={off}
          onClick={() => run((c) => c.setHorizontalRule())}
        >
          <Minus className="h-3.5 w-3.5" />
        </ToolbarButton>
        <span className="g5-rich-toolbar-separator" aria-hidden="true" />
        <ToolbarButton
          label="Undo"
          testId="rich-toolbar-undo"
          disabled={off || !state.canUndo}
          onClick={() => run((c) => c.undo())}
        >
          <Undo2 className="h-3.5 w-3.5" />
        </ToolbarButton>
        <ToolbarButton
          label="Redo"
          testId="rich-toolbar-redo"
          disabled={off || !state.canRedo}
          onClick={() => run((c) => c.redo())}
        >
          <Redo2 className="h-3.5 w-3.5" />
        </ToolbarButton>
      </div>
      {state.table && (
        <div
          className="g5-rich-toolbar-row"
          data-testid="rich-table-controls"
          aria-label="Table"
        >
          <ToolbarButton
            label="Insert row above"
            testId="rich-table-row-above"
            disabled={off}
            onClick={() => run((c) => c.addRowBefore())}
          >
            Row above
          </ToolbarButton>
          <ToolbarButton
            label="Insert row below"
            testId="rich-table-row-below"
            disabled={off}
            onClick={() => run((c) => c.addRowAfter())}
          >
            Row below
          </ToolbarButton>
          <ToolbarButton
            label="Insert column left"
            testId="rich-table-column-left"
            disabled={off}
            onClick={() => run((c) => c.addColumnBefore())}
          >
            Column left
          </ToolbarButton>
          <ToolbarButton
            label="Insert column right"
            testId="rich-table-column-right"
            disabled={off}
            onClick={() => run((c) => c.addColumnAfter())}
          >
            Column right
          </ToolbarButton>
          <ToolbarButton
            label="Delete row"
            testId="rich-table-delete-row"
            disabled={off}
            onClick={() => run((c) => c.deleteRow())}
          >
            Delete row
          </ToolbarButton>
          <ToolbarButton
            label="Delete column"
            testId="rich-table-delete-column"
            disabled={off}
            onClick={() => run((c) => c.deleteColumn())}
          >
            Delete column
          </ToolbarButton>
          <ToolbarButton
            label="Delete table"
            testId="rich-table-delete"
            disabled={off}
            onClick={() => run((c) => c.deleteTable())}
          >
            Delete table
          </ToolbarButton>
        </div>
      )}
      {linkOpen && (
        <LinkEditor
          editor={editor}
          initial={state.link}
          onClose={() => setLinkOpen(false)}
        />
      )}
    </div>
  );
}

export function RichMarkdownEditor(props: RichMarkdownEditorProps) {
  const { content, ontology, filePath, workspaceId, repoId, onHandle } = props;
  const propsRef = useRef(props);
  propsRef.current = props;
  // The text this editor's document stands for: the last it loaded or wrote.
  const written = useRef(content);
  const [generation, setGeneration] = useState(0);
  const loaded = useMemo(
    () => markdownToDoc(written.current, { ontology }),
    // A new generation loads whatever `written` holds then.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [generation, ontology],
  );
  const dirty = useRef(false);
  const setDirty = (value: boolean) => {
    if (dirty.current === value) return;
    dirty.current = value;
    propsRef.current.onPendingChange?.(value);
  };
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [linkOpen, setLinkOpen] = useState(false);
  const flushRef = useRef<() => boolean>(() => true);
  // The loaded file's line ending, for a new statement's lines.
  const eolRef = useRef<string>("\n");
  eolRef.current = loaded.baseline.eol;
  const cancelTimer = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
  };

  const extensions = useMemo(
    () =>
      richMarkdownExtensions({
        opaqueBlockView: ReactNodeViewRenderer(OpaqueBlockView),
        opaqueInlineView: ReactNodeViewRenderer(OpaqueInlineView, {
          as: "span",
        }),
        onStructureRefused: () =>
          setNotice(
            "Record identity, anchor and metadata lines can only change in plain text.",
          ),
        // Save runs the pre-save flush itself, so it sees what this writes.
        onSave: () => {
          if (propsRef.current.onSaveShortcut)
            propsRef.current.onSaveShortcut();
          else flushRef.current();
        },
        onLink: () => setLinkOpen(true),
        statements: {
          eol: () => eolRef.current,
          onRefused: setNotice,
        },
      }),
    [],
  );

  const editor = useEditor(
    {
      extensions,
      content: loaded.json,
      editable: props.editable,
      immediatelyRender: true,
      shouldRerenderOnTransaction: false,
      editorProps: EDITOR_PROPS,
      onUpdate: ({ transaction }) => {
        if (transaction.getMeta(RICH_LOAD_META)) return;
        setDirty(true);
        cancelTimer();
        timer.current = setTimeout(() => {
          timer.current = null;
          flushRef.current();
        }, FLUSH_DELAY_MS);
      },
      onBlur: () => {
        flushRef.current();
      },
    },
    [generation],
  );

  // The editor's own nodes, as loaded, decide which blocks are unchanged.
  useLayoutEffect(() => {
    if (editor && !editor.isDestroyed) loaded.baseline.seal(editor.state.doc);
  }, [editor, loaded]);

  const flush = useCallback((): boolean => {
    cancelTimer();
    if (!dirty.current || !editor || editor.isDestroyed) return true;
    setDirty(false);
    const next = docToMarkdown(editor.state.doc, loaded.baseline).text;
    if (next === written.current) return true;
    if (propsRef.current.onWrite(written.current, next)) {
      written.current = next;
      return true;
    }
    // The draft changed meanwhile or may not be written: show it as it is.
    written.current = propsRef.current.content;
    setGeneration((value) => value + 1);
    return false;
  }, [editor, loaded]);
  flushRef.current = flush;

  // A draft changed elsewhere replaces what this editor shows.
  useEffect(() => {
    if (content === written.current) return;
    cancelTimer();
    setDirty(false);
    written.current = content;
    setGeneration((value) => value + 1);
  }, [content]);

  useEffect(() => {
    if (editor && !editor.isDestroyed)
      editor.setEditable(props.editable, false);
  }, [editor, props.editable]);

  useEffect(() => {
    if (!notice) return;
    const id = setTimeout(() => setNotice(null), 6000);
    return () => clearTimeout(id);
  }, [notice]);

  const editorRef = useRef(editor);
  editorRef.current = editor;
  useEffect(() => {
    onHandle?.({
      flush: () => flushRef.current(),
      focus: () => editorRef.current?.commands.focus(),
    });
    return () => onHandle?.(null);
  }, [onHandle]);

  useEffect(() => cancelTimer, []);

  // Each request (seq) places the caret once; a reload does not take focus.
  const focusAt = props.focusAt;
  const placed = useRef<number | null>(null);
  useEffect(() => {
    if (!editor || editor.isDestroyed || !focusAt) return;
    if (placed.current === focusAt.seq) return;
    placed.current = focusAt.seq;
    placeCaret(editor, loaded.baseline, focusAt);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, focusAt?.seq]);

  const openSource = useCallback(
    (position: number) => {
      if (!editor || editor.isDestroyed) return;
      // A refused write reloads the editor; Source opens on the next request.
      if (!flushRef.current()) return;
      const { doc } = editor.state;
      const { text, starts } = docToMarkdown(doc, loaded.baseline);
      const index = doc.resolve(Math.min(position, doc.content.size)).index(0);
      const start = Math.max(starts[index] ?? 0, 0);
      propsRef.current.onOpenSource?.(text.slice(0, start).split("\n").length);
    },
    [editor, loaded],
  );

  const context = useMemo<RichContextValue>(
    () => ({
      render: (markdown, inline) => (
        <MarkdownPreview
          content={markdown}
          filePath={filePath}
          workspaceId={workspaceId}
          repoId={repoId}
          className={
            inline ? "g5-rich-inline-preview" : "g5-rich-block-preview"
          }
        />
      ),
      openSource: props.onOpenSource ? openSource : null,
    }),
    [filePath, workspaceId, repoId, openSource, props.onOpenSource],
  );

  if (!editor) return null;
  return (
    <RichContext.Provider value={context}>
      <div className="g5-rich-editor" data-testid="rich-editor">
        <RichToolbar
          editor={editor}
          linkOpen={linkOpen}
          setLinkOpen={setLinkOpen}
        />
        {notice && (
          <div
            className="g5-rich-notice"
            role="status"
            data-testid="rich-editor-notice"
          >
            {notice}
          </div>
        )}
        <div
          className="g5-rich-surface g5-doc-column px-6 pt-4 pb-8"
          data-testid="rich-editor-surface"
        >
          <EditorContent editor={editor} />
        </div>
      </div>
    </RichContext.Provider>
  );
}
