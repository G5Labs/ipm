// =============================================================================
// OrgAdminSettingsCard — Admin page organization settings
// =============================================================================
// The card edits the default VM idle timeout new workspaces start with (in
// minutes, sent as seconds) and the org VM quota (0–100). Covers loading the
// effective values, sending only changed fields, local range validation,
// server validation and failure messages, and the quota-0 warning.
// =============================================================================

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";

import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import {
  buildAdminSettingsPatch,
  OrgAdminSettingsCard,
} from "./OrgAdminSettingsCard";

const ORG_ID = "org-g5labs";

function serveSettings(defaultIdleTimeoutSecs: number, maxRunningVms: number) {
  const puts: unknown[] = [];
  server.use(
    http.get(`/api/v1/orgs/${ORG_ID}/admin-settings`, () =>
      HttpResponse.json({ data: { defaultIdleTimeoutSecs, maxRunningVms } }),
    ),
    http.put(`/api/v1/orgs/${ORG_ID}/admin-settings`, async ({ request }) => {
      const body = (await request.json()) as Record<string, number>;
      puts.push(body);
      return HttpResponse.json({
        data: { defaultIdleTimeoutSecs, maxRunningVms, ...body },
      });
    }),
  );
  return puts;
}

async function renderCard() {
  const result = renderApp(<OrgAdminSettingsCard orgId={ORG_ID} />);
  await screen.findByTestId("org-admin-settings-card");
  return result;
}

async function replaceValue(testId: string, value: string) {
  const input = screen.getByTestId(testId);
  await userEvent.clear(input);
  if (value) await userEvent.type(input, value);
}

