import React from "react";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Editor } from "@tiptap/core";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";
import { renderApp } from "../../test/render";
import {
  RichMarkdownEditor,
  type RichEditorHandle,
  type RichMarkdownEditorProps,
} from "./RichMarkdownEditor";

// `g5.documents.preview.rich-editing`: the editor writes the file's Markdown
// through the caller's guarded write, reloads when the draft changes
// elsewhere, keeps opaque content out of reach and protects record lines.

const DOC = "# Title\n\nFirst paragraph.\n\n<div>Raw HTML</div>\n\nLast one.\n";

function setup(overrides: Partial<RichMarkdownEditorProps> = {}) {
  let handle: RichEditorHandle | null = null;
  const onWrite = vi.fn<RichMarkdownEditorProps["onWrite"]>(() => true);
  const props: RichMarkdownEditorProps = {
    content: DOC,
    filePath: "docs/readme.md",
    workspaceId: "ws",
    repoId: "repo",
    ontology: false,
    editable: true,
    onWrite,
    onHandle: (next) => {
      handle = next;
    },
    ...overrides,
  };
  const view = renderApp(<RichMarkdownEditor {...props} />);
  const editor = () =>
    (screen.getByTestId("rich-editor-content") as unknown as { editor: Editor })
      .editor;
  return {
    ...view,
    props,
    onWrite,
    editor,
    flush: () => {
      let result = false;
      act(() => {
        result = handle!.flush();
      });
      return result;
    },
    rerender: (next: Partial<RichMarkdownEditorProps>) =>
      view.rerender(<RichMarkdownEditor {...props} {...next} />),
  };
}

/** The document position just after `text`'s first occurrence. */
function after(editor: Editor, text: string): number {
  let found = -1;
  editor.state.doc.descendants((node, pos) => {
    if (found >= 0) return false;
    if (node.isText && node.text?.includes(text))
      found = pos + node.text.indexOf(text) + text.length;
    return true;
  });
  expect(found).toBeGreaterThan(0);
  return found;
}

afterEach(() => {
  vi.useRealTimers();
});

