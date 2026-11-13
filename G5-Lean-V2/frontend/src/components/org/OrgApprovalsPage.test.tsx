/**
 * OrgApprovalsPageContent tests — org approval-definition library (mock D).
 *
 * Scenario ownership: test_contract/org_approvals_management D5. Covered:
 *   1. Library renders cards with the "Everyone below must approve"
 *      checklist, composition badges, the "Agent check" row with its
 *      Show prompt toggle, v1 "Created by" foot, and no version chrome.
 *   2. Search filters by name/description with a live count; composition
 *      filter chips narrow the grid.
 *   3. A plain member sees the library read-only (no New/Edit affordances).
 *   4. Create flow: submit gating (name + ≥1 human-or-agent + paired
 *      prompt), successful publish returns to the library with the new card.
 *   5. Edit flow: prefilled state, keep-decisions default, invalidate
 *      choice, 409 conflict renders the inline notice.
 *   6. Empty state renders the create CTA.
 *   7. Departed members render flagged and are not selectable pills.
 */

import type { ReactElement } from "react";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { Route, Routes } from "react-router-dom";
import { beforeEach, describe, expect, it } from "vitest";

import { renderApp } from "../../test/render";
import { server } from "../../test/msw/server";
import {
  resetMockOrgApprovalDefinitions,
  seedMockOrgApprovalDefinition,
} from "../../test/msw/handlers";
import { createMockUser, MOCK_ADMIN_SCOPES } from "../../test/fixtures";
import type { OrgApprovalDefinitionDTO } from "../../lib/types";
import { OrgApprovalsPageContent } from "./OrgApprovalsPageContent";

const ORG_SLUG = "g5labs";
const ORG_ID = "org-g5labs";
const ROUTE = `/${ORG_SLUG}/settings/approvals`;

function makeDefinition(
  overrides: Partial<OrgApprovalDefinitionDTO> = {},
): OrgApprovalDefinitionDTO {
  return {
    id: "apprdef-fixed-1",
    latestVersion: 3,
    name: "Production merge sign-off",
    description: "Required for merges into the production base workspace.",
    humans: [
      {
        userId: "user-alex",
        displayName: "Alex Chen",
        email: "alex@g5labs.dev",
        avatarUrl: null,
        stillMember: true,
      },
      {
        userId: "user-jordan",
        displayName: "Jordan Rivera",
        email: "jordan@g5labs.dev",
        avatarUrl: null,
        stillMember: true,
      },
    ],
    agentType: null,
    agentPrompt: null,
    agentHarness: null,
    agentModel: null,
    agentEffort: null,
    invalidatesPrevious: false,
    createdBy: { userId: "user-alex", displayName: "Alex Chen" },
    createdAt: "2026-08-11T09:00:00.000Z",
    firstCreatedBy: { userId: "user-alex", displayName: "Alex Chen" },
    firstCreatedAt: "2026-07-28T09:00:00.000Z",
    ...overrides,
  };
}

function renderApprovalsPage(opts?: {
  role?: "admin" | "member" | "viewer";
  scopes?: string[];
}) {
  const ui: ReactElement = (
    <Routes>
      <Route
        path="/:org/settings/approvals"
        element={<OrgApprovalsPageContent />}
      />
    </Routes>
  );
  return renderApp(ui, {
    route: ROUTE,
    user: createMockUser({
      orgs: [
        {
          orgId: ORG_ID,
          orgName: "G5 Labs",
          orgSlug: ORG_SLUG,
          role: opts?.role ?? "admin",
        },
      ],
      scopes: opts?.scopes ?? MOCK_ADMIN_SCOPES,
    }),
  });
}

