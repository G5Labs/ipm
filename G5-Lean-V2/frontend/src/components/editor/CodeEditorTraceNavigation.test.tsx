import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CodeEditor } from "./CodeEditor";

// Exercise navigation while the lazy native editor is still loading.
vi.mock("./loadMonaco", () => ({
  loadMonacoReact: () => new Promise(() => {}),
}));

const originalScrollToDescriptor = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "scrollTo",
);

describe("CodeEditor trace navigation", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    if (originalScrollToDescriptor) {
      Object.defineProperty(
        HTMLElement.prototype,
        "scrollTo",
        originalScrollToDescriptor,
      );
    } else {
      delete (HTMLElement.prototype as { scrollTo?: unknown }).scrollTo;
    }
  });

  it("scrolls to and highlights the requested code symbol and returns to the exact origin", async () => {
    const scrollTo = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollTo", {
      configurable: true,
      value: scrollTo,
    });
    const onNavigateBack = vi.fn();

    render(
      <CodeEditor
        filePath="generation/g5-api/src/routes/grounding.rs"
        content={"// header\n// open_physical: trace target\nfn body() {}"}
        language="rust"
        editable={false}
        showComments={false}
        comments={[]}
        onAddComment={vi.fn()}
        onResolveComment={vi.fn()}
        onReplyToComment={vi.fn()}
        scrollToIntentRecord="open_physical"
        scrollToIntentRecordSeq={1}
        navigationOrigin={{
          tabType: "spec",
          nodeId: ".semlift/specs/source.semlift.md",
          name: "source.semlift.md",
          intentRecordNumber: "D4",
          filePath: ".semlift/specs/source.semlift.md",
        }}
        onNavigateBack={onNavigateBack}
      />,
    );

    const back = await screen.findByTestId("navigate-back");
    expect(back).toHaveAttribute("title", "Back to source.semlift.md → D4");
    await waitFor(() =>
      expect(scrollTo).toHaveBeenCalledWith({ top: 0, behavior: "smooth" }),
    );

    fireEvent.click(back);
    expect(onNavigateBack).toHaveBeenCalledOnce();
  });

  it("offers the same Back over a rendered document a link opened", () => {
    const onNavigateBack = vi.fn();
    render(
      <CodeEditor
        filePath=".g5/ontology/features/receiving.g5.md"
        content="# Receiving"
        language="markdown"
        editable={false}
        showComments={false}
        comments={[]}
        preview={<p>Receiving</p>}
        navigationOrigin={{
          tabType: "file",
          nodeId: ".g5/ontology/root.g5.md",
          name: "root.g5.md",
          intentRecordNumber: "",
          filePath: ".g5/ontology/root.g5.md",
        }}
        onNavigateBack={onNavigateBack}
      />,
    );
    const back = screen
      .getAllByTestId("navigate-back")
      .find((button) => button.closest("[hidden], .hidden") === null)!;
    // A link outside any statement returns to the document itself.
    expect(back).toHaveAttribute("title", "Back to root.g5.md");
    fireEvent.click(back);
    expect(onNavigateBack).toHaveBeenCalledOnce();
  });
});
