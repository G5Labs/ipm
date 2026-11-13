// =============================================================================
// AddWorkerHostDialog — disk size field
// =============================================================================
// Ontology g5.runtime.placement.host-disk (issue #344): the host's boot disk is
// shared by all of its VMs, so its size scales with the VM budget. The dialog
// must show the size the SERVER would derive — computed from the coefficients
// the host-list envelope publishes, never from its own copy of the rule, since
// the server owns the sizing policy.
// =============================================================================

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import type { WorkerHostBootDiskSizing } from "../../lib/types";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { AddWorkerHostDialog } from "./AddWorkerHostDialog";

const ORG_ID = "org-disk-test";

/** Fixed provisioning policy published by the server. */
const SIZING: WorkerHostBootDiskSizing = {
  baseGb: 120,
  perSlotGb: 80,
  floorGb: 200,
  minGb: 30,
  maxGb: 2000,
};

/** Capture the provision POST body; returns a getter for it. */
function captureProvision() {
  const seen: { body: Record<string, unknown> | null } = { body: null };
  server.use(
    http.post(`/api/v1/orgs/${ORG_ID}/worker-hosts`, async ({ request }) => {
      seen.body = (await request.json()) as Record<string, unknown>;
      return HttpResponse.json(
        {
          data: {
            hostId: "host-disk",
            hostJwt: "mock-host-jwt",
            status: "provisioning",
            region: "us-central1",
            instanceType: "c3-standard-22",
            createdAt: "2026-10-07T00:00:00Z",
            workerProfile: "fat-default",
            runtimeLabels: {},
          },
        },
        { status: 201 },
      );
    }),
  );
  return seen;
}

function renderDialog(sizing?: WorkerHostBootDiskSizing) {
  return renderApp(
    <AddWorkerHostDialog
      open
      onClose={() => {}}
      orgId={ORG_ID}
      bootDiskSizing={sizing}
    />,
  );
}

const diskInput = () => screen.getByTestId("add-worker-host-boot-disk-input");
const instanceSelect = () =>
  screen.getByTestId("add-worker-host-instance-type-select");
const jobsInput = () => screen.getByLabelText("Workspace VM limit (optional)");

