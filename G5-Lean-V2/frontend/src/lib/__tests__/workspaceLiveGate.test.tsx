import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, renderHook, screen } from "@testing-library/react";
import {
  GatedControl,
  liveGateBanner,
  liveGateCanStart,
  liveGateIsWaiting,
  liveGateKind,
  liveGateReason,
  makeRemoteOffered,
  reviewIdentityKnownFrom,
  StartWorkspaceHint,
  useWorkspaceLiveGateValue,
  WorkspaceLiveGateProvider,
  type WorkspaceLiveGate,
} from "../workspaceLiveGate";
import { WORKSPACE_START_EVENT } from "../../components/workspace/reviewWorkspace";
import { fileViewStartLabel } from "../../components/files/FileView";

const runningOwner = {
  workspace: {
    isBase: false,
    kind: "working" as const,
    ownerUserId: "owner",
    readOnly: false,
  },
  vmStatus: "running" as const,
  userId: "owner",
  vmOwnerUserId: "owner",
  orgAdmin: false,
  reviewCopy: false,
  reviewIdentityKnown: true,
};

describe("shared live gate runtime authority", () => {
  it("changes the gate identity when a running VM is replaced", () => {
    const { result, rerender } = renderHook(
      ({ runtimeKey }) =>
        useWorkspaceLiveGateValue({ ...runningOwner, runtimeKey }),
      { initialProps: { runtimeKey: "vm-1:epoch-1" } },
    );
    const previous = result.current.gate;
    expect(previous.live).toBe(true);
    expect(previous.runtimeKey).toBe("vm-1:epoch-1");

    rerender({ runtimeKey: "vm-2:epoch-2" });
    expect(result.current.gate.live).toBe(true);
    expect(result.current.gate.runtimeKey).toBe("vm-2:epoch-2");
    expect(result.current.gate).not.toBe(previous);
  });

  it.each(["former-owner", null])(
    "withdraws live attachment when the VM owner becomes %s",
    (vmOwnerUserId) => {
      const { result, rerender } = renderHook(
        ({ vmOwnerUserId }: { vmOwnerUserId: string | null }) =>
          useWorkspaceLiveGateValue({ ...runningOwner, vmOwnerUserId }),
        { initialProps: { vmOwnerUserId: "owner" as string | null } },
      );
      expect(result.current.ownerHasRunningAttachment).toBe(true);
      rerender({ vmOwnerUserId });
      expect(result.current.ownerHasRunningAttachment).toBe(false);
      expect(result.current.gate.live).toBe(false);
      expect(result.current.gate.kind).toBe("live");
      expect(result.current.gate.vmKind).toBe("reader");
      expect(result.current.gate.owner).toBe(true);
      expect(result.current.gate.canWrite).toBe(true);
    },
  );
  it.each(["former-owner", null])(
    "keeps the workspace owner's explicit Start when a stopped VM names %s",
    (vmOwnerUserId) => {
      const { result } = renderHook(() =>
        useWorkspaceLiveGateValue({
          ...runningOwner,
          vmStatus: "stopped",
          vmOwnerUserId,
        }),
      );
      expect(result.current.gate.kind).toBe("stopped");
      expect(result.current.gate.canStart).toBe(true);
      expect(result.current.gate.live).toBe(false);
      expect(result.current.gate.vmKind).toBe("reader");
    },
  );
});

describe("fileViewStartLabel (g5.workspace.ui.vm-state)", () => {
  it("follows the local request flag without a gate, as before", () => {
    expect(fileViewStartLabel(null, false).label).toBe(
      "Start workspace to edit",
    );
    expect(fileViewStartLabel(null, true).label).toBe("Starting workspace…");
  });

  it("follows the VM state with a gate, so a failed start offers Start again", () => {
    expect(fileViewStartLabel("stopped", true).label).toBe(
      "Start workspace to edit",
    );
    expect(fileViewStartLabel("starting", false).label).toBe(
      "Starting workspace…",
    );
    expect(fileViewStartLabel("queued", false).label).toBe(
      "Starting workspace…",
    );
    expect(fileViewStartLabel("stopping", false).label).toBe(
      "Workspace is stopping…",
    );
    expect(fileViewStartLabel("failed", true)).toEqual({
      label: "Start workspace again to edit",
      title: "The workspace failed to start. Start it again to edit.",
    });
  });
});

const owner = {
  isOwner: true,
  isBase: false,
  readOnly: false,
  freezing: false,
};

