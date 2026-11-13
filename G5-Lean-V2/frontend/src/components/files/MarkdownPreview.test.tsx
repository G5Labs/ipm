import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../shared/MermaidDiagram", () => ({
  MermaidDiagram: ({ code }: { code: string }) => (
    <div data-testid="mermaid-diagram">{code}</div>
  ),
}));

const fileReads = vi.hoisted(() => ({
  byPath: new Map<string, { content: string; encoding: "utf-8" | "base64" }>(),
}));
vi.mock("../../hooks/useFiles", () => ({
  useWorkspaceFileContent: (_ws: string, _repo: string, path: string) => {
    const file = fileReads.byPath.get(path);
    return {
      state: file
        ? {
            status: "ok",
            data: {
              path,
              size: 1,
              hash: "h",
              source: "committed",
              repoOid: null,
              snapshotAt: null,
              ...file,
            },
          }
        : { status: "error", error: new Error("missing") },
      reload: vi.fn(),
    };
  },
}));

import { MarkdownPreview } from "./MarkdownPreview";
import { resolveRepoLink } from "./repoLinks";

function renderPreview(content: string, onOpen = vi.fn()) {
  const view = render(
    <MarkdownPreview
      content={content}
      filePath="docs/guide/a.md"
      workspaceId="ws-1"
      repoId="repo-1"
      onOpenWorkspaceFile={onOpen}
    />,
  );
  return { ...view, onOpen };
}

beforeEach(() => fileReads.byPath.clear());

describe("resolveRepoLink", () => {
  it("resolves relative, rooted and line links inside the repository", () => {
    expect(resolveRepoLink("docs/guide/a.md", "b.md")).toEqual({
      path: "docs/guide/b.md",
    });
    expect(resolveRepoLink("docs/guide/a.md", "../c/d.ts#L10-L20")).toEqual({
      path: "docs/c/d.ts",
      line: 10,
    });
    expect(resolveRepoLink("docs/guide/a.md", "/README.md")).toEqual({
      path: "README.md",
    });
    expect(resolveRepoLink("docs/guide/a.md", "./img%20one.png?raw=1")).toEqual(
      { path: "docs/guide/img one.png" },
    );
  });

  it("leaves external, in-page and escaping links alone", () => {
    expect(resolveRepoLink("docs/a.md", "https://example.com/x")).toBeNull();
    expect(resolveRepoLink("docs/a.md", "mailto:a@example.com")).toBeNull();
    expect(resolveRepoLink("docs/a.md", "//cdn.example.com/x.png")).toBeNull();
    expect(resolveRepoLink("docs/a.md", "#section")).toBeNull();
    expect(resolveRepoLink("docs/a.md", "../../outside.md")).toBeNull();
  });
});

