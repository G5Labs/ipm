// Why a workspace start is waiting for capacity, from the reason the API
// recorded with the refused or queued start (`WorkspaceVmView.lastError`).

/** A plain-language explanation of a queued start, for the toast and strip. */
export function describeCapacityWait(
  lastError: Record<string, unknown> | null | undefined,
): string {
  const reason = typeof lastError?.reason === "string" ? lastError.reason : "";
  const hostReason =
    typeof lastError?.host_reason === "string" ? lastError.host_reason : "";
  switch (reason) {
    case "preparing_capacity":
      return "Preparing capacity. A worker host is starting; your workspace will start automatically when it is ready.";
    case "capacity_stockout":
      return "Cloud capacity is temporarily unavailable in this region. Host provisioning retries automatically while this start request remains active.";
    case "capacity_configuration":
      return "A worker host could not be prepared. Ask an organization administrator to check the worker image policy and cloud configuration.";
    case "capacity_failed":
      return "Automatic host preparation failed or reached its retry limit. Check the Workers page for details, then cancel this request and try Start again.";
    case "capacity_growth_disabled":
      return "Every worker host is full and automatic host growth is off for this organization. The workspace starts when a slot frees up or an administrator adds a host.";
    case "capacity_limit":
      return "Waiting for existing hosts or cloud cleanup to finish before another host can be created.";
    case "capacity_image_stale":
      return "The worker image for new hosts is older than this version of G5, so a new host could not run this workspace. The workspace starts once a current image is published or an administrator adds a host with one.";
  }
  if (reason === "host_capacity_refused") {
    switch (hostReason) {
      case "disk":
        return "The workspace host doesn't have enough free disk space to start this workspace. Free disk space on the host; the start retries automatically.";
      case "slots":
        return "Every workspace slot on the host is in use. The workspace starts when one frees up.";
      case "boot_capacity":
        return "The workspace host is busy starting other workspaces. The start retries automatically.";
      case "draining":
        return "The workspace host is draining. The workspace starts when a ready host has room.";
      case "workspace_busy":
        return "The workspace host is still releasing this workspace's previous VM. The start retries automatically.";
    }
  }
  if (reason === "org_vm_quota") {
    const limit = typeof lastError?.limit === "number" ? lastError.limit : null;
    return limit === null
      ? "The organization is running its maximum number of workspaces. The workspace starts when one stops."
      : `The organization is running its maximum of ${limit} workspaces. The workspace starts when one stops.`;
  }
  if (
    reason === "host_tunnel_unavailable" ||
    reason === "host_control_unavailable"
  ) {
    return "The workspace host isn't connected right now. The start retries automatically.";
  }
  return "No worker host is ready yet. The workspace will start when capacity is available.";
}