describe("OrgAdminSettingsCard", () => {
  it("saves both disk allowances in GiB through the Admin API", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();
    const host = screen.getByLabelText(/host disk allowance H/i);
    const vm = screen.getByLabelText(/VM disk allowance V/i);
    expect(host).toHaveValue(120);
    expect(vm).toHaveValue(80);
    await userEvent.clear(host);
    await userEvent.type(host, "160");
    await userEvent.clear(vm);
    await userEvent.type(vm, "32");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));
    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([{ hostDiskAllowanceGib: 160, vmDiskAllowanceGib: 32 }]);
  });

  it("shows the effective values, the idle timeout in minutes", async () => {
    serveSettings(28_800, 10);
    await renderCard();

    expect(
      screen.getByLabelText(/default workspace vm idle timeout/i),
    ).toHaveValue(480);
    expect(screen.getByLabelText(/organization vm quota/i)).toHaveValue(10);
    // Nothing changed yet.
    expect(screen.getByTestId("org-admin-settings-save")).toBeDisabled();
  });

  it("saves only the changed field, converting minutes to seconds", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-default-idle-timeout", "90");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    expect(
      await screen.findByTestId("org-admin-settings-saved"),
    ).toBeInTheDocument();
    expect(puts).toEqual([{ defaultIdleTimeoutSecs: 5_400 }]);
    expect(
      screen.getByLabelText(/default workspace vm idle timeout/i),
    ).toHaveValue(90);
  });

  it("shows the personal idle default, 30 minutes when the API omits it", async () => {
    serveSettings(28_800, 10);
    await renderCard();

    expect(
      screen.getByLabelText(/default personal workspace vm idle timeout/i),
    ).toHaveValue(30);
  });

  it("saves the personal idle default in seconds, apart from the ordinary one", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-default-personal-idle-timeout", "45");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([{ defaultPersonalIdleTimeoutSecs: 2_700 }]);
    expect(
      screen.getByLabelText(/default personal workspace vm idle timeout/i),
    ).toHaveValue(45);
    expect(
      screen.getByLabelText(/default workspace vm idle timeout/i),
    ).toHaveValue(480);
  });

  it("saves both fields together", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-default-idle-timeout", "1440");
    await replaceValue("org-admin-max-running-vms", "100");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([
      { defaultIdleTimeoutSecs: 86_400, maxRunningVms: 100 },
    ]);
  });

  it.each([
    ["org-admin-default-idle-timeout", "0", /whole minutes from 1 to 1440/i],
    ["org-admin-default-idle-timeout", "1441", /whole minutes from 1 to 1440/i],
    [
      "org-admin-default-personal-idle-timeout",
      "0",
      /whole minutes from 1 to 1440/i,
    ],
    ["org-admin-max-running-vms", "101", /whole number from 0 to 100/i],
    ["org-admin-max-running-vms", "", /whole number from 0 to 100/i],
  ])(
    "rejects %s = %j locally without a request",
    async (testId, value, message) => {
      const puts = serveSettings(28_800, 10);
      await renderCard();

      await replaceValue(testId, value);
      await userEvent.click(screen.getByTestId("org-admin-settings-save"));

      expect(await screen.findByText(message)).toBeInTheDocument();
      expect(puts).toEqual([]);
    },
  );

  it("warns that a quota of 0 blocks every start", async () => {
    serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-max-running-vms", "0");

    expect(
      screen.getByText(/a quota of 0 blocks every workspace start/i),
    ).toBeInTheDocument();
  });

  it("shows the server's field validation message", async () => {
    serveSettings(28_800, 10);
    server.use(
      http.put(`/api/v1/orgs/${ORG_ID}/admin-settings`, () =>
        HttpResponse.json(
          {
            error: {
              code: "VALIDATION_ERROR",
              message: "Validation failed",
              details: { maxRunningVms: "Must be between 0 and 100" },
            },
          },
          { status: 422 },
        ),
      ),
    );
    await renderCard();

    await replaceValue("org-admin-max-running-vms", "20");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    expect(
      await screen.findByText("Must be between 0 and 100"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTestId("org-admin-settings-saved"),
    ).not.toBeInTheDocument();
  });

  it("shows a refused save", async () => {
    serveSettings(28_800, 10);
    server.use(
      http.put(`/api/v1/orgs/${ORG_ID}/admin-settings`, () =>
        HttpResponse.json(
          {
            error: {
              code: "FORBIDDEN",
              message: "Missing scope org:admin",
            },
          },
          { status: 403 },
        ),
      ),
    );
    await renderCard();

    await replaceValue("org-admin-max-running-vms", "20");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    await waitFor(() =>
      expect(
        screen.getByTestId("org-admin-settings-save-error"),
      ).toBeInTheDocument(),
    );
  });

  it("reports a failed load", async () => {
    server.use(
      http.get(`/api/v1/orgs/${ORG_ID}/admin-settings`, () =>
        HttpResponse.json(
          { error: { code: "FORBIDDEN", message: "Forbidden" } },
          { status: 403 },
        ),
      ),
    );
    renderApp(<OrgAdminSettingsCard orgId={ORG_ID} />);

    expect(
      await screen.findByTestId("org-admin-settings-load-error"),
    ).toBeInTheDocument();
  });
});

