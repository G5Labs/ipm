import React from "react";
import { act, fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderApp } from "../../test/render";

// Monaco loads only when the test says so.
const monaco = vi.hoisted(() => {
  let resolve!: (value: unknown) => void;
  const loaded = new Promise((done) => {
    resolve = done;
  });
  return { loaded, resolve };
});
vi.mock("../editor/loadMonaco", () => ({
  loadMonacoReact: () => monaco.loaded,
}));

import { FileCompareDialog } from "./FileCompareDialog";

/** Monaco's diff editor as a textarea for its modified side. */
const MockDiffEditor = ({
  modified,
  onMount,
}: {
  modified: string;
  onMount: (editor: unknown) => void;
}) => {
  const [value, setValue] = React.useState(modified);
  const current = React.useRef(modified);
  const changed = React.useRef<() => void>(() => undefined);
  const area = React.useRef<HTMLTextAreaElement>(null);
  React.useEffect(() => {
    onMount({
      getModifiedEditor: () => ({
        getValue: () => current.current,
        onDidChangeModelContent: (listener: () => void) => {
          changed.current = listener;
        },
        focus: () => area.current?.focus(),
      }),
    });
  }, [onMount]);
  return (
    <textarea
      ref={area}
      data-testid="mock-diff-modified"
      value={value}
      onChange={(event) => {
        current.current = event.target.value;
        setValue(current.current);
        changed.current();
      }}
    />
  );
};

describe("FileCompareDialog", () => {
  it("keeps text typed while Monaco loads when its diff editor takes over", async () => {
    const onApply = vi.fn();
    renderApp(
      <FileCompareDialog
        filePath="docs/a.md"
        language="markdown"
        newerContent="agent\n"
        ownContent="mine\n"
        description="The file changed."
        applyLabel="Use my merged text"
        onCancel={vi.fn()}
        onApply={onApply}
      />,
    );
    fireEvent.change(screen.getByTestId("file-compare-own"), {
      target: { value: "mine\nfirst\n" },
    });
    await act(async () => {
      monaco.resolve({ DiffEditor: MockDiffEditor });
      await monaco.loaded;
    });
    const modified = screen.getByTestId(
      "mock-diff-modified",
    ) as HTMLTextAreaElement;
    expect(modified).toHaveValue("mine\nfirst\n");
    fireEvent.change(modified, {
      target: { value: `${modified.value}second\n` },
    });
    fireEvent.click(screen.getByTestId("file-compare-apply"));
    expect(onApply).toHaveBeenCalledWith("mine\nfirst\nsecond\n");
  });

  it("takes the focus while open, closes on Escape and gives the focus back", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const onCancel = vi.fn();
    const view = renderApp(
      <FileCompareDialog
        filePath="docs/a.md"
        language="markdown"
        newerContent="agent\n"
        ownContent="mine\n"
        description="The file changed."
        applyLabel="Use my merged text"
        onCancel={onCancel}
        onApply={vi.fn()}
      />,
    );
    const dialog = screen.getByTestId("file-compare-dialog");
    // The text to merge has the focus (the fallback, or Monaco once loaded).
    await act(async () => {});
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(dialog);
    fireEvent.keyDown(document.activeElement as Element, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledOnce();
    view.unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });
});
