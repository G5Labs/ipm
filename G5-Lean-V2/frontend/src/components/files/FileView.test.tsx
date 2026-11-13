import React from "react";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { createMockUser } from "../../test/fixtures";
import { getApiToken, setApiToken, tokenPrincipal } from "../../lib/api";
import { createCommentAnchor } from "../../lib/commentAnchors";
import type { CommentDTO } from "../../lib/types";
import type { WorkspaceFileContent } from "../../hooks/useFiles";

/** Lines the Monaco stand-in was asked to reveal. */
const revealed = vi.hoisted(() => [] as number[]);

// Monaco as a textarea that honours readOnly, as CodeEditor configures it.
vi.mock("@monaco-editor/react", async () => {
  const ReactModule = await import("react");
  const MockMonaco = ({
    defaultValue,
    value = defaultValue,
    onChange,
    onMount,
    options,
  }: {
    value?: string;
    defaultValue?: string;
    onChange: (value: string) => void;
    onMount?: (editor: unknown, monaco: unknown) => void;
    options?: { readOnly?: boolean };
  }) => {
    ReactModule.useEffect(() => {
      onMount?.(
        {
          addCommand: () => undefined,
          getModel: () => null,
          revealLineInCenter: (line: number) => revealed.push(line),
        },
        {
          KeyMod: { CtrlCmd: 0 },
          KeyCode: { KeyS: 0 },
          editor: { defineTheme: () => undefined, setTheme: () => undefined },
        },
      );
    }, []);
    return ReactModule.createElement("textarea", {
      "data-testid": "mock-monaco-editor",
      value,
      readOnly: options?.readOnly === true,
      onChange: (event: React.ChangeEvent<HTMLTextAreaElement>) =>
        onChange(event.currentTarget.value),
    });
  };
  return {
    default: MockMonaco,
    loader: {
      init: () =>
        Promise.resolve({
          editor: { defineTheme: () => {}, setTheme: () => {} },
        }),
    },
  };
});

vi.mock("../shared/MermaidDiagram", () => ({
  MermaidDiagram: ({ code }: { code: string }) => <div>{code}</div>,
}));

// PDF.js never loads here: the viewer's own tests drive a stand-in.
vi.mock("./pdfjs", () => ({
  loadPdfJs: () => new Promise(() => undefined),
  pdfDocumentOptions: () => ({}),
}));

const downloads = vi.hoisted(() => ({ calls: [] as [Blob, string][] }));
vi.mock("../../lib/utils", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/utils")>()),
  downloadBlob: (blob: Blob, name: string) =>
    downloads.calls.push([blob, name]),
}));

import type { Editor } from "@tiptap/core";
import { FileView, type FileViewProps } from "./FileView";
import { HtmlPreviewShell } from "./HtmlPreviewShell";
import { JsonlTranscriptView } from "../shared/JsonlTranscriptView";
import { useFileDraftsAsDirtyEditors } from "./useFileDraftsAsDirtyEditors";
import {
  fetchWorkspaceFile,
  useSaveWorkspaceFile,
  useWorkspaceFileContent,
} from "../../hooks/useFiles";
import {
  fileDraftKey,
  getFileDraft,
  setFileDraft,
  updateFileDraft,
  saveFileEditing,
  saveFileViewMode,
} from "./fileDrafts";
import {
  dirtyEditorPaths,
  resetDirtyEditorsForTests,
} from "../editor/dirtyEditors";

const WS = "ws-1";
const REPO = "repo-1";
// The principal of the token `renderView` installs for Alex.
const PRINCIPAL = "user-alex:org-g5labs";
const README = "# Title\n\nHello.\n";

function file(
  overrides: Partial<WorkspaceFileContent> = {},
): WorkspaceFileContent {
  const content = overrides.content ?? README;
  return {
    path: "docs/readme.md",
    content,
    size: new TextEncoder().encode(content).length,
    encoding: "utf-8",
    hash: "blob-1",
    source: "live",
    repoOid: null,
    snapshotAt: null,
    vmId: "vm-1",
    ...overrides,
  };
}

function props(overrides: Partial<FileViewProps> = {}): FileViewProps {
  return {
    workspaceId: WS,
    repoId: REPO,
    filePath: "docs/readme.md",
    language: "markdown",
    file: file(),
    principal: PRINCIPAL,
    lineTarget: null,
    viewerOwnsWorkspace: true,
    ownerVmRunning: true,
    workspaceWritable: true,
    onSave: vi.fn(async () => ({ hash: "blob-2" })),
    onReload: vi.fn(async () => ({
      content: README,
      hash: "blob-1",
      vmId: "vm-1",
    })),
    onFetchCurrent: vi.fn(async () => ({ content: README, hash: "blob-1" })),
    onOpenWorkspaceFile: vi.fn(),
    onStartWorkspace: vi.fn(),
    editorProps: { showComments: false, comments: [] },
    ...overrides,
  };
}

const draftKey = fileDraftKey({
  person: "user-alex",
  workspaceId: WS,
  repoId: REPO,
  path: "docs/readme.md",
});

/** Renders with Alex signed in, so requests carry his token. */
function renderView(ui: React.ReactElement) {
  return renderApp(ui, { user: createMockUser() });
}

async function editor(): Promise<HTMLTextAreaElement> {
  return (await screen.findByTestId(
    "mock-monaco-editor",
  )) as HTMLTextAreaElement;
}

function pushes() {
  const bodies: Record<string, unknown>[] = [];
  return {
    bodies,
    answer(respond: (body: Record<string, unknown>) => Response) {
      server.use(
        http.post(
          "/api/v1/workspaces/:id/review/file-suggestions",
          async ({ request }) => {
            const body = (await request.json()) as Record<string, unknown>;
            bodies.push(body);
            return respond(body);
          },
        ),
      );
    },
  };
}

const created = () =>
  HttpResponse.json(
    { data: { publication: { id: "pub-1" }, suggestions: [{ id: "sug-1" }] } },
    { status: 201 },
  );

beforeEach(() => {
  downloads.calls.length = 0;
});

