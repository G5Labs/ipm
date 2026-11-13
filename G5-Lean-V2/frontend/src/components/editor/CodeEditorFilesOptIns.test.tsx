import React from "react";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";
import { ApiError } from "../../lib/api";
import { CodeEditor, type CodeEditorProps } from "./CodeEditor";
import {
  fileDraftKey,
  getFileDraft,
  newFileDraftId,
  setFileDraft,
} from "../files/fileDrafts";

// Monaco as a textarea honouring readOnly, with one glyph-margin button per
// line when the glyph margin is on (a click reports a glyph mouse-down).
vi.mock("@monaco-editor/react", async () => {
  const ReactModule = await import("react");
  type MouseListener = (event: {
    target: { type: number; position: { lineNumber: number } | null };
  }) => void;
  const MockMonaco = ({
    value,
    onChange,
    onMount,
    options,
  }: {
    value: string;
    onChange: (value: string) => void;
    onMount?: (editor: unknown, monaco: unknown) => void;
    options?: { readOnly?: boolean; glyphMargin?: boolean };
  }) => {
    const mouseDown = ReactModule.useRef<MouseListener[]>([]);
    ReactModule.useEffect(() => {
      onMount?.(
        {
          addCommand: () => undefined,
          getModel: () => null,
          onMouseDown: (listener: MouseListener) => {
            mouseDown.current.push(listener);
            return { dispose: () => undefined };
          },
          onMouseMove: () => ({ dispose: () => undefined }),
          onMouseLeave: () => ({ dispose: () => undefined }),
          createDecorationsCollection: () => ({
            set: () => undefined,
            clear: () => undefined,
          }),
        },
        {
          KeyMod: { CtrlCmd: 0 },
          KeyCode: { KeyS: 0 },
          Range: class {},
          editor: {
            defineTheme: () => undefined,
            setTheme: () => undefined,
            MouseTargetType: { GUTTER_GLYPH_MARGIN: 2 },
          },
        },
      );
    }, []);
    return ReactModule.createElement(
      "div",
      null,
      ReactModule.createElement("textarea", {
        "data-testid": "mock-monaco-editor",
        value,
        readOnly: options?.readOnly === true,
        onChange: (event: React.ChangeEvent<HTMLTextAreaElement>) =>
          onChange(event.currentTarget.value),
      }),
      options?.glyphMargin
        ? value.split("\n").map((_, index) =>
            ReactModule.createElement("button", {
              key: index,
              type: "button",
              "data-testid": `mock-monaco-glyph-${index + 1}`,
              onClick: () =>
                mouseDown.current.forEach((listener) =>
                  listener({
                    target: { type: 2, position: { lineNumber: index + 1 } },
                  }),
                ),
            }),
          )
        : null,
    );
  };
  return { default: MockMonaco };
});

const PATH = "src/app.ts";
const key = fileDraftKey({
  person: "user-alex",
  workspaceId: "ws-1",
  repoId: "repo-1",
  path: PATH,
});

function baseProps(overrides: Partial<CodeEditorProps> = {}): CodeEditorProps {
  return {
    filePath: PATH,
    content: "one\n",
    language: "typescript",
    editable: true,
    contentHash: "h1",
    observedVmId: "vm-1",
    showComments: false,
    comments: [],
    draft: { key, role: "owner" },
    onSave: vi.fn(async () => ({ hash: "h3" })),
    ...overrides,
  };
}

/** Renders the editor with props the test can replace, inside providers. */
function renderEditor(initial: CodeEditorProps) {
  const controls: { show: (next: CodeEditorProps) => void } = {
    show: () => undefined,
  };
  const Harness = () => {
    const [current, setCurrent] = React.useState(initial);
    React.useEffect(() => {
      controls.show = setCurrent;
    }, []);
    return <CodeEditor {...current} />;
  };
  const view = renderApp(<Harness />);
  return {
    ...view,
    show: (next: CodeEditorProps) => act(() => controls.show(next)),
  };
}

async function editor(): Promise<HTMLTextAreaElement> {
  return (await screen.findByTestId(
    "mock-monaco-editor",
  )) as HTMLTextAreaElement;
}