describe("RichMarkdownEditor", () => {
  it("shows the document and keeps raw HTML opaque", async () => {
    setup();
    const content = screen.getByTestId("rich-editor-content");
    expect(content).toHaveTextContent("Title");
    expect(content).toHaveTextContent("First paragraph.");
    const opaque = await screen.findByTestId("rich-opaque-block");
    expect(opaque).toHaveAttribute("data-kind", "html");
    expect(opaque).toHaveAttribute("contenteditable", "false");
    expect(opaque).toHaveTextContent("Raw HTML");
  });

  it("writes nothing when nothing changed", () => {
    const { flush, onWrite } = setup();
    expect(flush()).toBe(true);
    expect(onWrite).not.toHaveBeenCalled();
  });

  it("writes the edited Markdown and keeps untouched blocks verbatim", () => {
    const { editor, flush, onWrite } = setup();
    act(() => {
      editor()
        .chain()
        .insertContentAt(after(editor(), "First paragraph"), " more")
        .run();
    });
    expect(flush()).toBe(true);
    expect(onWrite).toHaveBeenCalledWith(
      DOC,
      DOC.replace("First paragraph.", "First paragraph more."),
    );
  });

  it("writes on its own after a pause", () => {
    vi.useFakeTimers();
    const { editor, onWrite } = setup();
    act(() => {
      editor().chain().insertContentAt(after(editor(), "Last one"), "!").run();
    });
    expect(onWrite).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(400);
    });
    expect(onWrite).toHaveBeenCalledWith(
      DOC,
      DOC.replace("Last one.", "Last one!."),
    );
  });

  it("formats a selection from the toolbar", () => {
    const { editor, flush, onWrite } = setup();
    const end = after(editor(), "First");
    act(() => {
      editor().commands.setTextSelection({
        from: end - "First".length,
        to: end,
      });
    });
    fireEvent.click(screen.getByTestId("rich-toolbar-bold"));
    flush();
    expect(onWrite).toHaveBeenLastCalledWith(
      DOC,
      DOC.replace("First paragraph.", "**First** paragraph."),
    );
  });

  it("offers table controls inside a table and writes the new row", () => {
    const table = "Intro.\n\n| A | B |\n| - | - |\n| 1 | 2 |\n";
    const { editor, flush, onWrite } = setup({ content: table });
    expect(screen.queryByTestId("rich-table-controls")).toBeNull();
    act(() => {
      editor().commands.setTextSelection(after(editor(), "1"));
    });
    fireEvent.click(screen.getByTestId("rich-table-row-below"));
    flush();
    const written = onWrite.mock.calls.at(-1)![1];
    expect(written.trim().split("\n")).toHaveLength(6);
    expect(written).toContain("| 1 | 2 |");
  });

  it("reloads when the draft changes elsewhere", () => {
    const { rerender } = setup();
    rerender({ content: "# Merged\n\nFrom Compare.\n" });
    expect(screen.getByTestId("rich-editor-content")).toHaveTextContent(
      "From Compare.",
    );
  });

  it("shows the draft as it is when a write is refused", () => {
    const onWrite = vi.fn(() => false);
    const { editor, flush } = setup({ onWrite });
    act(() => {
      editor()
        .chain()
        .insertContentAt(after(editor(), "Last one"), " typed")
        .run();
    });
    expect(flush()).toBe(false);
    expect(onWrite).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("rich-editor-content")).not.toHaveTextContent(
      "typed",
    );
  });

  it("refuses to delete a record's identity line and says why", () => {
    const record = "[I1. Uploaded intent] (Tags: human)\nKeep this content.\n";
    const { editor, flush, onWrite } = setup({ content: record });
    act(() => {
      editor().chain().selectAll().deleteSelection().run();
    });
    expect(screen.getByTestId("rich-editor-notice")).toHaveTextContent(
      "only change in plain text",
    );
    expect(screen.getByTestId("rich-opaque-block")).toHaveTextContent(
      "[I1. Uploaded intent]",
    );
    flush();
    expect(onWrite).not.toHaveBeenCalled();
  });

  it("asks to save on Mod+S", () => {
    const onSaveShortcut = vi.fn();
    setup({ onSaveShortcut });
    fireEvent.keyDown(screen.getByTestId("rich-editor-content"), {
      key: "s",
      ctrlKey: true,
    });
    expect(onSaveShortcut).toHaveBeenCalledTimes(1);
  });

  it("places the caret after the word a double-click named", async () => {
    const { editor } = setup({
      focusAt: { offset: DOC.indexOf("Last one."), word: "Last", seq: 1 },
    });
    await waitFor(() =>
      expect(editor().state.selection.from).toBe(after(editor(), "Last")),
    );
  });

  it("opens at the first text, past leading front matter", async () => {
    const { editor } = setup({
      content: "---\ntitle: x\n---\n\nBody.\n",
      focusAt: { offset: null, word: null, seq: 1 },
    });
    await waitFor(() =>
      expect(editor().state.selection.$from.parent.textContent).toBe("Body."),
    );
    expect(editor().state.selection.empty).toBe(true);
  });

  it("is read-only while it may not be written", () => {
    const { editor } = setup({ editable: false });
    expect(editor().isEditable).toBe(false);
    expect(screen.getByTestId("rich-toolbar-bold")).toBeDisabled();
  });

  it("opens Source at an opaque block's line", async () => {
    const onOpenSource = vi.fn();
    setup({ onOpenSource });
    fireEvent.click(await screen.findByTestId("rich-opaque-open-source"));
    expect(onOpenSource).toHaveBeenCalledWith(5);
  });

  it("reports edits that wait to be written", () => {
    const onPendingChange = vi.fn();
    const { editor, flush } = setup({ onPendingChange });
    act(() => {
      editor().chain().insertContentAt(after(editor(), "Last one"), "!").run();
    });
    expect(onPendingChange).toHaveBeenLastCalledWith(true);
    flush();
    expect(onPendingChange).toHaveBeenLastCalledWith(false);
  });
});

