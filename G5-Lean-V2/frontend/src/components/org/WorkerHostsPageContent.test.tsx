import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { CLOUD_WORKSPACE_UI_CAPABILITIES } from "../../lib/capabilities";
import type {
  ListWorkerHostsResponse,
  WorkerHost,
  WorkerHostWorkspace,
} from "../../lib/types";
import { createMockUser } from "../../test/fixtures";
import { server } from "../../test/msw/server";
import { renderApp } from "../../test/render";
import { WorkerHostsPageContent } from "./WorkerHostsPageContent";

const ORG_ID = "org-workers-test";
const HOSTS_URL = `/api/v1/orgs/${ORG_ID}/worker-hosts`;

function host(hostId: string, status: WorkerHost["status"]): WorkerHost {
  return {
    hostId,
    orgId: ORG_ID,
    instanceType: "c3-standard-22",
    region: "us-central1",
    status,
    createdAt: "2026-09-24T00:00:00Z",
  };
}

function serveHosts(getHosts: () => WorkerHost[]) {
  server.use(
    http.get(HOSTS_URL, () => {
      const hosts = getHosts();
      return HttpResponse.json({
        data: {
          hosts,
          total: hosts.length,
          promotedImageTracks: [],
        } satisfies ListWorkerHostsResponse,
      });
    }),
  );
}

function renderHostsPage() {
  return renderApp(
    <Routes>
      <Route
        path="/:org/demo"
        element={
          <WorkerHostsPageContent
            capabilities={CLOUD_WORKSPACE_UI_CAPABILITIES}
          />
        }
      />
    </Routes>,
    {
      route: "/g5/demo?view=workers",
      user: createMockUser({
        orgs: [
          {
            orgId: ORG_ID,
            orgSlug: "g5",
            orgName: "G5",
            role: "admin",
          },
        ],
      }),
    },
  );
}

