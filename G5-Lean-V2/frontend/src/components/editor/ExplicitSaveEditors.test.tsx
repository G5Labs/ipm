import React from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ApiError } from "../../lib/api";
import { CodeEditor } from "./CodeEditor";

vi.mock("@monaco-editor/react", async () => {
  const ReactModule = await import("react");
  return {
    default: ({
      value,
      onChange,
    }: {
      value: string;
      onChange: (value: string) => void;
    }) =>
      ReactModule.createElement("textarea", {
        "data-testid": "mock-monaco-editor",
        value,
        onChange: (event: React.ChangeEvent<HTMLTextAreaElement>) =>
          onChange(event.currentTarget.value),
      }),
    loader: {
      init: () =>
        Promise.resolve({
          editor: { defineTheme: vi.fn(), setTheme: vi.fn() },
        }),
    },
  };
});

describe("explicit editor Save", () => {
  it("saves a local code model with the hash that was originally read", async () => {
    const onSave = vi.fn().mockResolvedValue({ hash: "file-v2" });
    render(
      <CodeEditor
        filePath="src/main.ts"
        content="before"
        contentHash="file-v1"
        language="typescript"
        editable
        onSave={onSave}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
      />,
    );

    fireEvent.change(await screen.findByTestId("mock-monaco-editor"), {
      target: { value: "after" },
    });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));

    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith("after", "file-v1", null),
    );
    expect(screen.queryByTestId("code-editor-dirty-indicator")).toBeNull();
  });

  it("requires an explicit overwrite after a stale-hash conflict", async () => {
    const onSave = vi
      .fn()
      .mockRejectedValueOnce(
        new ApiError({
          code: "FILE_VERSION_CONFLICT",
          message: "stale file",
          status: 409,
          details: { hash: "file-v2", content: "other change" },
        }),
      )
      .mockResolvedValueOnce({ hash: "file-v3" });
    render(
      <CodeEditor
        filePath="src/main.ts"
        content="before"
        contentHash="file-v1"
        language="typescript"
        editable
        onSave={onSave}
        onReload={vi.fn()}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
      />,
    );

    fireEvent.change(await screen.findByTestId("mock-monaco-editor"), {
      target: { value: "my change" },
    });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    expect(
      await screen.findByTestId("code-editor-conflict-dialog"),
    ).toBeVisible();

    fireEvent.click(screen.getByRole("button", { name: "Save my version" }));
    await waitFor(() =>
      expect(onSave).toHaveBeenNthCalledWith(2, "my change", "file-v2", null),
    );
  });

  it("keeps freshly reloaded code bytes while parent props are stale", async () => {
    const onSave = vi.fn().mockRejectedValueOnce(
      new ApiError({
        code: "FILE_VERSION_CONFLICT",
        message: "stale file",
        status: 409,
        details: { hash: "file-v2", content: "external code" },
      }),
    );
    const onReload = vi.fn().mockResolvedValue({
      content: "external code",
      hash: "file-v2",
    });
    render(
      <CodeEditor
        filePath="src/main.ts"
        content="before"
        contentHash="file-v1"
        language="typescript"
        editable
        onSave={onSave}
        onReload={onReload}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
      />,
    );
    fireEvent.change(await screen.findByTestId("mock-monaco-editor"), {
      target: { value: "my code" },
    });
    fireEvent.click(screen.getByTestId("code-editor-save-button"));
    fireEvent.click(await screen.findByTestId("code-editor-conflict-reload"));
    await waitFor(() => expect(onReload).toHaveBeenCalledOnce());
    await waitFor(() =>
      expect(screen.getByTestId("mock-monaco-editor")).toHaveValue(
        "external code",
      ),
    );
    expect(screen.getByTestId("mock-monaco-editor")).not.toHaveValue("before");
  });
});