describe("CodeEditor in FILES", () => {
  it("compares a Save conflict and saves the merged text against the newer version", async () => {
    const onSave = vi
      .fn()
      .mockRejectedValueOnce(
        new ApiError({
          code: "FILE_VERSION_CONFLICT",
          message: "changed",
          status: 409,
          details: { hash: "h2", content: "two\n" },
        }),
      )
      .mockResolvedValueOnce({ hash: "h3" });
    renderEditor(baseProps({ onSave }));
    fireEvent.change(await editor(), { target: { value: "one\nmine\n" } });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    fireEvent.click(await screen.findByTestId("code-editor-conflict-compare"));

    const newer = (await screen.findByTestId(
      "file-compare-newer",
    )) as HTMLTextAreaElement;
    expect(newer).toHaveValue("two\n");
    const own = screen.getByTestId("file-compare-own") as HTMLTextAreaElement;
    expect(own).toHaveValue("one\nmine\n");
    fireEvent.change(own, { target: { value: "two\nmine\n" } });
    fireEvent.click(screen.getByTestId("file-compare-apply"));
    expect(await editor()).toHaveValue("two\nmine\n");
    // The draft now starts from the newer version.
    expect(getFileDraft(key)?.baseline).toMatchObject({
      content: "two\n",
      hash: "h2",
    });

    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() =>
      expect(onSave).toHaveBeenLastCalledWith("two\nmine\n", "h2", "vm-1"),
    );
  });

  it("says when the file changes on disk under unsaved edits, and Reload takes the newer version", async () => {
    const { show } = renderEditor(baseProps());
    fireEvent.change(await editor(), { target: { value: "one\nmine\n" } });
    expect(
      screen.queryByTestId("code-editor-disk-change"),
    ).not.toBeInTheDocument();

    show(baseProps({ content: "agent\n", contentHash: "h2" }));
    expect(
      await screen.findByTestId("code-editor-disk-change"),
    ).toBeInTheDocument();
    // The unsaved text is kept until the person chooses.
    expect(await editor()).toHaveValue("one\nmine\n");

    fireEvent.click(screen.getByTestId("code-editor-disk-change-reload"));
    expect(await editor()).toHaveValue("agent\n");
    expect(
      screen.queryByTestId("code-editor-disk-change"),
    ).not.toBeInTheDocument();
    expect(getFileDraft(key)).toBeNull();
  });

  it("saves the person's text over a version that changed on disk when they choose Save my version", async () => {
    const onSave = vi.fn(async () => ({ hash: "h3" }));
    const { show } = renderEditor(baseProps({ onSave }));
    fireEvent.change(await editor(), { target: { value: "one\nmine\n" } });
    show(baseProps({ onSave, content: "agent\n", contentHash: "h2" }));
    fireEvent.click(
      await screen.findByTestId("code-editor-disk-change-overwrite"),
    );
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith("one\nmine\n", "h2", "vm-1"),
    );
    await waitFor(() =>
      expect(
        screen.queryByTestId("code-editor-disk-change"),
      ).not.toBeInTheDocument(),
    );
    expect(await editor()).toHaveValue("one\nmine\n");
    expect(getFileDraft(key)).toBeNull();
  });

  it("offers a reader Compare and Reload, but no Save, when the file changes under a suggestion", async () => {
    const { show } = renderEditor(
      baseProps({ onSave: undefined, draft: { key, role: "suggest" } }),
    );
    fireEvent.change(await editor(), { target: { value: "one\nmine\n" } });
    show(
      baseProps({
        onSave: undefined,
        draft: { key, role: "suggest" },
        content: "owner\n",
        contentHash: "h2",
      }),
    );
    expect(
      await screen.findByTestId("code-editor-disk-change-compare"),
    ).toBeInTheDocument();
    expect(
      screen.getByTestId("code-editor-disk-change-reload"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("code-editor-disk-change-overwrite"),
    ).not.toBeInTheDocument();
  });

  it("reopens a draft whose file changed meanwhile with the change pending", async () => {
    setFileDraft(key, {
      id: newFileDraftId(),
      role: "owner",
      content: "one\nmine\n",
      baseline: { content: "one\n", hash: "h1", vmId: "vm-1" },
    });
    renderEditor(baseProps({ content: "agent\n", contentHash: "h2" }));
    expect(await editor()).toHaveValue("one\nmine\n");
    expect(screen.getByTestId("code-editor-disk-change")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("code-editor-disk-change-compare"));
    expect(await screen.findByTestId("file-compare-newer")).toHaveValue(
      "agent\n",
    );
  });

  it("saves text another writer put in the draft before the buffer shows it", async () => {
    const onSave = vi.fn(async () => ({ hash: "h3" }));
    renderEditor(baseProps({ onSave, pendingEdit: true }));
    await editor();
    const save = screen.getByTestId("code-editor-save-button");
    // Rich text writes when Save takes its focus, and Save runs before this
    // editor renders the draft (`g5.documents.preview.rich-editing`).
    act(() => {
      setFileDraft(key, {
        id: newFileDraftId(),
        role: "owner",
        content: "one\nrich\n",
        baseline: { content: "one\n", hash: "h1", vmId: "vm-1" },
      });
      fireEvent.click(save);
    });
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith("one\nrich\n", "h1", "vm-1"),
    );
    expect(await editor()).toHaveValue("one\nrich\n");
  });

  it("drops its edits when the draft is dropped elsewhere, and never writes it back", async () => {
    renderEditor(baseProps());
    fireEvent.change(await editor(), { target: { value: "one\nmine\n" } });
    expect(getFileDraft(key)?.content).toBe("one\nmine\n");
    act(() => setFileDraft(key, null));
    expect(await editor()).toHaveValue("one\n");
    expect(getFileDraft(key)).toBeNull();
  });

  it("offers Add comment in read-only Monaco's glyph margin to whoever may start a thread", async () => {
    const { show } = renderEditor(
      baseProps({
        editable: false,
        draft: null,
        showComments: true,
        onAddComment: vi.fn(),
      }),
    );
    expect(await editor()).toHaveAttribute("readonly");
    fireEvent.click(await screen.findByTestId("mock-monaco-glyph-1"));
    expect(await screen.findByLabelText("Comment")).toBeInTheDocument();

    show(
      baseProps({
        editable: false,
        draft: null,
        showComments: true,
        onAddComment: undefined,
      }),
    );
    await waitFor(() =>
      expect(
        screen.queryByTestId("mock-monaco-glyph-1"),
      ).not.toBeInTheDocument(),
    );
  });
});