describe("MarkdownPreview", () => {
  it("renders GitHub-flavored Markdown with GitHub's ids and alerts", () => {
    const { container } = renderPreview(
      [
        "# Getting started",
        "",
        "> [!WARNING]",
        "> Back up first.",
        "",
        "| a | b |",
        "|---|---|",
        "| 1 | 2 |",
        "",
        "- [x] done",
        "- [ ] todo",
        "",
        "Text with ~~old~~ and a note[^1].",
        "",
        "[^1]: The note.",
      ].join("\n"),
    );
    expect(container.querySelector("h1")?.id).toBe(
      "user-content-getting-started",
    );
    const alert = container.querySelector(
      ".markdown-alert.markdown-alert-warning",
    );
    expect(alert?.querySelector(".markdown-alert-title")?.textContent).toBe(
      "Warning",
    );
    expect(alert?.textContent).toContain("Back up first.");
    expect(alert?.textContent).not.toContain("[!WARNING]");
    expect(container.querySelectorAll("table td")).toHaveLength(2);
    expect(container.querySelectorAll("input[type=checkbox]")).toHaveLength(2);
    expect(container.querySelector("del")?.textContent).toBe("old");
    expect(container.querySelector("#user-content-fn-1")).not.toBeNull();
  });

  it("shows question fences as code, as GitHub does", () => {
    const { container } = renderPreview(
      [
        "```g5-question-ref",
        '{"path": "docs/ws/questions/plan.g5q.json", "id": "pick"}',
        "```",
      ].join("\n"),
    );
    // `g5.documents.files-view.preview`: no referenced-question card here.
    expect(screen.queryByTestId("referenced-question-card")).toBeNull();
    expect(container.querySelector("pre code")?.textContent).toContain(
      "docs/ws/questions/plan.g5q.json",
    );
  });

  it("follows in-page links to their prefixed ids", () => {
    const scrolled = vi.fn();
    const { container } = renderPreview("## Usage\n\n[jump](#usage)\n");
    const heading = container.querySelector<HTMLElement>(
      "#user-content-usage",
    )!;
    heading.scrollIntoView = scrolled;
    fireEvent.click(screen.getByText("jump"));
    expect(scrolled).toHaveBeenCalled();
  });

  it("renders math, highlighted code and Mermaid diagrams", () => {
    const { container } = renderPreview(
      "Inline $x^2$.\n\n```ts\nconst a = 1;\n```\n\n```mermaid\ngraph TD; A-->B\n```\n",
    );
    expect(container.querySelector(".katex")).not.toBeNull();
    expect(
      container.querySelector("code.language-ts span[style]"),
    ).not.toBeNull();
    expect(screen.getByTestId("mermaid-diagram").textContent).toBe(
      "graph TD; A-->B",
    );
  });

  it("puts a Copy control on code blocks, copying their exact text", async () => {
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    try {
      renderPreview(
        [
          "```ts",
          "const a = '<b>';",
          "",
          "  return a;",
          "```",
          "",
          "```",
          "plain",
          "```",
          "",
          "    indented code",
          "",
          "```mermaid",
          "graph TD; A-->B",
          "```",
          "",
          "```math",
          "x^2",
          "```",
        ].join("\n"),
      );
      // Mermaid and math render as a diagram and a formula, not as code.
      const buttons = screen.getAllByRole("button", { name: "Copy code" });
      expect(buttons).toHaveLength(3);
      for (const button of buttons) fireEvent.click(button);
      await waitFor(() =>
        expect(screen.getAllByText("Copied")).toHaveLength(3),
      );
      expect(writeText.mock.calls.map(([text]) => text)).toEqual([
        "const a = '<b>';\n\n  return a;",
        "plain",
        "indented code",
      ]);
    } finally {
      Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it("sanitizes scripts, event handlers and javascript: links", () => {
    const { container } = renderPreview(
      '<script>window.bad = 1</script>\n\n<img src="x.png" onerror="window.bad = 1">\n\n[click](javascript:alert(1))\n\n<a href="javascript:alert(1)">raw</a>\n',
    );
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("[onerror]")).toBeNull();
    for (const anchor of Array.from(container.querySelectorAll("a"))) {
      expect(anchor.getAttribute("href") ?? "").not.toMatch(/^javascript:/i);
    }
  });

  it("opens repository links in FILES and external links in a new tab", () => {
    const { onOpen } = renderPreview(
      "[next](../other.md#L4) and [site](https://example.com)\n",
    );
    fireEvent.click(screen.getByText("next"));
    expect(onOpen).toHaveBeenCalledWith("docs/other.md", "repo-1", 4);
    const external = screen.getByText("site");
    expect(external.getAttribute("target")).toBe("_blank");
    expect(external.getAttribute("rel")).toBe("noopener noreferrer");
  });

  it("loads relative images from the workspace and marks missing ones", () => {
    fileReads.byPath.set("docs/guide/diagram.png", {
      content: "iVBORw0KGgo=",
      encoding: "base64",
    });
    const { container } = renderPreview(
      "![Diagram](diagram.png)\n\n![Gone](gone.png)\n",
    );
    // The image is a typed object URL, as the standalone viewer shows it.
    const image = container.querySelector("img");
    expect(image?.getAttribute("src")).toBe("blob:mock-url");
    const created = vi.mocked(URL.createObjectURL).mock.calls.at(-1)?.[0];
    expect(created).toBeInstanceOf(Blob);
    expect((created as Blob).type).toBe("image/png");
    expect((created as Blob).size).toBe(8);
    expect(image?.getAttribute("alt")).toBe("Diagram");
    expect(
      screen.getByTestId("markdown-preview-image-unavailable").textContent,
    ).toBe("Gone");
  });
});
