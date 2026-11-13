import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DocumentPreview } from "./DocumentPreview";

const content =
  '# R\n\n<a id="x"></a>\n[I · x]\n\nSee [the guide](../../docs/Receiving Guide.md#L5) and `[code](a b.md)`.\n\n## Metadata\n\n### x\n\n- agent\n\nSource: [Header analysis](../../../docs/analysis/Company Information.md#L52-L218); [unsafe](javascript: alert 1)\n';

function mount(filePath: string) {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <DocumentPreview
        workspaceId=""
        repoId="repo"
        filePath={filePath}
        content={content}
      />
    </QueryClientProvider>,
  );
}

describe("links with unencoded spaces in ontology files", () => {
  it("render as links in statements and metadata, never from code or schemes", () => {
    mount(".g5/ontology/features/receiving.g5.md");
    expect(screen.getByRole("link", { name: "the guide" })).toHaveAttribute(
      "href",
      "../../docs/Receiving%20Guide.md#L5",
    );
    expect(screen.getByText("[code](a b.md)")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("semantic-record-details-toggle"));
    expect(
      screen.getByRole("link", { name: "Header analysis" }),
    ).toHaveAttribute(
      "href",
      "../../../docs/analysis/Company%20Information.md#L52-L218",
    );
    expect(screen.queryByRole("link", { name: "unsafe" })).toBeNull();
  });

  it("leave other documents unchanged", () => {
    mount("docs/notes.md");
    expect(screen.queryByRole("link", { name: "the guide" })).toBeNull();
  });
});
