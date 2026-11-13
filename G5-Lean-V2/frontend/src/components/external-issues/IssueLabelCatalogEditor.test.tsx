/**
 * IssueLabelCatalogEditor — one domain's label catalog
 * (g5.issue.labels.surfaces, g5.issue.labels.contract).
 *
 * Covered: the catalog listed with colours and descriptions; create with
 * name, colour (with or without `#`) and description, refreshed after; a
 * name another label has refused with that label shown; an inline edit
 * sending only changed fields; delete only after a confirmation that says
 * the label leaves every issue; GitHub's unknown outcome asking for a
 * reload; an untrusted GitHub domain reviewing what is sent first; viewers
 * reading only; client-side validation before any request.
 */

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HttpResponse, http } from "msw";
import { beforeEach, describe, expect, it } from "vitest";

import type { ExternalIssueLabelDTO } from "../../lib/types";
import { makeLabelPage } from "../../test/externalIssueFixtures";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import {
  IssueLabelCatalogEditor,
  type IssueLabelDomain,
} from "./IssueLabelCatalogEditor";

const LABELS_URL = "/api/v1/orgs/org-g5labs/issue-domains/domain-1/labels";

const NATIVE: IssueLabelDomain = {
  domainId: "domain-1",
  displayKey: "ACME",
  provider: "g5",
  contentTrusted: true,
};

/** A catalog served from `labels`, which tests change as writes land. */
function serveCatalog(labels: ExternalIssueLabelDTO[]) {
  const state = { labels: [...labels], reads: 0 };
  server.use(
    http.get(LABELS_URL, () => {
      state.reads += 1;
      return HttpResponse.json({
        data: makeLabelPage({ displayKey: "ACME", labels: state.labels }),
      });
    }),
  );
  return state;
}

function renderEditor(domain: IssueLabelDomain = NATIVE, canEdit = true) {
  return renderApp(
    <IssueLabelCatalogEditor
      orgId="org-g5labs"
      domain={domain}
      canEdit={canEdit}
    />,
    { user: createMockUser() },
  );
}

const BUG = { id: "101", name: "bug", color: "d73a4a", description: "Broken" };
const WIP = { id: "103", name: "wip", color: null, description: "" };