describe("AddWorkerHostDialog disk size", () => {
  it("follows the instance type's slot count in its placeholder and hint", async () => {
    const user = userEvent.setup();
    renderDialog(SIZING);

    // c3-standard-4 defaults to 1 slot: 120 + 80 = 200.
    await user.selectOptions(instanceSelect(), "c3-standard-4");
    expect(diskInput()).toHaveAttribute("placeholder", "200");
    expect(screen.getByTestId("boot-disk-hint")).toHaveTextContent(
      "Defaults to 200 GiB (minimum).",
    );

    // c3-standard-22 defaults to 5 slots: 120 + 5 x 80 = 520.
    await user.selectOptions(instanceSelect(), "c3-standard-22");
    expect(diskInput()).toHaveAttribute("placeholder", "520");
    expect(screen.getByTestId("boot-disk-hint")).toHaveTextContent(
      "Defaults to 520 GiB (minimum).",
    );

    // c3-standard-44 defaults to 11 slots: 120 + 11 x 80 = 1000.
    await user.selectOptions(instanceSelect(), "c3-standard-44");
    expect(diskInput()).toHaveAttribute("placeholder", "1000");
  });

  it("follows a typed workspace-vm-limit value, since the budget sizes the disk", async () => {
    const user = userEvent.setup();
    renderDialog(SIZING);
    await user.selectOptions(instanceSelect(), "c3-standard-22");
    expect(diskInput()).toHaveAttribute("placeholder", "520");

    // Pinning 11 workspace VMs on the same shape must move the disk default with it.
    await user.type(jobsInput(), "11");
    await waitFor(() =>
      expect(diskInput()).toHaveAttribute("placeholder", "1000"),
    );

    // Back to blank ⇒ back to the shape's default budget.
    await user.clear(jobsInput());
    await waitFor(() =>
      expect(diskInput()).toHaveAttribute("placeholder", "520"),
    );
  });

  it("shows an unsupported minimum without clamping", async () => {
    const user = userEvent.setup();
    renderDialog(SIZING);
    await user.selectOptions(instanceSelect(), "c3-standard-44");
    await user.type(jobsInput(), "24");
    expect(diskInput()).toHaveAttribute("placeholder", "2040");
    expect(screen.getByTestId("boot-disk-hint")).toHaveTextContent(
      "Defaults to 2040 GiB (minimum).",
    );
  });

  it("uses the published coefficients when the server changes its policy", () => {
    renderDialog({ ...SIZING, baseGb: 160, perSlotGb: 90 });
    expect(diskInput()).toHaveAttribute("placeholder", "250");
  });

  it("omits an empty disk size so the server derives and stores nothing", async () => {
    const user = userEvent.setup();
    const seen = captureProvision();
    renderDialog(SIZING);

    await user.click(screen.getByTestId("add-worker-host-submit"));
    await waitFor(() => expect(seen.body).not.toBeNull());
    expect(seen.body).not.toHaveProperty("bootDiskGb");
  });

  it("sends a typed disk size as bootDiskGb", async () => {
    const user = userEvent.setup();
    const seen = captureProvision();
    renderDialog(SIZING);

    await user.type(diskInput(), "512");
    await user.click(screen.getByTestId("add-worker-host-submit"));
    await waitFor(() => expect(seen.body).not.toBeNull());
    expect(seen.body).toMatchObject({ bootDiskGb: 512 });
  });

  it("blocks a size outside the published bounds before any request", async () => {
    const user = userEvent.setup();
    const seen = captureProvision();
    renderDialog(SIZING);

    // Below the provider minimum: GCE cannot create a disk below the image.
    await user.type(diskInput(), "29");
    await user.click(screen.getByTestId("add-worker-host-submit"));
    expect(
      await screen.findByText(
        "Disk size must be a whole number between 200 and 2000 GiB.",
      ),
    ).toBeVisible();
    expect(seen.body).toBeNull();

    // Above the cost cap.
    await user.clear(diskInput());
    await user.type(diskInput(), "2001");
    await user.click(screen.getByTestId("add-worker-host-submit"));
    await waitFor(() =>
      expect(
        screen.getByText(
          "Disk size must be a whole number between 200 and 2000 GiB.",
        ),
      ).toBeVisible(),
    );
    expect(seen.body).toBeNull();
  });

  it("blocks a non-positive size even with no published bounds", async () => {
    const user = userEvent.setup();
    const seen = captureProvision();
    renderDialog();

    // With no coefficients there is no range to check, but a non-positive size
    // is nonsense at any configuration and must not reach the server.
    await user.type(diskInput(), "0");
    await user.click(screen.getByTestId("add-worker-host-submit"));
    expect(
      await screen.findByText(
        "Disk size must be a positive whole number of GiB.",
      ),
    ).toBeVisible();
    expect(seen.body).toBeNull();
  });

  it("rejects an override below the calculated minimum", async () => {
    const user = userEvent.setup();
    const seen = captureProvision();
    renderDialog(SIZING);

    // base + one slot = 200 GB; a smaller override may be insufficient.
    await user.type(diskInput(), "100");
    expect(screen.getByTestId("boot-disk-hint")).toHaveTextContent(
      "Requires at least 200 GiB for this host's VM slots.",
    );

    await user.click(screen.getByTestId("add-worker-host-submit"));
    expect(seen.body).toBeNull();
    expect(
      await screen.findByText(/Disk size must be a whole number between 200/),
    ).toBeVisible();
  });

  it("shows no derived default when the server published no coefficients", async () => {
    const user = userEvent.setup();
    const seen = captureProvision();
    renderDialog();

    // Nothing to compute from, so the dialog invents no numbers.
    expect(diskInput()).not.toHaveAttribute("placeholder");
    expect(
      screen.getByText("Leave blank for the server default."),
    ).toBeVisible();

    // A typed value still reaches the server, which stays authoritative (422).
    await user.type(diskInput(), "29");
    await user.click(screen.getByTestId("add-worker-host-submit"));
    await waitFor(() => expect(seen.body).not.toBeNull());
    expect(seen.body).toMatchObject({ bootDiskGb: 29 });
  });
});
