// Actual MarkdownRenderer (trusted and restricted) and FILES MarkdownPreview,
// for the shared code-block Copy control (`g5.documents.markdown.code-copy`).
import React from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarkdownRenderer } from "../../src/components/shared/MarkdownRenderer";
import { MarkdownPreview } from "../../src/components/files/MarkdownPreview";
import "../../src/index.css";

document.documentElement.dataset.theme = "alpine-graphite";
document.documentElement.dataset.mode = "light";

const trustedFence = 'def greet(name):\n\n    return f"<hi> {name}"';
const plainFence = "  two leading spaces\n\ttab";
const restrictedFence = "graph LR\n  A --> B";
const previewFence = 'fn main() {\n    println!("hi");\n}';

const client = new QueryClient();
createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={client}>
    <main style={{ maxWidth: 640, padding: 16, background: "var(--g5-bg0)" }}>
      <section data-testid="trusted">
        <MarkdownRenderer
          content={[
            "```python",
            trustedFence,
            "```",
            "",
            "```",
            plainFence,
            "```",
            "",
            "```mermaid",
            "graph TD; A-->B",
            "```",
          ].join("\n")}
        />
      </section>
      <section data-testid="restricted">
        <MarkdownRenderer
          trustedContent={false}
          content={["```mermaid", restrictedFence, "```"].join("\n")}
        />
      </section>
      <section data-testid="preview">
        <MarkdownPreview
          content={["```rust", previewFence, "```"].join("\n")}
          filePath="README.md"
          workspaceId="workspace-fixture"
          repoId="repo-fixture"
        />
      </section>
    </main>
  </QueryClientProvider>,
);
