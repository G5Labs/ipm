import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MarkdownPreview } from "./MarkdownPreview";

describe("shared read-only Markdown links", () => {
  it("keeps a qualified file reference on the workspace navigation delegate", () => {
    render(
      <MarkdownPreview content="See [`main.rs`](g5://file/g5d/src/main.rs?repo=g5-code#L12)." />,
    );
    const link = screen.getByRole("link", { name: "main.rs" });
    expect(link).toHaveAttribute(
      "href",
      "g5://file/g5d/src/main.rs?repo=g5-code#L12",
    );
    expect(link).toHaveAttribute("data-g5-file-link", "true");
    expect(link).not.toHaveAttribute("target");
    expect(link.querySelector("code")).toHaveTextContent("main.rs");
  });

  it("renders nested links and code through the common pipeline", () => {
    const { container } = render(
      <MarkdownPreview content="**Before generation — [ontology-planning](.agents/skills/planning/SKILL.md)** with *a `code` example*." />,
    );
    expect(container.querySelector("strong a")).toHaveAttribute(
      "href",
      ".agents/skills/planning/SKILL.md",
    );
    expect(container.querySelector("em code")).toHaveTextContent("code");
    expect(container.querySelector("[contenteditable]")).toBeNull();
  });

  it.each([
    '[open](g5://file/x" onmouseover="confirm`x`)',
    '[x](https://e.com/a"onfocus=boom())',
    "[x](javascript:alert)",
    '<a href="javascript:alert(1)" onmouseover="alert(1)">raw</a>',
  ])(
    "does not turn a link payload into executable attributes: %s",
    (content) => {
      const { container } = render(<MarkdownPreview content={content} />);
      expect(
        container.querySelector("[onmouseover], [onfocus], script, iframe"),
      ).toBeNull();
      expect(container.querySelector('a[href^="javascript:"]')).toBeNull();
    },
  );

  it("keeps restricted, retired and historical app navigation inert", () => {
    const content =
      "**[file](g5://file/private.md)** and [old](g5://ontology/core#D1)";
    const view = render(
      <MarkdownPreview content={content} trustedContent={false} />,
    );
    expect(view.container.querySelector("a")).toBeNull();
    expect(view.container).toHaveTextContent("g5://file/private.md");
    view.rerender(<MarkdownPreview content={content} historical />);
    expect(view.container.querySelector("a")).toBeNull();
  });
});
