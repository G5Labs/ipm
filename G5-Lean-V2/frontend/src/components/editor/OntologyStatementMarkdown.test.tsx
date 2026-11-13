import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { OntologyStatementMarkdown } from "./OntologyStatementMarkdown";

describe("ontology statement image references", () => {
  it.each(["./diagram.png", "https://example.com/diagram.png"])(
    "renders a valid image with the shared Markdown pipeline: %s",
    (target) => {
      render(<OntologyStatementMarkdown content={`![Diagram](${target})`} />);
      expect(screen.getByRole("img", { name: "Diagram" })).toHaveAttribute(
        "src",
        target,
      );
      expect(screen.queryByRole("link")).not.toBeInTheDocument();
    },
  );

  it.each(["", "javascript:alert%281%29", "data:text/html,unsafe"])(
    "renders plain alt text for an empty or rejected image target: %s",
    (target) => {
      const { container } = render(
        <OntologyStatementMarkdown content={`![Diagram](${target})`} />,
      );
      expect(screen.getByText("Diagram")).toBeVisible();
      expect(container.querySelector("a")).toBeNull();
      expect(screen.queryByRole("img")).not.toBeInTheDocument();
    },
  );

  it("provides fallback text when the target and alt text are empty", () => {
    render(<OntologyStatementMarkdown content="![]()" />);
    expect(screen.getByText("Image")).toBeVisible();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it("keeps historical image references non-navigable", () => {
    const { container } = render(
      <OntologyStatementMarkdown
        content="![Diagram](./diagram.png)"
        historical
      />,
    );
    expect(screen.getByText("Diagram")).toBeVisible();
    expect(container.querySelector("a")).toBeNull();
    expect(screen.queryByRole("img")).not.toBeInTheDocument();
  });
});