describe("FileView reading", () => {
  it.each(
    (["md", "jsonl", "svg"] as const).flatMap((extension) =>
      [1024 * 1024, 1024 * 1024 + 1].map((length) => ({ extension, length })),
    ),
  )(
    "defers $extension previews only above 1 Mi code units and loads them explicitly ($length)",
    async ({ extension, length }) => {
      const filePath = `docs/preview.${extension}`;
      const content = {
        md: "# Preview\n\nText.",
        jsonl: '{"preview":true}',
        svg: '<svg xmlns="http://www.w3.org/2000/svg"></svg>',
      }[extension].padEnd(length, " ");
      const language = {
        md: "markdown",
        jsonl: "json",
        svg: "xml",
      }[extension];
      // JSONL uses the same adapter as WorkspacePage; Markdown and SVG
      // use FileView's built-in renderers.
      const renderPreview: FileViewProps["renderPreview"] =
        extension === "jsonl"
          ? (text) => (
              <JsonlTranscriptView displayPath={filePath} content={text} />
            )
          : undefined;
      const preview = () =>
        screen.queryByTestId(
          {
            md: "document-preview",
            jsonl: "jsonl-transcript-view",
            svg: "image-view",
          }[extension],
        );
      const onSave = vi.fn();
      renderView(
        <FileView
          {...props({
            filePath,
            language,
            file: file({ path: filePath, content }),
            onSave,
            renderPreview,
          })}
        />,
      );
      if (length > 1024 * 1024) {
        expect(preview()).toBeNull();
        expect(
          screen.getByTestId("file-view-deferred-preview"),
        ).toBeInTheDocument();
        fireEvent.click(screen.getByTestId("file-view-load-preview"));
      } else {
        expect(screen.queryByTestId("file-view-load-preview")).toBeNull();
      }
      expect(preview()).toBeInTheDocument();
      fireEvent.click(screen.getByTestId("file-view-switch-mode"));
      expect((await editor()).value).toBe(content);
      expect(onSave).not.toHaveBeenCalled();
    },
  );

  it("opens large HTML automatically and preserves its complete Source", async () => {
    const filePath = "docs/preview.html";
    const content = "<h1>Preview</h1>".padEnd(3 * 1024 * 1024, " ");
    const onSave = vi.fn();
    renderView(
      <FileView
        {...props({
          filePath,
          language: "html",
          file: file({ path: filePath, content }),
          onSave,
          renderPreview: (html) => (
            <HtmlPreviewShell
              displayPath={filePath}
              title="Large HTML preview"
              html={html}
            />
          ),
        })}
      />,
    );
    expect(screen.queryByTestId("file-view-load-preview")).toBeNull();
    expect(screen.getByTitle("Large HTML preview")).toHaveAttribute(
      "srcdoc",
      expect.stringContaining(content),
    );
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    expect((await editor()).value).toBe(content);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("admits a large owner edit but explains the reader's suggestion size limit", async () => {
    const content = "x".repeat(2 * 1024 * 1024 + 1);
    const viewProps = props({
      file: file({ content }),
      viewerOwnsWorkspace: false,
    });
    const view = renderView(<FileView {...viewProps} />);
    expect(screen.getByTestId("file-view-suggestion-size")).toHaveTextContent(
      "too large to suggest",
    );
    expect(screen.queryByTestId("file-view-suggest")).toBeNull();
    view.unmount();
    renderView(<FileView {...viewProps} viewerOwnsWorkspace />);
    expect(screen.queryByTestId("file-view-suggestion-size")).toBeNull();
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(await editor()).not.toHaveAttribute("readonly");
  });

  it("gates suggestion pushes by UTF-8 bytes while retaining an oversized draft for correction", async () => {
    const content = "é".repeat(1024 * 1024);
    const sent = pushes();
    sent.answer(created);
    renderView(
      <FileView
        {...props({ file: file({ content }), viewerOwnsWorkspace: false })}
      />,
    );
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    fireEvent.change(await editor(), { target: { value: content + "é" } });
    expect(screen.getByTestId("file-view-suggestion-size")).toHaveTextContent(
      "Your draft is kept",
    );
    expect(screen.getByTestId("file-view-push-review")).toBeDisabled();
    expect(getFileDraft(draftKey)?.content).toBe(content + "é");
    expect(sent.bodies).toHaveLength(0);
    fireEvent.change(await editor(), {
      target: { value: content.slice(0, -1) + "a" },
    });
    expect(screen.queryByTestId("file-view-suggestion-size")).toBeNull();
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    await waitFor(() => expect(sent.bodies).toHaveLength(1));
    expect(sent.bodies[0].content).toBe(content.slice(0, -1) + "a");
  });

  it("opens Markdown rendered and switches to read-only plain text and back", async () => {
    renderView(<FileView {...props()} />);
    expect(screen.getByTestId("markdown-preview")).toHaveTextContent("Title");
    // No mode tabs: one switch at the foot of the view
    // (`g5.documents.editor.modes`).
    expect(screen.queryByRole("tablist")).toBeNull();
    // Every control sits at the right of the editor's file row.
    const row = screen.getByTestId("code-editor-file-row");
    expect(within(row).getByTestId("code-editor-breadcrumb")).toHaveTextContent(
      "docsreadme.md",
    );
    const toggle = within(row).getByTestId("file-view-switch-mode");
    expect(within(row).getByTestId("file-view-edit")).toBeInTheDocument();
    expect(toggle).toHaveTextContent("Switch to plain text");
    fireEvent.click(toggle);
    expect(await editor()).toHaveAttribute("readonly");
    expect(screen.queryByTestId("markdown-preview")).not.toBeInTheDocument();
    expect(screen.getByTestId("file-view-switch-mode")).toHaveTextContent(
      "Switch to rich text",
    );
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    expect(screen.getByTestId("markdown-preview")).toHaveTextContent("Title");
    expect(screen.queryByTestId("file-view-done")).toBeNull();
  });

  it("names the source it shows in the file row, in both modes", async () => {
    renderView(
      <FileView
        {...props({
          viewerOwnsWorkspace: false,
          file: file({
            source: "flush",
            vmId: null,
            repoOid: "abcdef1234567890",
            snapshotAt: "2026-10-04T10:00:00Z",
          }),
        })}
      />,
    );
    const badge = () => screen.getByTestId("code-editor-source-badge");
    expect(badge()).toHaveTextContent("Saved workspace · abcdef12");
    expect(badge().getAttribute("title")).toContain(
      "Observed: 2026-10-04T10:00:00Z",
    );
    expect(badge().closest(".hidden")).toBeNull();
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    await editor();
    expect(badge()).toHaveTextContent("Saved workspace · abcdef12");
    expect(badge().closest(".hidden")).toBeNull();
  });

  it("opens other text in read-only Code view, and a line target in Code view", async () => {
    renderView(
      <FileView
        {...props({
          filePath: "src/main.ts",
          language: "typescript",
          file: file({ path: "src/main.ts", content: "const a = 1;\n" }),
        })}
      />,
    );
    expect(await editor()).toHaveAttribute("readonly");
    // A file with no rendered form has one mode and no switch.
    expect(
      screen.queryByTestId("file-view-switch-mode"),
    ).not.toBeInTheDocument();
  });

  it("shows plain text for a Markdown file opened at a line", async () => {
    renderView(<FileView {...props({ lineTarget: 2 })} />);
    expect(await editor()).toBeInTheDocument();
    expect(screen.queryByTestId("markdown-preview")).not.toBeInTheDocument();
    expect(screen.getByTestId("file-view-switch-mode")).toHaveTextContent(
      "Switch to rich text",
    );
  });

  it("writes a thread from selected text in the rendered document, quoting it", async () => {
    const user = userEvent.setup();
    const content = "# Title\n\nFirst line here.\n\nSecond line here.\n";
    const onAddComment = vi.fn(async () => undefined);
    const onToggleComments = vi.fn();
    renderView(
      <FileView
        {...props({
          file: file({ content }),
          editorProps: {
            showComments: true,
            comments: [],
            onAddComment,
            onToggleComments,
            commentDraftKey: "repo:docs/readme.md",
          },
        })}
      />,
    );
    const host = await screen.findByTestId("document-preview-content");
    const paragraph = screen.getByText("Second line here.");
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.mouseUp(host);
    fireEvent.click(
      await screen.findByTestId("document-preview-selection-comment-button"),
    );
    const panel = screen.getByTestId("file-view-preview-threads");
    const card = await within(panel).findByTestId("comment-draft-card");
    expect(card).toHaveTextContent("Second line here.");
    await user.type(within(card).getByLabelText("Comment"), "Why second?");
    await user.click(within(card).getByTestId("draft-comment-save"));
    await waitFor(() => expect(onAddComment).toHaveBeenCalledTimes(1));
    expect(onAddComment).toHaveBeenCalledWith(
      expect.objectContaining({
        anchor: expect.objectContaining({ lineNumber: 5 }),
        content: "> Second line here.\n\nWhy second?",
      }),
    );
  });

  it("shows a citation's line after a jump to another line from a comment card", async () => {
    revealed.length = 0;
    const anchor = await createCommentAnchor(README, 3);
    const comment: CommentDTO = {
      id: "thread-3",
      workspaceId: WS,
      authorId: "user-alex",
      authorName: "Alex Chen",
      authorAvatarUrl: null,
      scope: "line",
      repoId: REPO,
      filePath: "docs/readme.md",
      lineNumber: 3,
      lineHash: anchor.lineHash,
      contentHash: anchor.contentHash,
      anchorContext: anchor.anchorContext,
      commitSha: null,
      content: "Hello?",
      resolved: false,
      resolvedAt: null,
      originBranch: "main",
      replyCount: 0,
      createdAt: "2026-10-30T10:00:00Z",
      updatedAt: "2026-10-30T10:00:00Z",
    };
    // The page's citation arrives as a prop; this stands in for the page.
    function Cited() {
      const [lineTarget, setLineTarget] = React.useState<number | null>(null);
      return (
        <>
          <button
            type="button"
            data-testid="cite-line-1"
            onClick={() => setLineTarget(1)}
          >
            Cite line 1
          </button>
          <FileView
            {...props({
              lineTarget,
              editorProps: { showComments: true, comments: [comment] },
            })}
          />
        </>
      );
    }
    renderView(<Cited />);
    // Preview lists the thread; its label opens plain text at line 3.
    fireEvent.click(
      await screen.findByRole("button", { name: "Open line 3 in plain text" }),
    );
    await waitFor(() => expect(revealed).toEqual([3]));
    // A citation to line 1 then shows line 1, not the jump kept before it.
    fireEvent.click(screen.getByTestId("cite-line-1"));
    await waitFor(() => expect(revealed).toEqual([3, 1]));
  });

  it.each([
    {
      name: "base64 bytes",
      over: { encoding: "base64" as const, content: "AAEC" },
      label: "Binary file",
    },
    {
      name: "a NUL byte in a live read",
      over: { content: "a\u0000b" },
      label: "Binary file",
    },
    {
      name: "a NUL byte in a durable read",
      over: { content: "a\u0000b", source: "committed" as const, vmId: null },
      label: "Binary file",
    },
    {
      name: "text over the shared 32 MiB read limit",
      over: { content: "x", size: 32 * 1024 * 1024 + 1 },
      label: "Too large to display",
    },
  ])(
    "shows a card with Download and no editor for $name",
    async ({ over, label }) => {
      renderView(<FileView {...props({ file: file(over) })} />);
      const card = screen.getByTestId("file-view-unavailable");
      expect(card).toHaveTextContent(label);
      expect(
        screen.queryByTestId("mock-monaco-editor"),
      ).not.toBeInTheDocument();
      expect(screen.queryByTestId("file-view-edit")).not.toBeInTheDocument();
      expect(screen.queryByTestId("file-view-suggest")).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId("file-view-download"));
      expect(downloads.calls).toHaveLength(1);
      expect(downloads.calls[0][1]).toBe("readme.md");
      expect(downloads.calls[0][0].size).toBeGreaterThan(0);
    },
  );
});

describe("FileView owner editing", () => {
  it.each(["", "[I1. Uploaded intent]\nOriginal café 中文 😀.\n"])(
    "edits an uploaded Project Doc with exact content %j without legacy requests",
    async (initial) => {
      const requested = vi.fn(() =>
        HttpResponse.json({ error: "Resource not found" }, { status: 404 }),
      );
      server.use(
        http.all("*/api/v1/grounding/review-metadata", requested),
        http.all("*/api/v1/grounding/review-actions", requested),
      );
      const path = `docs/${WS}/request.md`;
      const next =
        initial + "\nNew **content** with  spaces and Unicode: café 中文 😀.\n";
      const onSave = vi.fn(async () => ({ hash: "saved" }));
      const mounted = renderView(
        <FileView
          {...props({
            filePath: path,
            file: file({ content: initial }),
            onSave,
          })}
        />,
      );
      expect(screen.getByTestId("document-preview")).toBeVisible();
      fireEvent.click(screen.getByTestId("file-view-edit"));
      // Markdown opens in Rich text; plain text edits the same draft.
      expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
      fireEvent.click(screen.getByTestId("file-view-switch-mode"));
      expect(await editor()).toHaveValue(initial);
      fireEvent.change(await editor(), { target: { value: next } });
      // Done shows the unsaved text in the reading view; Save stays offered.
      fireEvent.click(screen.getByTestId("file-view-done"));
      expect(screen.getByTestId("markdown-preview")).toHaveTextContent(
        "New content",
      );
      fireEvent.click(screen.getByTestId("code-editor-save-button"));
      await waitFor(() =>
        expect(onSave).toHaveBeenCalledWith(next, "blob-1", "vm-1"),
      );
      // Saved from the reading view: nothing is left to save or discard.
      await waitFor(() => expect(getFileDraft(draftKey)).toBeNull());
      expect(screen.queryByTestId("code-editor-save-button")).toBeNull();
      expect(screen.queryByTestId("file-view-discard")).toBeNull();
      mounted.unmount();
      renderView(
        <FileView
          {...props({
            filePath: path,
            file: file({ content: next, hash: "saved" }),
          })}
        />,
      );
      fireEvent.click(screen.getByTestId("file-view-switch-mode"));
      expect(await editor()).toHaveValue(next);
      expect(requested).not.toHaveBeenCalled();
      expect(screen.queryByText(/Review metadata unavailable/)).toBeNull();
    },
  );

  it("edits in place, previews the unsaved text and saves it", async () => {
    const onSave = vi.fn(async () => ({ hash: "blob-2" }));
    renderView(<FileView {...props({ onSave })} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    const textarea = await editor();
    expect(textarea).not.toHaveAttribute("readonly");
    fireEvent.change(textarea, { target: { value: "# Changed\n" } });

    // Done returns to reading with the unsaved text; Edit resumes editing.
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(screen.getByTestId("markdown-preview")).toHaveTextContent("Changed");
    expect(await editor()).toHaveAttribute("readonly");
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(screen.getByTestId("file-view-switch-mode")).toHaveTextContent(
      "Switch to plain text editing",
    );
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    expect(await editor()).toHaveValue("# Changed\n");
    expect(await editor()).not.toHaveAttribute("readonly");

    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith("# Changed\n", "blob-1", "vm-1"),
    );
    await waitFor(() => expect(getFileDraft(draftKey)).toBeNull());
  });

  it("keeps edits made while a Save is in flight unsaved", async () => {
    let finish!: (value: { hash: string }) => void;
    const onSave = vi.fn(
      () => new Promise<{ hash: string }>((resolve) => (finish = resolve)),
    );
    renderView(<FileView {...props({ onSave })} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    const textarea = await editor();
    fireEvent.change(textarea, { target: { value: "# One\n" } });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    fireEvent.change(textarea, { target: { value: "# One\n# Two\n" } });
    await act(async () => finish({ hash: "blob-2" }));
    const draft = getFileDraft(draftKey);
    expect(draft?.content).toBe("# One\n# Two\n");
    expect(draft?.baseline).toEqual({
      content: "# One\n",
      hash: "blob-2",
      vmId: "vm-1",
    });
  });

  it.each(["Discard", "a drop from outside the view"] as const)(
    "leaves nothing unsaved when a Save finishes after %s",
    async (ending) => {
      let finish!: (value: { hash: string }) => void;
      const onSave = vi.fn(
        () => new Promise<{ hash: string }>((resolve) => (finish = resolve)),
      );
      renderView(<FileView {...props({ onSave })} />);
      fireEvent.click(screen.getByTestId("file-view-edit"));
      fireEvent.change(await editor(), { target: { value: "# Mine\n" } });
      fireEvent.click(screen.getByTestId("code-editor-save-button"));
      if (ending === "Discard") {
        fireEvent.click(screen.getByTestId("file-view-discard"));
        fireEvent.click(screen.getByTestId("file-view-discard-confirm"));
      } else {
        // A review copy's Discard drops it through the page's mirror.
        act(() => setFileDraft(draftKey, null));
      }
      await act(async () => finish({ hash: "blob-2" }));
      if (ending === "Discard") {
        fireEvent.click(screen.getByTestId("file-view-edit"));
      }
      // The editor shows the file as it is, and no draft comes back.
      expect(await editor()).toHaveValue(README);
      await act(async () => {});
      expect(getFileDraft(draftKey)).toBeNull();
    },
  );

  it("acknowledges a Save that finishes after the view closed, without recreating a dropped draft", async () => {
    let finish!: (value: { hash: string }) => void;
    const onSave = vi.fn(
      () => new Promise<{ hash: string }>((resolve) => (finish = resolve)),
    );
    const view = renderView(<FileView {...props({ onSave })} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    fireEvent.change(await editor(), { target: { value: "# Saved\n" } });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    view.unmount();
    await act(async () => finish({ hash: "blob-2" }));
    // Everything typed was submitted, so nothing is left unsaved.
    expect(getFileDraft(draftKey)).toBeNull();
  });

  it("offers a stopped owner Start workspace to edit, and edits once live bytes arrive", async () => {
    const onStartWorkspace = vi.fn();
    const stopped = props({
      ownerVmRunning: false,
      file: file({ source: "committed", vmId: null }),
      onStartWorkspace,
    });
    const controls: { show: (next: FileViewProps) => void } = {
      show: () => undefined,
    };
    const Harness = () => {
      const [current, setCurrent] = React.useState(stopped);
      React.useEffect(() => {
        controls.show = setCurrent;
      }, []);
      return <FileView {...current} />;
    };
    renderView(<Harness />);
    expect(screen.queryByTestId("file-view-edit")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("file-view-start-to-edit"));
    expect(onStartWorkspace).toHaveBeenCalled();
    expect(screen.getByTestId("file-view-start-to-edit")).toBeDisabled();

    // Running, but the durable bytes are still shown: no editing yet.
    act(() => controls.show({ ...stopped, ownerVmRunning: true }));
    expect(screen.queryByTestId("file-view-done")).not.toBeInTheDocument();
    // The live read arrives: editing begins on it, in Rich text.
    act(() =>
      controls.show({ ...stopped, ownerVmRunning: true, file: file() }),
    );
    expect(await screen.findByTestId("file-view-done")).toBeInTheDocument();
    expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
    expect(await editor()).not.toHaveAttribute("readonly");
  });

  it("offers no editing on a workspace that accepts no writes", () => {
    renderView(<FileView {...props({ workspaceWritable: false })} />);
    expect(screen.queryByTestId("file-view-edit")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("file-view-start-to-edit"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("file-view-suggest")).not.toBeInTheDocument();
  });
});

describe("FileView Saves in flight", () => {
  type Navigation = "stay" | "close" | "reopen";

  /**
   * The page as it runs: the file read and Save hooks over HTTP and the
   * page's draft mirror. The first PUT waits until `finish` answers it;
   * later ones are answered at once, as saved.
   */
  function servePage(answer: "saved" | "refused") {
    let current = file();
    const pending: {
      finish: () => void;
      sent: boolean;
      bodies: { content: string; expectedHash: string | null }[];
    } = { finish: () => undefined, sent: false, bodies: [] };
    // Set once another writer changed the file after the Save landed: the
    // Save's answer then leaves that version on the server.
    const keepServerFile = { value: false };
    const saved = (content: string, hash: string) => {
      if (!keepServerFile.value) current = file({ content, hash });
      return HttpResponse.json({
        data: { hash, size: content.length, encoding: "utf-8", vmId: "vm-1" },
      });
    };
    server.use(
      http.get("/api/v1/workspaces/:id/files/*", () =>
        HttpResponse.json({ data: current }),
      ),
      http.put("/api/v1/workspaces/:id/files/*", async ({ request }) => {
        const body = (await request.json()) as {
          content: string;
          expectedHash: string | null;
        };
        pending.bodies.push(body);
        if (pending.sent) return saved(body.content, "saved-again");
        pending.sent = true;
        return new Promise<Response>((resolve) => {
          pending.finish = () =>
            resolve(
              answer === "refused"
                ? HttpResponse.json(
                    {
                      error: {
                        code: "VALIDATION_ERROR",
                        message: "Save refused",
                      },
                    },
                    { status: 422 },
                  )
                : saved(body.content, "saved-hash"),
            );
        });
      }),
    );
    const controls: {
      show: (visible: boolean) => void;
      refresh: () => Promise<unknown>;
    } = { show: () => undefined, refresh: async () => undefined };
    const Page = () => {
      const [visible, setVisible] = React.useState(true);
      const read = useWorkspaceFileContent(WS, REPO, "docs/readme.md");
      React.useEffect(() => {
        controls.show = setVisible;
        controls.refresh = read.reload;
      }, [read.reload]);
      useFileDraftsAsDirtyEditors("user-alex", WS);
      const save = useSaveWorkspaceFile();
      if (read.state.status !== "ok" || !visible) return null;
      return (
        <FileView
          {...props({
            file: read.state.data,
            onSave: (content, expectedHash, expectedVmId) =>
              save.mutateAsync({
                workspaceId: WS,
                repoId: REPO,
                path: "docs/readme.md",
                content,
                expectedHash,
                expectedVmId,
              }),
          })}
        />
      );
    };
    renderView(<Page />);
    /**
     * Another version of the file reaches the page, as a refresh would, and
     * is rendered before anything else happens (queries tell their
     * observers on a timer).
     */
    const deliver = async (next: WorkspaceFileContent) => {
      current = next;
      await act(async () => {
        await controls.refresh();
        await new Promise((resolve) => setTimeout(resolve, 20));
      });
    };
    return { pending, controls, deliver, keepServerFile };
  }

  const settleSave = async (page: ReturnType<typeof servePage>) =>
    act(async () => {
      page.pending.finish();
      await new Promise((resolve) => setTimeout(resolve, 50));
    });

  async function startSave(page: ReturnType<typeof servePage>) {
    fireEvent.click(await screen.findByTestId("file-view-edit"));
    fireEvent.change(await editor(), { target: { value: "# Submitted\n" } });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() => expect(page.pending.sent).toBe(true));
  }

  async function reopen(page: ReturnType<typeof servePage>) {
    act(() => page.controls.show(false));
    act(() => page.controls.show(true));
    await editor();
  }

  /** Save "# Submitted", type the original text back, then move as asked. */
  async function typeBackDuringSave(
    navigation: Navigation,
    page: ReturnType<typeof servePage>,
  ) {
    fireEvent.click(await screen.findByTestId("file-view-edit"));
    fireEvent.change(await editor(), { target: { value: "# Submitted\n" } });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() => expect(page.pending.sent).toBe(true));
    fireEvent.change(await editor(), { target: { value: README } });
    if (navigation !== "stay") act(() => page.controls.show(false));
    if (navigation === "reopen") act(() => page.controls.show(true));
    await act(async () => {
      page.pending.finish();
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    if (navigation === "close") act(() => page.controls.show(true));
  }

  it.each(["stay", "close", "reopen"] as const)(
    "keeps text typed back to the original during a Save, unsaved against the saved version (%s)",
    async (navigation) => {
      const page = servePage("saved");
      await typeBackDuringSave(navigation, page);
      expect(await editor()).toHaveValue(README);
      expect(getFileDraft(draftKey)?.content).toBe(README);
      expect(getFileDraft(draftKey)?.baseline).toMatchObject({
        content: "# Submitted\n",
        hash: "saved-hash",
      });
      expect(getFileDraft(draftKey)?.saving).toBeUndefined();
      expect(
        screen.queryByTestId("code-editor-disk-change"),
      ).not.toBeInTheDocument();
    },
  );

  it.each(["stay", "reopen"] as const)(
    "offers Save for text typed back during a Save when the saved version shows first (%s)",
    async (navigation) => {
      const page = servePage("saved");
      fireEvent.click(await screen.findByTestId("file-view-edit"));
      fireEvent.change(await editor(), {
        target: { value: "# Submitted\n" },
      });
      fireEvent.click(screen.getByTestId("code-editor-save-button"));
      await waitFor(() => expect(page.pending.sent).toBe(true));
      fireEvent.change(await editor(), { target: { value: README } });
      if (navigation === "reopen") {
        act(() => page.controls.show(false));
        act(() => page.controls.show(true));
      }
      // The saved version reaches the page before the Save's own answer.
      await page.deliver(
        file({ content: "# Submitted\n", hash: "saved-hash" }),
      );
      expect(await editor()).toHaveValue(README);
      await act(async () => {
        page.pending.finish();
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      expect(getFileDraft(draftKey)).toMatchObject({
        content: README,
        baseline: { content: "# Submitted\n", hash: "saved-hash" },
      });
      expect(
        screen.queryByTestId("code-editor-disk-change"),
      ).not.toBeInTheDocument();
      expect(screen.getByTestId("code-editor-dirty-indicator")).toBeVisible();
      const save = screen.getByTestId("code-editor-save-button");
      expect(save).toBeEnabled();
      fireEvent.click(save);
      await waitFor(() =>
        expect(page.pending.bodies[1]).toMatchObject({
          content: README,
          expectedHash: "saved-hash",
        }),
      );
    },
  );

  it.each(["stay", "reopen"] as const)(
    "shows a change on disk that arrived during a Save once the Save is refused (%s)",
    async (navigation) => {
      const page = servePage("refused");
      fireEvent.click(await screen.findByTestId("file-view-edit"));
      fireEvent.change(await editor(), {
        target: { value: "# Submitted\n" },
      });
      fireEvent.click(screen.getByTestId("code-editor-save-button"));
      await waitFor(() => expect(page.pending.sent).toBe(true));
      if (navigation === "reopen") {
        act(() => page.controls.show(false));
        act(() => page.controls.show(true));
      }
      await page.deliver(
        file({ content: "# Agent changed this file\n", hash: "agent-hash" }),
      );
      // Nothing is said while the Save may still be what changed it.
      expect(
        screen.queryByTestId("code-editor-disk-change"),
      ).not.toBeInTheDocument();
      await act(async () => {
        page.pending.finish();
        await new Promise((resolve) => setTimeout(resolve, 50));
      });
      expect(getFileDraft(draftKey)?.content).toBe("# Submitted\n");
      expect(await editor()).toHaveValue("# Submitted\n");
      expect(
        await screen.findByTestId("code-editor-disk-change"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("code-editor-disk-change-compare"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("code-editor-disk-change-reload"),
      ).toBeInTheDocument();
      expect(
        screen.getByTestId("code-editor-disk-change-overwrite"),
      ).toBeInTheDocument();
    },
  );

  it.each(["stay", "reopen"] as const)(
    "lets a clean buffer take a version that arrived during a refused Save (%s)",
    async (navigation) => {
      const page = servePage("refused");
      await startSave(page);
      fireEvent.change(await editor(), { target: { value: README } });
      if (navigation === "reopen") await reopen(page);
      await page.deliver(file({ content: "# Agent\n", hash: "agent" }));
      await settleSave(page);
      expect(await editor()).toHaveValue("# Agent\n");
      expect(getFileDraft(draftKey)).toBeNull();
      expect(screen.getByTestId("code-editor-save-button")).toBeDisabled();
    },
  );

  it.each(["stay", "reopen"] as const)(
    "drops the version Save my version replaced, keeping later typing (%s)",
    async (navigation) => {
      const page = servePage("saved");
      fireEvent.click(await screen.findByTestId("file-view-edit"));
      fireEvent.change(await editor(), {
        target: { value: "# Submitted\n" },
      });
      await page.deliver(file({ content: "# Agent\n", hash: "agent" }));
      fireEvent.click(screen.getByTestId("code-editor-disk-change-overwrite"));
      await waitFor(() => expect(page.pending.sent).toBe(true));
      fireEvent.change(await editor(), {
        target: { value: "# Later typing\n" },
      });
      if (navigation === "reopen") await reopen(page);
      await settleSave(page);
      expect(await editor()).toHaveValue("# Later typing\n");
      expect(getFileDraft(draftKey)?.baseline).toMatchObject({
        content: "# Submitted\n",
        hash: "saved-hash",
      });
      expect(
        screen.queryByTestId("code-editor-disk-change"),
      ).not.toBeInTheDocument();
      expect(screen.getByTestId("code-editor-save-button")).toBeEnabled();
    },
  );

  it.each(["stay", "reopen"] as const)(
    "treats a Save's own landing as no change, also when reopened after it (%s)",
    async (navigation) => {
      const page = servePage("saved");
      await startSave(page);
      fireEvent.change(await editor(), {
        target: { value: "# Later typing\n" },
      });
      await page.deliver(
        file({ content: "# Submitted\n", hash: "saved-hash" }),
      );
      if (navigation === "reopen") await reopen(page);
      await settleSave(page);
      expect(await editor()).toHaveValue("# Later typing\n");
      expect(getFileDraft(draftKey)?.baseline.content).toBe("# Submitted\n");
      expect(
        screen.queryByTestId("code-editor-disk-change"),
      ).not.toBeInTheDocument();
    },
  );

  it.each(["stay", "reopen"] as const)(
    "shows a version written after the Save landed as a change on disk (%s)",
    async (navigation) => {
      const page = servePage("saved");
      await startSave(page);
      fireEvent.change(await editor(), {
        target: { value: "# Later typing\n" },
      });
      if (navigation === "reopen") await reopen(page);
      await page.deliver(
        file({ content: "# Submitted\n", hash: "saved-hash" }),
      );
      // Another writer changes the file before the Save's answer arrives.
      page.keepServerFile.value = true;
      await page.deliver(file({ content: "# Agent after\n", hash: "agent" }));
      await settleSave(page);
      expect(
        (await fetchWorkspaceFile(WS, REPO, "docs/readme.md")).content,
      ).toBe("# Agent after\n");
      expect(await editor()).toHaveValue("# Later typing\n");
      fireEvent.click(
        await screen.findByTestId("code-editor-disk-change-compare"),
      );
      expect(await screen.findByTestId("file-compare-newer")).toHaveValue(
        "# Agent after\n",
      );
    },
  );

  it.each(["stay", "reopen"] as const)(
    "lets a clean buffer follow a version written after the Save landed (%s)",
    async (navigation) => {
      const page = servePage("saved");
      await startSave(page);
      if (navigation === "reopen") await reopen(page);
      await page.deliver(
        file({ content: "# Submitted\n", hash: "saved-hash" }),
      );
      page.keepServerFile.value = true;
      await page.deliver(file({ content: "# Agent after\n", hash: "agent" }));
      await settleSave(page);
      expect(await editor()).toHaveValue("# Agent after\n");
      expect(getFileDraft(draftKey)).toBeNull();
    },
  );

  it.each(["stay", "close", "reopen"] as const)(
    "returns clean to the original file when that Save is refused (%s)",
    async (navigation) => {
      const page = servePage("refused");
      await typeBackDuringSave(navigation, page);
      expect(await editor()).toHaveValue(README);
      await waitFor(() => expect(getFileDraft(draftKey)).toBeNull());
    },
  );
});

describe("FileView drafts", () => {
  it("keeps unsaved text across a remount and reopens it in Edit", async () => {
    const view = renderView(<FileView {...props()} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    fireEvent.change(await editor(), { target: { value: "# Kept\n" } });
    view.unmount();
    expect(getFileDraft(draftKey)?.content).toBe("# Kept\n");

    renderView(<FileView {...props()} />);
    expect(await screen.findByTestId("file-view-done")).toBeInTheDocument();
    expect(await editor()).toHaveValue("# Kept\n");
  });

  it("asks before Discard, then drops the draft and returns to reading", async () => {
    renderView(<FileView {...props()} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    fireEvent.change(await editor(), { target: { value: "# Gone\n" } });
    fireEvent.click(screen.getByTestId("file-view-discard"));
    fireEvent.click(screen.getByTestId("file-view-discard-cancel"));
    expect(getFileDraft(draftKey)).not.toBeNull();
    fireEvent.click(screen.getByTestId("file-view-discard"));
    fireEvent.click(screen.getByTestId("file-view-discard-confirm"));
    expect(getFileDraft(draftKey)).toBeNull();
    expect(screen.getByTestId("markdown-preview")).toHaveTextContent("Title");
  });

  // The editor stays mounted while reading, so an ended draft must not
  // linger in it as unsaved text.
  it.each([
    {
      ending: "Discard",
      role: "owner",
      start: "file-view-edit",
      over: {},
    },
    {
      ending: "a pushed review",
      role: "reader",
      start: "file-view-suggest",
      over: {
        viewerOwnsWorkspace: false,
        ownerVmRunning: false,
        file: file({ source: "committed", vmId: null }),
      },
    },
  ] as const)(
    "leaves no edits behind after $ending ($role): the next version reads as is and editing starts from it",
    async ({ ending, start, over }) => {
      resetDirtyEditorsForTests();
      pushes().answer(created);
      const initial = props(over);
      const controls: { show: (next: FileViewProps) => void } = {
        show: () => undefined,
      };
      const Harness = () => {
        const [current, setCurrent] = React.useState(initial);
        React.useEffect(() => {
          controls.show = setCurrent;
        }, []);
        return <FileView {...current} />;
      };
      renderView(<Harness />);
      fireEvent.click(screen.getByTestId(start));
      fireEvent.change(await editor(), { target: { value: "# Mine\n" } });
      if (ending === "Discard") {
        fireEvent.click(screen.getByTestId("file-view-discard"));
        fireEvent.click(screen.getByTestId("file-view-discard-confirm"));
      } else {
        fireEvent.click(screen.getByTestId("file-view-push-review"));
        fireEvent.click(screen.getByTestId("file-view-push-submit"));
        await screen.findByTestId("file-view-push-done");
      }
      expect(getFileDraft(draftKey)).toBeNull();
      await waitFor(() => expect(dirtyEditorPaths()).toEqual([]));

      const newer = "# Title\n\nNewer.\n";
      act(() =>
        controls.show({
          ...initial,
          file: { ...initial.file, content: newer, hash: "blob-2" },
        }),
      );
      expect(
        screen.queryByTestId("code-editor-disk-change"),
      ).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId(start));
      expect(await editor()).toHaveValue(newer);
      expect(dirtyEditorPaths()).toEqual([]);
    },
  );
});

describe("FileView reader suggestions", () => {
  const reader = (over: Partial<FileViewProps> = {}) =>
    props({
      viewerOwnsWorkspace: false,
      ownerVmRunning: false,
      file: file({ source: "committed", vmId: null, repoOid: "oid-1" }),
      ...over,
    });

  it("suggests changes and pushes them with a note, with the owner's VM stopped", async () => {
    const pushed = pushes();
    pushed.answer(created);
    renderView(<FileView {...reader()} />);
    expect(screen.queryByTestId("file-view-edit")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    expect(screen.getByTestId("file-view-suggesting")).toBeInTheDocument();
    expect(
      screen.queryByTestId("code-editor-save-button"),
    ).not.toBeInTheDocument();
    fireEvent.change(await editor(), {
      target: { value: "# Title\n\nHello, world.\n" },
    });

    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.change(screen.getByTestId("file-view-push-note"), {
      target: { value: "Friendlier" },
    });
    fireEvent.click(screen.getByTestId("file-view-push-submit"));

    expect(await screen.findByTestId("file-view-push-done")).toHaveTextContent(
      "Pushed 1 suggestion",
    );
    expect(pushed.bodies).toEqual([
      {
        repoId: REPO,
        path: "docs/readme.md",
        baseContent: README,
        content: "# Title\n\nHello, world.\n",
        note: "Friendlier",
        allowStaleBase: false,
      },
    ]);
    expect(getFileDraft(draftKey)).toBeNull();
    expect(screen.getByTestId("markdown-preview")).toBeInTheDocument();
  });

  it("warns about a stale start, merges through Compare, or pushes anyway", async () => {
    const pushed = pushes();
    pushed.answer((body) =>
      body.allowStaleBase || body.baseContent === "# Title\n\nNewer.\n"
        ? created()
        : HttpResponse.json(
            {
              error: {
                code: "FILE_VERSION_CONFLICT",
                message: "changed",
                details: { hash: "blob-9" },
              },
            },
            { status: 409 },
          ),
    );
    const onFetchCurrent = vi.fn(async () => ({
      content: "# Title\n\nNewer.\n",
      hash: "blob-9",
    }));
    renderView(<FileView {...reader({ onFetchCurrent })} />);
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    fireEvent.change(await editor(), {
      target: { value: "# Title\n\nMine.\n" },
    });
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    expect(await screen.findByTestId("file-view-stale")).toBeInTheDocument();
    expect(getFileDraft(draftKey)?.content).toBe("# Title\n\nMine.\n");

    // Compare: merge into the newer version, which becomes the start.
    fireEvent.click(screen.getByTestId("file-view-stale-compare"));
    const own = (await screen.findByTestId(
      "file-compare-own",
    )) as HTMLTextAreaElement;
    expect(own).toHaveValue("# Title\n\nMine.\n");
    fireEvent.change(own, {
      target: { value: "# Title\n\nNewer and mine.\n" },
    });
    fireEvent.click(screen.getByTestId("file-compare-apply"));
    await waitFor(() =>
      expect(getFileDraft(draftKey)?.baseline.content).toBe(
        "# Title\n\nNewer.\n",
      ),
    );
    expect(await editor()).toHaveValue("# Title\n\nNewer and mine.\n");
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    await screen.findByTestId("file-view-push-done");
    expect(pushed.bodies.at(-1)).toMatchObject({
      baseContent: "# Title\n\nNewer.\n",
      content: "# Title\n\nNewer and mine.\n",
      allowStaleBase: false,
    });
  });

  it("pushes against the original start when the reader chooses Push anyway", async () => {
    const pushed = pushes();
    pushed.answer((body) =>
      body.allowStaleBase
        ? created()
        : HttpResponse.json(
            {
              error: {
                code: "FILE_VERSION_CONFLICT",
                message: "changed",
                details: { hash: "blob-9" },
              },
            },
            { status: 409 },
          ),
    );
    renderView(<FileView {...reader()} />);
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    fireEvent.change(await editor(), {
      target: { value: "# Title\n\nMine.\n" },
    });
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    fireEvent.click(await screen.findByTestId("file-view-stale-push-anyway"));
    await screen.findByTestId("file-view-push-done");
    expect(pushed.bodies.at(-1)).toMatchObject({
      baseContent: README,
      content: "# Title\n\nMine.\n",
      allowStaleBase: true,
    });
  });

  it("keeps every view of a draft locked while its push is in flight, then drops what was pushed", async () => {
    let answer!: () => void;
    server.use(
      http.post(
        "/api/v1/workspaces/:id/review/file-suggestions",
        () =>
          new Promise((resolve) => {
            answer = () => resolve(created());
          }) as unknown as Response,
      ),
    );
    const first = renderView(<FileView {...reader()} />);
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    fireEvent.change(await editor(), {
      target: { value: "# Title\n\nHello, reader.\n" },
    });
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    await waitFor(() => expect(getFileDraft(draftKey)?.pushing).toBe(true));

    // Leave and come back while the push is still in flight.
    first.unmount();
    renderView(<FileView {...reader()} />);
    expect(await screen.findByTestId("file-view-pushing")).toBeInTheDocument();
    const textarea = await editor();
    expect(textarea).toHaveAttribute("readonly");
    fireEvent.change(textarea, { target: { value: "typed meanwhile" } });
    expect(getFileDraft(draftKey)?.content).toBe("# Title\n\nHello, reader.\n");
    expect(screen.getByTestId("file-view-push-review")).toBeDisabled();
    expect(screen.getByTestId("file-view-discard")).toBeDisabled();

    await act(async () => answer());
    await waitFor(() => expect(getFileDraft(draftKey)).toBeNull());
    expect(screen.queryByTestId("file-view-pushing")).not.toBeInTheDocument();
  });

  it("drops only the version it pushed, keeping text that changed meanwhile", async () => {
    let answer!: () => void;
    server.use(
      http.post(
        "/api/v1/workspaces/:id/review/file-suggestions",
        () =>
          new Promise((resolve) => {
            answer = () => resolve(created());
          }) as unknown as Response,
      ),
    );
    renderView(<FileView {...reader()} />);
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    fireEvent.change(await editor(), {
      target: { value: "# Title\n\nPushed.\n" },
    });
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    await waitFor(() => expect(getFileDraft(draftKey)?.pushing).toBe(true));
    const pushedId = getFileDraft(draftKey)!.id;
    act(() => {
      updateFileDraft(draftKey, pushedId, (current) => ({
        ...current,
        content: "# Title\n\nPushed.\n\nAnd later.\n",
      }));
    });

    await act(async () => answer());
    await waitFor(() => expect(getFileDraft(draftKey)?.pushing).toBe(false));
    expect(getFileDraft(draftKey)?.content).toBe(
      "# Title\n\nPushed.\n\nAnd later.\n",
    );
    expect(screen.getByTestId("file-view-suggesting")).toBeInTheDocument();
  });

  it("never sends Push review under another person's token, and sends it after the same person's renewal", async () => {
    const pushed = pushes();
    pushed.answer(created);
    renderView(<FileView {...reader()} />);
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    fireEvent.change(await editor(), {
      target: { value: "# Title\n\nAlex's words.\n" },
    });
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.change(screen.getByTestId("file-view-push-note"), {
      target: { value: "Alex's note" },
    });
    const shownTo = tokenPrincipal(getApiToken());
    expect(shownTo).toBe(PRINCIPAL);
    const [sub, org] = PRINCIPAL.split(":");
    const encode = (value: object) =>
      btoa(JSON.stringify(value))
        .replace(/\+/g, "-")
        .replace(/\//g, "_")
        .replace(/=+$/, "");
    const token = (person: string, signature: string) =>
      `${encode({ alg: "HS256" })}.${encode({ sub: person, org, scopes: [] })}.${signature}`;

    act(() => setApiToken(token("user-bob", "bob")));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    expect(await screen.findByTestId("file-view-push-error")).toHaveTextContent(
      "signed in as someone else",
    );
    expect(pushed.bodies).toEqual([]);
    expect(getFileDraft(draftKey)?.content).toBe("# Title\n\nAlex's words.\n");
    expect(getFileDraft(draftKey)?.pushing).toBe(false);

    act(() => setApiToken(token(sub, "renewed")));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    expect(
      await screen.findByTestId("file-view-push-done"),
    ).toBeInTheDocument();
    expect(pushed.bodies).toHaveLength(1);
    expect(pushed.bodies[0]).not.toHaveProperty("principal");
  });

  it("keeps the draft when a push is refused, and takes no typing while it is sent", async () => {
    let refuse!: () => void;
    const pushed = pushes();
    pushed.answer(
      () =>
        new Promise<Response>((resolve) => {
          refuse = () =>
            resolve(
              HttpResponse.json(
                {
                  error: {
                    code: "VALIDATION_ERROR",
                    message: "No changes to suggest",
                  },
                },
                { status: 422 },
              ),
            );
        }) as unknown as Response,
    );
    renderView(<FileView {...reader()} />);
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    const textarea = await editor();
    fireEvent.change(textarea, { target: { value: "# Title\n\nHello.\n\n" } });
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    await waitFor(() => expect(textarea).toHaveAttribute("readonly"));
    fireEvent.change(textarea, { target: { value: "lost?" } });
    await act(async () => refuse());
    expect(await screen.findByTestId("file-view-push-error")).toHaveTextContent(
      "No changes to suggest",
    );
    expect(getFileDraft(draftKey)?.content).toBe("# Title\n\nHello.\n\n");
    expect(textarea).toHaveValue("# Title\n\nHello.\n\n");
  });
});

describe("domain preview Save uses the same session", () => {
  it("saves an HTML action without publishing, then explicitly Saves source with a checkpoint", async () => {
    const writes: Record<string, unknown>[] = [];
    server.use(
      http.get("/api/v1/workspaces/:id/files/*", () =>
        HttpResponse.json({ data: file() }),
      ),
      http.put("/api/v1/workspaces/:id/files/*", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        writes.push(body);
        return HttpResponse.json({
          data: {
            path: "docs/readme.md",
            hash: `saved-${writes.length}`,
            size: String(body.content).length,
            encoding: "utf-8",
            vmId: "vm-1",
          },
        });
      }),
    );
    function Page() {
      const read = useWorkspaceFileContent(WS, REPO, "docs/readme.md");
      const save = useSaveWorkspaceFile();
      if (read.state.status !== "ok") return null;
      return (
        <FileView
          {...props({
            file: read.state.data,
            onSave: (content, expectedHash, expectedVmId, options) =>
              save.mutateAsync({
                workspaceId: WS,
                repoId: REPO,
                path: "docs/readme.md",
                content,
                expectedHash,
                expectedVmId,
                commitAndPush: options?.commitAndPush,
              }),
            renderPreview: (_content, actions) => (
              <button
                disabled={!actions.canSave}
                onClick={() => void actions.save("# Answered\n")}
              >
                Apply answer
              </button>
            ),
          })}
        />
      );
    }
    renderView(<Page />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Apply answer" }),
    );
    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toMatchObject({
      content: "# Answered\n",
      expectedHash: "blob-1",
      expectedVmId: "vm-1",
      commitAndPush: false,
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Apply answer" }),
      ).toBeEnabled(),
    );
    // A domain write never enters editing; Edit opens HTML as plain text.
    expect(screen.queryByTestId("file-view-done")).toBeNull();
    fireEvent.click(screen.getByTestId("file-view-edit"));
    fireEvent.change(await editor(), {
      target: { value: "# Explicit source edit\n" },
    });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() => expect(writes).toHaveLength(2));
    expect(writes[1]).toMatchObject({
      content: "# Explicit source edit\n",
      expectedHash: "saved-1",
      expectedVmId: "vm-1",
      commitAndPush: true,
    });
  });

  it("keeps a failed HTML action in the shared draft and serializes it with Source Save", async () => {
    let reject!: (error: Error) => void;
    const save = vi.fn(
      () =>
        new Promise<{ hash: string }>((_resolve, fail) => {
          reject = fail;
        }),
    );
    renderView(
      <FileView
        {...props({
          onSave: save,
          renderPreview: (_text, actions) => (
            <button
              disabled={!actions.canSave}
              onClick={() => void actions.save("domain edit").catch(() => {})}
            >
              Answer in HTML
            </button>
          ),
        })}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Answer in HTML" }));
    expect(save).toHaveBeenCalledWith("domain edit", "blob-1", "vm-1", {
      commitAndPush: false,
    });
    expect(screen.getByTestId("code-editor-save-button")).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Answer in HTML" }),
    ).toBeDisabled();
    await act(async () => reject(new Error("Publication refused")));
    await screen.findByText(/Save failed: Publication refused/);
    expect(getFileDraft(draftKey)?.content).toBe("domain edit");
    // The failed write stays a draft the reading view offers to Save or
    // Discard; Edit continues from it in plain text.
    expect(screen.getByTestId("file-view-discard")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(await screen.findByTestId("mock-monaco-editor")).toHaveValue(
      "domain edit",
    );
    expect(await editor()).not.toHaveAttribute("readonly");
    expect(save).toHaveBeenCalledTimes(1);
  });
});

describe("FileView rich editing in Preview", () => {
  // `g5.documents.preview.rich-editing`: Markdown is edited as a rendered
  // document in Preview; its writes join the same draft, Save and Push.
  const richEditor = () =>
    (screen.getByTestId("rich-editor-content") as unknown as { editor: Editor })
      .editor;
  const typeAfter = (text: string, insert: string) =>
    act(() => {
      const editor = richEditor();
      let at = -1;
      editor.state.doc.descendants((node, pos) => {
        if (at >= 0) return false;
        if (node.isText && node.text?.includes(text))
          at = pos + node.text.indexOf(text) + text.length;
        return true;
      });
      editor.chain().insertContentAt(at, insert).run();
    });

  it("opens Markdown in Rich text and saves what was typed", async () => {
    const onSave = vi.fn(async () => ({ hash: "blob-2" }));
    renderView(<FileView {...props({ onSave })} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
    typeAfter("Hello", ", world");
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        "# Title\n\nHello, world.\n",
        "blob-1",
        "vm-1",
      ),
    );
  });

  it("shows the written Markdown in plain text, and the draft in the reading view after Done", async () => {
    renderView(<FileView {...props()} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    await screen.findByTestId("rich-editor");
    const toggle = () => screen.getByTestId("file-view-switch-mode");
    expect(toggle()).toHaveTextContent("Switch to plain text editing");
    typeAfter("Hello", " again");
    fireEvent.click(toggle());
    expect(await editor()).toHaveValue("# Title\n\nHello again.\n");
    expect(await editor()).not.toHaveAttribute("readonly");
    expect(getFileDraft(draftKey)?.content).toBe("# Title\n\nHello again.\n");
    expect(toggle()).toHaveTextContent("Switch to rich text editing");
    fireEvent.click(toggle());
    expect(screen.getByTestId("rich-editor-content")).toHaveTextContent(
      "Hello again.",
    );
    // Done is always available: it keeps the draft, shows it in the reading
    // view and leaves Save and Discard there (`g5.documents.preview.rich-editing`).
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(screen.queryByTestId("rich-editor")).toBeNull();
    expect(screen.getByTestId("document-preview")).toHaveTextContent(
      "Hello again.",
    );
    expect(screen.queryByTestId("file-view-done")).toBeNull();
    expect(screen.getByTestId("file-view-discard")).toBeInTheDocument();
    expect(screen.getByTestId("code-editor-save-button")).toBeEnabled();
    expect(screen.getByTestId("code-editor-dirty-indicator")).toBeVisible();
    expect(toggle()).toHaveTextContent("Switch to plain text");
    // Plain text while reading shows the draft read-only.
    fireEvent.click(toggle());
    expect(await editor()).toHaveValue("# Title\n\nHello again.\n");
    expect(await editor()).toHaveAttribute("readonly");
  });

  it("reopens editing when the person left while editing, even without a draft", async () => {
    // A Save clears the draft while editing goes on; the file comes back as
    // it was left (`g5.documents.editor.modes`).
    saveFileViewMode(draftKey, "preview");
    saveFileEditing(draftKey, true);
    renderView(<FileView {...props()} />);
    expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
    expect(screen.getByTestId("file-view-done")).toBeInTheDocument();
  });

  it("reopens reading with the draft applied after Done, and saves it from there", async () => {
    const onSave = vi.fn(async () => ({ hash: "blob-2" }));
    const view = renderView(<FileView {...props({ onSave })} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    await screen.findByTestId("rich-editor");
    typeAfter("Hello", " kept");
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(getFileDraft(draftKey)?.content).toBe("# Title\n\nHello kept.\n");
    view.unmount();
    renderView(<FileView {...props({ onSave })} />);
    expect(screen.queryByTestId("rich-editor")).toBeNull();
    expect(screen.getByTestId("document-preview")).toHaveTextContent(
      "Hello kept.",
    );
    expect(screen.queryByTestId("file-view-done")).toBeNull();
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        "# Title\n\nHello kept.\n",
        "blob-1",
        "vm-1",
      ),
    );
    await waitFor(() => expect(getFileDraft(draftKey)).toBeNull());
    expect(screen.queryByTestId("file-view-discard")).toBeNull();
    expect(screen.getByTestId("file-view-edit")).toBeInTheDocument();
  });

  it("drops the draft on Discard from the reading view after Done", async () => {
    renderView(<FileView {...props()} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    await screen.findByTestId("rich-editor");
    typeAfter("Hello", " gone");
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(screen.getByTestId("document-preview")).toHaveTextContent(
      "Hello gone.",
    );
    fireEvent.click(screen.getByTestId("file-view-discard"));
    fireEvent.click(screen.getByTestId("file-view-discard-confirm"));
    expect(getFileDraft(draftKey)).toBeNull();
    expect(screen.getByTestId("document-preview")).toHaveTextContent("Hello.");
    expect(screen.queryByTestId("file-view-discard")).toBeNull();
    expect(screen.queryByTestId("rich-editor")).toBeNull();
  });

  it("keeps a reader's Push review and draft in the reading view after Done", async () => {
    renderView(
      <FileView
        {...props({
          viewerOwnsWorkspace: false,
          ownerVmRunning: false,
          file: file({ source: "committed", vmId: null, repoOid: "oid-1" }),
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    await screen.findByTestId("rich-editor");
    typeAfter("Hello", ", reader");
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(screen.queryByTestId("rich-editor")).toBeNull();
    expect(screen.getByTestId("document-preview")).toHaveTextContent(
      "Hello, reader.",
    );
    expect(screen.getByTestId("file-view-push-review")).toBeEnabled();
    expect(screen.getByTestId("file-view-suggesting")).toBeInTheDocument();
    expect(screen.getByTestId("file-view-discard")).toBeInTheDocument();
    expect(screen.queryByTestId("code-editor-save-button")).toBeNull();
    expect(screen.getByTestId("file-view-suggest")).toBeInTheDocument();
  });

  it("switches an HTML preview to plain text and back, and edits it as plain text", async () => {
    renderView(
      <FileView
        {...props({
          filePath: "docs/page.html",
          language: "html",
          file: file({ path: "docs/page.html", content: "<p>hi</p>\n" }),
          renderPreview: (content) => (
            <div data-testid="html-fixture">{content}</div>
          ),
        })}
      />,
    );
    expect(screen.getByTestId("html-fixture")).toHaveTextContent("<p>hi</p>");
    const toggle = () => screen.getByTestId("file-view-switch-mode");
    expect(toggle()).toHaveTextContent("Switch to plain text");
    fireEvent.click(toggle());
    expect(await editor()).toHaveAttribute("readonly");
    expect(screen.queryByTestId("html-fixture")).toBeNull();
    expect(toggle()).toHaveTextContent("Switch to preview");
    fireEvent.click(toggle());
    expect(screen.getByTestId("html-fixture")).toBeInTheDocument();
    // HTML edits as plain text; its preview stays one switch away.
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(await editor()).not.toHaveAttribute("readonly");
    expect(screen.queryByTestId("rich-editor")).toBeNull();
    expect(toggle()).toHaveTextContent("Switch to preview");
    fireEvent.click(toggle());
    expect(screen.getByTestId("html-fixture")).toBeInTheDocument();
    expect(toggle()).toHaveTextContent("Switch to plain text editing");
    expect(screen.getByTestId("file-view-done")).toBeInTheDocument();
  });

  it("opens Rich text from the reading view after Done at a double-clicked block", async () => {
    renderView(<FileView {...props()} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    await screen.findByTestId("rich-editor");
    fireEvent.click(screen.getByTestId("file-view-done"));
    fireEvent.doubleClick(
      within(screen.getByTestId("document-preview-content")).getByText(
        "Hello.",
      ),
    );
    expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
    await waitFor(() =>
      expect(richEditor().state.selection.$from.parent.textContent).toBe(
        "Hello.",
      ),
    );
  });

  it("returns to reading on Done when nothing was typed", async () => {
    renderView(<FileView {...props()} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    await screen.findByTestId("rich-editor");
    // Discard holds its place while editing, so a draft written when Done
    // takes focus moves no button from under the pointer.
    expect(screen.getByTestId("file-view-discard")).toBeDisabled();
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(screen.queryByTestId("rich-editor")).toBeNull();
    expect(screen.getByTestId("document-preview")).toBeInTheDocument();
    expect(getFileDraft(draftKey)).toBeNull();
    expect(screen.queryByTestId("file-view-discard")).toBeNull();
  });

  it("starts editing at a double-clicked block", async () => {
    renderView(<FileView {...props()} />);
    const paragraph = within(
      screen.getByTestId("document-preview-content"),
    ).getByText("Hello.");
    fireEvent.doubleClick(paragraph);
    expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
    await waitFor(() => {
      const { selection } = richEditor().state;
      expect(selection.$from.parent.textContent).toBe("Hello.");
    });
  });

  it("starts editing on the e key from the reading view", async () => {
    renderView(<FileView {...props()} />);
    fireEvent.keyDown(screen.getByTestId("document-preview"), { key: "e" });
    expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
  });

  it("does not start editing for a person who may not edit or suggest", () => {
    renderView(
      <FileView
        {...props({ viewerOwnsWorkspace: false, workspaceWritable: false })}
      />,
    );
    fireEvent.doubleClick(
      within(screen.getByTestId("document-preview-content")).getByText(
        "Hello.",
      ),
    );
    fireEvent.keyDown(screen.getByTestId("document-preview"), { key: "e" });
    expect(screen.queryByTestId("rich-editor")).toBeNull();
  });

  it("lets a reader suggest from Rich text and pushes the edit", async () => {
    const pushed = pushes();
    pushed.answer(created);
    renderView(
      <FileView
        {...props({
          viewerOwnsWorkspace: false,
          ownerVmRunning: false,
          file: file({ source: "committed", vmId: null, repoOid: "oid-1" }),
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    await screen.findByTestId("rich-editor");
    typeAfter("Hello", ", reader");
    fireEvent.click(screen.getByTestId("file-view-push-review"));
    fireEvent.click(screen.getByTestId("file-view-push-submit"));
    expect(
      await screen.findByTestId("file-view-push-done"),
    ).toBeInTheDocument();
    expect(pushed.bodies[0]).toMatchObject({
      baseContent: README,
      content: "# Title\n\nHello, reader.\n",
    });
  });

  it("drops the editor's draft on Discard", async () => {
    renderView(<FileView {...props()} />);
    fireEvent.click(screen.getByTestId("file-view-edit"));
    await screen.findByTestId("rich-editor");
    typeAfter("Hello", " gone");
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    expect(getFileDraft(draftKey)).not.toBeNull();
    fireEvent.click(screen.getByTestId("file-view-discard"));
    fireEvent.click(screen.getByTestId("file-view-discard-confirm"));
    expect(getFileDraft(draftKey)).toBeNull();
    expect(screen.queryByTestId("rich-editor")).toBeNull();
  });

  it("keeps Source editing for files without a Markdown preview", async () => {
    renderView(
      <FileView
        {...props({
          filePath: "src/app.ts",
          language: "typescript",
          file: file({ path: "src/app.ts", content: "const a = 1;\n" }),
        })}
      />,
    );
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(await editor()).toHaveValue("const a = 1;\n");
    expect(screen.queryByTestId("rich-editor")).toBeNull();
  });
});

// `g5.documents.viewers`: the extension sends a file to its viewer.
describe("FileView viewers", () => {
  // A 1×1 PNG.
  const PNG =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

  it("opens a PDF in the PDF viewer, with no editor or Download card", () => {
    renderView(
      <FileView
        {...props({
          filePath: "docs/report.pdf",
          language: "pdf",
          file: file({ encoding: "base64", content: "JVBERi0xLjQK" }),
        })}
      />,
    );
    expect(screen.getByTestId("pdf-view")).toHaveAttribute(
      "data-file-path",
      "docs/report.pdf",
    );
    expect(screen.getByTestId("pdf-view-loading")).toBeInTheDocument();
    expect(screen.queryByTestId("mock-monaco-editor")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("file-view-unavailable"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("file-view-edit")).not.toBeInTheDocument();
  });

  it("opens a raster image in the image viewer", () => {
    renderView(
      <FileView
        {...props({
          filePath: "img/photo.png",
          language: "png",
          file: file({ encoding: "base64", content: PNG }),
        })}
      />,
    );
    expect(screen.getByTestId("image-view")).toHaveAttribute(
      "data-file-path",
      "img/photo.png",
    );
    expect(screen.getByTestId("image-view-image").getAttribute("src")).toBe(
      "blob:mock-url",
    );
    expect(screen.getByTestId("image-view-dimensions")).toHaveTextContent(
      "1 × 1",
    );
    expect(screen.queryByTestId("mock-monaco-editor")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("file-view-unavailable"),
    ).not.toBeInTheDocument();
  });

  it("sends a text file misnamed as an image to the viewer, which offers Download when it fails to decode", () => {
    renderView(
      <FileView
        {...props({
          filePath: "img/notes.png",
          language: "png",
          file: file({ content: "just text" }),
        })}
      />,
    );
    expect(screen.queryByTestId("mock-monaco-editor")).not.toBeInTheDocument();
    fireEvent.error(screen.getByTestId("image-view-image"));
    expect(screen.getByTestId("image-view-notice")).toHaveAttribute(
      "data-reason",
      "undecodable",
    );
    fireEvent.click(screen.getByTestId("image-view-notice-download"));
    expect(downloads.calls).toEqual([[expect.any(Blob), "notes.png"]]);
  });

  it("shows an SVG as its image and edits it as plain text", async () => {
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg"><script>1</script></svg>';
    renderView(
      <FileView
        {...props({
          filePath: "art/logo.svg",
          language: "xml",
          file: file({ content: svg }),
        })}
      />,
    );
    // The rendered mode is the image, under the editor's own file row.
    expect(screen.getByTestId("file-view")).toBeInTheDocument();
    const image = screen.getByTestId("image-view-image");
    expect(image.getAttribute("src")).toBe("blob:mock-url");
    const created = vi
      .mocked(URL.createObjectURL)
      .mock.calls.at(-1)?.[0] as Blob;
    expect(created.type).toBe("image/svg+xml");
    expect(screen.getByTestId("image-view-file-row")).not.toHaveTextContent(
      "logo.svg",
    );
    // The SVG is never injected into the document: its script is no element.
    expect(document.querySelector("script")).toBeNull();
    const toggle = screen.getByTestId("file-view-switch-mode");
    expect(toggle).toHaveTextContent("Switch to plain text");
    fireEvent.click(toggle);
    expect((await editor()).value).toBe(svg);
    expect(screen.getByTestId("file-view-switch-mode")).toHaveTextContent(
      "Switch to preview",
    );
    // Edit starts in plain text, as for HTML.
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(await editor()).not.toHaveAttribute("readonly");
    expect(screen.queryByTestId("image-view")).not.toBeInTheDocument();
    expect(screen.getByTestId("file-view-switch-mode")).toHaveTextContent(
      "Switch to preview",
    );
  });

  it("keeps the Download card for other binary files", () => {
    renderView(
      <FileView
        {...props({
          filePath: "build/app.zip",
          language: "zip",
          file: file({ encoding: "base64", content: "UEsDBA==" }),
        })}
      />,
    );
    const card = screen.getByTestId("file-view-unavailable");
    expect(card).toHaveAttribute("data-reason", "binary");
    expect(card).toHaveTextContent("Binary file");
    expect(screen.queryByTestId("image-view")).not.toBeInTheDocument();
  });
});

describe("FileView file row", () => {
  it("holds the reading view's Compare and filters, and only while reading", async () => {
    // `g5.documents.editor.modes`: the file row is the only toolbar.
    const path = ".g5/ontology/row.g5.md";
    const content =
      '# Row\n\n<a id="row.a"></a>\n[D · row.a]\n\nA statement.\n\n## Metadata\n\n### row.a\n\n- human-provided\n';
    renderView(
      <FileView
        {...props({
          filePath: path,
          file: file({ path, content }),
        })}
      />,
    );
    const row = await screen.findByTestId("code-editor-file-row");
    const toolbar = await within(row).findByTestId("document-preview-toolbar");
    expect(
      within(toolbar).getByTestId("document-preview-compare"),
    ).toBeVisible();
    expect(within(toolbar).getByTestId("toolbar-tag-human")).toBeVisible();
    // It comes before the row's own controls.
    expect(
      toolbar.compareDocumentPosition(
        within(row).getByTestId("file-view-switch-mode"),
      ) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      within(screen.getByTestId("document-preview")).queryByTestId(
        "document-preview-toolbar",
      ),
    ).toBeNull();
    // Plain text and Rich text editing have no reading view to compare.
    fireEvent.click(within(row).getByTestId("file-view-switch-mode"));
    await waitFor(() =>
      expect(screen.queryByTestId("document-preview-toolbar")).toBeNull(),
    );
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    expect(
      await within(row).findByTestId("document-preview-toolbar"),
    ).toBeVisible();
    fireEvent.click(screen.getByTestId("file-view-edit"));
    await waitFor(() =>
      expect(screen.queryByTestId("document-preview-toolbar")).toBeNull(),
    );
  });
});

describe("FileView targets, comments and keys", () => {
  /** Selects `text` in the rendered Preview and releases the mouse there. */
  const select = (text: string) => {
    const element = screen.getByTestId("document-preview-content");
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const at = node.textContent?.indexOf(text) ?? -1;
      if (at < 0) continue;
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + text.length);
      window.getSelection()!.removeAllRanges();
      window.getSelection()!.addRange(range);
      fireEvent.mouseUp(element);
      return;
    }
    throw new Error(`no text ${text}`);
  };
  const settle = () => new Promise((resolve) => setTimeout(resolve, 10));

  it("opens the reading view for a statement or heading target from plain text and Rich text", async () => {
    // `g5.documents.ontology.links`
    const target = (seq: number) => ({
      showComments: false,
      comments: [],
      scrollToIntentRecord: "#title",
      scrollToIntentRecordSeq: seq,
    });
    let setEditorProps!: (next: FileViewProps["editorProps"]) => void;
    function Harness({
      expose,
    }: {
      expose: (set: (next: FileViewProps["editorProps"]) => void) => void;
    }) {
      const [editorProps, set] = React.useState<FileViewProps["editorProps"]>({
        showComments: false,
        comments: [],
      });
      React.useEffect(() => expose(set), [expose]);
      return <FileView {...props({ editorProps })} />;
    }
    renderView(
      <Harness
        expose={(set) => {
          setEditorProps = set;
        }}
      />,
    );
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    expect(screen.queryByTestId("document-preview")).toBeNull();
    act(() => setEditorProps(target(1)));
    expect(await screen.findByTestId("document-preview")).toBeVisible();
    // From Rich text the target leaves editing, as Done does.
    fireEvent.click(screen.getByTestId("file-view-edit"));
    expect(await screen.findByTestId("rich-editor")).toBeInTheDocument();
    act(() => setEditorProps(target(2)));
    expect(await screen.findByTestId("document-preview")).toBeVisible();
    expect(screen.queryByTestId("rich-editor")).toBeNull();
    expect(screen.getByTestId("file-view-edit")).toBeVisible();
    // The handled request no longer holds the view.
    fireEvent.click(screen.getByTestId("file-view-switch-mode"));
    expect(screen.queryByTestId("document-preview")).toBeNull();
  });

  it("opens a file left editing at a statement or heading target for reading", async () => {
    // `g5.documents.ontology.links`: a link into a file the person left in
    // Rich text leaves editing as Done does, also when it opens the file and
    // Rich text could open only after the target was shown (here once the
    // workspace runs).
    saveFileViewMode(draftKey, "preview");
    saveFileEditing(draftKey, true);
    let setRunning!: (running: boolean) => void;
    function Harness({
      expose,
    }: {
      expose: (set: (running: boolean) => void) => void;
    }) {
      const [ownerVmRunning, set] = React.useState(false);
      React.useEffect(() => expose(set), [expose]);
      return (
        <FileView
          {...props({
            ownerVmRunning,
            editorProps: {
              showComments: false,
              comments: [],
              scrollToIntentRecord: "#title",
              scrollToIntentRecordSeq: 1,
            },
          })}
        />
      );
    }
    renderView(
      <Harness
        expose={(set) => {
          setRunning = set;
        }}
      />,
    );
    expect(await screen.findByTestId("document-preview")).toBeVisible();
    act(() => setRunning(true));
    await settle();
    expect(screen.getByTestId("document-preview")).toBeVisible();
    expect(screen.queryByTestId("rich-editor")).toBeNull();
    expect(screen.getByTestId("file-view-edit")).toBeVisible();
  });

  it("offers a reader no Comment while composing a suggestion", async () => {
    // `g5.documents.comments.authority`: their text is not pushed yet.
    renderView(
      <FileView
        {...props({
          viewerOwnsWorkspace: false,
          editorProps: {
            showComments: true,
            comments: [],
            onAddComment: vi.fn(async () => undefined),
          },
        })}
      />,
    );
    select("Hello");
    expect(
      await screen.findByTestId("document-preview-selection-comment-button"),
    ).toBeVisible();
    // Their suggestion, typed and then read after Done, is unpushed text.
    fireEvent.click(screen.getByTestId("file-view-suggest"));
    fireEvent.click(await screen.findByTestId("file-view-switch-mode"));
    fireEvent.change(await editor(), {
      target: { value: "# Title\n\nHello there\n" },
    });
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(await screen.findByTestId("document-preview")).toBeVisible();
    select("Hello");
    await settle();
    expect(
      screen.queryByTestId("document-preview-selection-comment-button"),
    ).toBeNull();
  });

  it("saves an owner's draft on Mod+S in the reading view, and otherwise only keeps the browser's dialog away", async () => {
    const onSave = vi.fn(async () => ({ hash: "blob-2" }));
    renderView(
      <FileView
        {...props({
          onSave,
          editorProps: {
            showComments: true,
            comments: [],
            onAddComment: vi.fn(async () => undefined),
            onToggleComments: vi.fn(),
          },
        })}
      />,
    );
    const press = (target: Element) =>
      fireEvent.keyDown(target, { key: "s", ctrlKey: true });
    const content = () => screen.getByTestId("document-preview-content");
    expect(press(content())).toBe(false);
    await settle();
    expect(onSave).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("file-view-edit"));
    fireEvent.click(await screen.findByTestId("file-view-switch-mode"));
    fireEvent.change(await editor(), { target: { value: "# Changed\n" } });
    // Done keeps the draft, and Save, in the reading view.
    fireEvent.click(screen.getByTestId("file-view-done"));
    expect(await screen.findByTestId("document-preview")).toBeVisible();
    // In a comment box it only keeps the browser's dialog away.
    select("Changed");
    fireEvent.click(
      await screen.findByTestId("document-preview-selection-comment-button"),
    );
    const comment = await screen.findByRole("textbox", { name: "Comment" });
    expect(press(comment)).toBe(false);
    await settle();
    expect(onSave).not.toHaveBeenCalled();
    expect(press(content())).toBe(false);
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith("# Changed\n", "blob-1", "vm-1"),
    );
  });
});

describe("findings decisions write the live file (g5.documents.findings.decision)", () => {
  const FINDINGS = ".g5/TLA_verification/bugs_issues.md";
  const CONTENT =
    '# Bugs\n\n<a id="a"></a>\n### Receipts can exceed the ceiling\n\nSeverity: High · Decision: Open\n\nDetails.\n';
  const findingsKey = fileDraftKey({
    person: "user-alex",
    workspaceId: WS,
    repoId: REPO,
    path: FINDINGS,
  });
  const findingsFile = (content = CONTENT) => file({ path: FINDINGS, content });

  it("writes the decision without a commit and leaves no draft", async () => {
    const writes: Record<string, unknown>[] = [];
    server.use(
      // The live file holds what was written last.
      http.get("/api/v1/workspaces/:id/files/*", () =>
        HttpResponse.json({
          data: writes.length
            ? findingsFile(String(writes.at(-1)!.content))
            : findingsFile(),
        }),
      ),
      http.put("/api/v1/workspaces/:id/files/*", async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        writes.push(body);
        return HttpResponse.json({
          data: {
            path: FINDINGS,
            hash: `saved-${writes.length}`,
            size: String(body.content).length,
            encoding: "utf-8",
            vmId: "vm-1",
          },
        });
      }),
    );
    function Page() {
      const read = useWorkspaceFileContent(WS, REPO, FINDINGS);
      const save = useSaveWorkspaceFile();
      if (read.state.status !== "ok") return null;
      return (
        <FileView
          {...props({
            filePath: FINDINGS,
            file: read.state.data,
            onSave: (content, expectedHash, expectedVmId, options) =>
              save.mutateAsync({
                workspaceId: WS,
                repoId: REPO,
                path: FINDINGS,
                content,
                expectedHash,
                expectedVmId,
                commitAndPush: options?.commitAndPush,
              }),
          })}
        />
      );
    }
    renderView(<Page />);
    const irrelevant = await screen.findByRole("button", {
      name: "Irrelevant",
    });
    await waitFor(() => expect(irrelevant).toBeEnabled());
    fireEvent.click(irrelevant);
    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toMatchObject({
      content: CONTENT.replace(
        "Decision: Open",
        "Decision: Irrelevant — user decided this issue is not important.",
      ),
      expectedHash: "blob-1",
      expectedVmId: "vm-1",
      commitAndPush: false,
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Irrelevant" }),
      ).toHaveAttribute("aria-pressed", "true"),
    );
    expect(getFileDraft(findingsKey)).toBeNull();
  });

  it("drops the draft of a refused decision, so nothing shows it as made", async () => {
    server.use(
      http.get("/api/v1/workspaces/:id/files/*", () =>
        HttpResponse.json({ data: findingsFile() }),
      ),
      http.put("/api/v1/workspaces/:id/files/*", () =>
        HttpResponse.json(
          { error: { code: "INTERNAL", message: "boom" } },
          { status: 500 },
        ),
      ),
    );
    function Page() {
      const read = useWorkspaceFileContent(WS, REPO, FINDINGS);
      const save = useSaveWorkspaceFile();
      if (read.state.status !== "ok") return null;
      return (
        <FileView
          {...props({
            filePath: FINDINGS,
            file: read.state.data,
            onSave: (content, expectedHash, expectedVmId, options) =>
              save.mutateAsync({
                workspaceId: WS,
                repoId: REPO,
                path: FINDINGS,
                content,
                expectedHash,
                expectedVmId,
                commitAndPush: options?.commitAndPush,
              }),
          })}
        />
      );
    }
    renderView(<Page />);
    const irrelevant = await screen.findByRole("button", {
      name: "Irrelevant",
    });
    await waitFor(() => expect(irrelevant).toBeEnabled());
    fireEvent.click(irrelevant);
    expect(
      await screen.findByText(/decision could not be written/),
    ).toBeVisible();
    expect(getFileDraft(findingsKey)).toBeNull();
    expect(screen.getByRole("button", { name: "Irrelevant" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByRole("button", { name: "Irrelevant" })).toBeEnabled();
  });

  it("renders a large findings file at once", async () => {
    const big = `${CONTENT}\n${"Filler words. ".repeat(100000)}\n`;
    expect(big.length).toBeGreaterThan(1024 * 1024);
    renderView(
      <FileView {...props({ filePath: FINDINGS, file: findingsFile(big) })} />,
    );
    expect(
      await screen.findByRole("button", { name: "Irrelevant" }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("file-view-deferred-preview")).toBeNull();
  });

  it("gives the reason decisions are unavailable", async () => {
    renderView(
      <FileView
        {...props({
          filePath: FINDINGS,
          file: findingsFile(),
          ownerVmRunning: false,
        })}
      />,
    );
    expect(
      await screen.findByRole("button", { name: "Irrelevant" }),
    ).toHaveAttribute(
      "title",
      "The workspace owner decides here while the workspace is running.",
    );
    cleanup();
    renderView(
      <FileView
        {...props({
          filePath: FINDINGS,
          file: findingsFile(),
          viewerOwnsWorkspace: false,
        })}
      />,
    );
    expect(
      await screen.findByRole("button", { name: "Irrelevant" }),
    ).toHaveAttribute("title", "Only the workspace owner decides here.");
    cleanup();
    setFileDraft(findingsKey, null);
    renderView(
      <FileView {...props({ filePath: FINDINGS, file: findingsFile() })} />,
    );
    expect(
      await screen.findByRole("button", { name: "Irrelevant" }),
    ).toBeEnabled();
  });
});
