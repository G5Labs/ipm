// =============================================================================
// G5 Client — MarkdownRenderer Component Tests
// =============================================================================
// Tests the read-only markdown rendering component covering all block types:
// headings, paragraphs, code blocks (with Rust highlighting), tables,
// callouts, lists, blockquotes, horizontal rules, and frontmatter suppression.
// =============================================================================

import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MarkdownRenderer } from "./MarkdownRenderer";

// Mock MermaidDiagram since it relies on external library
vi.mock("./MermaidDiagram", () => ({
  MermaidDiagram: ({ code }: { code: string }) => (
    <div data-testid="mermaid-diagram">{code}</div>
  ),
}));

vi.mock("./ReactFlowDiagramWidget", () => ({
  ReactFlowDiagramWidget: ({ code }: { code: string }) => (
    <div data-testid="react-flow-diagram-widget">{code}</div>
  ),
}));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function renderMd(content: string) {
  return render(<MarkdownRenderer content={content} />);
}

describe("MarkdownRenderer", () => {
  it("renders nothing for empty content", () => {
    const { container } = renderMd("");
    expect(container.innerHTML).toBe("");
  });

  it("renders nothing for null content", () => {
    const { container } = render(
      <MarkdownRenderer content={null as unknown as string} />,
    );
    expect(container.innerHTML).toBe("");
  });

  it("applies custom className", () => {
    const { container } = render(
      <MarkdownRenderer content="# Hello" className="my-custom-class" />,
    );
    expect(
      container.firstElementChild?.classList.contains("my-custom-class"),
    ).toBe(true);
  });

  // =========================================================================
  // Headings
  // =========================================================================

  describe("headings", () => {
    it("renders h1", () => {
      renderMd("# Title");
      const h1 = screen.getByRole("heading", { level: 1 });
      expect(h1).toBeInTheDocument();
      expect(h1.textContent).toBe("Title");
    });

    it("renders h2", () => {
      renderMd("## Subtitle");
      expect(screen.getByRole("heading", { level: 2 })).toBeInTheDocument();
    });

    it("renders h3", () => {
      renderMd("### Section");
      expect(screen.getByRole("heading", { level: 3 })).toBeInTheDocument();
    });

    it("renders h4–h6", () => {
      renderMd("#### H4\n##### H5\n###### H6");
      expect(screen.getByRole("heading", { level: 4 })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 5 })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 6 })).toBeInTheDocument();
    });

    it("applies correct styling classes for each level", () => {
      renderMd("# Big\n## Medium\n### Small");
      const h1 = screen.getByRole("heading", { level: 1 });
      expect(h1.closest(".markdown-body")).not.toBeNull();
      const h2 = screen.getByRole("heading", { level: 2 });
      expect(h2.closest(".markdown-body")).not.toBeNull();
      const h3 = screen.getByRole("heading", { level: 3 });
      expect(h3.closest(".markdown-body")).not.toBeNull();
    });

    it.each([undefined, "slug"] as const)(
      "keeps unique ids for nested and setext headings in %s mode",
      (mode) => {
        render(
          <MarkdownRenderer
            content={"Setext\n======\n\n> ## Nested\n\n# Top level"}
            headingIdMode={mode}
          />,
        );
        const ids = screen.getAllByRole("heading").map((heading) => heading.id);
        expect(ids).toHaveLength(3);
        expect(ids.every(Boolean)).toBe(true);
        expect(new Set(ids).size).toBe(3);
        expect(ids).not.toContain("-1");
      },
    );

    it("can render stable slug heading ids for guide anchors", () => {
      render(
        <MarkdownRenderer
          content={[
            "### 4.1. PM — Document an Existing Area of the System",
            "### 4.1. PM — Document an Existing Area of the System",
          ].join("\n")}
          headingIdMode="slug"
        />,
      );

      const headings = screen.getAllByRole("heading", { level: 3 });
      expect(headings[0]).toHaveAttribute(
        "id",
        "4-1-pm-document-an-existing-area-of-the-system",
      );
      expect(headings[1]).toHaveAttribute(
        "id",
        "4-1-pm-document-an-existing-area-of-the-system-2",
      );
    });
  });

  // =========================================================================
  // Paragraphs
  // =========================================================================

  describe("paragraphs", () => {
    it("renders a simple paragraph", () => {
      renderMd("Hello world");
      expect(screen.getByText("Hello world")).toBeInTheDocument();
    });

    it("joins adjacent lines into one paragraph", () => {
      renderMd("Line one\nLine two");
      expect(screen.getByText("Line one Line two")).toBeInTheDocument();
    });

    it("renders inline bold in paragraphs", () => {
      renderMd("This is **bold** text");
      const strong = document.querySelector("strong");
      expect(strong).toBeInTheDocument();
      expect(strong?.textContent).toBe("bold");
    });

    it("renders inline italic in paragraphs", () => {
      renderMd("This is *italic* text");
      const em = document.querySelector("em");
      expect(em).toBeInTheDocument();
      expect(em?.textContent).toBe("italic");
    });

    it("renders inline code in paragraphs", () => {
      renderMd("Use `npm install` to install");
      const code = document.querySelector("p code");
      expect(code).toBeInTheDocument();
      expect(code?.textContent).toBe("npm install");
    });

    it("renders links in paragraphs", () => {
      renderMd("Visit [Example](https://example.com)");
      const link = screen.getByRole("link", { name: "Example" });
      expect(link).toBeInTheDocument();
      expect(link.getAttribute("href")).toBe("https://example.com");
      expect(link.getAttribute("target")).toBe("_blank");
    });

    it("leaves retired node links as readable, inert text", () => {
      const { container } = renderMd(
        "See [core](g5://ontology/core/search#D1) and [bad](g5://ontology/../../etc/passwd).",
      );
      expect(container.querySelector("a")).toBeNull();
      expect(container).toHaveTextContent("core");
      expect(container).toHaveTextContent("bad");
    });
    it("renders a [Q1. …] record as a framed block with badge and chips", () => {
      renderMd(
        [
          "[Q1. New task status] (Tags: answered, high)",
          "",
          "Context prose for the question.",
        ].join("\n"),
      );
      const frame = screen.getByTestId("markdown-question-frame");
      expect(frame).toBeInTheDocument();
      expect(
        within(frame).getByTestId("semantic-record-kind"),
      ).toHaveTextContent("Question");
      expect(within(frame).getByTestId("semantic-record-id")).not.toBeVisible();
      expect(frame.textContent).toContain("New task status");
      expect(frame.textContent).toContain("Answered");
      expect(frame.textContent).toContain("High");
      expect(frame.textContent).toContain("Context prose for the question.");
      expect(frame.textContent).not.toContain("[Q1.");
    });

    it("renders an [I7a. …] label without tags as a styled header", () => {
      renderMd("[I7a. Human requirement]");
      const header = screen.getByTestId("semantic-record");
      expect(header.textContent).toContain("I7a");
      expect(header.textContent).toContain("Human requirement");
    });

    it("keeps mid-paragraph label references as ordinary prose", () => {
      renderMd("This behavior is owned by [D8] and friends.");
      expect(screen.queryByTestId("markdown-semantic-label")).toBeNull();
      expect(
        screen.getByText("This behavior is owned by [D8] and friends."),
      ).toBeInTheDocument();
    });

    it("keeps prose lines beyond the label line inside the question frame", () => {
      renderMd("[Q1. Title] (Tags: high)\ncontinued prose on the next line");
      const frame = screen.getByTestId("markdown-question-frame");
      expect(frame.textContent).toContain("continued prose on the next line");
      expect(frame.textContent).not.toContain("[Q1.");
    });
  });

  // =========================================================================
  // Images
  // =========================================================================

  describe("images", () => {
    it("renders standalone image blocks", () => {
      renderMd('![Workspace screenshot](imgs/workspace.png "Workspace")');

      const image = screen.getByRole("img", {
        name: "Workspace screenshot",
      });
      expect(image).toHaveAttribute("src", "imgs/workspace.png");
      expect(image).toHaveAttribute("title", "Workspace");
      expect(image.closest(".markdown-body")).not.toBeNull();
      expect(image).toHaveAttribute("loading", "lazy");
    });

    it("resolves relative guide image paths under the guide base path", () => {
      render(
        <MarkdownRenderer
          content="![Guide image](imgs/getting-started.png)"
          basePath="/user-guide/"
        />,
      );

      expect(screen.getByRole("img", { name: "Guide image" })).toHaveAttribute(
        "src",
        "/user-guide/imgs/getting-started.png",
      );
    });

    it("does not render javascript protocol image sources", () => {
      renderMd("![Unsafe](javascript:alert)");

      expect(
        screen.queryByRole("img", { name: "Unsafe" }),
      ).not.toBeInTheDocument();
    });

    it("does not render data protocol image sources", () => {
      renderMd("![Unsafe](data:text/html;base64,PHNjcmlwdA==)");

      expect(
        screen.queryByRole("img", { name: "Unsafe" }),
      ).not.toBeInTheDocument();
    });
  });

  // =========================================================================
  // Code blocks
  // =========================================================================

  describe("code blocks", () => {
    it("renders a code block with language label", () => {
      renderMd("```rust\nfn main() {}\n```");
      // Language label should be present
      expect(screen.getByText("rust")).toBeInTheDocument();
      // Code should contain the content
      const codeEl = document.querySelector("code");
      expect(codeEl).toBeInTheDocument();
    });

    it("renders a code block without language", () => {
      renderMd("```\nplain text\n```");
      const codeEl = document.querySelector("code");
      expect(codeEl).toBeInTheDocument();
    });

    it("applies Rust syntax highlighting to rust code blocks", () => {
      renderMd("```rust\nfn main() {}\n```");
      const codeEl = document.querySelector("code");
      expect(codeEl?.innerHTML).toContain(
        'style="color:var(--g5-syn-keyword)"',
      );
    });

    it("applies syntax highlighting to non-Rust code blocks", () => {
      renderMd('```html\n<div class="test">\n```');
      const codeEl = document.querySelector("code");
      // The markup is escaped text (no real <div> injected), and hljs
      // wraps tag tokens in styled spans.
      expect(codeEl?.querySelector("div")).toBeNull();
      expect(codeEl?.textContent).toContain('<div class="test">');
    });

    it("renders mermaid code blocks as MermaidDiagram", () => {
      renderMd("```mermaid\ngraph LR\n  A --> B\n```");
      expect(screen.getByTestId("mermaid-diagram")).toBeInTheDocument();
    });

    it("renders g5-js code blocks inside a sandboxed iframe", () => {
      renderMd("```g5-js\ng5.setText('hello from sandbox');\n```");

      const widget = screen.getByTestId("sandboxed-js-widget");
      const frame = screen.getByTestId("sandboxed-js-widget-frame");

      expect(widget).toBeInTheDocument();
      expect(frame).toBeInstanceOf(HTMLIFrameElement);
      expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
      expect(frame.getAttribute("sandbox")).not.toContain("allow-same-origin");
      expect(frame.getAttribute("referrerpolicy")).toBe("no-referrer");
      expect(frame.getAttribute("srcdoc")).toContain("connect-src 'none'");
      expect(frame.getAttribute("srcdoc")).not.toContain("unsafe-eval");
      expect(frame.getAttribute("srcdoc")).not.toContain("new Function");
    });

    it("renders g5-reactflow code blocks as allowlisted React Flow widgets", () => {
      renderMd(
        '```g5-reactflow\n{"nodes":[{"id":"a","label":"A"}],"edges":[]}\n```',
      );

      expect(screen.getByTestId("react-flow-diagram-widget")).toHaveTextContent(
        '"nodes"',
      );
    });

    it("renders g5-question code blocks as read-only question cards", () => {
      renderMd(
        [
          "```g5-question",
          "schema: 1",
          "id: pick-a-backend",
          "status: open",
          "prompt: Which backend?",
          "options:",
          "  - label: Postgres",
          "    description: Reuse the primary database.",
          "    recommended: true",
          "```",
          "",
          "```rust",
          "fn main() {}",
          "```",
        ].join("\n"),
      );

      const card = screen.getByTestId("question-card");
      expect(card).toBeInTheDocument();
      expect(card).toHaveTextContent("Which backend?");
      expect(card).toHaveTextContent("Recommended");
      // The question fence renders as a card, not a labeled code block…
      expect(screen.queryByText("g5-question")).not.toBeInTheDocument();
      // …while other fences keep their plain CodeBlock rendering.
      expect(screen.getByText("rust")).toBeInTheDocument();
    });
  });

  describe("code block Copy", () => {
    afterEach(() => {
      vi.useRealTimers();
      Reflect.deleteProperty(navigator, "clipboard");
    });

    function stubClipboard(writeText: (text: string) => Promise<void>) {
      const spy = vi.fn(writeText);
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText: spy },
      });
      return spy;
    }

    it("copies the exact fence text, not the highlighted markup", async () => {
      const writeText = stubClipboard(async () => {});
      const fence = '  <div class="a">\n\n\tconst x = 1; // "quoted" & <b>\n';
      renderMd("```html\n" + fence + "\n```");

      fireEvent.click(screen.getByRole("button", { name: "Copy code" }));

      expect(await screen.findByText("Copied")).toBeInTheDocument();
      expect(writeText).toHaveBeenCalledTimes(1);
      expect(writeText).toHaveBeenCalledWith(fence);
    });

    it("shows the header with Copy for a fence without a language", async () => {
      const writeText = stubClipboard(async () => {});
      renderMd("```\nplain text\n```");

      const block = screen.getByTestId("markdown-code-block");
      fireEvent.click(within(block).getByRole("button", { name: "Copy code" }));

      await screen.findByText("Copied");
      expect(writeText).toHaveBeenCalledWith("plain text");
    });

    it("copies restricted fences, which render as inert code", async () => {
      const writeText = stubClipboard(async () => {});
      render(
        <MarkdownRenderer
          content={"```mermaid\ngraph LR\n  A --> B\n```"}
          trustedContent={false}
        />,
      );

      expect(screen.queryByTestId("mermaid-diagram")).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Copy code" }));

      await screen.findByText("Copied");
      expect(writeText).toHaveBeenCalledWith("graph LR\n  A --> B");
    });

    it("leaves diagram, widget and question fences without a Copy control", () => {
      renderMd(
        [
          "```mermaid",
          "graph LR",
          "```",
          "",
          "```g5-reactflow",
          '{"nodes":[],"edges":[]}',
          "```",
          "",
          "```g5-question",
          "schema: 1",
          "id: q",
          "status: open",
          "prompt: Which?",
          "options:",
          "  - label: A",
          "```",
        ].join("\n"),
      );

      expect(
        screen.queryByRole("button", { name: "Copy code" }),
      ).not.toBeInTheDocument();
    });

    it("says so when the clipboard is unavailable", async () => {
      Reflect.deleteProperty(navigator, "clipboard");
      renderMd("```sh\nls\n```");

      fireEvent.click(screen.getByRole("button", { name: "Copy code" }));

      const status = screen.getByRole("status");
      expect(await within(status).findByText("Couldn't copy")).toBeVisible();
      expect(screen.queryByText("Copied")).not.toBeInTheDocument();
    });

    it("says so when the browser refuses the write", async () => {
      stubClipboard(async () => {
        throw new DOMException("Document is not focused.", "NotAllowedError");
      });
      renderMd("```sh\nls\n```");

      fireEvent.click(screen.getByRole("button", { name: "Copy code" }));

      expect(await screen.findByText("Couldn't copy")).toBeInTheDocument();
    });

    it("is reachable and usable from the keyboard", async () => {
      const user = userEvent.setup();
      const writeText = stubClipboard(async () => {});
      renderMd("```sh\necho hi\n```");

      await user.tab();
      expect(screen.getByRole("button", { name: "Copy code" })).toHaveFocus();
      await user.keyboard("{Enter}");

      expect(await screen.findByText("Copied")).toBeInTheDocument();
      expect(writeText).toHaveBeenCalledWith("echo hi");
    });

    it("clears the Copied feedback after a moment", async () => {
      vi.useFakeTimers();
      stubClipboard(async () => {});
      renderMd("```sh\nls\n```");

      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Copy code" }));
      });
      expect(screen.getByRole("status")).toHaveTextContent("Copied");

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1500);
      });
      expect(screen.getByRole("status")).toHaveTextContent("");
    });

    it("reports only the latest of overlapping presses", async () => {
      let refuseFirst: () => void = () => {};
      let calls = 0;
      stubClipboard(
        () =>
          new Promise<void>((resolve, reject) => {
            calls += 1;
            if (calls === 1) {
              refuseFirst = () => reject(new Error("refused"));
            } else {
              resolve();
            }
          }),
      );
      renderMd("```sh\nls\n```");
      const button = screen.getByRole("button", { name: "Copy code" });

      fireEvent.click(button);
      fireEvent.click(button);
      await screen.findByText("Copied");
      await act(async () => refuseFirst());

      expect(screen.getByRole("status")).toHaveTextContent("Copied");
    });
  });

  // =========================================================================
  // Tables
  // =========================================================================

  describe("tables", () => {
    it("renders a GFM pipe table", () => {
      renderMd("| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |");
      const table = document.querySelector("table");
      expect(table).toBeInTheDocument();
    });

    it("renders table headers", () => {
      renderMd("| Name | Age |\n| --- | --- |\n| Alice | 30 |");
      const headers = document.querySelectorAll("th");
      expect(headers).toHaveLength(2);
      expect(headers[0].textContent).toBe("Name");
      expect(headers[1].textContent).toBe("Age");
    });

    it("renders table rows", () => {
      renderMd("| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |");
      const rows = document.querySelectorAll("tbody tr");
      expect(rows).toHaveLength(2);
    });

    it("renders inline formatting in table cells", () => {
      renderMd("| Name | Status |\n| --- | --- |\n| Alice | **active** |");
      const strong = document.querySelector("td strong");
      expect(strong).toBeInTheDocument();
      expect(strong?.textContent).toBe("active");
    });
  });

  // =========================================================================
  // Callouts
  // =========================================================================

  describe("callouts", () => {
    it("renders a note callout with info icon", () => {
      renderMd(":::note\nThis is a note\n:::");
      expect(screen.getByText("ℹ️")).toBeInTheDocument();
      expect(screen.getByText(/This is a note/)).toBeInTheDocument();
    });

    it("renders a warning callout with warning icon", () => {
      renderMd(":::warning\nBe careful\n:::");
      expect(screen.getByText("⚠️")).toBeInTheDocument();
    });

    it("renders a tip callout with lightbulb icon", () => {
      renderMd(":::tip\nHelpful hint\n:::");
      expect(screen.getByText("💡")).toBeInTheDocument();
    });

    it("renders a danger callout with alert icon", () => {
      renderMd(":::danger\nCritical issue\n:::");
      expect(screen.getByText("🚨")).toBeInTheDocument();
    });
  });

  // =========================================================================
  // Lists
  // =========================================================================

  describe("lists", () => {
    it("renders bullet lists (unordered)", () => {
      renderMd("- Alpha\n- Beta\n- Gamma");
      const list = document.querySelector("ul");
      expect(list).toBeInTheDocument();
      const items = document.querySelectorAll("ul li");
      expect(items).toHaveLength(3);
      expect(items[0].textContent).toBe("Alpha");
    });

    it("renders numbered lists (ordered)", () => {
      renderMd("1. First\n2. Second\n3. Third");
      const list = document.querySelector("ol");
      expect(list).toBeInTheDocument();
      const items = document.querySelectorAll("ol li");
      expect(items).toHaveLength(3);
    });

    it("preserves ordered-list starts across interrupted guide blocks", () => {
      renderMd(
        "1. Fork workspace\n\n![Fork dialog](imgs/fork.png)\n\n2. Describe intent",
      );

      const lists = document.querySelectorAll("ol");
      expect(lists).toHaveLength(2);
      expect(lists[0]).not.toHaveAttribute("start");
      expect(lists[0].querySelector("li")?.textContent).toBe("Fork workspace");
      expect(lists[1]).toHaveAttribute("start", "2");
      expect(lists[1].querySelector("li")?.textContent).toBe("Describe intent");
    });

    it("renders inline formatting in list items", () => {
      renderMd("- **Bold item**\n- *Italic item*");
      const strong = document.querySelector("li strong");
      expect(strong).toBeInTheDocument();
      expect(strong?.textContent).toBe("Bold item");
    });
  });

  // =========================================================================
  // Blockquotes
  // =========================================================================

  describe("blockquotes", () => {
    it("renders a blockquote", () => {
      renderMd("> This is a quote");
      const bq = document.querySelector("blockquote");
      expect(bq).toBeInTheDocument();
      expect(bq?.textContent).toContain("This is a quote");
    });

    it("renders multi-line blockquotes", () => {
      renderMd("> Line one\n> Line two");
      const bq = document.querySelector("blockquote");
      expect(bq).toBeInTheDocument();
    });

    it("applies left border styling", () => {
      renderMd("> Quote");
      const bq = document.querySelector("blockquote");
      expect(bq?.closest(".markdown-body")).not.toBeNull();
    });
  });

  // =========================================================================
  // Horizontal rules
  // =========================================================================

  describe("horizontal rules", () => {
    it("renders --- as <hr>", () => {
      renderMd("Paragraph\n\n---");
      const hr = document.querySelector("hr");
      expect(hr).toBeInTheDocument();
    });
  });

  // =========================================================================
  // Frontmatter suppression
  // =========================================================================

  it("can render inside a timeline article without introducing another article", () => {
    render(
      <article>
        <MarkdownRenderer content="A comment with **Markdown**." />
      </article>,
    );
    expect(screen.getAllByRole("article")).toHaveLength(1);
    expect(screen.getByRole("article")).toHaveTextContent(
      "A comment with Markdown.",
    );
  });

  describe("frontmatter", () => {
    it("does not render frontmatter content visually", () => {
      renderMd("---\ntitle: Secret\n---\n\n# Visible");
      // The heading should be visible
      expect(screen.getByRole("heading")).toBeInTheDocument();
      // The frontmatter key/value should NOT be visible
      expect(screen.queryByText("Secret")).not.toBeInTheDocument();
    });

    it("does not add any DOM elements for frontmatter", () => {
      const { container } = renderMd("---\nkey: value\n---");
      // The prose wrapper exists, but there should be no content children rendered
      const proseDivChildren = container.querySelector(
        '[data-testid="markdown-preview"]',
      )?.children;
      expect(proseDivChildren?.length ?? 0).toBe(0);
    });
  });

  // =========================================================================
  // Mixed content
  // =========================================================================

  describe("mixed content", () => {
    it("renders a document with multiple block types", () => {
      const md = `# Title

A paragraph with **bold** text.

## Features

- Feature one
- Feature two

\`\`\`rust
fn hello() {}
\`\`\`

> A quote

| Col1 | Col2 |
| --- | --- |
| row1 | data |

:::tip
A helpful tip
:::`;

      renderMd(md);

      // Verify multiple block types are present
      expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();
      expect(screen.getByRole("heading", { level: 2 })).toBeInTheDocument();
      expect(document.querySelector("strong")).toBeInTheDocument();
      expect(document.querySelector("ul")).toBeInTheDocument();
      expect(document.querySelector("code")).toBeInTheDocument();
      expect(document.querySelector("blockquote")).toBeInTheDocument();
      expect(document.querySelector("table")).toBeInTheDocument();
      expect(screen.getByText("💡")).toBeInTheDocument();
    });
  });

  // =========================================================================
  // Emoji support
  // =========================================================================

  describe("emoji shortcodes", () => {
    it("replaces emoji shortcodes in paragraphs", () => {
      renderMd(":rocket: Launch");
      expect(screen.getByText(/🚀/)).toBeInTheDocument();
    });

    it("replaces emoji shortcodes in list items", () => {
      renderMd("- :check: Done");
      expect(screen.getByText(/✅/)).toBeInTheDocument();
    });
  });
});

