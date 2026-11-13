import { describe, expect, it } from "vitest";

import { workspaceNameFromIssue } from "./workspaceNameFromIssue";

const issue = (number: number, title: string) => ({
  locator: { number },
  title,
});

describe("workspaceNameFromIssue", () => {
  it("combines the issue number with a slug of its title", () => {
    expect(workspaceNameFromIssue(issue(12, "Crash on save"))).toBe(
      "issue-12-crash-on-save",
    );
    expect(
      workspaceNameFromIssue(issue(7, "  Fix: `refresh()` races (again!) ")),
    ).toBe("issue-7-fix-refresh-races-again");
    expect(workspaceNameFromIssue(issue(3, "Café naïve résumé"))).toBe(
      "issue-3-cafe-naive-resume",
    );
  });

  it("falls back to the number when the title has no usable characters", () => {
    expect(workspaceNameFromIssue(issue(42, "🚀🚀"))).toBe("issue-42");
    expect(workspaceNameFromIssue(issue(42, ""))).toBe("issue-42");
  });

  it("cuts long titles at a word boundary", () => {
    const name = workspaceNameFromIssue(
      issue(
        9,
        "Workspace switcher navigates to coming soon when the name contains a slash",
      ),
    );
    expect(name).toBe(
      "issue-9-workspace-switcher-navigates-to-coming-soon-when",
    );
    expect(name.endsWith("-")).toBe(false);
    // A cut inside a word drops the partial word.
    expect(
      workspaceNameFromIssue(
        issue(9, "Workspace switcher navigates to coming soon whenever it can"),
      ),
    ).toBe("issue-9-workspace-switcher-navigates-to-coming-soon");
  });
});