describe("liveGateKind (g5.workspace.ui.gates)", () => {
  it("orders refusals the way the server does: Base, reader, policy, VM", () => {
    expect(liveGateKind({ ...owner, isBase: true, vmStatus: "running" })).toBe(
      "base",
    );
    expect(
      liveGateKind({ ...owner, isOwner: false, vmStatus: "running" }),
    ).toBe("reader");
    expect(
      liveGateKind({ ...owner, readOnly: true, vmStatus: "running" }),
    ).toBe("read_only");
    expect(
      liveGateKind({ ...owner, freezing: true, vmStatus: "running" }),
    ).toBe("freezing");
  });

  it("names each VM state instead of collapsing them to stopped", () => {
    expect(liveGateKind({ ...owner, vmStatus: "running" })).toBe("live");
    expect(liveGateKind({ ...owner, vmStatus: "stopped" })).toBe("stopped");
    expect(liveGateKind({ ...owner, vmStatus: "failed" })).toBe("failed");
    expect(liveGateKind({ ...owner, vmStatus: "starting" })).toBe("starting");
    expect(liveGateKind({ ...owner, vmStatus: "queued_for_capacity" })).toBe(
      "queued",
    );
    expect(liveGateKind({ ...owner, vmStatus: "stopping" })).toBe("stopping");
    expect(liveGateKind({ ...owner, vmStatus: null })).toBe("unknown");
  });

  it("offers Start only for stopped and failed", () => {
    expect(liveGateCanStart("stopped")).toBe(true);
    expect(liveGateCanStart("failed")).toBe(true);
    for (const kind of [
      "live",
      "starting",
      "queued",
      "stopping",
      "unknown",
      "reader",
      "base",
      "read_only",
      "freezing",
    ] as const) {
      expect(liveGateCanStart(kind)).toBe(false);
    }
  });

  it("separates waiting states from permanent refusals", () => {
    expect(liveGateIsWaiting("stopped")).toBe(true);
    expect(liveGateIsWaiting("starting")).toBe(true);
    expect(liveGateIsWaiting("reader")).toBe(false);
    expect(liveGateIsWaiting("base")).toBe(false);
    expect(liveGateIsWaiting("live")).toBe(false);
  });
});

describe("reviewIdentityKnownFrom (g5.governance.review-workspace.exclusions)", () => {
  it("knows the identity once the review context answers", () => {
    expect(
      reviewIdentityKnownFrom({
        reviewContextStatus: "ok",
        listStatus: "loading",
        listHasCurrent: false,
      }),
    ).toBe(true);
  });

  it("trusts the list only when it names the current workspace", () => {
    expect(
      reviewIdentityKnownFrom({
        reviewContextStatus: "loading",
        listStatus: "ok",
        listHasCurrent: true,
      }),
    ).toBe(true);
    // A cached list from before the copy was created does not name it.
    expect(
      reviewIdentityKnownFrom({
        reviewContextStatus: "loading",
        listStatus: "ok",
        listHasCurrent: false,
      }),
    ).toBe(false);
    expect(
      reviewIdentityKnownFrom({
        reviewContextStatus: "error",
        listStatus: "error",
        listHasCurrent: false,
      }),
    ).toBe(false);
  });
});

describe("liveGateReason (g5.workspace.ui.vm-state)", () => {
  it("is empty when live", () => {
    expect(liveGateReason("live", "save")).toBeNull();
  });

  it("names the state and the verb", () => {
    expect(liveGateReason("stopped", "send messages")).toBe(
      "Start the workspace to send messages.",
    );
    expect(liveGateReason("failed", "save")).toBe(
      "The workspace failed to start. Start it again to save.",
    );
    expect(liveGateReason("starting", "save")).toBe(
      "The workspace is starting; you can save once it is running.",
    );
    expect(liveGateReason("queued", "save")).toBe(
      "The workspace is waiting for capacity; you can save once it is running.",
    );
    expect(liveGateReason("stopping", "save")).toBe(
      "The workspace is stopping. Start it again to save.",
    );
    expect(liveGateReason("reader", "send messages")).toBe(
      "Only the owner of a running workspace can send messages.",
    );
    expect(liveGateReason("read_only", "save")).toContain("read-only");
  });

  it("names a review copy the way its owner reads it", () => {
    expect(
      liveGateReason("stopped", "update", {
        workspaceNoun: "your review workspace",
      }),
    ).toBe("Start your review workspace to update.");
  });
});

