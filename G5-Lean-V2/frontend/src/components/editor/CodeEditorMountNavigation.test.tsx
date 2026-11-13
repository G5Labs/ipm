import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CodeEditor, type CodeEditorProps } from "./CodeEditor";
import { loadMonacoReact } from "./loadMonaco";

const { revealLineInCenter } = vi.hoisted(() => ({
  revealLineInCenter: vi.fn(),
}));

vi.mock("@monaco-editor/react", async () => {
  const React = await import("react");
  const monaco = {
    KeyMod: { CtrlCmd: 0 },
    KeyCode: { KeyS: 0 },
    editor: { defineTheme: vi.fn(), setTheme: vi.fn() },
  };
  return {
    loader: { init: async () => monaco },
    default: ({
      onMount,
    }: {
      onMount: (editor: unknown, monaco: unknown) => void;
    }) =>
      React.createElement(
        "button",
        {
          onClick: () =>
            onMount(
              { getModel: () => null, addCommand: vi.fn(), revealLineInCenter },
              monaco,
            ),
        },
        "Finish loading Monaco",
      ),
  };
});

describe("CodeEditor navigation before Monaco is ready", () => {
  it.each<{
    name: string;
    target: Partial<CodeEditorProps>;
  }>([
    { name: "a file citation", target: { scrollToLine: 150 } },
    { name: "a trace target", target: { scrollToIntentRecord: "late_symbol" } },
  ])("reveals $name after the editor mounts", async ({ target }) => {
    // Rendering schedules navigation before the asynchronous Monaco import.
    // Control that timer even when CI takes longer than its 150ms delay.
    vi.useFakeTimers();
    revealLineInCenter.mockClear();
    render(
      <CodeEditor
        filePath="notes.md"
        content={Array.from({ length: 200 }, (_, i) =>
          i === 149 ? "late_symbol: target" : `line ${i + 1}`,
        ).join("\n")}
        language="markdown"
        editable={false}
        showComments={false}
        comments={[]}
        {...target}
      />,
    );
    await act(async () => {
      await loadMonacoReact();
    });
    const mount = screen.getByRole("button", {
      name: "Finish loading Monaco",
    });
    // The initial navigation timer expires before the native editor is ready.
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(revealLineInCenter).not.toHaveBeenCalled();

    fireEvent.click(mount);
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(revealLineInCenter).toHaveBeenCalledWith(150);
  });
});
