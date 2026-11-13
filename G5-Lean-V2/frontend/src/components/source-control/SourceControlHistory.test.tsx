import { expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { server } from "../../test/msw/server";
import type { SourceRepo } from "../../lib/sourceControlSelection";
import { SourceControlHistory } from "./SourceControlHistory";

it.each([true, false])(
  "retains pages for target-only advances and resets changed source cuts (unpushedOnly=%s)",
  async (unpushedOnly) => {
    const reads: {
      pushed: string;
      cursor: string | null;
      base: string | null;
    }[] = [];
    server.use(
      http.post(
        "/api/v1/workspaces/base/source-control/history",
        async ({ request }) => {
          const input = (await request.json()) as {
            pushed: string;
            cursor: string | null;
            base: string | null;
          };
          reads.push(input);
          return HttpResponse.json({
            data: {
              commits: [
                {
                  sha: input.pushed,
                  summary: `History ${input.pushed[0]}`,
                  authorName: "Author",
                  committedAt: 1_700_000_000,
                  parents: [],
                },
              ],
              nextCursor: input.pushed,
            },
          });
        },
      ),
    );
    const repo: SourceRepo = {
      repoId: "code",
      name: "Code",
      base: unpushedOnly ? null : "d".repeat(40),
      parent: unpushedOnly ? null : "c".repeat(40),
      comparison: unpushedOnly
        ? null
        : {
            mergeBase: "d".repeat(40),
            mergeBaseCommit: {
              sha: "d".repeat(40),
              summary: "Merge base commit",
              authorName: "Author",
              committedAt: 1_700_000_000,
              parents: [],
            },
            parentMerges: [],
            workspaceBase: null,
            workspaceBaseInHistory: false,
            sinceWorkspaceBaseCount: 0,
          },
      pushed: "a".repeat(40),
      commits: [],
      unpushedOnly,
      truncated: false,
      error: null,
      live: null,
    };
    const onToggle = vi.fn();
    const props = {
      workspaceId: "base",
      repo,
      chosen: [],
      onToggle,
      remember: vi.fn(),
    };
    const view = render(<SourceControlHistory {...props} />);
    const user = userEvent.setup();
    await user.click(screen.getByText("Show history"));
    await screen.findByText("History a");
    expect(reads[0].base).toBe(repo.base);
    const retained = screen.getByRole("checkbox", { name: "aaaaaaa in Code" });
    if (!unpushedOnly) {
      view.rerender(
        <SourceControlHistory
          {...props}
          repo={{ ...repo, parent: "e".repeat(40) }}
        />,
      );
      expect(screen.getByRole("checkbox", { name: "aaaaaaa in Code" })).toBe(
        retained,
      );
      expect(reads).toHaveLength(1);
    }
    view.rerender(
      <SourceControlHistory
        {...props}
        repo={{ ...repo, pushed: "b".repeat(40) }}
      />,
    );
    expect(screen.queryByText("History a")).toBeNull();
    expect(reads).toHaveLength(1);
    await user.click(screen.getByText("Show history"));
    await screen.findByText("History b");
    expect(reads.map(({ pushed, cursor }) => ({ pushed, cursor }))).toEqual([
      { pushed: "a".repeat(40), cursor: null },
      { pushed: "b".repeat(40), cursor: null },
    ]);
    await user.click(screen.getByRole("checkbox", { name: "bbbbbbb in Code" }));
    expect(onToggle).toHaveBeenCalledWith("b".repeat(40), true);
  },
);
