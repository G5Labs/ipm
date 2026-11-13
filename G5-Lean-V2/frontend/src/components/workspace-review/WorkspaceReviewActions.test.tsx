/** Production dialog contracts; real-stack journeys additionally verify durable
 * authorization and persistence in e2e/flows/workspace-review-*.spec.ts. */
import { beforeEach, describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import { createMockUser } from "../../test/fixtures";
import type { OrgApprovalDefinitionDTO, WorkspacePhase } from "../../lib/types";
import type {
  ReviewOverview,
  ReviewRequest,
} from "../../hooks/useWorkspaceReviewFlow";
import { WorkspaceReviewActions } from "./WorkspaceReviewActions";
const ws = "review-workspace",
  org = "org-g5labs",
  owner = "user-alex",
  reviewer = "user-jordan";
const url = `/api/v1/workspaces/${ws}`;
const heads = [
  {
    repoId: "code",
    workspaceId: ws,
    branchName: ws,
    headCommit: "a".repeat(40),
  },
  {
    repoId: "docs",
    workspaceId: ws,
    branchName: ws,
    headCommit: "b".repeat(40),
  },
];
const human = (id: string) => ({
  userId: id,
  displayName: id,
  email: `${id}@example.test`,
  avatarUrl: null,
  stillMember: true,
});
const definition: OrgApprovalDefinitionDTO = {
  id: "security",
  latestVersion: 1,
  name: "Security",
  description: "Security review",
  humans: [human(owner), human(reviewer), human("other")],
  agentType: null,
  agentPrompt: null,
  invalidatesPrevious: false,
  phases: ["be", "se", "ie"],
  createdBy: { userId: owner, displayName: owner },
  createdAt: "2026-10-12T10:00:00Z",
  firstCreatedBy: { userId: owner, displayName: owner },
  firstCreatedAt: "2026-10-12T10:00:00Z",
};
let data: ReviewOverview;
let posts: Record<string, unknown>[];
const task = (phase: WorkspacePhase, person = reviewer): ReviewRequest => ({
  id: `security-${phase}-${person}`,
  definitionId:
    phase === "be"
      ? "business"
      : phase === "se"
        ? "security"
        : "implementation",
  definitionVersion: 1,
  name:
    phase === "be"
      ? "Business"
      : phase === "se"
        ? "Security"
        : "Implementation",
  phase,
  actorUserId: person,
  requestedBy: owner,
  state: "pending",
  createdAt: "2026-10-12T11:00:00Z",
  closedAt: null,
});
function mount(id = reviewer, search = "") {
  return renderApp(
    <Routes>
      <Route
        path="/:org/:workspace"
        element={
          <WorkspaceReviewActions workspaceId={ws} definitionId="security" />
        }
      />
    </Routes>,
    {
      route: `/g5labs/review-workspace${search}`,
      user: createMockUser({
        id,
        orgs: [
          { orgId: org, orgSlug: "g5labs", orgName: "G5 Labs", role: "member" },
        ],
      }),
    },
  );
}
async function openReview() {
  await userEvent
    .setup()
    .click(await screen.findByRole("button", { name: "Approve" }));
  return screen.getByRole("dialog", { name: "Submit review" });
}
beforeEach(() => {
  sessionStorage.clear();
  posts = [];
  data = {
    workspace: {
      ownerUserId: owner,
      targetWorkspaceId: "parent",
      phase: "se",
      readOnly: false,
    },
    pr: {
      id: "pr-1",
      title: "Implementation",
      description: "Implementation decisions",
      sourceHeads: heads,
      createdAt: "2026-10-12T10:00:00Z",
    },
    heads: structuredClone(heads),
    baseHeads: heads,
    parentHeads: heads,
    pushedAt: {},
    repos: [
      { id: "code", name: "Code", role: "source", rootPath: "code" },
      { id: "docs", name: "Docs", role: "docs", rootPath: "docs" },
    ],
    requests: [task("be"), task("se"), task("ie"), task("se", "other")],
    decisions: [],
  };
  server.use(
    http.get(`${url}/review-flow`, () => HttpResponse.json({ data })),
    http.get(`/api/v1/orgs/${org}/approval-definitions`, () =>
      HttpResponse.json({
        data: [
          definition,
          {
            ...definition,
            id: "business",
            name: "Business",
            humans: [human(reviewer)],
          },
        ],
      }),
    ),
    http.post(`${url}/approvals/decisions`, async ({ request: r }) => {
      const body = (await r.json()) as Record<string, unknown>;
      posts.push(body);
      const t = data.requests.find((t) => t.id === body.requestId)!;
      t.state = "settled";
      data.decisions.unshift({
        id: "decision-1",
        requestId: t.id,
        definitionId: t.definitionId,
        name: t.name,
        phase: t.phase,
        actorUserId: reviewer,
        outcome: String(body.outcome),
        comment: String(body.comment ?? ""),
        reviewedGitHeads: body.reviewedGitHeads as typeof heads,
        createdAt: "2026-10-12T12:00:00Z",
      });
      return HttpResponse.json({ data: {} });
    }),
    http.post(`${url}/approvals/requests`, async ({ request: r }) => {
      posts.push((await r.json()) as Record<string, unknown>);
      return HttpResponse.json({ data: {} });
    }),
  );
});
describe("addressed requirement decisions", () => {
  it("shows only the addressed requirement and captures both repository heads", async () => {
    mount();
    const d = within(await openReview());
    expect(d.getAllByRole("option")).toHaveLength(1);
    await userEvent.click(d.getByRole("button", { name: "Submit review" }));
    await waitFor(() =>
      expect(posts[0]).toMatchObject({
        requestId: task("se").id,
        reviewedGitHeads: heads,
        outcome: "approved",
      }),
    );
  });
  it("offers self approval only under both captured and current policy", async () => {
    data.requests = [{ ...task("se", owner), allowSelfApproval: true }];
    const app = mount(owner);
    await waitFor(() =>
      expect(
        app.queryClient.getQueryData(["workspace-review-flow", ws]),
      ).toBeTruthy(),
    );
    expect(screen.queryByRole("button", { name: "Approve" })).toBeNull();
    server.use(
      http.get(`/api/v1/orgs/${org}/approval-definitions`, () =>
        HttpResponse.json({
          data: [{ ...definition, allowSelfApproval: true }],
        }),
      ),
    );
    await app.queryClient.invalidateQueries({
      queryKey: ["org-approval-definitions"],
    });
    // A remount creates a fresh library read with the new policy.
    app.unmount();
    mount(owner);
    expect(
      await screen.findByRole("button", { name: "Approve" }),
    ).toBeEnabled();
  });
  it("gives an unrequested configured pool member no decision", async () => {
    data.requests = data.requests.filter((r) => r.actorUserId !== "other");
    const app = mount("other");
    await waitFor(() =>
      expect(
        app.queryClient.getQueryData(["workspace-review-flow", ws]),
      ).toBeTruthy(),
    );
    expect(screen.queryByRole("button", { name: "Approve" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Reject" })).toBeNull();
  });
  it("retains a rejection draft and requires a reason", async () => {
    const user = userEvent.setup();
    const app = mount();
    await user.click(await screen.findByRole("button", { name: "Reject" }));
    let d = within(screen.getByRole("dialog"));
    expect(d.getByRole("button", { name: "Submit review" })).toBeDisabled();
    await user.type(d.getByLabelText("Review comment"), "Please fix the race");
    await user.keyboard("{Escape}");
    app.unmount();
    mount();
    await user.click(await screen.findByRole("button", { name: "Approve" }));
    d = within(screen.getByRole("dialog"));
    expect(d.getByRole("radio", { name: "Request changes" })).toBeChecked();
    expect(d.getByLabelText("Review comment")).toHaveValue(
      "Please fix the race",
    );
    await user.click(d.getByRole("button", { name: "Submit review" }));
    await waitFor(() =>
      expect(posts[0]).toMatchObject({
        outcome: "rejected",
        comment: "Please fix the race",
      }),
    );
  });
  it("keeps the captured vector after a concurrent docs push", async () => {
    const user = userEvent.setup();
    const app = mount();
    const d = within(await openReview());
    await user.type(d.getByLabelText("Review comment"), "Captured original");
    data.heads = [heads[0], { ...heads[1], headCommit: "c".repeat(40) }];
    await app.queryClient.invalidateQueries({
      queryKey: ["workspace-review-flow", ws],
    });
    expect(await d.findByRole("status")).toHaveTextContent(
      "A new push is available in Docs",
    );
    await user.click(d.getByRole("button", { name: "Submit review" }));
    await waitFor(() =>
      expect(posts[0]).toMatchObject({
        reviewedGitHeads: heads,
        comment: "Captured original",
      }),
    );
  });
  it("retries a failed decision without losing outcome or text", async () => {
    let fail = true;
    server.use(
      http.post(`${url}/approvals/decisions`, async ({ request }) => {
        posts.push((await request.json()) as Record<string, unknown>);
        return fail
          ? HttpResponse.json(
              { error: { message: "Connection failed" } },
              { status: 503 },
            )
          : HttpResponse.json({ data: {} });
      }),
    );
    const user = userEvent.setup();
    mount();
    const d = within(await openReview());
    await user.click(d.getByRole("radio", { name: "Request changes" }));
    await user.type(d.getByLabelText("Review comment"), "Keep this reason");
    await user.click(d.getByRole("button", { name: "Submit review" }));
    expect(await d.findByRole("alert")).toBeVisible();
    fail = false;
    await user.click(d.getByRole("button", { name: "Submit review" }));
    await waitFor(() => expect(posts).toHaveLength(2));
    expect(posts[1]).toEqual(posts[0]);
  });
  it("keeps a disappeared request's draft and refuses retargeting", async () => {
    const user = userEvent.setup();
    const app = mount();
    const d = within(await openReview());
    await user.type(d.getByLabelText("Review comment"), "Keep this draft");
    data.pr = null;
    data.requests = [];
    await app.queryClient.invalidateQueries({
      queryKey: ["workspace-review-flow", ws],
    });
    await user.click(d.getByRole("button", { name: "Submit review" }));
    expect(await d.findByRole("alert")).toHaveTextContent(
      "no longer available",
    );
    expect(d.getByLabelText("Review comment")).toHaveValue("Keep this draft");
    expect(posts).toHaveLength(0);
  });
  it("shows the immutable request note to its recipient", async () => {
    data.requests.find((r) => r.definitionId === "security")!.note =
      "Focus on input admission";
    mount();
    const d = within(await openReview());
    expect(d.getByText("Focus on input admission")).toBeVisible();
  });
});