describe("OrgAdminSettingsCard — refetch and stale errors", () => {
  function serveChangingSettings() {
    let quota = 10;
    server.use(
      http.get(`/api/v1/orgs/${ORG_ID}/admin-settings`, () =>
        HttpResponse.json({
          data: { defaultIdleTimeoutSecs: 28_800, maxRunningVms: quota },
        }),
      ),
    );
    return (next: number) => {
      quota = next;
    };
  }

  it("keeps edited fields and refreshes untouched ones when the settings refetch", async () => {
    const puts: unknown[] = [];
    const setServerQuota = serveChangingSettings();
    server.use(
      http.put(`/api/v1/orgs/${ORG_ID}/admin-settings`, async ({ request }) => {
        const body = (await request.json()) as Record<string, number>;
        puts.push(body);
        return HttpResponse.json({
          data: { defaultIdleTimeoutSecs: 28_800, maxRunningVms: 20, ...body },
        });
      }),
    );
    const { queryClient } = await renderCard();

    await replaceValue("org-admin-default-idle-timeout", "90");
    // Another administrator raises the quota meanwhile.
    setServerQuota(20);
    await queryClient.refetchQueries({
      queryKey: ["org-admin-settings", ORG_ID],
    });

    expect(
      screen.getByLabelText(/default workspace vm idle timeout/i),
    ).toHaveValue(90);
    await waitFor(() =>
      expect(screen.getByLabelText(/organization vm quota/i)).toHaveValue(20),
    );

    // Saving sends only the edit, never the stale quota.
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));
    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([{ defaultIdleTimeoutSecs: 5_400 }]);
  });

  it("disables the inputs while a save is pending", async () => {
    serveSettings(28_800, 10);
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.put(`/api/v1/orgs/${ORG_ID}/admin-settings`, async ({ request }) => {
        const body = (await request.json()) as Record<string, number>;
        await gate;
        return HttpResponse.json({
          data: { defaultIdleTimeoutSecs: 28_800, maxRunningVms: 10, ...body },
        });
      }),
    );
    await renderCard();

    await replaceValue("org-admin-max-running-vms", "30");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    await waitFor(() =>
      expect(screen.getByLabelText(/organization vm quota/i)).toBeDisabled(),
    );
    expect(
      screen.getByLabelText(/default workspace vm idle timeout/i),
    ).toBeDisabled();
    release();
    await screen.findByTestId("org-admin-settings-saved");
    expect(screen.getByLabelText(/organization vm quota/i)).toBeEnabled();
    expect(screen.getByLabelText(/organization vm quota/i)).toHaveValue(30);
  });

  it("shows refetched values in an untouched form", async () => {
    const setServerQuota = serveChangingSettings();
    const { queryClient } = await renderCard();

    setServerQuota(20);
    await queryClient.refetchQueries({
      queryKey: ["org-admin-settings", ORG_ID],
    });

    await waitFor(() =>
      expect(screen.getByLabelText(/organization vm quota/i)).toHaveValue(20),
    );
  });

  it("clears only the edited field's error", async () => {
    serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-default-idle-timeout", "0");
    await replaceValue("org-admin-max-running-vms", "101");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));
    expect(
      await screen.findByText(/whole minutes from 1 to 1440/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/whole number from 0 to 100/i)).toBeInTheDocument();

    await replaceValue("org-admin-default-idle-timeout", "60");

    expect(
      screen.queryByText(/whole minutes from 1 to 1440/i),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/whole number from 0 to 100/i)).toBeInTheDocument();
  });

  it("reads back a saved value after remounting (mock store persists by org)", async () => {
    const first = await renderCard();
    await replaceValue("org-admin-max-running-vms", "42");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));
    await screen.findByTestId("org-admin-settings-saved");
    first.unmount();

    renderApp(<OrgAdminSettingsCard orgId={ORG_ID} />);
    await screen.findByTestId("org-admin-settings-card");
    expect(screen.getByLabelText(/organization vm quota/i)).toHaveValue(42);
  });
});