describe("OrgApprovalsPageContent", () => {
  beforeEach(() => {
    resetMockOrgApprovalDefinitions();
  });

  it("disables inactive approvers when creating a required approval", async () => {
    server.use(
      http.get("/api/v1/orgs/:orgId/members", () =>
        HttpResponse.json({
          data: [
            { userId: "user-alex", displayName: "Alex Chen", role: "admin" },
            {
              userId: "user-jordan",
              displayName: "Jordan Rivera",
              role: "member",
              deactivatedAt: "2026-10-20T12:00:00Z",
            },
          ],
        }),
      ),
    );
    const user = userEvent.setup();
    renderApprovalsPage();
    await user.click(await screen.findByTestId("org-approvals-new"));
    const inactive = await screen.findByTestId(
      "org-approvals-member-pill-user-jordan",
    );
    expect(inactive).toBeDisabled();
    expect(inactive).toHaveTextContent(/inactive/i);
    await user.type(
      screen.getByTestId("org-approvals-name-input"),
      "Required approval",
    );
    await user.click(inactive);
    expect(screen.getByTestId("org-approvals-submit")).toBeDisabled();
    await user.click(screen.getByTestId("org-approvals-member-pill-user-alex"));
    expect(screen.getByTestId("org-approvals-submit")).toBeEnabled();
  });

  it("renders library cards with the approver checklist, composition badges, v1 provenance, and no version chrome", async () => {
    seedMockOrgApprovalDefinition(ORG_ID, makeDefinition());
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        id: "apprdef-fixed-2",
        name: "Schema-change review",
        description: "Destructive DDL and lock safety.",
        humans: [
          {
            userId: "user-taylor",
            displayName: "Taylor Kim",
            email: "taylor@g5labs.dev",
            avatarUrl: null,
            stillMember: true,
          },
        ],
        agentType: "simple",
        agentPrompt: "Approve only if the diff contains no destructive DDL.",
        firstCreatedBy: { userId: "user-jordan", displayName: "Jordan Rivera" },
      }),
    );

    renderApprovalsPage();

    const cards = await screen.findAllByTestId("org-approvals-card");
    expect(cards).toHaveLength(2);

    const prod = cards.find((c) =>
      within(c).queryByText("Production merge sign-off"),
    )!;
    expect(prod).toBeDefined();
    // ALL-OF checklist: one caption over every approver row, no AND tokens.
    expect(
      within(prod).getByText("All reviewers must approve"),
    ).toBeInTheDocument();
    expect(within(prod).getByText("Alex Chen")).toBeInTheDocument();
    expect(within(prod).getByText("Jordan Rivera")).toBeInTheDocument();
    expect(within(prod).queryByText("AND")).not.toBeInTheDocument();
    // Composition badge.
    expect(within(prod).getByText("2 people")).toBeInTheDocument();
    // v1 provenance foot.
    expect(within(prod).getByText(/Created by Alex Chen/)).toBeInTheDocument();
    // No version chrome on cards (user decision): latestVersion never renders.
    expect(within(prod).queryByText(/v3/)).not.toBeInTheDocument();

    const schema = cards.find((c) =>
      within(c).queryByText("Schema-change review"),
    )!;
    expect(within(schema).getByText("Agent check")).toBeInTheDocument();
    expect(within(schema).getByText("1 person + agent")).toBeInTheDocument();
    expect(
      within(schema).getByText(/Created by Jordan Rivera/),
    ).toBeInTheDocument();
    // The prompt is collapsed behind the Show prompt toggle.
    const prompt = "Approve only if the diff contains no destructive DDL.";
    expect(within(schema).queryByText(prompt)).not.toBeInTheDocument();
    fireEvent.click(within(schema).getByText("Show prompt"));
    await waitFor(() => {
      expect(within(schema).getByText(prompt)).toBeInTheDocument();
    });
    fireEvent.click(within(schema).getByText("Hide prompt"));
    await waitFor(() => {
      expect(within(schema).queryByText(prompt)).not.toBeInTheDocument();
    });
  });

  it("filters cards by search and updates the count", async () => {
    seedMockOrgApprovalDefinition(ORG_ID, makeDefinition());
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({ id: "apprdef-fixed-2", name: "Schema-change review" }),
    );

    renderApprovalsPage();
    await screen.findAllByTestId("org-approvals-card");
    expect(screen.getByTestId("org-approvals-count")).toHaveTextContent(
      "2 definitions",
    );
    // No agent prompts anywhere: the show-all-prompts toggle is not rendered.
    expect(
      screen.queryByTestId("org-approvals-show-all-prompts"),
    ).not.toBeInTheDocument();

    fireEvent.change(screen.getByTestId("org-approvals-search"), {
      target: { value: "schema" },
    });
    await waitFor(() => {
      expect(screen.getAllByTestId("org-approvals-card")).toHaveLength(1);
    });
    expect(screen.getByTestId("org-approvals-count")).toHaveTextContent(
      "1 definition",
    );
    expect(screen.getByText("Schema-change review")).toBeInTheDocument();
  });

  it("filters cards by composition chips", async () => {
    seedMockOrgApprovalDefinition(ORG_ID, makeDefinition()); // 2 humans
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        id: "apprdef-fixed-2",
        name: "Schema-change review",
        agentType: "simple",
        agentPrompt: "Approve only if the diff contains no destructive DDL.",
      }),
    );
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        id: "apprdef-fixed-3",
        name: "Security review",
        humans: [],
        agentType: "simple",
        agentPrompt: "Approve only if no new vulnerabilities are introduced.",
      }),
    );

    renderApprovalsPage();
    await screen.findAllByTestId("org-approvals-card");
    expect(screen.getAllByTestId("org-approvals-card")).toHaveLength(3);

    fireEvent.click(screen.getByTestId("org-approvals-filter-agents-only"));
    await waitFor(() => {
      expect(screen.getAllByTestId("org-approvals-card")).toHaveLength(1);
    });
    expect(screen.getByText("Security review")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("org-approvals-filter-includes-agent"));
    await waitFor(() => {
      expect(screen.getAllByTestId("org-approvals-card")).toHaveLength(2);
    });

    fireEvent.click(screen.getByTestId("org-approvals-filter-people"));
    await waitFor(() => {
      expect(screen.getAllByTestId("org-approvals-card")).toHaveLength(1);
    });
    expect(screen.getByText("Production merge sign-off")).toBeInTheDocument();
    expect(screen.getByTestId("org-approvals-count")).toHaveTextContent(
      "1 definition",
    );

    fireEvent.click(screen.getByTestId("org-approvals-filter-all"));
    await waitFor(() => {
      expect(screen.getAllByTestId("org-approvals-card")).toHaveLength(3);
    });
  });

  it("show all prompts toggle expands and collapses every agent prompt at once", async () => {
    const ddlPrompt = "Approve only if the diff contains no destructive DDL.";
    const vulnPrompt = "Approve only if no new vulnerabilities are introduced.";
    seedMockOrgApprovalDefinition(ORG_ID, makeDefinition()); // humans only
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        id: "apprdef-fixed-2",
        name: "Schema-change review",
        agentType: "simple",
        agentPrompt: ddlPrompt,
      }),
    );
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        id: "apprdef-fixed-3",
        name: "Security review",
        humans: [],
        agentType: "simple",
        agentPrompt: vulnPrompt,
      }),
    );

    renderApprovalsPage();
    await screen.findAllByTestId("org-approvals-card");

    const toggle = screen.getByTestId("org-approvals-show-all-prompts");
    expect(toggle).toHaveTextContent("Show all prompts");
    expect(screen.queryByText(ddlPrompt)).not.toBeInTheDocument();
    expect(screen.queryByText(vulnPrompt)).not.toBeInTheDocument();

    fireEvent.click(toggle);
    await waitFor(() => {
      expect(screen.getByText(ddlPrompt)).toBeInTheDocument();
    });
    expect(screen.getByText(vulnPrompt)).toBeInTheDocument();
    expect(toggle).toHaveTextContent("Hide all prompts");

    fireEvent.click(toggle);
    await waitFor(() => {
      expect(screen.queryByText(ddlPrompt)).not.toBeInTheDocument();
    });
    expect(screen.queryByText(vulnPrompt)).not.toBeInTheDocument();
  });

  it("renders read-only for a plain member: no New definition, no Edit", async () => {
    seedMockOrgApprovalDefinition(ORG_ID, makeDefinition());

    renderApprovalsPage({ role: "member", scopes: ["workspaces:read"] });

    await screen.findAllByTestId("org-approvals-card");
    expect(screen.queryByTestId("org-approvals-new")).not.toBeInTheDocument();
    expect(screen.queryByTestId("org-approvals-edit")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("org-approvals-new-card"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText(/Read-only — publishing definitions requires/),
    ).toBeInTheDocument();
  });

  it("renders the empty state with a create CTA", async () => {
    renderApprovalsPage();
    expect(
      await screen.findByTestId("org-approvals-empty"),
    ).toBeInTheDocument();
    expect(screen.getByText("No approval definitions yet")).toBeInTheDocument();
  });

  it("creates with any-reviewer/self-off defaults and preserves explicit policy edits", async () => {
    const user = userEvent.setup();
    renderApprovalsPage();
    await user.click(await screen.findByTestId("org-approvals-new"));
    expect(
      screen.getByTestId("org-approvals-advanced-content"),
    ).not.toBeVisible();
    await user.click(screen.getByTestId("org-approvals-advanced-toggle"));
    expect(
      screen.getByLabelText("Human approvals may override agents"),
    ).toBeChecked();
    expect(screen.getByLabelText("Reviewer approval requirement")).toHaveValue(
      "any",
    );
    expect(screen.getByLabelText("Allow self approval")).not.toBeChecked();
    await user.type(
      screen.getByTestId("org-approvals-name-input"),
      "Quorum policy",
    );
    await user.click(screen.getByTestId("org-approvals-member-pill-user-alex"));
    await user.click(screen.getByTestId("org-approvals-submit"));
    const card = await screen.findByTestId("org-approvals-card");
    expect(card).toHaveTextContent("At least one reviewer must approve");
    expect(card).toHaveTextContent("Self approval disallowed");
    await user.click(within(card).getByTestId("org-approvals-edit"));
    expect(
      screen.getByTestId("org-approvals-advanced-content"),
    ).not.toBeVisible();
    await user.click(screen.getByTestId("org-approvals-advanced-toggle"));
    await user.click(
      screen.getByLabelText("Human approvals may override agents"),
    );
    await user.selectOptions(
      screen.getByLabelText("Reviewer approval requirement"),
      "all",
    );
    await user.click(screen.getByLabelText("Allow self approval"));
    await user.click(screen.getByTestId("org-approvals-advanced-toggle"));
    expect(
      screen.getByTestId("org-approvals-advanced-content"),
    ).not.toBeVisible();
    await user.click(screen.getByTestId("org-approvals-submit"));
    const edited = await screen.findByTestId("org-approvals-card");
    expect(edited).toHaveTextContent("All reviewers must approve");
    expect(edited).toHaveTextContent("Self approval allowed");
    await user.click(within(edited).getByTestId("org-approvals-edit"));
    await user.click(screen.getByTestId("org-approvals-advanced-toggle"));
    expect(
      screen.getByLabelText("Human approvals may override agents"),
    ).not.toBeChecked();
    expect(screen.getByLabelText("Reviewer approval requirement")).toHaveValue(
      "all",
    );
    expect(screen.getByLabelText("Allow self approval")).toBeChecked();
  });

  it("create flow: gates submit on composition rules, then publishes v1 and returns to the library", async () => {
    const user = userEvent.setup();
    renderApprovalsPage();
    await screen.findByTestId("org-approvals-empty");

    await user.click(screen.getByTestId("org-approvals-new"));
    const submit = screen.getByTestId("org-approvals-submit");
    // Empty form: disabled (no name).
    expect(submit).toBeDisabled();

    // Name alone is not enough — needs ≥1 human or an agent.
    await user.type(
      screen.getByTestId("org-approvals-name-input"),
      "Production merge sign-off",
    );
    expect(submit).toBeDisabled();

    // Agent without a prompt keeps it disabled (paired-prompt rule).
    await user.click(screen.getByTestId("org-approvals-agent-simple"));
    await waitFor(() => {
      expect(
        screen.getByTestId("org-approvals-agent-prompt"),
      ).toBeInTheDocument();
    });
    expect(submit).toBeDisabled();
    await user.click(screen.getByTestId("org-approvals-agent-none"));
    await waitFor(() => {
      expect(
        screen.queryByTestId("org-approvals-agent-prompt"),
      ).not.toBeInTheDocument();
    });

    // Selecting a human approver satisfies composition.
    await user.click(screen.getByTestId("org-approvals-member-pill-user-alex"));
    expect(submit).toBeEnabled();

    await user.click(submit);
    // Back on the library with the new card.
    const cards = await screen.findAllByTestId("org-approvals-card");
    expect(cards).toHaveLength(1);
    expect(
      within(cards[0]).getByText("Production merge sign-off"),
    ).toBeInTheDocument();
  });

  it("edit flow: prefills, defaults to keep-decisions, flags departed members, publishes the next version", async () => {
    const user = userEvent.setup();
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        humans: [
          {
            userId: "user-alex",
            displayName: "Alex Chen",
            email: "alex@g5labs.dev",
            avatarUrl: null,
            stillMember: true,
          },
          {
            userId: "user-gone",
            displayName: "Jonas Weiss",
            email: "",
            avatarUrl: null,
            stillMember: false,
          },
        ],
      }),
    );

    renderApprovalsPage();
    const card = (await screen.findAllByTestId("org-approvals-card"))[0];
    // Departed member is flagged on the card.
    expect(within(card).getByText("left org")).toBeInTheDocument();

    await user.click(screen.getByTestId("org-approvals-edit"));
    // Prefilled name; keep-decisions default; departed note; no pill for the
    // departed user (only current members are selectable).
    expect(screen.getByTestId("org-approvals-name-input")).toHaveValue(
      "Production merge sign-off",
    );
    expect(screen.getByTestId("org-approvals-keep-decisions")).toBeChecked();
    expect(screen.getByTestId("org-approvals-departed-note")).toHaveTextContent(
      /Jonas Weiss/,
    );
    expect(
      screen.queryByTestId("org-approvals-member-pill-user-gone"),
    ).not.toBeInTheDocument();

    await user.click(screen.getByTestId("org-approvals-invalidate-decisions"));
    await waitFor(() => {
      expect(
        screen.getByTestId("org-approvals-invalidate-decisions"),
      ).toBeChecked();
    });
    await user.click(screen.getByTestId("org-approvals-submit"));

    // Back on the library; the mock store bumped the version content.
    await screen.findAllByTestId("org-approvals-card");
  });

  it("keeps inactive members on the historical definition but excludes them from a newly published version", async () => {
    const definition = makeDefinition();
    definition.humans[1].deactivatedAt = "2026-10-20T12:00:00Z";
    seedMockOrgApprovalDefinition(ORG_ID, definition);
    let submitted: { humanUserIds: string[] } | undefined;
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/approval-definitions/:definitionId/versions",
        async ({ request }) => {
          submitted = (await request.json()) as { humanUserIds: string[] };
          return HttpResponse.json(
            { data: { ...definition, latestVersion: 4 } },
            { status: 201 },
          );
        },
      ),
    );
    renderApprovalsPage();
    const user = userEvent.setup();
    const card = (await screen.findAllByTestId("org-approvals-card"))[0];
    expect(within(card).getByText("Jordan Rivera")).toBeVisible();
    expect(within(card).getByText("inactive")).toBeVisible();
    await user.click(screen.getByTestId("org-approvals-edit"));
    expect(
      screen.getByText(/inactive approvers remain in history/),
    ).toBeVisible();
    await user.click(screen.getByTestId("org-approvals-submit"));
    await waitFor(() => expect(submitted?.humanUserIds).toEqual(["user-alex"]));
  });

  it("edit flow: a 409 conflict renders the inline notice and stays on the editor", async () => {
    const user = userEvent.setup();
    seedMockOrgApprovalDefinition(ORG_ID, makeDefinition());
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/approval-definitions/:definitionId/versions",
        () =>
          HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message: "published concurrently",
              },
            },
            { status: 409 },
          ),
      ),
    );

    renderApprovalsPage();
    await screen.findAllByTestId("org-approvals-card");
    await user.click(screen.getByTestId("org-approvals-edit"));
    await waitFor(() => {
      expect(
        screen.getByTestId("org-approvals-name-input"),
      ).toBeInTheDocument();
    });
    await user.click(screen.getByTestId("org-approvals-submit"));

    expect(
      await screen.findByTestId("org-approvals-conflict"),
    ).toHaveTextContent(/published a new version/);
    // Still on the editor, but Save is disabled: stale form state must not be
    // publishable over the winner's version.
    expect(screen.getByTestId("org-approvals-submit")).toBeDisabled();
    // The explicit reload action returns to the library.
    await user.click(screen.getByTestId("org-approvals-conflict-reload"));
    await screen.findAllByTestId("org-approvals-card");
  });

  // -------------------------------------------------------------------------
  // Earliest relevant phase (g5.workbench.gates)
  // -------------------------------------------------------------------------

  it("library cards show phase chips reflecting each definition's recommendation", async () => {
    seedMockOrgApprovalDefinition(ORG_ID, makeDefinition()); // IE default
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        id: "apprdef-be",
        name: "Business sign-off",
        phases: ["be", "se"],
      }),
    );
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({ id: "apprdef-inert", name: "Inert", phases: [] }),
    );
    renderApprovalsPage();

    const cards = await screen.findAllByTestId("org-approvals-card");
    const ieCard = cards.find((c) =>
      within(c).queryByText("Production merge sign-off"),
    )!;
    expect(
      within(ieCard).getByTestId("org-approvals-card-phase-ie"),
    ).toBeInTheDocument();
    expect(
      within(ieCard).queryByTestId("org-approvals-card-phase-be"),
    ).not.toBeInTheDocument();
    expect(
      within(ieCard).queryByTestId("org-approvals-card-phase-advisory"),
    ).not.toBeInTheDocument();

    const beCard = cards.find((c) =>
      within(c).queryByText("Business sign-off"),
    )!;
    expect(
      within(beCard).getByTestId("org-approvals-card-phase-be"),
    ).toBeInTheDocument();
    expect(
      within(beCard).getByTestId("org-approvals-card-phase-se"),
    ).toBeInTheDocument();
    expect(
      within(beCard).queryByTestId("org-approvals-card-phase-ie"),
    ).not.toBeInTheDocument();
    expect(
      within(beCard).getByTestId("org-approvals-card-phase-be"),
    ).toHaveTextContent(/Recommended for Business Engineering/);

    const inertCard = cards.find((c) => within(c).queryByText("Inert"))!;
    expect(
      within(inertCard).getByTestId("org-approvals-card-phase-none"),
    ).toHaveTextContent(/No phase recommendation/);
  });

  it("shows one earliest relevant phase dropdown outside Advanced, defaulting to IE", async () => {
    const user = userEvent.setup();
    renderApprovalsPage();
    await screen.findByTestId("org-approvals-empty");
    await user.click(screen.getByTestId("org-approvals-new"));
    const phase = screen.getByRole("combobox", {
      name: "Earliest relevant phase",
    });
    expect(phase).toBeVisible();
    expect(phase).toHaveValue("ie");
    expect(phase).not.toHaveAttribute("multiple");
    expect(within(phase).getAllByRole("option")).toHaveLength(3);
    expect(
      phase.closest('[data-testid="org-approvals-advanced-content"]'),
    ).toBeNull();
    await user.selectOptions(phase, "be");
    expect(phase).toHaveValue("be");
    await user.selectOptions(phase, "se");
    expect(phase).toHaveValue("se");
  });

  it.each([
    { phases: ["se", "be"] as ("be" | "se" | "ie")[], expected: "be" },
    { phases: [] as ("be" | "se" | "ie")[], expected: "ie" },
    { phases: ["se"] as ("be" | "se" | "ie")[], expected: "se" },
  ])(
    "hydrates the earliest legacy phase and saves only it: $phases",
    async ({ phases, expected }) => {
      const user = userEvent.setup();
      seedMockOrgApprovalDefinition(ORG_ID, makeDefinition({ phases }));
      const captured: Array<Record<string, unknown>> = [];
      server.use(
        http.post(
          "/api/v1/orgs/:orgId/approval-definitions/:definitionId/versions",
          async ({ request }) => {
            const body = (await request.json()) as Record<string, unknown>;
            captured.push(body);
            return HttpResponse.json({
              data: makeDefinition({
                phases: [expected as "be" | "se" | "ie"],
                latestVersion: 2,
              }),
            });
          },
        ),
      );
      renderApprovalsPage();
      await screen.findAllByTestId("org-approvals-card");
      await user.click(screen.getByTestId("org-approvals-edit"));
      expect(
        screen.getByRole("combobox", { name: "Earliest relevant phase" }),
      ).toHaveValue(expected);
      await user.click(screen.getByTestId("org-approvals-submit"));
      await screen.findAllByTestId("org-approvals-card");
      expect(captured).toHaveLength(1);
      expect(captured[0]).toMatchObject({ phases: [expected] });
    },
  );

  it("submit carries only the selected phase and the created card shows its recommendation", async () => {
    const user = userEvent.setup();
    const captured: Array<Record<string, unknown>> = [];
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/approval-definitions",
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          captured.push(body);
          const def = makeDefinition({
            id: "apprdef-phased",
            name: String(body.name),
            phases: body.phases as ("be" | "se" | "ie")[],
          });
          seedMockOrgApprovalDefinition(ORG_ID, def);
          return HttpResponse.json({ data: def }, { status: 201 });
        },
      ),
    );

    renderApprovalsPage();
    await screen.findByTestId("org-approvals-empty");
    await user.click(screen.getByTestId("org-approvals-new"));
    await user.type(
      screen.getByTestId("org-approvals-name-input"),
      "Business sign-off",
    );
    await user.click(screen.getByTestId("org-approvals-member-pill-user-alex"));
    await user.selectOptions(
      screen.getByRole("combobox", { name: "Earliest relevant phase" }),
      "be",
    );
    await user.click(screen.getByTestId("org-approvals-submit"));

    const cards = await screen.findAllByTestId("org-approvals-card");
    const created = cards.find((c) =>
      within(c).queryByText("Business sign-off"),
    )!;
    expect(
      within(created).getByTestId("org-approvals-card-phase-be"),
    ).toBeInTheDocument();
    expect(
      within(created).queryByTestId("org-approvals-card-phase-ie"),
    ).not.toBeInTheDocument();

    expect(captured).toHaveLength(1);
    expect(captured[0]).toMatchObject({ phases: ["be"] });
  });

  it("keeps shared launch controls mounted while Advanced is collapsed and submits exact defaults", async () => {
    const user = userEvent.setup();
    const captured: Array<Record<string, unknown>> = [];
    server.use(
      http.post(
        "/api/v1/orgs/:orgId/approval-definitions",
        async ({ request }) => {
          const body = (await request.json()) as Record<string, unknown>;
          captured.push(body);
          const def = makeDefinition({
            id: "apprdef-agent-default",
            name: String(body.name),
            humans: [],
            agentType: "simple",
            agentPrompt: String(body.agentPrompt),
            agentHarness: body.agentHarness as "claude_code",
            agentModel: String(body.agentModel),
            agentEffort: String(body.agentEffort),
          });
          seedMockOrgApprovalDefinition(ORG_ID, def);
          return HttpResponse.json({ data: def }, { status: 201 });
        },
      ),
    );

    renderApprovalsPage();
    await screen.findByTestId("org-approvals-empty");
    await user.click(screen.getByTestId("org-approvals-new"));
    await user.type(
      screen.getByTestId("org-approvals-name-input"),
      "Agent review",
    );
    await user.click(screen.getByTestId("org-approvals-agent-simple"));
    await user.type(
      screen.getByTestId("org-approvals-agent-prompt"),
      "Approve only when every required invariant holds.",
    );

    expect(
      screen.getByTestId("org-approvals-advanced-content"),
    ).not.toBeVisible();
    expect(
      screen.getByTestId("org-approvals-launch-agent-select"),
    ).toHaveTextContent("Claude Code");
    expect(
      screen.getByTestId("org-approvals-launch-model-select"),
    ).toHaveTextContent("Opus 5.5");
    expect(
      screen.getByTestId("org-approvals-launch-effort-select"),
    ).toHaveTextContent("High");
    expect(screen.getByTestId("org-approvals-launch-mode-display")).toHaveValue(
      "Bypass",
    );
    expect(
      screen.getByTestId("org-approvals-launch-mode-display"),
    ).toHaveAttribute("readonly");

    await user.click(screen.getByTestId("org-approvals-submit"));
    await screen.findAllByTestId("org-approvals-card");
    expect(captured).toHaveLength(1);
    expect(captured[0]).toMatchObject({
      agentType: "simple",
      agentHarness: "claude_code",
      agentModel: "claude-opus-5-5",
      agentEffort: "high",
    });
    expect(captured[0]).not.toHaveProperty("agentMode");
    expect(captured[0]).not.toHaveProperty("permissionMode");
  });

  it("hydrates an existing definition's exact launch and derives Codex Mode as read-only", async () => {
    const user = userEvent.setup();
    seedMockOrgApprovalDefinition(
      ORG_ID,
      makeDefinition({
        humans: [],
        agentType: "simple",
        agentPrompt: "Review the candidate.",
        agentHarness: "codex",
        agentModel: "gpt-5.6-sol",
        agentEffort: "ultra",
      }),
    );

    renderApprovalsPage();
    await screen.findAllByTestId("org-approvals-card");
    await user.click(screen.getByTestId("org-approvals-edit"));
    await user.click(screen.getByTestId("org-approvals-advanced-toggle"));

    expect(
      screen.getByTestId("org-approvals-launch-agent-select"),
    ).toHaveTextContent("Codex");
    expect(
      screen.getByTestId("org-approvals-launch-model-select"),
    ).toHaveTextContent("GPT 5.6 Sol");
    expect(
      screen.getByTestId("org-approvals-launch-effort-select"),
    ).toHaveTextContent("Ultra");
    expect(screen.getByTestId("org-approvals-launch-mode-display")).toHaveValue(
      "Full Access",
    );
    expect(
      screen.getByTestId("org-approvals-launch-mode-display"),
    ).toHaveAttribute("readonly");
    expect(
      screen.getByTestId("org-approvals-advanced-badge"),
    ).toHaveTextContent("4 changed");
  });
});