describe("shared question metadata", () => {
  it.each([
    "[Q1. Open question]\n\nQuestion body.",
    "## Question heading\n\n[Q · example.question]\n\nQuestion body.\n\n## Metadata\n\n### example.question\n- uncertain\n",
  ])(
    "keeps identifiers in collapsed Metadata without trace controls",
    (content) => {
      renderMd(content);
      const frame = screen.getByTestId("semantic-record");
      const id = within(frame).getByTestId("semantic-record-id");
      const title = within(frame).queryByTestId("semantic-record-title");
      if (content.startsWith("[Q1.")) {
        expect(title).toBeVisible();
        expect(title).toHaveTextContent("Open question");
      } else {
        expect(title).toBeNull();
        expect(
          screen.queryByRole("heading", { name: "Question heading" }),
        ).toBeNull();
      }
      expect(screen.queryByRole("heading", { name: "Metadata" })).toBeNull();
      expect(id).not.toBeVisible();
      expect(
        within(frame).queryByTestId("semantic-record-parent-control"),
      ).toBeNull();
      expect(
        within(frame).queryByTestId("semantic-record-child-control"),
      ).toBeNull();
      fireEvent.click(
        within(frame).getByTestId("semantic-record-details-toggle"),
      );
      expect(id).toBeVisible();
      if (title) {
        expect(
          within(frame).getAllByTestId("semantic-record-title"),
        ).toHaveLength(1);
        expect(title).toBeVisible();
      }
    },
  );
});
