import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FileView } from "../files/FileView";
vi.mock("@monaco-editor/react", () => ({
  default: ({
    value,
    defaultValue,
    onChange,
    options,
  }: {
    value?: string;
    defaultValue?: string;
    onChange: (value: string) => void;
    options?: { readOnly?: boolean };
  }) => (
    <textarea
      data-testid="mock-monaco-editor"
      value={value ?? defaultValue}
      readOnly={options?.readOnly}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
  loader: {
    init: () =>
      Promise.resolve({
        editor: { defineTheme: () => {}, setTheme: () => {} },
      }),
  },
}));
import {
  fileDraftKey,
  getFileDraft,
  newFileDraftId,
  retainFileDraftsFor,
  setFileDraft,
} from "../files/fileDrafts";
const path = ".g5/ontology/root.g5.md";
const source =
  '# Root\n\n<a id="test.body"></a>\n[I · test.body]\n\nOriginal body.\n\n## Metadata\n\n### test.body\n\n- human-provided\n';
const key = fileDraftKey({
  person: "person",
  workspaceId: "ws",
  repoId: "repo",
  path,
});
afterEach(() => retainFileDraftsFor(null));
function mount(
  save = vi.fn().mockResolvedValue({ hash: "new" }),
  hash = "old",
  vm = "vm-one",
) {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <FileView
        key={key}
        workspaceId="ws"
        repoId="repo"
        filePath={path}
        principal="person:org"
        language="markdown"
        file={{
          path,
          content: source,
          hash,
          vmId: vm,
          size: source.length,
          encoding: "utf-8",
          source: "live",
          repoOid: null,
          snapshotAt: null,
        }}
        lineTarget={null}
        viewerOwnsWorkspace
        ownerVmRunning
        workspaceWritable
        onSave={save}
        onReload={async () => ({ content: source, hash, vmId: vm })}
        onFetchCurrent={async () => ({ content: source, hash, vmId: vm })}
        onStartWorkspace={() => {}}
        editorProps={{ comments: [], showComments: false }}
      />
    </QueryClientProvider>,
  );
}
async function edit(text: string, existing = "Original body.") {
  const start = screen.queryByTestId("file-view-edit");
  if (start) fireEvent.click(start);
  const input = await screen.findByTestId("mock-monaco-editor");
  fireEvent.change(input, {
    target: {
      value: (input as HTMLTextAreaElement).value.replace(existing, text),
    },
  });
}
describe("Core Ontology shares Files drafts", () => {
  it("retains unsaved text across unmount and saves against its original hash and VM", async () => {
    const first = mount();
    await edit("Unsaved body.");
    expect(getFileDraft(key)?.content).toContain("Unsaved body.");
    first.unmount();
    const save = vi.fn().mockResolvedValue({ hash: "saved" });
    mount(save, "newer-server-hash", "replacement-vm");
    expect(
      ((await screen.findByTestId("mock-monaco-editor")) as HTMLTextAreaElement)
        .value,
    ).toContain("Unsaved body.");
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith(
        expect.stringContaining("Unsaved body."),
        "old",
        "vm-one",
      ),
    );
    await waitFor(() => expect(getFileDraft(key)).toBeNull());
  });
  it("restores a draft created in Files and preserves it after a failed Save", async () => {
    setFileDraft(key, {
      id: newFileDraftId(),
      role: "owner",
      content: source.replace("Original body.", "Files body."),
      baseline: { content: source, hash: "files-hash", vmId: "files-vm" },
    });
    const save = vi.fn().mockRejectedValue(new Error("Publication failed"));
    mount(save);
    expect(
      ((await screen.findByTestId("mock-monaco-editor")) as HTMLTextAreaElement)
        .value,
    ).toContain("Files body.");
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await screen.findByText(/Save failed: Publication failed/);
    expect(save).toHaveBeenCalledWith(
      expect.stringContaining("Files body."),
      "files-hash",
      "files-vm",
    );
    expect(getFileDraft(key)?.content).toContain("Files body.");
    expect(getFileDraft(key)?.saving ?? 0).toBe(0);
  });
  it("does not let a late Save erase a replacement Files draft", async () => {
    let finish!: (value: { hash: string }) => void;
    const save = vi.fn(
      () =>
        new Promise<{ hash: string }>((resolve) => {
          finish = resolve;
        }),
    );
    const view = mount(save);
    await edit("Submitted body.");
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    expect(getFileDraft(key)?.saving).toBe(1);
    view.unmount();
    const replacement = {
      id: newFileDraftId(),
      role: "owner" as const,
      content: "replacement",
      baseline: { content: source, hash: "newbase", vmId: "vm-two" },
    };
    setFileDraft(key, replacement);
    await act(async () => finish({ hash: "saved" }));
    expect(getFileDraft(key)).toEqual(replacement);
  });
  it("retains edits typed while Save is pending against the newly saved baseline", async () => {
    let finish!: (value: { hash: string }) => void;
    mount(
      vi.fn(
        () =>
          new Promise<{ hash: string }>((resolve) => {
            finish = resolve;
          }),
      ),
    );
    await edit("Submitted body.");
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    await edit("Later body.", "Submitted body.");
    await act(async () => finish({ hash: "saved" }));
    expect(getFileDraft(key)?.content).toContain("Later body.");
    expect(getFileDraft(key)?.baseline.content).toContain("Submitted body.");
    expect(getFileDraft(key)?.baseline.hash).toBe("saved");
  });
  it("applies workspace Discard to the mounted Source session", async () => {
    mount();
    await edit("Discard this body.");
    await act(async () => setFileDraft(key, null));
    expect(screen.getByTestId("mock-monaco-editor")).toHaveValue(source);
    expect(screen.queryByText("Discard this body.")).toBeNull();
    expect(screen.getByTestId("code-editor-save-button")).toBeDisabled();
  });
});
