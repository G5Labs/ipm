import { beforeEach, it, expect, vi } from "vitest";
import { screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import type {
  ApprovalRecipientNeedsDTO,
  ApprovalRequestPolicyDTO,
  WorkspaceOutgoingRequirementDTO,
  OrgApprovalDefinitionDTO,
} from "../../lib/types";
import {
  ApprovalRequestComposer,
  type ApprovalRequestPrefill,
} from "./ApprovalRequestComposer";

const human = (id: string) => ({
  userId: id,
  displayName: id,
  stillMember: true,
});
const none: ApprovalRecipientNeedsDTO = {
  agent: false,
  humans: [],
  randomHumanFrom: [],
};
const requirement = (
  id: string,
  all = false,
  outstanding: Partial<ApprovalRecipientNeedsDTO> = { agent: true },
): WorkspaceOutgoingRequirementDTO => ({
  definition: {
    id,
    name: id,
    latestVersion: 1,
    humans: [human("ben"), human("rahul")],
    agentType: "simple",
    requireAllReviewers: all,
  } as OrgApprovalDefinitionDTO,
  callerCanDecide: false,
  state: "pending",
  approvers: [],
  decisionLog: [],
  requests: [],
  requestNeeds: {
    outstanding: { ...none, ...outstanding },
    repeat: { ...none, agent: true },
    agentRequestable: true,
    agentApproved: false,
  },
});
let posts: Record<string, unknown>[], failAgent: boolean, hasPr: boolean;
beforeEach(() => {
  sessionStorage.clear();
  posts = [];
  failAgent = false;
  hasPr = true;
  server.use(
    http.get("/api/v1/workspaces/ws/review-flow", () =>
      HttpResponse.json({ data: { pr: hasPr ? { id: "pr" } : null } }),
    ),
    http.post(
      "/api/v1/workspaces/ws/approvals/requests",
      async ({ request }) => {
        posts.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json({ data: { requests: [{ reused: false }] } });
      },
    ),
    http.post(
      "/api/v1/workspaces/ws/approvals/reviews",
      async ({ request }) => {
        posts.push((await request.json()) as Record<string, unknown>);
        return failAgent
          ? HttpResponse.json(
              { error: { message: "Lost acknowledgement" } },
              { status: 503 },
            )
          : HttpResponse.json({ data: { reused: false } });
      },
    ),
  );
});
async function open(
  requirements: WorkspaceOutgoingRequirementDTO[],
  opts: {
    policy?: ApprovalRequestPolicyDTO;
    prefill?: ApprovalRequestPrefill;
  } = {},
) {
  const user = userEvent.setup();
  const app = renderApp(
    <ApprovalRequestComposer
      workspaceId="ws"
      ownerId="owner"
      requirements={requirements}
      requestPolicy={opts.policy}
      disabled={false}
      prefill={opts.prefill ?? null}
    />,
  );
  if (!opts.prefill) {
    const button = await screen.findByRole("button", { name: /Request ▾/ });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
  }
  const dialog = await screen.findByRole("dialog");
  return { user, app, dialog, d: within(dialog) };
}
const pairs = (d: ReturnType<typeof within>) =>
  d
    .queryAllByTestId("approval-request-pair")
    .map((p: HTMLElement) => p.textContent);
const include = "Include satisfied approval requirements";
const modeTrigger = (d: ReturnType<typeof within>) =>
  d.getByTestId("approval-request-mode");
async function chooseMode(
  user: ReturnType<typeof userEvent.setup>,
  d: ReturnType<typeof within>,
  mode: "all" | "agents" | "pick",
) {
  await user.click(modeTrigger(d));
  await user.click(d.getByTestId(`approval-request-mode-option-${mode}`));
}

it("opens fresh with no mode, no Selected approvals and Review request disabled", async () => {
  const { user, d } = await open([requirement("UI")]);
  expect(d.getByRole("heading", { name: "Request approvals" })).toBeVisible();
  // A visible label and unselected placeholder prompt a choice; the menu
  // still lists exactly the three existing modes.
  expect(d.getByText("Request mode", { exact: true })).toBeVisible();
  const trigger = modeTrigger(d);
  expect(d.getByRole("button", { name: "Request mode" })).toBe(trigger);
  expect(trigger).toHaveAttribute("aria-haspopup", "listbox");
  expect(trigger).toHaveTextContent(/^Select▾$/);
  expect(d.queryByTestId("approval-request-selected")).toBeNull();
  expect(d.getByRole("checkbox", { name: include })).toBeEnabled();
  expect(d.getByRole("button", { name: "Review request" })).toBeDisabled();
  await user.click(trigger);
  expect(
    within(d.getByRole("listbox"))
      .getAllByRole("option")
      .map((option) => option.textContent),
  ).toEqual([
    "Request all",
    "Request agents only",
    "Pick approvals and approvers",
  ]);
  // Escape closes only the menu, not the dialog.
  await user.keyboard("{Escape}");
  expect(d.queryByRole("listbox")).toBeNull();
  expect(screen.getByRole("dialog")).toBeVisible();
  // The keyboard chooses a mode as with the native select it replaces, and
  // the open list carries the control's name.
  trigger.focus();
  await user.keyboard("{ArrowDown}");
  const list = d.getByRole("listbox", { name: "Request mode" });
  expect(
    within(list).getByRole("option", { name: "Request all" }),
  ).toHaveFocus();
  await user.keyboard("{ArrowDown}{Enter}");
  expect(d.queryByRole("listbox")).toBeNull();
  expect(trigger).toHaveTextContent("Request agents only");
  expect(trigger).toHaveFocus();
  // The mode explanation opens over the dialog and closes with ×, leaving
  // the request dialog open.
  await user.click(d.getByRole("button", { name: "About request options" }));
  const options = screen.getByRole("dialog", { name: "Request options" });
  const bullets = within(options)
    .getAllByRole("listitem")
    .map((item) => item.textContent?.replace(/\s+/g, " ").trim());
  expect(bullets.slice(0, 4)).toEqual([
    "Approved requirements are satisfied and skipped unless Include satisfied approval requirements is checked.",
    "Request all selects each requirement needing a request and its needed reviewers. For any-of, it starts with the configured agent; when a human is needed, one configured human is chosen at random.",
    "Request agents only selects configured agents for requirements needing requests.",
    "Pick approvals and approvers lets you choose them directly, including satisfied requirements.",
  ]);
  // Exactly the four named phrases are bold.
  expect(
    Array.from(options.querySelectorAll("strong")).map((b) => b.textContent),
  ).toEqual([
    include,
    "Request all",
    "Request agents only",
    "Pick approvals and approvers",
  ]);
  await user.click(within(options).getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog", { name: "Request options" })).toBeNull();
  // × replaces the old Close text button.
  const close = d.getByRole("button", { name: "Close" });
  expect(close).not.toHaveTextContent("Close");
  await user.click(close);
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("closes on an outside click", async () => {
  const { user } = await open([requirement("UI")]);
  await user.click(screen.getByTestId("approval-request-dialog-overlay"));
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("Request all uses the server's needs, skips approved requirements unless included and resolves one random human", async () => {
  const random = vi
    .spyOn(crypto, "getRandomValues")
    .mockImplementation((value) => {
      if (value instanceof Uint32Array) value[0] = 2 ** 31; // second of two
      return value;
    });
  try {
    const done = requirement("Done");
    done.state = "approved";
    const humans = requirement("Humans", false, {
      agent: false,
      randomHumanFrom: ["ben", "rahul"],
    });
    const rejected = requirement("Rejected", false, {
      agent: false,
      humans: ["ben"],
    });
    rejected.decisionLog = [
      {
        requestId: "r",
        actorType: "human",
        actor: { userId: "ben", displayName: "ben" },
        outcome: "rejected",
        comment: null,
        definitionVersion: 1,
        superseded: false,
        reviewedGitHeads: null,
        createdAt: "2026-10-30T00:00:00Z",
      },
    ];
    const covered = requirement("Covered", false, {});
    covered.requestNeeds!.outstanding = none;
    const { user, d } = await open([done, humans, rejected, covered]);
    await chooseMode(user, d, "all");
    expect(pairs(d)).toEqual(["Humans → rahul", "Rejected → ben"]);
    await user.click(d.getByLabelText("Why these selections"));
    // The explanation opens in its own popup over the request dialog.
    const why = screen.getByRole("dialog", { name: "Selected approvals" });
    expect(
      within(why).getByText(
        "Humans: rahul was chosen at random from configured humans because a human review is needed.",
      ),
    ).toBeVisible();
    expect(
      within(why).getByText(
        "Rejected: ben previously rejected and must be requested again.",
      ),
    ).toBeVisible();
    expect(within(why).getByText("Done: already approved")).toBeVisible();
    expect(
      within(why).getByText(
        "Covered: no new reviewer needed or an active request is pending",
      ),
    ).toBeVisible();
    // Escape dismisses only the explanation; the request dialog stays.
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Selected approvals" }),
    ).toBeNull();
    expect(screen.getByTestId("approval-request-dialog")).toBeVisible();
    // Selected approvals carries no overall status badge.
    expect(
      within(d.getByTestId("approval-request-selected")).queryByText(
        /^(Rejected|Pending|Approved)$/,
      ),
    ).toBeNull();
    await user.click(d.getByRole("checkbox", { name: include }));
    expect(pairs(d)).toEqual([
      "Done → Done agent",
      "Humans → rahul",
      "Rejected → ben",
    ]);
    // Reselecting the current mode keeps the fixed selection; it does not
    // draw another random human.
    const draws = random.mock.calls.length;
    await chooseMode(user, d, "all");
    expect(random.mock.calls.length).toBe(draws);
    expect(pairs(d)).toEqual([
      "Done → Done agent",
      "Humans → rahul",
      "Rejected → ben",
    ]);
  } finally {
    random.mockRestore();
  }
});

it("Request agents only selects requestable agents not already approved", async () => {
  const humanOnly = requirement("Humans");
  humanOnly.definition.agentType = null;
  const approvedAgent = requirement("Vetoed");
  approvedAgent.requestNeeds!.agentApproved = true;
  const waiting = requirement("Waiting");
  waiting.requestNeeds!.agentRequestable = false;
  const { user, d } = await open([
    humanOnly,
    approvedAgent,
    waiting,
    requirement("New"),
  ]);
  await chooseMode(user, d, "agents");
  expect(pairs(d)).toEqual(["New → New agent"]);
  await user.click(d.getByLabelText("Why these selections"));
  expect(
    within(
      screen.getByRole("dialog", { name: "Selected approvals" }),
    ).getByText("Humans: no agent configured"),
  ).toBeVisible();
});

it("Pick shows Selected approvals only after a checkbox and disables the satisfied option", async () => {
  const satisfied = requirement("Satisfied");
  satisfied.state = "approved";
  const busy = requirement("Busy");
  busy.requests = [
    { actorType: "agent", state: "pending", agentReviews: [] },
  ] as unknown as NonNullable<WorkspaceOutgoingRequirementDTO["requests"]>;
  const { user, d } = await open([satisfied, busy]);
  await chooseMode(user, d, "pick");
  expect(d.getByRole("checkbox", { name: include })).toBeDisabled();
  expect(d.queryByTestId("approval-request-selected")).toBeNull();
  const busyGroup = within(d.getByRole("group", { name: "Busy" }));
  // An active request is never duplicated.
  expect(
    busyGroup.getByRole("checkbox", { name: /Busy agent/ }),
  ).toBeDisabled();
  const group = within(d.getByRole("group", { name: "Satisfied" }));
  await user.click(group.getByRole("checkbox", { name: "ben" }));
  await user.click(group.getByRole("checkbox", { name: "Satisfied agent" }));
  expect(pairs(d)).toEqual(["Satisfied → Satisfied agent", "Satisfied → ben"]);
  await user.click(group.getByRole("checkbox", { name: "ben" }));
  await user.click(group.getByRole("checkbox", { name: "Satisfied agent" }));
  expect(d.queryByTestId("approval-request-selected")).toBeNull();
});

it("keeps the human note across mode changes and sends it only to humans; success closes the dialog", async () => {
  const { user, d } = await open([
    requirement("All", true, { agent: true, humans: ["ben"] }),
  ]);
  await user.type(
    d.getByRole("textbox", { name: "Note for reviewers (optional)" }),
    "Please check the docs",
  );
  await chooseMode(user, d, "pick");
  await chooseMode(user, d, "all");
  expect(
    d.getByRole("textbox", { name: "Note for reviewers (optional)" }),
  ).toHaveValue("Please check the docs");
  expect(pairs(d)).toEqual(["All → All agent", "All → ben"]);
  await user.click(d.getByRole("button", { name: "Review request" }));
  expect(d.getByRole("heading", { name: "Review request" })).toBeVisible();
  expect(d.getByText(/Selected 2 recipient/)).toBeVisible();
  expect(posts).toHaveLength(0);
  await user.click(d.getByRole("button", { name: "Submit" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(posts).toHaveLength(2);
  const humanPost = posts.find((p) => p.actorUserIds)!;
  const agentPost = posts.find((p) => !p.actorUserIds)!;
  expect(humanPost).toMatchObject({
    actorUserIds: ["ben"],
    note: "Please check the docs",
  });
  expect(agentPost.note).toBeUndefined();
  expect(screen.queryByText("Accepted")).toBeNull();
  expect(sessionStorage.length).toBe(0);
});

it("shows failed recipients with an actionable retry that keeps stable operations after remount", async () => {
  failAgent = true;
  const { user, app, d } = await open([
    requirement("All", true, { agent: true, humans: ["ben", "rahul"] }),
  ]);
  await chooseMode(user, d, "all");
  await user.click(d.getByRole("button", { name: "Review request" }));
  await user.click(d.getByRole("button", { name: "Submit" }));
  await waitFor(() => expect(posts).toHaveLength(3));
  const alert = await d.findByRole("alert");
  expect(alert).toHaveTextContent(/Failed or unconfirmed/);
  expect(alert.closest("li")).toHaveTextContent("All → All agent");
  expect(d.getAllByText("Request sent")).toHaveLength(2);
  expect(d.queryByText("Accepted")).toBeNull();
  const retry = d.getByRole("button", { name: "Retry failed requests" });
  expect(retry).toBeEnabled();
  const agent = posts.find((p) => !p.actorUserIds)!;
  app.unmount();
  failAgent = false;
  const next = await open([requirement("All", true)]);
  await next.user.dblClick(
    next.d.getByRole("button", { name: "Retry failed requests" }),
  );
  await waitFor(() => expect(posts).toHaveLength(4));
  expect(posts[3]).toEqual(agent);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it("a fulfilled recipient of an unfinished batch is requested again under a new operation", async () => {
  failAgent = true;
  const first = await open([
    requirement("All", true, { agent: true, humans: ["ben"] }),
  ]);
  await chooseMode(first.user, first.d, "all");
  await first.user.click(
    first.d.getByRole("button", { name: "Review request" }),
  );
  await first.user.click(first.d.getByRole("button", { name: "Submit" }));
  await waitFor(() => expect(posts).toHaveLength(2));
  await first.d.findByRole("alert");
  const sent = posts.find((p) => p.actorUserIds)!;
  first.app.unmount();
  failAgent = false;
  // Request again for the fulfilled reviewer while the agent still failed.
  const again = await open([requirement("All", true)], {
    prefill: { token: 2, picks: [{ definitionId: "All", recipientId: "ben" }] },
  });
  await again.user.click(
    again.d.getByRole("button", { name: "Review request" }),
  );
  await again.user.click(again.d.getByRole("button", { name: "Submit" }));
  await waitFor(() => expect(posts).toHaveLength(3));
  expect(posts[2]).toMatchObject({ actorUserIds: ["ben"] });
  expect(posts[2].clientRequestId).toBeTruthy();
  expect(posts[2].clientRequestId).not.toEqual(sent.clientRequestId);
});

it("a prefilled entry opens Pick with exactly its recipients checked", async () => {
  const { user, d } = await open([requirement("Docs"), requirement("UI")], {
    prefill: {
      token: 1,
      picks: [{ definitionId: "UI", recipientId: "rahul" }],
    },
  });
  expect(modeTrigger(d)).toHaveTextContent("Pick approvals and approvers");
  expect(
    d.getByRole("button", {
      name: "Request mode Pick approvals and approvers",
    }),
  ).toBe(modeTrigger(d));
  const checked = d
    .getAllByRole("checkbox")
    .filter((box) => (box as HTMLInputElement).checked);
  expect(checked).toHaveLength(1);
  expect(
    within(d.getByRole("group", { name: "UI" })).getByRole("checkbox", {
      name: "rahul",
    }),
  ).toBeChecked();
  expect(pairs(d)).toEqual(["UI → rahul"]);
  await user.click(d.getByRole("button", { name: "Review request" }));
  await user.click(d.getByRole("button", { name: "Submit" }));
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0]).toMatchObject({
    definitionId: "UI",
    actorUserIds: ["rahul"],
  });
});

it.each([
  [false, "human"],
  [false, "agent"],
  [true, "human"],
  [true, "agent"],
] as const)(
  "applies the request PR rule (prRequired=%s) to %s requests alike without a PR",
  async (prRequired, kind) => {
    hasPr = false;
    const r = requirement("UI");
    const { d } = await open([r], {
      policy: { prRequired, onSourceCommitVectorChange: "IGNORE" },
      prefill: {
        token: 1,
        picks: [
          {
            definitionId: "UI",
            recipientId: kind === "agent" ? "agent" : "ben",
          },
        ],
      },
    });
    const group = within(d.getByRole("group", { name: "UI" }));
    const box = group.getByRole("checkbox", {
      name: kind === "agent" ? "UI agent" : "ben",
    });
    if (prRequired) {
      expect(
        await d.findByText("Approval requests require a submitted, open PR."),
      ).toBeVisible();
      expect(box).toBeDisabled();
      expect(d.getByRole("button", { name: "Review request" })).toBeDisabled();
    } else {
      expect(
        d.queryByText("Approval requests require a submitted, open PR."),
      ).toBeNull();
      expect(box).toBeChecked();
      expect(d.getByRole("button", { name: "Review request" })).toBeEnabled();
    }
  },
);

it("random choice excludes agents and is absent when no human choices exist", async () => {
  const random = vi
    .spyOn(crypto, "getRandomValues")
    .mockImplementation((value) => {
      if (value instanceof Uint32Array) value[0] = 0;
      return value;
    });
  try {
    const { user, d, app } = await open([requirement("Mixed")]);
    await chooseMode(user, d, "pick");
    await user.click(
      d.getByRole("button", { name: "Choose human reviewer at random" }),
    );
    expect(d.getByRole("checkbox", { name: "ben" })).toBeChecked();
    expect(d.getByRole("checkbox", { name: "Mixed agent" })).not.toBeChecked();
    app.unmount();
    const only = requirement("Agent only");
    only.definition.humans = [];
    const next = await open([only]);
    await chooseMode(next.user, next.d, "pick");
    expect(
      next.d.queryByRole("button", { name: "Choose human reviewer at random" }),
    ).toBeNull();
  } finally {
    random.mockRestore();
  }
});