describe("IssueLabelCatalogEditor", () => {
  let catalog: ReturnType<typeof serveCatalog>;
  beforeEach(() => {
    catalog = serveCatalog([BUG, WIP]);
  });

  it("creates a label and refreshes the catalog", async () => {
    const user = userEvent.setup();
    const bodies: unknown[] = [];
    server.use(
      http.post(LABELS_URL, async ({ request }) => {
        const body = (await request.json()) as { name: string };
        bodies.push(body);
        const label = { id: "104", name: body.name, color: "0075ca" };
        catalog.labels.push(label);
        return HttpResponse.json(
          {
            data: {
              outcome: "applied",
              label,
              githubLogin: null,
              message: null,
            },
          },
          { status: 201 },
        );
      }),
    );
    renderEditor();

    expect(
      await screen.findByTestId("label-catalog-row-bug"),
    ).toHaveTextContent("bugBroken");
    await user.click(screen.getByTestId("label-catalog-new"));
    // Nothing can be sent until the form is valid.
    expect(screen.getByTestId("label-form-save")).toBeDisabled();
    await user.type(screen.getByTestId("label-form-name"), "docs");
    await user.type(screen.getByTestId("label-form-color"), "#12");
    expect(screen.getByTestId("label-form-invalid")).toHaveTextContent(
      "A colour is six hex digits",
    );
    expect(screen.getByTestId("label-form-save")).toBeDisabled();
    await user.type(screen.getByTestId("label-form-color"), "34AB");
    await user.type(
      screen.getByTestId("label-form-description"),
      " Documentation ",
    );
    await user.click(screen.getByTestId("label-form-save"));

    expect(await screen.findByTestId("label-catalog-row-docs")).toBeVisible();
    expect(bodies).toEqual([
      { name: "docs", color: "1234ab", description: "Documentation" },
    ]);
    expect(screen.queryByTestId("label-create-form")).not.toBeInTheDocument();
  });

  it("never submits a form it is opened over", async () => {
    // The editor opens over the issue page's label form and the create
    // dialog; saving a label with Enter must not submit those too.
    const user = userEvent.setup();
    let outerSubmits = 0;
    server.use(
      http.post(LABELS_URL, () =>
        HttpResponse.json(
          {
            data: {
              outcome: "applied",
              label: { id: "105", name: "docs", color: null },
              githubLogin: null,
              message: null,
            },
          },
          { status: 201 },
        ),
      ),
    );
    renderApp(
      <form
        onSubmit={(event) => {
          event.preventDefault();
          outerSubmits += 1;
        }}
      >
        <IssueLabelCatalogEditor orgId="org-g5labs" domain={NATIVE} canEdit />
      </form>,
      { user: createMockUser() },
    );

    await user.click(await screen.findByTestId("label-catalog-new"));
    await user.type(screen.getByTestId("label-form-name"), "docs{Enter}");
    await waitFor(() =>
      expect(screen.queryByTestId("label-create-form")).not.toBeInTheDocument(),
    );
    expect(outerSubmits).toBe(0);
  });

  it("refuses a name another label has, showing that label", async () => {
    const user = userEvent.setup();
    server.use(
      http.post(LABELS_URL, () =>
        HttpResponse.json(
          {
            error: {
              code: "CONFLICT",
              message: "a label named bug already exists",
              details: { code: "label_exists", label: BUG },
              requestId: "",
            },
          },
          { status: 409 },
        ),
      ),
    );
    renderEditor();

    await user.click(await screen.findByTestId("label-catalog-new"));
    await user.type(screen.getByTestId("label-form-name"), "BUG");
    await user.type(screen.getByTestId("label-form-color"), "00ff00");
    await user.click(screen.getByTestId("label-form-save"));

    const error = await screen.findByTestId("label-form-error");
    expect(error).toHaveTextContent(
      "A label named “bug” already exists. Choose another name, or edit that label.",
    );
    expect(within(error).getByTestId("label-form-existing")).toHaveTextContent(
      "Existing label:bugBroken",
    );
    // The form and its values are kept.
    expect(screen.getByTestId("label-form-name")).toHaveValue("BUG");
  });

  it("edits a label inline, sending only what changed", async () => {
    const user = userEvent.setup();
    const bodies: unknown[] = [];
    server.use(
      http.patch(`${LABELS_URL}/101`, async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json({
          data: {
            outcome: "applied",
            label: { ...BUG, description: "" },
            githubLogin: null,
            message: null,
          },
        });
      }),
    );
    renderEditor();

    await user.click(await screen.findByTestId("label-catalog-edit-bug"));
    const form = screen.getByTestId("label-edit-form");
    expect(within(form).getByTestId("label-form-name")).toHaveValue("bug");
    expect(within(form).getByTestId("label-form-save")).toBeDisabled();
    await user.clear(within(form).getByTestId("label-form-description"));
    await user.click(within(form).getByTestId("label-form-save"));

    await waitFor(() =>
      expect(screen.queryByTestId("label-edit-form")).not.toBeInTheDocument(),
    );
    // An emptied description clears it; name and colour are not sent.
    expect(bodies).toEqual([{ description: "" }]);
  });

  it("deletes only after confirming that the label leaves every issue", async () => {
    const user = userEvent.setup();
    let deletes = 0;
    server.use(
      http.delete(`${LABELS_URL}/101`, () => {
        deletes += 1;
        catalog.labels = catalog.labels.filter((label) => label.id !== "101");
        return HttpResponse.json({
          data: {
            outcome: "applied",
            label: BUG,
            githubLogin: null,
            message: null,
          },
        });
      }),
    );
    renderEditor();

    await user.click(await screen.findByTestId("label-catalog-delete-bug"));
    expect(
      screen.getByTestId("label-catalog-delete-confirm"),
    ).toHaveTextContent(
      "Delete the label “bug” from ACME? It is also removed from every issue that has it. This cannot be undone.",
    );
    await user.click(screen.getByTestId("label-catalog-delete-cancel"));
    expect(deletes).toBe(0);

    await user.click(screen.getByTestId("label-catalog-delete-bug"));
    await user.click(screen.getByTestId("label-catalog-delete-confirm-button"));
    await waitFor(() =>
      expect(
        screen.queryByTestId("label-catalog-row-bug"),
      ).not.toBeInTheDocument(),
    );
    expect(deletes).toBe(1);
  });

  it("asks for a reload when GitHub does not confirm a write", async () => {
    const user = userEvent.setup();
    server.use(
      http.delete(`${LABELS_URL}/101`, () =>
        HttpResponse.json({
          data: {
            outcome: "unknown",
            label: null,
            githubLogin: "alex-gh",
            message: "GitHub timed out",
          },
        }),
      ),
    );
    renderEditor({ ...NATIVE, provider: "github", displayKey: "acme/api" });

    await user.click(await screen.findByTestId("label-catalog-delete-bug"));
    await user.click(screen.getByTestId("label-catalog-delete-confirm-button"));
    expect(
      await screen.findByTestId("label-form-unconfirmed"),
    ).toHaveTextContent(
      "GitHub did not confirm the change, so it may or may not have applied. Reload the labels to see the catalog before trying again. (GitHub timed out)",
    );
    const reads = catalog.reads;
    await user.click(screen.getByTestId("label-form-reload"));
    await waitFor(() => expect(catalog.reads).toBeGreaterThan(reads));
  });

  it("reviews exactly what is sent first on an untrusted GitHub domain", async () => {
    const user = userEvent.setup();
    const bodies: unknown[] = [];
    server.use(
      http.post(LABELS_URL, async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json(
          {
            data: {
              outcome: "applied",
              label: { id: "105", name: "p1", color: null },
              githubLogin: "alex-gh",
              message: null,
            },
          },
          { status: 201 },
        );
      }),
    );
    renderEditor({
      domainId: "domain-1",
      displayKey: "acme/api",
      provider: "github",
      contentTrusted: false,
    });

    await user.click(await screen.findByTestId("label-catalog-new"));
    await user.type(screen.getByTestId("label-form-name"), "p1");
    await user.click(screen.getByTestId("label-form-review-button"));
    const review = screen.getByTestId("label-form-review");
    expect(review).toHaveTextContent(
      "This label will be created in ACME on GitHub as your connected GitHub account.",
    );
    expect(review).toHaveTextContent("Namep1");
    expect(bodies).toEqual([]);
    await user.click(screen.getByTestId("label-form-save"));
    await waitFor(() => expect(bodies).toEqual([{ name: "p1" }]));
  });

  it("lets viewers read the catalog only", async () => {
    renderEditor(NATIVE, false);

    expect(await screen.findByTestId("label-catalog-row-bug")).toBeVisible();
    expect(
      screen.getByText("Only members with write permission can change labels."),
    ).toBeVisible();
    expect(screen.queryByTestId("label-catalog-new")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("label-catalog-edit-bug"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("label-catalog-delete-bug"),
    ).not.toBeInTheDocument();
  });
});