describe("liveGateBanner", () => {
  it("keeps the reader and Base text and names each owner state", () => {
    expect(liveGateBanner("live")).toBeNull();
    expect(liveGateBanner("reader")?.message).toBe(
      "Read-only workspace. Fork workspace to make edits.",
    );
    expect(liveGateBanner("base")?.message).toBe(
      "Read-only workspace. Fork workspace to make edits.",
    );
    expect(liveGateBanner("stopped")).toEqual({
      message:
        "Workspace is stopped. Start it to edit; durable files remain available for browsing.",
      action: "start",
    });
    expect(liveGateBanner("failed")?.action).toBe("start");
    expect(liveGateBanner("starting")).toEqual({
      message: "Workspace is starting…",
      action: "stop",
    });
    expect(
      liveGateBanner("queued", { waitReason: "no host has a free slot" })
        ?.message,
    ).toBe("Workspace is waiting for capacity: no host has a free slot");
    expect(liveGateBanner("stopping")?.action).toBeNull();
    expect(liveGateBanner("read_only")).toEqual({
      message: "Read-only workspace. Fork workspace to make edits.",
      action: null,
    });
  });
});

function gate(overrides: Partial<WorkspaceLiveGate>): WorkspaceLiveGate {
  return {
    kind: "stopped",
    canWrite: true,
    live: false,
    canStart: true,
    reviewCopy: false,
    owner: true,
    reviewIdentityKnown: true,
    workspaceNoun: "the workspace",
    ...overrides,
  };
}

describe("StartWorkspaceHint (g5.workspace.ui.unavailable)", () => {
  it("renders nothing without a gate or when live", () => {
    const { container } = render(<StartWorkspaceHint verb="save" />);
    expect(container).toBeEmptyDOMElement();
    const live = render(
      <WorkspaceLiveGateProvider value={gate({ kind: "live", live: true })}>
        <StartWorkspaceHint verb="save" />
      </WorkspaceLiveGateProvider>,
    );
    expect(live.container).toBeEmptyDOMElement();
  });

  it("shows the reason and asks the header to Start", () => {
    const onStart = vi.fn();
    window.addEventListener(WORKSPACE_START_EVENT, onStart);
    render(
      <WorkspaceLiveGateProvider value={gate({})}>
        <StartWorkspaceHint verb="save" />
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.getByTestId("start-workspace-hint")).toHaveTextContent(
      "Start the workspace to save.",
    );
    fireEvent.click(screen.getByTestId("start-workspace-hint-start"));
    expect(onStart).toHaveBeenCalledTimes(1);
    window.removeEventListener(WORKSPACE_START_EVENT, onStart);
  });

  it("offers no Start while the workspace is starting", () => {
    render(
      <WorkspaceLiveGateProvider
        value={gate({ kind: "starting", canStart: false })}
      >
        <StartWorkspaceHint verb="save" />
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.getByTestId("start-workspace-hint")).toHaveTextContent(
      "starting",
    );
    expect(screen.queryByTestId("start-workspace-hint-start")).toBeNull();
  });
});

