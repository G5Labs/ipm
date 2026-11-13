// =============================================================================
// PlatformHome — operator console tests
// =============================================================================
// The operator organization's Home (g5.organization.platform-operator.home):
// the organizations list for every operator role, and the create form for
// members and above (g5.organization.platform-operator.create-org).
// =============================================================================

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { PlatformHome } from "./PlatformHome";

function renderConsole(role: string) {
  return renderApp(<PlatformHome orgSlug="operator" role={role} />, {
    route: "/operator/home",
  });
}

describe("PlatformHome", () => {
  it("lists tenant organizations as text, without links into them", async () => {
    renderConsole("viewer");

    expect(
      screen.getByRole("heading", { name: "Platform operator" }),
    ).toBeVisible();
    const rows = await screen.findAllByTestId("platform-org-row");
    expect(rows.map((row) => row.getAttribute("data-org-slug"))).toEqual([
      "acme",
      "g5labs",
    ]);
    expect(within(rows[0]).getByText("Acme Corp")).toBeVisible();
    expect(within(rows[0]).getByText("acme")).toBeVisible();
    expect(within(rows[0]).getByText("4")).toBeVisible();
    // Operators are not tenant members (tenant-boundary).
    expect(within(rows[0]).queryByRole("link")).toBeNull();
    expect(screen.getByTestId("platform-orgs-count")).toHaveTextContent("2");
  });

  it("shows the list only to viewers", async () => {
    renderConsole("viewer");

    await screen.findAllByTestId("platform-org-row");
    expect(screen.queryByTestId("platform-create-org")).toBeNull();
    expect(screen.queryByTestId("platform-manage-operators")).toBeNull();
  });

  it.each(["member", "admin"])(
    "offers the create form to an operator %s",
    async (role) => {
      renderConsole(role);

      expect(
        await screen.findByTestId("platform-create-org-name"),
      ).toBeVisible();
      expect(
        screen.getByTestId("platform-create-org-admin-email"),
      ).toBeVisible();
    },
  );

  it.each([
    ["member", false],
    ["admin", true],
  ])("links an operator %s to managing operators: %s", async (role, shown) => {
    renderConsole(role);

    await screen.findAllByTestId("platform-org-row");
    const link = screen.queryByTestId("platform-manage-operators");
    if (shown) {
      expect(link).toHaveAttribute("href", "/operator/base?view=members");
    } else {
      expect(link).toBeNull();
    }
  });

  it("derives the slug from the name until it is edited", async () => {
    const user = userEvent.setup();
    renderConsole("member");

    const name = await screen.findByTestId("platform-create-org-name");
    const slug = screen.getByTestId("platform-create-org-slug");
    await user.type(name, "Acme Robotics!");
    expect(slug).toHaveValue("acme-robotics");

    await user.clear(slug);
    await user.type(slug, "robots");
    await user.type(name, " Inc");
    expect(slug).toHaveValue("robots");
  });

  it("creates an organization for an Admin, reports a new account and refreshes the list", async () => {
    const user = userEvent.setup();
    const bodies: unknown[] = [];
    let created = false;
    server.use(
      http.get("/api/v1/platform/orgs", () =>
        HttpResponse.json({
          data: [
            {
              id: "org-acme",
              name: "Acme Corp",
              slug: "acme",
              createdAt: "2026-01-02T00:00:00Z",
              memberCount: 4,
            },
            ...(created
              ? [
                  {
                    id: "org-new",
                    name: "Globex",
                    slug: "globex",
                    createdAt: "2026-10-24T00:00:00Z",
                    memberCount: 1,
                  },
                ]
              : []),
          ],
        }),
      ),
      http.post("/api/v1/platform/orgs", async ({ request }) => {
        bodies.push(await request.json());
        created = true;
        return HttpResponse.json(
          {
            data: {
              org: {
                id: "org-new",
                name: "Globex",
                slug: "globex",
                createdAt: "2026-10-24T00:00:00Z",
              },
              admin: {
                userId: "user-hank",
                email: "hank@globex.example",
                accountCreated: true,
              },
            },
          },
          { status: 201 },
        );
      }),
    );
    renderConsole("member");

    expect(await screen.findAllByTestId("platform-org-row")).toHaveLength(1);
    await user.type(screen.getByTestId("platform-create-org-name"), "Globex");
    await user.type(
      screen.getByTestId("platform-create-org-admin-email"),
      " Hank@Globex.example ",
    );
    await user.click(screen.getByTestId("platform-create-org-submit"));

    const result = await screen.findByTestId("platform-create-org-result");
    expect(result).toHaveTextContent("Globex");
    expect(result).toHaveTextContent("globex");
    expect(screen.getByTestId("platform-create-org-admin")).toHaveTextContent(
      "hank@globex.example: new account — they sign in with this email.",
    );
    expect(bodies).toEqual([
      {
        name: "Globex",
        slug: "globex",
        adminEmail: "Hank@Globex.example",
      },
    ]);
    await waitFor(() =>
      expect(screen.getAllByTestId("platform-org-row")).toHaveLength(2),
    );
    // The form is ready for the next organization.
    expect(screen.getByTestId("platform-create-org-name")).toHaveValue("");
  });

  it("reports an existing administrator account", async () => {
    const user = userEvent.setup();
    server.use(
      http.post("/api/v1/platform/orgs", () =>
        HttpResponse.json(
          {
            data: {
              org: {
                id: "org-new",
                name: "Initech",
                slug: "initech",
                createdAt: "2026-10-24T00:00:00Z",
              },
              admin: {
                userId: "user-bill",
                email: "bill@initech.example",
                accountCreated: false,
              },
            },
          },
          { status: 201 },
        ),
      ),
    );
    renderConsole("admin");

    await user.type(
      await screen.findByTestId("platform-create-org-name"),
      "Initech",
    );
    await user.type(
      screen.getByTestId("platform-create-org-admin-email"),
      "bill@initech.example",
    );
    await user.click(screen.getByTestId("platform-create-org-submit"));

    expect(
      await screen.findByTestId("platform-create-org-admin"),
    ).toHaveTextContent("bill@initech.example: existing account.");
  });

  it.each([
    [
      409,
      "CONFLICT",
      "Slug taken",
      "An organization with the slug “acme” already exists.",
    ],
    [
      422,
      "VALIDATION_ERROR",
      "The slug 'acme' is reserved",
      "The slug 'acme' is reserved",
    ],
    [
      403,
      "FORBIDDEN",
      "Forbidden",
      "Your operator role cannot create organizations.",
    ],
  ])("shows a %s refusal inline", async (status, code, message, expected) => {
    const user = userEvent.setup();
    server.use(
      http.post("/api/v1/platform/orgs", () =>
        HttpResponse.json(
          { error: { code, message, requestId: "req-platform" } },
          { status },
        ),
      ),
    );
    renderConsole("member");

    await user.type(
      await screen.findByTestId("platform-create-org-name"),
      "Acme",
    );
    await user.type(
      screen.getByTestId("platform-create-org-admin-email"),
      "owner@acme.example",
    );
    await user.click(screen.getByTestId("platform-create-org-submit"));

    expect(
      await screen.findByTestId("platform-create-org-error"),
    ).toHaveTextContent(expected);
    expect(screen.queryByTestId("platform-create-org-result")).toBeNull();
    // The entered values stay for correction.
    expect(screen.getByTestId("platform-create-org-name")).toHaveValue("Acme");
  });

  it("explains a refused list", async () => {
    server.use(
      http.get("/api/v1/platform/orgs", () =>
        HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "Forbidden",
              requestId: "req-platform",
            },
          },
          { status: 403 },
        ),
      ),
    );
    renderConsole("viewer");

    expect(await screen.findByTestId("platform-orgs-error")).toHaveTextContent(
      "Only active platform operators can list organizations.",
    );
  });
});
