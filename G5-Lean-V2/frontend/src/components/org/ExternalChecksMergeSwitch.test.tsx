import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../../test/msw/server";
import { ExternalChecksMergeSwitch } from "./ExternalChecksMergeSwitch";

function renderSwitch(canEdit: boolean) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ExternalChecksMergeSwitch orgId="org-1" canEdit={canEdit} />
    </QueryClientProvider>,
  );
}

describe("ExternalChecksMergeSwitch", () => {
  it("reflects the org setting and writes the switch on toggle", async () => {
    server.use(
      http.get("/api/v1/orgs/org-1/config", () =>
        HttpResponse.json({
          data: { settings: { external_checks_block_merge: false } },
        }),
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
          data: { settings: { external_checks_block_merge: true } },
        });
      }),
    );

    renderSwitch(true);

    const toggle = await screen.findByTestId("external-checks-merge-switch");
    await waitFor(() => expect(toggle).not.toBeDisabled());
    expect(toggle).not.toBeChecked();

    await userEvent.click(toggle);

    await waitFor(() => expect(put.body).not.toBeNull());
    expect(put.body?.settings).toEqual({ external_checks_block_merge: true });
  });

  it("disables the control for a non-admin", async () => {
    server.use(
      http.get("/api/v1/orgs/org-1/config", () =>
        HttpResponse.json({ data: { settings: {} } }),
      ),
    );
    renderSwitch(false);
    const toggle = await screen.findByTestId("external-checks-merge-switch");
    expect(toggle).toBeDisabled();
  });
});