// In ontology files Rich text shows prose only: record lines stay in the
// document without a box, statements keep Preview's bullet and outline, and
// editing keys stop at a statement's boundary.
const ONTOLOGY = [
  "# Title",
  "",
  "Intro prose.",
  "",
  '<a id="x.a"></a>',
  "[D · x.a]",
  "",
  "First body.",
  "",
  "Second body.",
  "",
  '<a id="x.b"></a>',
  "[I · x.b]",
  "",
  "Other body.",
  "",
  "## Metadata",
  "",
  "### x.a",
  "",
  "- agent",
  "",
  "### x.b",
  "",
  "- agent",
  "",
].join("\n");

describe("RichMarkdownEditor in an ontology file", () => {
  const ontology = (overrides: Partial<RichMarkdownEditorProps> = {}) =>
    setup({
      content: ONTOLOGY,
      filePath: ".g5/ontology/x.g5.md",
      ontology: true,
      ...overrides,
    });
  const paragraph = (text: string) =>
    screen.getByText(text, { selector: "p" }) as HTMLElement;
  const keyDown = (key: string) =>
    fireEvent.keyDown(screen.getByTestId("rich-editor-content"), { key });
  /** Collapse the selection at a position. */
  const caret = (editor: Editor, pos: number) =>
    act(() => {
      editor.view.dispatch(
        editor.state.tr.setSelection(
          TextSelection.create(editor.state.doc, pos),
        ),
      );
    });

  /** Positions of the hidden statement markers. */
  const statementMarkers = (editor: Editor) => {
    const found: number[] = [];
    editor.state.doc.forEach((node, pos) => {
      if (node.attrs.statement) found.push(pos);
    });
    return found;
  };

  it("keeps record lines in the document without showing them", async () => {
    const { editor } = ontology();
    const content = screen.getByTestId("rich-editor-content");
    expect(content).not.toHaveTextContent("[D · x.a]");
    expect(content).not.toHaveTextContent("x.b");
    expect(content).not.toHaveTextContent("Metadata");
    const hidden = await screen.findAllByTestId("rich-hidden-line");
    expect(screen.queryAllByTestId("rich-opaque-block")).toHaveLength(0);
    expect(hidden).toHaveLength(7);
    for (const line of hidden) {
      expect(line).toBeEmptyDOMElement();
      expect(line.closest('[data-g5-hidden="true"]')).not.toBeNull();
    }
    // Each line is still in the document, protected.
    let protectedLines = 0;
    editor().state.doc.forEach((node) => {
      if (node.attrs.protected) protectedLines++;
    });
    expect(protectedLines).toBe(7);
  });

  it("marks each statement's body for its bullet and outline", () => {
    ontology();
    const first = paragraph("First body.");
    const second = paragraph("Second body.");
    const other = paragraph("Other body.");
    expect(first).toHaveClass("g5-rich-statement", "g5-rich-statement-first");
    expect(first).not.toHaveClass("g5-rich-statement-last");
    expect(second).toHaveClass("g5-rich-statement", "g5-rich-statement-last");
    expect(second.dataset.g5Statement).toBe(first.dataset.g5Statement);
    expect(other).toHaveClass(
      "g5-rich-statement-first",
      "g5-rich-statement-last",
    );
    expect(other.dataset.g5Statement).not.toBe(first.dataset.g5Statement);
    // Ordinary prose and section headings show as before.
    expect(paragraph("Intro prose.")).not.toHaveClass("g5-rich-statement");
    expect(screen.getByRole("heading", { name: "Title" })).not.toHaveClass(
      "g5-rich-statement",
    );
  });

  it("outlines the statement under the pointer and the caret", () => {
    const { editor } = ontology();
    fireEvent.mouseOver(paragraph("Second body."));
    expect(paragraph("First body.")).toHaveClass("is-hovered");
    expect(paragraph("Other body.")).not.toHaveClass("is-hovered");
    fireEvent.mouseLeave(screen.getByTestId("rich-editor-content"));
    expect(paragraph("First body.")).not.toHaveClass("is-hovered");
    caret(editor(), after(editor(), "Other body"));
    expect(paragraph("Other body.")).toHaveClass("is-current");
    expect(paragraph("First body.")).not.toHaveClass("is-current");
  });

  it("consumes Backspace at a statement's start and Delete at its end", () => {
    const { editor, flush, onWrite } = ontology();
    caret(editor(), after(editor(), "First body.") - "First body.".length);
    const before = editor().state.doc;
    keyDown("Backspace");
    expect(editor().state.doc.eq(before)).toBe(true);
    caret(editor(), after(editor(), "Second body."));
    keyDown("Delete");
    expect(editor().state.doc.eq(before)).toBe(true);
    caret(editor(), after(editor(), "Other body.") - "Other body.".length);
    keyDown("Backspace");
    expect(editor().state.doc.eq(before)).toBe(true);
    caret(editor(), after(editor(), "Other body."));
    keyDown("Delete");
    expect(editor().state.doc.eq(before)).toBe(true);
    expect(screen.queryByTestId("rich-editor-notice")).toBeNull();
    expect(flush()).toBe(true);
    expect(onWrite).not.toHaveBeenCalled();
    // Inside a statement the keys edit as usual: Backspace joins its blocks.
    caret(editor(), after(editor(), "Second body.") - "Second body.".length);
    keyDown("Backspace");
    flush();
    expect(onWrite).toHaveBeenCalledWith(
      ONTOLOGY,
      ONTOLOGY.replace(
        "First body.\n\nSecond body.",
        "First body.Second body.",
      ),
    );
  });

  /** The ID of the statement a write added. */
  const addedId = (text: string) =>
    /\[I · (user\.[0-9a-f-]{36})\]/.exec(text)?.[1] ?? "";

  it("adds a paragraph to the statement on Shift+Enter at its end", () => {
    const { editor, flush, onWrite } = ontology();
    caret(editor(), after(editor(), "Other body."));
    fireEvent.keyDown(screen.getByTestId("rich-editor-content"), {
      key: "Enter",
      shiftKey: true,
    });
    act(() => {
      editor().commands.insertContent("Added.");
    });
    expect(paragraph("Added.")).toHaveClass(
      "g5-rich-statement",
      "g5-rich-statement-last",
    );
    expect(paragraph("Added.").dataset.g5Statement).toBe(
      paragraph("Other body.").dataset.g5Statement,
    );
    flush();
    expect(onWrite).toHaveBeenCalledWith(
      ONTOLOGY,
      ONTOLOGY.replace("Other body.\n", "Other body.\n\nAdded.\n"),
    );
  });

  it("adds an Intent with its anchor and metadata on Enter at a statement's end", async () => {
    const { editor, flush, onWrite } = ontology();
    caret(editor(), after(editor(), "First body.") + 2 + "Second body.".length);
    expect(editor().state.selection.$from.parent.textContent).toBe(
      "Second body.",
    );
    keyDown("Enter");
    // The caret is in the new statement's empty body, which shows the hint.
    expect(editor().state.selection.$from.parent.childCount).toBe(0);
    expect(
      await screen.findByTestId("rich-statement-placeholder"),
    ).toHaveTextContent("Type the statement, or press Backspace to remove it");
    act(() => {
      editor().commands.insertContent("New one.");
    });
    expect(screen.queryByTestId("rich-statement-placeholder")).toBeNull();
    expect(paragraph("New one.")).toHaveClass(
      "g5-rich-statement-first",
      "g5-rich-statement-last",
    );
    expect(paragraph("New one.").dataset.g5Statement).not.toBe(
      paragraph("Second body.").dataset.g5Statement,
    );
    // Record lines stay hidden; nothing reads as an error.
    expect(screen.queryByTestId("rich-editor-notice")).toBeNull();
    expect(screen.queryAllByTestId("rich-opaque-block")).toHaveLength(0);
    flush();
    const written = onWrite.mock.calls[0][1];
    const id = addedId(written);
    expect(id).not.toBe("");
    // After the statement, and its metadata after the statement's own entry.
    expect(written).toBe(
      ONTOLOGY.replace(
        "Second body.\n",
        `Second body.\n\n<a id="${id}"></a>\n[I · ${id}]\n\nNew one.\n`,
      ).replace(
        "### x.a\n\n- agent\n",
        `### x.a\n\n- agent\n\n### ${id}\n\n- human-provided\n`,
      ),
    );
  });

  it("adds after the last statement and writes the file's own line endings", () => {
    const crlf = ONTOLOGY.replace(/\n/g, "\r\n");
    const { editor, flush, onWrite } = ontology({ content: crlf });
    caret(editor(), after(editor(), "Other body."));
    keyDown("Enter");
    act(() => {
      editor().commands.insertContent("Last.");
    });
    flush();
    const written = onWrite.mock.calls[0][1];
    const id = addedId(written);
    expect(written).toBe(
      crlf
        .replace(
          "Other body.\r\n",
          `Other body.\r\n\r\n<a id="${id}"></a>\r\n[I · ${id}]\r\n\r\nLast.\r\n`,
        )
        .replace(
          /- agent\r\n$/,
          `- agent\r\n\r\n### ${id}\r\n\r\n- human-provided\r\n`,
        ),
    );
  });

  it("keeps Enter's meaning inside a statement, in its lists and on an empty statement", () => {
    const content = ONTOLOGY.replace(
      "Other body.",
      "Other body.\n\n- one\n- two",
    );
    const { editor, flush, onWrite } = ontology({ content });
    // In the middle of a paragraph: split within the statement.
    caret(editor(), after(editor(), "First"));
    keyDown("Enter");
    expect(paragraph("body.")).toHaveClass("g5-rich-statement");
    expect(paragraph("body.").dataset.g5Statement).toBe(
      paragraph("First").dataset.g5Statement,
    );
    // At the end of a list ending the statement: a new item.
    caret(editor(), after(editor(), "two"));
    keyDown("Enter");
    act(() => {
      editor().commands.insertContent("three");
    });
    expect(screen.getByText("three").closest("li")).not.toBeNull();
    flush();
    expect(addedId(onWrite.mock.calls[0][1])).toBe("");
  });

  it("gives a statement without prose an empty body that writes nothing", async () => {
    const content = ONTOLOGY.replace("Other body.\n\n", "");
    const { editor, flush, onWrite } = ontology({ content });
    expect(
      await screen.findByTestId("rich-statement-placeholder"),
    ).toBeInTheDocument();
    // Enter there does nothing.
    caret(editor(), statementMarkers(editor())[1] + 2);
    const before = editor().state.doc;
    keyDown("Enter");
    expect(editor().state.doc.eq(before)).toBe(true);
    flush();
    expect(onWrite).not.toHaveBeenCalled();
    // Typing gives it prose.
    act(() => {
      editor().commands.insertContent("Now written.");
    });
    flush();
    expect(onWrite).toHaveBeenCalledWith(
      content,
      content.replace("[I · x.b]\n", "[I · x.b]\n\nNow written.\n"),
    );
  });

  it("refuses to add or delete while a record line shows for repair", () => {
    const content = ONTOLOGY.replace("### x.b", "### x.c");
    const { editor, flush, onWrite } = ontology({ content });
    caret(editor(), after(editor(), "First body.") + 2 + "Second body.".length);
    const before = editor().state.doc;
    keyDown("Enter");
    expect(editor().state.doc.eq(before)).toBe(true);
    expect(screen.getByTestId("rich-editor-notice")).toHaveTextContent(
      "repair them in plain text",
    );
    flush();
    expect(onWrite).not.toHaveBeenCalled();
  });

  it("undoes and redoes an added statement as one change", () => {
    const { editor, flush, onWrite } = ontology();
    caret(editor(), after(editor(), "Other body."));
    keyDown("Enter");
    act(() => {
      editor().commands.undo();
    });
    expect(statementMarkers(editor())).toHaveLength(2);
    act(() => {
      editor().commands.redo();
    });
    expect(statementMarkers(editor())).toHaveLength(3);
    expect(screen.queryByTestId("rich-editor-notice")).toBeNull();
    flush();
    expect(addedId(onWrite.mock.calls[0][1])).not.toBe("");
  });

  it("refuses an edit that takes a statement's lines without its metadata", () => {
    const { editor, flush, onWrite } = ontology();
    const from = after(editor(), "Second body.");
    const to = after(editor(), "Other body.") - "Other body.".length;
    act(() => {
      editor().view.dispatch(editor().state.tr.delete(from, to));
    });
    expect(screen.getByTestId("rich-editor-notice")).toHaveTextContent(
      "only change in plain text",
    );
    expect(statementMarkers(editor())).toHaveLength(2);
    flush();
    expect(onWrite).not.toHaveBeenCalled();
  });

  // x.b without prose: Backspace in its empty body deletes it.
  const EMPTY_B = ONTOLOGY.replace("Other body.\n\n", "");
  const WITHOUT_B = EMPTY_B.replace(
    '<a id="x.b"></a>\n[I · x.b]\n\n',
    "",
  ).replace("\n\n### x.b\n\n- agent", "");
  const backspaceInB = (editor: Editor) => {
    caret(editor, statementMarkers(editor)[1] + 2);
    keyDown("Backspace");
  };

  it("deletes an empty statement whole on Backspace, at once and undoably", () => {
    const { editor, flush, onWrite } = ontology({ content: EMPTY_B });
    backspaceInB(editor());
    expect(statementMarkers(editor())).toHaveLength(1);
    expect(screen.queryByTestId("rich-editor-notice")).toBeNull();
    expect(editor().state.selection.from).toBe(after(editor(), "Second body."));
    flush();
    expect(onWrite).toHaveBeenLastCalledWith(EMPTY_B, WITHOUT_B);
    // One change: undo brings it back.
    act(() => {
      editor().commands.undo();
    });
    expect(statementMarkers(editor())).toHaveLength(2);
    expect(screen.queryByTestId("rich-editor-notice")).toBeNull();
    flush();
    expect(onWrite).toHaveBeenLastCalledWith(WITHOUT_B, EMPTY_B);
  });

  it("deletes a statement others link to without asking, leaving the links", () => {
    const content = EMPTY_B.replace(
      "First body.",
      "First body, see [the other](#x.b).",
    );
    const { editor, flush, onWrite } = ontology({ content });
    backspaceInB(editor());
    expect(statementMarkers(editor())).toHaveLength(1);
    flush();
    expect(onWrite).toHaveBeenCalledWith(
      content,
      WITHOUT_B.replace("First body.", "First body, see [the other](#x.b)."),
    );
  });

  it("adds and deletes when the Metadata section also holds prose", () => {
    // The prose keeps the Metadata heading visible; nothing needs repair.
    const prose = EMPTY_B.replace(
      "## Metadata\n\n",
      "## Metadata\n\nNotes on these entries.\n\n",
    );
    const { editor, flush, onWrite } = ontology({ content: prose });
    backspaceInB(editor());
    expect(statementMarkers(editor())).toHaveLength(1);
    expect(screen.queryByTestId("rich-editor-notice")).toBeNull();
    flush();
    const without = WITHOUT_B.replace(
      "## Metadata\n\n",
      "## Metadata\n\nNotes on these entries.\n\n",
    );
    expect(onWrite).toHaveBeenLastCalledWith(prose, without);
    caret(editor(), after(editor(), "Second body."));
    keyDown("Enter");
    expect(statementMarkers(editor())).toHaveLength(2);
    expect(screen.queryByTestId("rich-editor-notice")).toBeNull();
    act(() => {
      editor().commands.insertContent("Again.");
    });
    flush();
    const written = onWrite.mock.calls.at(-1)![1];
    const id = addedId(written);
    expect(written).toBe(
      without
        .replace(
          "Second body.\n",
          `Second body.\n\n<a id="${id}"></a>\n[I · ${id}]\n\nAgain.\n`,
        )
        .replace(/- agent\n$/, `- agent\n\n### ${id}\n\n- human-provided\n`),
    );
  });

  it("refuses to delete while a record line shows for repair", () => {
    // An unpaired metadata entry stays visible for repair.
    const content = `${EMPTY_B}\n### x.c\n\n- agent\n`;
    const { editor, flush, onWrite } = ontology({ content });
    backspaceInB(editor());
    expect(statementMarkers(editor())).toHaveLength(2);
    expect(screen.getByTestId("rich-editor-notice")).toHaveTextContent(
      "repair them in plain text",
    );
    flush();
    expect(onWrite).not.toHaveBeenCalled();
  });

  it("moves a selection off a hidden line in its direction of travel", () => {
    const { editor } = ontology();
    const { doc } = editor().state;
    let marker = -1;
    doc.forEach((node, pos) => {
      if (marker < 0 && node.attrs.statement) marker = pos;
    });
    const otherStart = after(editor(), "Other body.") - "Other body.".length;
    // Backwards from the next statement's first block: to the end of the
    // previous statement.
    caret(editor(), otherStart);
    let markerB = -1;
    doc.forEach((node, pos) => {
      if (node.attrs.statement && pos > marker) markerB = pos;
    });
    act(() => {
      editor().view.dispatch(
        editor().state.tr.setSelection(
          NodeSelection.create(editor().state.doc, markerB),
        ),
      );
    });
    expect(editor().state.selection.empty).toBe(true);
    expect(editor().state.selection.from).toBe(after(editor(), "Second body."));
    // Forwards from the previous statement's end: to the next one's start.
    act(() => {
      editor().view.dispatch(
        editor().state.tr.setSelection(
          NodeSelection.create(editor().state.doc, markerB),
        ),
      );
    });
    expect(editor().state.selection.from).toBe(otherStart);
    // Nothing visible follows the Metadata section: forwards stays put.
    caret(editor(), after(editor(), "Other body."));
    let footer = -1;
    doc.forEach((node, pos) => {
      if (footer < 0 && String(node.attrs.src).startsWith("## Metadata"))
        footer = pos;
    });
    act(() => {
      editor().view.dispatch(
        editor().state.tr.setSelection(
          NodeSelection.create(editor().state.doc, footer),
        ),
      );
    });
    expect(editor().state.selection.from).toBe(after(editor(), "Other body."));
  });

  it("still refuses deleting record lines, saying why", async () => {
    const { editor, flush, onWrite } = ontology();
    act(() => {
      editor().chain().selectAll().deleteSelection().run();
    });
    expect(screen.getByTestId("rich-editor-notice")).toHaveTextContent(
      "only change in plain text",
    );
    expect(await screen.findAllByTestId("rich-hidden-line")).toHaveLength(7);
    flush();
    expect(onWrite).not.toHaveBeenCalled();
  });

  it("opens at a statement's first body block for its marker's offset", async () => {
    const { editor } = ontology({
      focusAt: { offset: ONTOLOGY.indexOf("[I · x.b]"), word: null, seq: 1 },
    });
    await waitFor(() =>
      expect(editor().state.selection.$from.parent.textContent).toBe(
        "Other body.",
      ),
    );
    expect(editor().state.selection.$from.parentOffset).toBe(0);
  });
});