describe("OrgAdminSettingsCard — managed capacity group", () => {
  function group() {
    return screen.getByRole("group", { name: /managed capacity/i });
  }

  it("groups the managed-capacity controls in one labelled box", async () => {
    serveSettings(28_800, 10);
    await renderCard();

    const box = group();
    expect(box.tagName).toBe("FIELDSET");
    expect(
      within(box).getByRole("switch", { name: /add hosts automatically/i }),
    ).toBeInTheDocument();
    expect(
      within(box).getByLabelText(/machine type for added hosts/i),
    ).toBeInTheDocument();
    expect(
      within(box).getByRole("switch", {
        name: /remove empty hosts automatically/i,
      }),
    ).toBeInTheDocument();
    expect(
      within(box).getByLabelText(/remove a host after it is empty for/i),
    ).toBeInTheDocument();
    expect(
      within(box).getByLabelText(/free slots to keep ready/i),
    ).toBeInTheDocument();
    // The workspace idle default and the quota stay outside the group.
    expect(
      within(box).queryByLabelText(/default workspace vm idle timeout/i),
    ).not.toBeInTheDocument();
    expect(
      within(box).queryByLabelText(/organization vm quota/i),
    ).not.toBeInTheDocument();
  });

  it("shows today's behavior when the API does not report the fields", async () => {
    serveSettings(28_800, 10);
    await renderCard();

    expect(
      screen.getByRole("switch", { name: /add hosts automatically/i }),
    ).toBeChecked();
    expect(screen.getByLabelText(/machine type for added hosts/i)).toHaveValue(
      "c3-standard-4",
    );
    expect(
      screen.getByRole("switch", { name: /remove empty hosts automatically/i }),
    ).toBeChecked();
    expect(
      screen.getByLabelText(/remove a host after it is empty for/i),
    ).toHaveValue(60);
    expect(screen.getByLabelText(/free slots to keep ready/i)).toHaveValue(1);
    expect(
      screen.getByLabelText(/running workspace vms per owner/i),
    ).toHaveValue(5);
    expect(screen.getByLabelText(/workspaces per owner/i)).toHaveValue(50);
    expect(screen.getByTestId("org-admin-settings-save")).toBeDisabled();
  });

  it("saves both per-owner limits together", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-max-running-vms-per-owner", "3");
    await replaceValue("org-admin-max-workspaces-per-owner", "1000");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([
      { maxRunningVmsPerOwner: 3, maxWorkspacesPerOwner: 1000 },
    ]);
  });

  it.each([
    [
      "org-admin-max-running-vms-per-owner",
      "101",
      /whole number from 0 to 100/i,
    ],
    ["org-admin-max-running-vms-per-owner", "", /whole number from 0 to 100/i],
    [
      "org-admin-max-workspaces-per-owner",
      "1001",
      /whole number from 0 to 1000/i,
    ],
    [
      "org-admin-max-workspaces-per-owner",
      "2.5",
      /whole number from 0 to 1000/i,
    ],
  ])("rejects %s = %j without a request", async (testId, value, message) => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await replaceValue(testId, value);
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    expect(await screen.findByText(message)).toBeInTheDocument();
    expect(puts).toEqual([]);
  });

  it("warns that a per-owner limit of 0 blocks every start or new workspace", async () => {
    serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-max-running-vms-per-owner", "0");
    await replaceValue("org-admin-max-workspaces-per-owner", "0");

    expect(
      screen.getByText(/a limit of 0 blocks every workspace start/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/a limit of 0 blocks every new workspace/i),
    ).toBeInTheDocument();
  });

  it("saves the free-slot floor on its own", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await replaceValue("org-admin-min-free-slots", "2");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([{ minFreeSlots: 2 }]);
  });

  it.each(["101", "1.5", ""])(
    "rejects a free-slot floor of %j without a request",
    async (value) => {
      const puts = serveSettings(28_800, 10);
      await renderCard();

      await replaceValue("org-admin-min-free-slots", value);
      await userEvent.click(screen.getByTestId("org-admin-settings-save"));

      expect(
        await screen.findByText(/whole number from 0 to 100/i),
      ).toBeInTheDocument();
      expect(puts).toEqual([]);
    },
  );

  it("disables each dependent input when its switch is off and saves the switch", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await userEvent.click(
      screen.getByRole("switch", { name: /add hosts automatically/i }),
    );
    await userEvent.click(
      screen.getByRole("switch", { name: /remove empty hosts automatically/i }),
    );

    expect(
      screen.getByLabelText(/machine type for added hosts/i),
    ).toBeDisabled();
    expect(
      screen.getByLabelText(/remove a host after it is empty for/i),
    ).toBeDisabled();
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));
    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([{ autoAddHosts: false, autoRetireIdleHosts: false }]);
  });

  it("turns automatic continuation of interrupted chats off", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    const toggle = screen.getByRole("switch", {
      name: /continue interrupted chats automatically/i,
    });
    expect(toggle).toBeChecked();
    await userEvent.click(toggle);
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));
    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([{ automaticContinuationEnabled: false }]);
  });

  it("saves a new machine type and removal delay in seconds", async () => {
    const puts = serveSettings(28_800, 10);
    await renderCard();

    await userEvent.selectOptions(
      screen.getByLabelText(/machine type for added hosts/i),
      "c3-standard-8",
    );
    await replaceValue("org-admin-idle-host-retire", "30");
    await userEvent.click(screen.getByTestId("org-admin-settings-save"));

    await screen.findByTestId("org-admin-settings-saved");
    expect(puts).toEqual([
      { autoHostInstanceType: "c3-standard-8", idleHostRetireSecs: 1_800 },
    ]);
  });

  it.each(["14", "1441", ""])(
    "rejects a removal delay of %j without a request",
    async (value) => {
      const puts = serveSettings(28_800, 10);
      await renderCard();

      await replaceValue("org-admin-idle-host-retire", value);
      await userEvent.click(screen.getByTestId("org-admin-settings-save"));

      expect(
        await screen.findByText(/whole minutes from 15 to 1440/i),
      ).toBeInTheDocument();
      expect(puts).toEqual([]);
    },
  );

  it("reflects values the API reports", async () => {
    server.use(
      http.get(`/api/v1/orgs/${ORG_ID}/admin-settings`, () =>
        HttpResponse.json({
          data: {
            defaultIdleTimeoutSecs: 28_800,
            maxRunningVms: 10,
            autoAddHosts: false,
            autoHostInstanceType: "c3-standard-22",
            autoRetireIdleHosts: true,
            idleHostRetireSecs: 7_200,
            minFreeSlots: 3,
            maxRunningVmsPerOwner: 2,
            maxWorkspacesPerOwner: 20,
          },
        }),
      ),
    );
    await renderCard();

    expect(
      screen.getByRole("switch", { name: /add hosts automatically/i }),
    ).not.toBeChecked();
    expect(screen.getByLabelText(/machine type for added hosts/i)).toHaveValue(
      "c3-standard-22",
    );
    expect(
      screen.getByLabelText(/remove a host after it is empty for/i),
    ).toHaveValue(120);
    expect(screen.getByLabelText(/free slots to keep ready/i)).toHaveValue(3);
    expect(
      screen.getByLabelText(/running workspace vms per owner/i),
    ).toHaveValue(2);
    expect(screen.getByLabelText(/workspaces per owner/i)).toHaveValue(20);
  });
});

