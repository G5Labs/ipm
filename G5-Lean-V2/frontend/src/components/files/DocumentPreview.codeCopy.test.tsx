// Documents g5.documents.markdown.code-copy: DocumentPreview's read-only code
// blocks carry the shared Copy control, copying the block's exact text, and a
// Mermaid block renders its diagram without one.
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DocumentPreview } from "./DocumentPreview";

vi.mock("../shared/MermaidDiagram", () => ({
  MermaidDiagram: ({ code }: { code: string }) => (
    <div data-testid="mermaid-diagram">{code}</div>
  ),
}));

const FENCE = 'const x = 1; // "quoted" & <b>\n\tindented';

const DOCUMENT = [
  "# Notes",
  "",
  "```ts",
  FENCE,
  "```",
  "",
  "```mermaid",
  "graph LR",
  "  A --> B",
  "```",
  "",
].join("\n");

function mount(content = DOCUMENT) {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <DocumentPreview
        workspaceId=""
        repoId=""
        filePath="docs/ws/notes.md"
        content={content}
      />
    </QueryClientProvider>,
  );
}

function stubClipboard(writeText: (text: string) => Promise<void>) {
  const spy = vi.fn(writeText);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: spy },
  });
  return spy;
}

describe("DocumentPreview code block Copy", () => {
  afterEach(() => {
    Reflect.deleteProperty(navigator, "clipboard");
  });

  it("copies a read-only code block's exact text", async () => {
    const writeText = stubClipboard(async () => {});
    mount();

    const buttons = screen.getAllByRole("button", { name: "Copy code" });
    expect(buttons).toHaveLength(1);
    fireEvent.click(buttons[0]);

    expect(await screen.findByText("Copied")).toBeInTheDocument();
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(writeText).toHaveBeenCalledWith(FENCE);
  });

  it("reports a refused clipboard write", async () => {
    stubClipboard(async () => {
      throw new DOMException("denied", "NotAllowedError");
    });
    mount();

    fireEvent.click(screen.getByRole("button", { name: "Copy code" }));

    expect(await screen.findByText("Couldn't copy")).toBeInTheDocument();
  });

  it("renders a Mermaid block's diagram without a Copy control", () => {
    mount("```mermaid\ngraph LR\n  A --> B\n```\n");

    const diagram = screen.getByTestId("mermaid-diagram");
    expect(diagram).toHaveTextContent("graph LR");
    expect(screen.queryByRole("button", { name: "Copy code" })).toBeNull();
  });
});