beforeEach(() => {
  server.use(
    http.get(`${HOSTS_URL}/workspaces`, () => HttpResponse.json({ data: [] })),
    http.get("/api/v1/instance/info", () =>
      HttpResponse.json({ data: { selfHosted: false } }),
    ),
  );
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("WorkerHostsPageContent", () => {
  it("displays the reported slot size and keeps provisional sizes pending", async () => {
    serveHosts(() => [
      {
        ...host("sized-host", "ready"),
        slotsTotal: 6,
        slotSizeReported: true,
        slotCpus: 3,
        slotMemMib: 13273,
      },
      {
        ...host("pending-host", "provisioning"),
        slotSizeReported: false,
        slotCpus: 2,
        slotMemMib: 4096,
      },
    ]);
    renderHostsPage();
    const table = await screen.findByRole("table", { name: "Hosts" });
    expect(within(table).getByText("3 vCPU / 13.0 GiB")).toBeVisible();
    expect(within(table).getByText("Pending")).toBeVisible();
    expect(
      within(table).queryByText("2 vCPU / 4.0 GiB"),
    ).not.toBeInTheDocument();
  });

  it("labels a process-isolation host as unisolated", async () => {
    serveHosts(() => [
      {
        ...host("process-host", "ready"),
        capabilities: ["workspace_vm:v1", "isolation:process"],
      },
      {
        ...host("vm-host", "ready"),
        capabilities: ["workspace_vm:v1", "isolation:cloud-hypervisor"],
      },
    ]);
    renderHostsPage();
    const table = await screen.findByRole("table", { name: "Hosts" });
    expect(within(table).getAllByText("process (unisolated)")).toHaveLength(1);
    const processRow = within(table)
      .getAllByTestId("worker-host-row")
      .find((row) => row.getAttribute("data-host-id") === "process-host");
    expect(processRow).toBeDefined();
    expect(
      within(processRow as HTMLElement).getByText("process (unisolated)"),
    ).toBeVisible();
  });

  it("does not show who created each host", async () => {
    serveHosts(() => [
      {
        ...host("host-a", "ready"),
        createdByUserId: "user-a",
        createdByDisplayName: "Ada Admin",
        createdByEmail: "ada@example.com",
      },
      host("host-b", "ready"),
    ]);
    renderHostsPage();
    const table = await screen.findByRole("table", { name: "Hosts" });
    expect(
      within(table).queryByRole("columnheader", { name: "Created by" }),
    ).not.toBeInTheDocument();
    expect(within(table).queryByText("Ada Admin")).not.toBeInTheDocument();
    expect(
      within(table).queryByText("ada@example.com"),
    ).not.toBeInTheDocument();
    expect(within(table).queryByText("Not recorded")).not.toBeInTheDocument();
  });

  it("shows each host's identity and creation age", async () => {
    vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-09-27T00:00:00Z"));
    serveHosts(() => [host("12345678-1234-4321-8123-123456789abc", "ready")]);
    renderHostsPage();
    const table = await screen.findByRole("table", { name: "Hosts" });
    expect(screen.getByRole("heading", { name: "Hosts" })).toBeVisible();
    expect(
      screen.getByText("Hosts run the workspace VMs for this organization.", {
        exact: true,
      }),
    ).toBeVisible();
    expect(
      screen.getByText("Workspaces with non-stopped VMs.", { exact: true }),
    ).toBeVisible();
    expect(
      within(table).getByRole("columnheader", { name: "Host ID" }),
    ).toBeVisible();
    expect(
      within(table).getByTitle("12345678-1234-4321-8123-123456789abc"),
    ).toHaveTextContent("12345678");
    expect(
      within(table).getByRole("columnheader", { name: "Age" }),
    ).toBeVisible();
    expect(within(table).getByText("3d ago")).toHaveAttribute(
      "datetime",
      "2026-09-24T00:00:00Z",
    );
  });

  it("links each workspace name to its workspace, including after a rename", async () => {
    serveHosts(() => [host("host-a", "ready")]);
    let workspaceName = "review-test-2-review-audit-3ea2df2d";
    server.use(
      http.get(`${HOSTS_URL}/workspaces`, () =>
        HttpResponse.json({
          data: [
            {
              workspaceId: "workspace-a",
              workspaceName,
              hostId: "host-a",
              status: "running",
              ownerUserId: "owner",
              ownerDisplayName: "Owner",
              ownerEmail: "owner@example.com",
              startedAt: null,
              lastActivityAt: null,
            } satisfies WorkerHostWorkspace,
          ],
        }),
      ),
    );
    const { queryClient } = renderHostsPage();
    const name = await screen.findByRole("link", { name: workspaceName });
    expect(name).toHaveAttribute(
      "href",
      `/g5/${encodeURIComponent(workspaceName)}`,
    );
    // No disclosure on hover or focus: the link itself is the affordance.
    const user = userEvent.setup();
    await user.hover(name);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    await user.unhover(name);
    await user.tab();
    await user.tab();
    await user.tab();
    expect(name).toHaveFocus();
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    expect(name).toHaveClass("truncate");

    workspaceName = "renamed-workspace-with-an-even-longer-review-name";
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["org-worker-host-workspaces", ORG_ID],
      });
    });
    await waitFor(() => expect(name).toHaveTextContent(workspaceName));
    expect(name).toHaveAttribute(
      "href",
      `/g5/${encodeURIComponent(workspaceName)}`,
    );
  });

  it("groups non-stopped workspaces by Host ID, with unassigned VMs last", async () => {
    const workspace = (
      name: string,
      hostId: string | null,
      status: WorkerHostWorkspace["status"],
    ): WorkerHostWorkspace => ({
      workspaceId: name,
      workspaceName: name,
      hostId,
      status,
      startedAt: null,
      lastActivityAt: null,
      ownerUserId: "current-owner",
      ownerDisplayName: "Current Owner",
      ownerEmail: "owner@example.com",
    });
    serveHosts(() => [host("host-a", "ready"), host("host-b", "ready")]);
    let workspaces = [
      workspace("unassigned", null, "failed"),
      workspace("on-b", "host-b", "running"),
      workspace("on-a-z", "host-a", "starting"),
      workspace("stopped", "host-a", "stopped"),
      workspace("on-a-a", "host-a", "stopping"),
      workspace("failed", null, "failed"),
    ];
    server.use(
      http.get(`${HOSTS_URL}/workspaces`, () =>
        HttpResponse.json({ data: workspaces }),
      ),
    );
    const { queryClient } = renderHostsPage();
    const table = await screen.findByRole("table", {
      name: "Workspaces with non-stopped VMs",
    });
    const names = () =>
      within(table)
        .getAllByRole("row")
        .slice(1)
        .map((row) => within(row).getAllByRole("cell")[1].textContent);
    expect(names()).toEqual([
      "on-a-a",
      "on-a-z",
      "on-b",
      "failed",
      "unassigned",
    ]);
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((cell) => cell.textContent),
    ).toEqual(["Host ID", "Name", "Owner", "Status", "Age", "Last activity"]);
    expect(within(table).getAllByText("Current Owner")).toHaveLength(5);
    expect(within(table).getAllByText("Unassigned")).toHaveLength(2);
    expect(within(table).queryByText("stopped")).not.toBeInTheDocument();

    workspaces = [
      workspace("on-a-a", "host-a", "stopped"),
      {
        ...workspace("unassigned", "host-b", "starting"),
        ownerDisplayName: "New Owner",
      },
      workspace("on-a-z", "host-a", "running"),
    ];
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["org-worker-host-workspaces", ORG_ID],
      });
    });
    await waitFor(() => expect(names()).toEqual(["on-a-z", "unassigned"]));
    expect(within(table).getByText("New Owner")).toBeVisible();
    expect(within(table).queryByText("Unassigned")).not.toBeInTheDocument();
  });

  it("shows current VM status, start age, and activity, clearing timestamps on restart", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-09-24T16:00:00Z"));
    serveHosts(() => [host("host-a", "ready")]);
    let vm: WorkerHostWorkspace = {
      workspaceId: "demo",
      workspaceName: "demo",
      hostId: "host-a",
      status: "running",
      ownerUserId: "owner",
      ownerDisplayName: "Owner",
      ownerEmail: "owner@example.com",
      startedAt: "2026-09-24T14:00:00Z",
      lastActivityAt: "2026-09-24T15:48:00Z",
    };
    server.use(
      http.get(`${HOSTS_URL}/workspaces`, () =>
        HttpResponse.json({ data: [vm] }),
      ),
    );
    const { queryClient } = renderHostsPage();
    const table = await screen.findByRole("table", {
      name: "Workspaces with non-stopped VMs",
    });
    const cells = () =>
      within(within(table).getAllByRole("row")[1]).getAllByRole("cell");
    expect(cells()[3]).toHaveTextContent("running");
    expect(cells()[4]).toHaveTextContent("2h ago");
    expect(cells()[5]).toHaveTextContent("12m ago");
    for (const [index, timestamp] of [
      [4, vm.startedAt],
      [5, vm.lastActivityAt],
    ] as const) {
      const time = cells()[index].querySelector("time");
      expect(time).toHaveAttribute("datetime", timestamp);
      expect(time).toHaveAttribute(
        "title",
        new Date(timestamp!).toLocaleString(),
      );
    }
    const refresh = async (update: Partial<WorkerHostWorkspace>) => {
      vm = { ...vm, ...update };
      await act(async () => {
        await queryClient.invalidateQueries({
          queryKey: ["org-worker-host-workspaces", ORG_ID],
        });
      });
    };
    for (const status of ["starting", "failed"] as const) {
      await refresh({ status, startedAt: null, lastActivityAt: null });
      await waitFor(() => expect(cells()[3]).toHaveTextContent(status));
      expect(cells()[4]).toHaveTextContent("—");
      expect(cells()[5]).toHaveTextContent("—");
      expect(within(cells()[4]).getByLabelText("Not available")).toBeVisible();
      expect(within(cells()[5]).getByLabelText("Not available")).toBeVisible();
      expect(within(table).queryByText("2h ago")).not.toBeInTheDocument();
    }
    await refresh({
      status: "running",
      startedAt: "2026-09-24T15:59:50Z",
      lastActivityAt: "2026-09-24T15:59:55Z",
    });
    await waitFor(() => expect(cells()[3]).toHaveTextContent("running"));
    expect(cells()[4]).toHaveTextContent("Just now");
    expect(cells()[5]).toHaveTextContent("Just now");
    // Identical API data must not freeze the displayed ages while idle.
    vi.mocked(Date.now).mockReturnValue(Date.parse("2026-09-24T16:01:00Z"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(cells()[4]).toHaveTextContent("1m ago");
    expect(cells()[5]).toHaveTextContent("1m ago");
    await refresh({ status: "stopped" });
    expect(
      await screen.findByText("No workspace VMs are active."),
    ).toBeVisible();
  });

  it("reports a workspace-list failure without hiding the host list", async () => {
    serveHosts(() => [host("running", "ready")]);
    server.use(
      http.get(
        `${HOSTS_URL}/workspaces`,
        () => new HttpResponse(null, { status: 500 }),
      ),
    );
    renderHostsPage();
    expect(
      await screen.findByText("Failed to load workspace VMs."),
    ).toBeVisible();
    expect(screen.getByText("ready")).toBeVisible();
    expect(
      screen.queryByText("No workspace VMs are active."),
    ).not.toBeInTheDocument();
  });

  it("hides terminated records while retaining every active host status", async () => {
    serveHosts(() => [
      host("old", "terminated"),
      host("starting", "provisioning"),
      host("running", "ready"),
      host("disconnected", "unreachable"),
      host("stopping", "draining"),
      host("removed", "terminated"),
    ]);
    renderHostsPage();

    const table = await screen.findByRole("table");
    expect(within(table).queryByText("terminated")).not.toBeInTheDocument();
    for (const status of ["provisioning", "ready", "unreachable", "draining"]) {
      expect(within(table).getByText(status)).toBeVisible();
    }
    expect(screen.getAllByTestId("worker-host-row")).toHaveLength(4);
    expect(screen.queryByText(/\d+ active\./)).not.toBeInTheDocument();
  });

  it("shows the empty state when only terminated records remain", async () => {
    serveHosts(() => [host("removed", "terminated")]);
    renderHostsPage();

    expect(await screen.findByText("No workspace hosts yet.")).toBeVisible();
    expect(screen.queryByText(/\d+ active\./)).not.toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add host" })).toBeVisible();
  });

  it("removes a draining row when a refreshed response reports termination", async () => {
    let hosts = [host("stopping", "draining"), host("running", "ready")];
    serveHosts(() => hosts);
    const { queryClient } = renderHostsPage();
    expect(await screen.findByText("draining")).toBeVisible();

    hosts = [host("stopping", "terminated"), host("running", "ready")];
    await act(async () => {
      await queryClient.invalidateQueries({
        queryKey: ["org-worker-hosts", ORG_ID],
      });
    });

    await waitFor(() => {
      expect(screen.getAllByTestId("worker-host-row")).toHaveLength(1);
    });
    expect(screen.getByText("ready")).toBeVisible();
    expect(screen.queryByText(/\d+ active\./)).not.toBeInTheDocument();
    expect(screen.queryByText("terminated")).not.toBeInTheDocument();
  });

  it("keeps a host after a failed removal and hides it after a successful retry", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    let hosts = [host("running", "ready")];
    let attempts = 0;
    serveHosts(() => hosts);
    server.use(
      http.delete(`${HOSTS_URL}/running`, () => {
        attempts += 1;
        if (attempts === 1) {
          return HttpResponse.json(
            { error: { code: "INTERNAL", message: "Removal failed" } },
            { status: 500 },
          );
        }
        hosts = [host("running", "terminated")];
        return new HttpResponse(null, { status: 204 });
      }),
    );
    renderHostsPage();

    await user.click(
      await screen.findByRole("button", { name: "Remove host" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Removal failed",
    );
    expect(screen.getByText("ready")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Remove host" }));
    expect(await screen.findByText("No workspace hosts yet.")).toBeVisible();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByText("terminated")).not.toBeInTheDocument();
    expect(screen.queryByText(/\d+ active\./)).not.toBeInTheDocument();
    expect(attempts).toBe(2);
    expect(confirm).toHaveBeenCalledWith("Remove host running?");
  });

  // g5.runtime.placement.runtime-identity: operators see which ready hosts
  // take no new VMs because their runtime is behind the control plane's.
  it("labels a stale-runtime host with a plain chip and nothing else", async () => {
    serveHosts(() => [
      {
        ...host("stale-host", "ready"),
        runtimeState: "stale",
        runtimeIdentity: { module_catalog_version: "2026.09.01-old" },
      },
      { ...host("silent-host", "ready"), runtimeState: "unknown" },
      { ...host("current-host", "ready"), runtimeState: "current" },
      { ...host("new-host", "provisioning"), runtimeState: "unknown" },
    ]);
    renderHostsPage();
    const table = await screen.findByRole("table", { name: "Hosts" });
    const rows = within(table).getAllByTestId("worker-host-row");
    const byId = (id: string) =>
      rows.find((row) => row.getAttribute("data-host-id") === id)!;
    const stale = within(byId("stale-host")).getByTestId(
      "worker-host-runtime-state",
    );
    expect(within(stale).getByText("stale")).toBeVisible();
    // No detail beyond the word: no tooltip, no catalog version.
    expect(stale.getAttribute("title")).toBeNull();
    expect(stale).not.toHaveTextContent("2026.09.01-old");
    // Hosts that are current, or have not published a runtime, get no chip.
    for (const id of ["silent-host", "current-host", "new-host"]) {
      expect(
        within(byId(id)).queryByTestId("worker-host-runtime-state"),
      ).not.toBeInTheDocument();
    }
  });

  // g5.architecture.host_forced_removal: a row that shows occupancy asks for
  // consent to stop its VMs and sends force with that consent.
  it("confirms with the VM count and forces removal of an occupied host", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    let hosts = [{ ...host("busy", "ready"), occupancy: 2, slotsTotal: 4 }];
    const requests: string[] = [];
    serveHosts(() => hosts);
    server.use(
      http.delete(`${HOSTS_URL}/busy`, ({ request }) => {
        requests.push(new URL(request.url).search);
        hosts = [];
        return new HttpResponse(null, { status: 204 });
      }),
    );
    renderHostsPage();

    await user.click(
      await screen.findByRole("button", { name: "Remove host" }),
    );
    expect(await screen.findByText("No workspace hosts yet.")).toBeVisible();
    expect(confirm).toHaveBeenCalledWith(
      "Host busy still runs 2 workspace VMs. Removing it stops them. Remove anyway?",
    );
    expect(requests).toEqual(["?force=true"]);
  });

  it("offers to remove anyway when the backend refuses a plain removal", async () => {
    const user = userEvent.setup();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    let hosts = [host("raced", "ready")];
    const requests: string[] = [];
    serveHosts(() => hosts);
    server.use(
      http.delete(`${HOSTS_URL}/raced`, ({ request }) => {
        const search = new URL(request.url).search;
        requests.push(search);
        if (search !== "?force=true") {
          return HttpResponse.json(
            {
              error: {
                code: "CONFLICT",
                message:
                  "Conflict: worker host raced still owns 1 active workspace VM(s); drain it or retry with force=true",
              },
            },
            { status: 409 },
          );
        }
        hosts = [];
        return new HttpResponse(null, { status: 204 });
      }),
    );
    renderHostsPage();

    await user.click(
      await screen.findByRole("button", { name: "Remove host" }),
    );
    const refusal = await screen.findByTestId("worker-host-removal-refused");
    expect(refusal).toHaveTextContent("still owns 1 active workspace VM(s)");
    expect(refusal).toHaveTextContent("stops its workspace VMs");
    // Keeping the host clears the offer without a request.
    await user.click(
      within(refusal).getByRole("button", { name: "Keep host" }),
    );
    expect(
      screen.queryByTestId("worker-host-removal-refused"),
    ).not.toBeInTheDocument();
    expect(screen.getByText("ready")).toBeVisible();
    expect(requests).toEqual([""]);

    await user.click(screen.getByRole("button", { name: "Remove host" }));
    await user.click(
      within(
        await screen.findByTestId("worker-host-removal-refused"),
      ).getByRole("button", { name: "Remove anyway" }),
    );
    expect(await screen.findByText("No workspace hosts yet.")).toBeVisible();
    expect(
      screen.queryByTestId("worker-host-removal-refused"),
    ).not.toBeInTheDocument();
    expect(requests).toEqual(["", "", "?force=true"]);
  });
});
