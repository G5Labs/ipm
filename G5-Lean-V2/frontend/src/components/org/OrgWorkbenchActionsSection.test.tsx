import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../../test/msw/server";
import { OrgWorkbenchActionsSection } from "./OrgWorkbenchActionsSection";

function renderSection() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <OrgWorkbenchActionsSection orgId="org-1" />
    </QueryClientProvider>,
  );
}

describe("OrgWorkbenchActionsSection (ui g5.workbench.review-prompts)", () => {
  it("shows Re-run Verification off by default and writes the switch on toggle", async () => {
    // An organization that never touched the switch.
    server.use(
      http.get("/api/v1/orgs/org-1/config", () =>
        HttpResponse.json({ data: { settings: {} } }),
      ),
    );
    // A holder object: a closure-assigned `let` narrows to `null` for tsc.
    const put: { body: { settings?: Record<string, unknown> } | null } = {
      body: null,
    };
    server.use(
      http.put("/api/v1/orgs/org-1/config", async ({ request }) => {
        put.body = (await request.json()) as typeof put.body;
        return HttpResponse.json({
          data: { settings: { verification_action_enabled: true } },
        });
      }),
    );

    renderSection();

    const toggle = await screen.findByTestId(
      "org-admin-verification-action-switch",
    );
    await waitFor(() => expect(toggle).not.toBeDisabled());
    expect(toggle).not.toBeChecked();
    expect(
      screen.getByTestId("org-admin-verification-action-row"),
    ).toHaveTextContent("Off");

    await userEvent.click(toggle);

    await waitFor(() => expect(put.body).not.toBeNull());
    expect(put.body?.settings).toEqual({ verification_action_enabled: true });
  });

  it("reads On from the organization's setting and writes Off", async () => {
    server.use(
      http.get("/api/v1/orgs/org-1/config", () =>
        HttpResponse.json({
          data: { settings: { verification_action_enabled: true } },
        }),
      ),
    );
    const put: { body: { settings?: Record<string, unknown> } | null } = {
      body: null,
    };
    server.use(
      http.put("/api/v1/orgs/org-1/config", async ({ request }) => {
        put.body = (await request.json()) as typeof put.body;
        return HttpResponse.json({ data: { settings: {} } });
      }),
    );

    renderSection();

    const toggle = await screen.findByTestId(
      "org-admin-verification-action-switch",
    );
    await waitFor(() => expect(toggle).toBeChecked());
    await userEvent.click(toggle);
    await waitFor(() => expect(put.body).not.toBeNull());
    expect(put.body?.settings).toEqual({ verification_action_enabled: false });
  });

  it("keeps the switch disabled and says so when the setting cannot be loaded", async () => {
    server.use(
      http.get("/api/v1/orgs/org-1/config", () =>
        HttpResponse.json({ error: "boom" }, { status: 500 }),
      ),
    );
    renderSection();
    expect(
      await screen.findByTestId("org-admin-verification-action-error"),
    ).toHaveTextContent("The organization setting could not be loaded.");
    expect(
      screen.getByTestId("org-admin-verification-action-switch"),
    ).toBeDisabled();
  });
});
