import { describe, expect, it } from "vitest";
import { describeCapacityWait } from "./vmCapacityWait";

describe("describeCapacityWait", () => {
  it.each([
    [{ reason: "preparing_capacity" }, /Preparing capacity/],
    [
      { reason: "capacity_stockout" },
      /Cloud capacity is temporarily unavailable/,
    ],
    [
      { reason: "capacity_configuration" },
      /image policy and cloud configuration/,
    ],
    [{ reason: "capacity_failed" }, /retry limit/],
    [{ reason: "capacity_limit" }, /cloud cleanup/],
    [{ reason: "capacity_image_stale" }, /worker image for new hosts is older/],
    [{ reason: "capacity_growth_disabled" }, /automatic host growth is off/],
    [
      { reason: "host_capacity_refused", host_reason: "disk" },
      /enough free disk space/,
    ],
    [
      { reason: "host_capacity_refused", host_reason: "slots" },
      /slot on the host is in use/,
    ],
    [
      { reason: "host_capacity_refused", host_reason: "boot_capacity" },
      /busy starting other workspaces/,
    ],
    [{ reason: "host_capacity_refused", host_reason: "draining" }, /draining/],
    [
      { reason: "host_capacity_refused", host_reason: "workspace_busy" },
      /still releasing this workspace's previous VM/,
    ],
    [{ reason: "org_vm_quota", limit: 3 }, /maximum of 3 workspaces/],
    [{ reason: "host_tunnel_unavailable" }, /isn't connected/],
    [{ reason: "no_ready_host" }, /No worker host is ready yet/],
    [null, /No worker host is ready yet/],
  ])("explains %j", (lastError, expected) => {
    expect(describeCapacityWait(lastError)).toMatch(expected);
  });
});