describe("buildAdminSettingsPatch", () => {
  const current = { defaultIdleTimeoutSecs: 28_800, maxRunningVms: 10 };
  const capacity = {
    hostDiskAllowanceGib: "120",
    vmDiskAllowanceGib: "80",
    autoAddHosts: true,
    autoHostInstanceType: "c3-standard-4",
    autoRetireIdleHosts: true,
    automaticContinuationEnabled: true,
    idleHostRetireMinutes: "60",
    minFreeSlots: "1",
    personalIdleMinutes: "30",
    maxRunningVmsPerOwner: "5",
    maxWorkspacesPerOwner: "50",
  };

  it("returns no fields when nothing changed", () => {
    expect(
      buildAdminSettingsPatch(
        { ...capacity, idleMinutes: "480", maxRunningVms: "10" },
        current,
      ),
    ).toEqual({ patch: {}, errors: {} });
  });

  it("accepts the inclusive bounds", () => {
    expect(
      buildAdminSettingsPatch(
        { ...capacity, idleMinutes: "1", maxRunningVms: "0" },
        current,
      ),
    ).toEqual({
      patch: { defaultIdleTimeoutSecs: 60, maxRunningVms: 0 },
      errors: {},
    });
  });

  it("rejects fractional and signed input", () => {
    const { errors } = buildAdminSettingsPatch(
      { ...capacity, idleMinutes: "1.5", maxRunningVms: "-1" },
      current,
    );
    expect(Object.keys(errors).sort()).toEqual([
      "defaultIdleTimeoutSecs",
      "maxRunningVms",
    ]);
  });

  it("sends the personal idle default only when it changed", () => {
    expect(
      buildAdminSettingsPatch(
        {
          ...capacity,
          idleMinutes: "480",
          maxRunningVms: "10",
          personalIdleMinutes: "60",
        },
        { ...current, defaultPersonalIdleTimeoutSecs: 1_800 },
      ),
    ).toEqual({
      patch: { defaultPersonalIdleTimeoutSecs: 3_600 },
      errors: {},
    });
  });
});