describe("GatedControl (g5.workspace.ui.unavailable)", () => {
  it("renders the control unwrapped by interaction when it is available", () => {
    render(
      <WorkspaceLiveGateProvider value={gate({ kind: "live", live: true })}>
        <GatedControl verb="save" testId="save-hint">
          <button type="button" data-testid="save">
            Save
          </button>
        </GatedControl>
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.queryByTestId("save-hint-trigger")).toBeNull();
    fireEvent.click(screen.getByTestId("save"));
    expect(screen.queryByTestId("save-hint")).toBeNull();
  });

  it("shows nothing until the disabled control is tried, then the reason and the header's Start", () => {
    const onStart = vi.fn();
    window.addEventListener(WORKSPACE_START_EVENT, onStart);
    render(
      <WorkspaceLiveGateProvider value={gate({})}>
        <GatedControl verb="save" testId="save-hint">
          <button type="button" data-testid="save" disabled>
            Save
          </button>
        </GatedControl>
      </WorkspaceLiveGateProvider>,
    );
    expect(screen.queryByTestId("save-hint")).toBeNull();
    fireEvent.click(screen.getByTestId("save-hint-trigger"));
    expect(screen.getByTestId("save-hint")).toHaveTextContent(
      "Start the workspace to save.",
    );
    fireEvent.click(screen.getByTestId("save-hint-start"));
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("save-hint")).toBeNull();
    window.removeEventListener(WORKSPACE_START_EVENT, onStart);
  });

  it("opens from the keyboard and closes on Escape or a click elsewhere", () => {
    render(
      <WorkspaceLiveGateProvider value={gate({})}>
        <GatedControl verb="save" testId="save-hint">
          <button type="button" data-testid="save" disabled>
            Save
          </button>
        </GatedControl>
      </WorkspaceLiveGateProvider>,
    );
    const trigger = screen.getByTestId("save-hint-trigger");
    fireEvent.keyDown(trigger, { key: "Enter" });
    expect(screen.getByTestId("save-hint")).toBeVisible();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByTestId("save-hint")).toBeNull();
    fireEvent.click(trigger);
    expect(screen.getByTestId("save-hint")).toBeVisible();
    fireEvent.pointerDown(document.body);
    expect(screen.queryByTestId("save-hint")).toBeNull();
  });

  it("lets a working control inside the wrapper keep its click", () => {
    const onCancel = vi.fn();
    render(
      <WorkspaceLiveGateProvider value={gate({})}>
        <GatedControl verb="answer" testId="answer-hint">
          <button type="button" data-testid="answer" disabled>
            Answer
          </button>
          <button type="button" data-testid="cancel" onClick={onCancel}>
            Cancel
          </button>
        </GatedControl>
      </WorkspaceLiveGateProvider>,
    );
    fireEvent.click(screen.getByTestId("cancel"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("answer-hint")).toBeNull();
  });

  it("offers no Start for a state that is not startable, and takes a supplied reason", () => {
    render(
      <WorkspaceLiveGateProvider
        value={gate({ kind: "starting", canStart: false })}
      >
        <GatedControl
          verb="save"
          reason="Saving waits for the workspace."
          testId="save-hint"
        >
          <button type="button" data-testid="save" disabled>
            Save
          </button>
        </GatedControl>
      </WorkspaceLiveGateProvider>,
    );
    fireEvent.click(screen.getByTestId("save-hint-trigger"));
    expect(screen.getByTestId("save-hint")).toHaveTextContent(
      "Saving waits for the workspace.",
    );
    expect(screen.queryByTestId("save-hint-start")).toBeNull();
  });
});

describe("makeRemoteOffered (g5.workspace.ui.reader-publication)", () => {
  const gate = (over: Partial<WorkspaceLiveGate>): WorkspaceLiveGate => ({
    kind: "live",
    canWrite: true,
    live: true,
    canStart: false,
    reviewCopy: false,
    reviewIdentityKnown: true,
    owner: true,
    orgAdmin: false,
    workspaceNoun: "the workspace",
    ...over,
  });
  const base = { kind: "base" as const, canWrite: false, live: false };

  it("offers Make Remote off Base only to an owner who is an administrator", () => {
    // An owner who is not an organization administrator cannot (the #1096 rule).
    expect(makeRemoteOffered(gate({}))).toBe(false);
    expect(makeRemoteOffered(gate({ orgAdmin: true }))).toBe(true);
    // Durable authority, not the VM: a stopped owner-administrator keeps it.
    expect(
      makeRemoteOffered(gate({ kind: "stopped", live: false, orgAdmin: true })),
    ).toBe(true);
    expect(
      makeRemoteOffered(
        gate({ kind: "read_only", canWrite: false, orgAdmin: true }),
      ),
    ).toBe(false);
    // An administrator who does not own an ordinary workspace cannot.
    expect(
      makeRemoteOffered(
        gate({ kind: "reader", canWrite: false, owner: false, orgAdmin: true }),
      ),
    ).toBe(false);
  });

  it("offers it on Base to any organization administrator, as its owner", () => {
    // Base's owner is an administrator.
    expect(makeRemoteOffered(gate({ ...base, orgAdmin: true }))).toBe(true);
    // A non-owner administrator on Base (base-admin).
    expect(
      makeRemoteOffered(gate({ ...base, owner: false, orgAdmin: true })),
    ).toBe(true);
    // A non-administrator never, even Base's owner.
    expect(makeRemoteOffered(gate({ ...base, orgAdmin: false }))).toBe(false);
    expect(
      makeRemoteOffered(gate({ ...base, owner: false, orgAdmin: false })),
    ).toBe(false);
  });

  it("never offers it on a review copy or before the page knows", () => {
    expect(makeRemoteOffered(gate({ reviewCopy: true }))).toBe(false);
    expect(makeRemoteOffered(gate({ reviewIdentityKnown: false }))).toBe(false);
  });
});
