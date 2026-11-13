import { describe, expect, it } from "vitest";

import { workspaceReadableDocPath } from "./WorkspacePage";

describe("workspaceReadableDocPath", () => {
  it("materializes project docs and views under the docs repo mount", () => {
    expect(
      workspaceReadableDocPath(
        "docs/architecture_sot_contradiction_register.md",
      ),
    ).toBe("docs/docs/architecture_sot_contradiction_register.md");
    expect(workspaceReadableDocPath("views/architecture.pm_view.md")).toBe(
      "docs/views/architecture.pm_view.md",
    );
    expect(workspaceReadableDocPath("designs/README.md")).toBe(
      "docs/designs/README.md",
    );
  });

  it("does not double-prefix already materialized docs repo paths", () => {
    expect(workspaceReadableDocPath("docs/docs/already.md")).toBe(
      "docs/docs/already.md",
    );
    expect(workspaceReadableDocPath("docs/views/already.pm_view.md")).toBe(
      "docs/views/already.pm_view.md",
    );
    expect(workspaceReadableDocPath("docs/designs/README.md")).toBe(
      "docs/designs/README.md",
    );
  });

  it("leaves non-docs-repo paths unchanged", () => {
    expect(workspaceReadableDocPath("src/main.rs")).toBe("src/main.rs");
  });
});
