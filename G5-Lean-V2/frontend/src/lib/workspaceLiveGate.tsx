import React, {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { WorkspaceDetail, WorkspaceVmStatus } from "./types";
import { isPersonalWorkspace } from "./workspaceKind";
import { requestWorkspaceStart } from "../components/workspace/reviewWorkspace";
import { classNames } from "./utils";

/**
 * Why a live-attachment action is not available right now
 * (`g5.workspace.ui.gates`, `g5.workspace.ui.vm-state`):
 *
 * - `live`: the owner's VM is running and the workspace accepts writes.
 * - `base`, `read_only`, `freezing`, `reader`: the viewer can never take the
 *   action here; such controls are omitted, not disabled.
 * - the VM states: the owner could take the action once the VM runs; such
 *   controls stay visible, disabled, with the reason and a Start in reach
 *   (`g5.workspace.ui.unavailable`).
 * - `unknown`: the VM state has not been read yet.
 */
export type LiveGateKind =
  | "live"
  | "base"
  | "read_only"
  | "freezing"
  | "reader"
  | "stopped"
  | "failed"
  | "starting"
  | "queued"
  | "stopping"
  | "unknown";

export interface LiveGateInput {
  isOwner: boolean;
  isBase: boolean;
  readOnly: boolean;
  freezing: boolean;
  /** `null` while the VM state has not been read. */
  vmStatus: WorkspaceVmStatus | string | null;
}

/** The gate kind for a workspace, in the order the server would refuse. */
export function liveGateKind(input: LiveGateInput): LiveGateKind {
  if (input.isBase) return "base";
  if (!input.isOwner) return "reader";
  if (input.readOnly) return "read_only";
  if (input.freezing) return "freezing";
  switch (input.vmStatus) {
    case "running":
      return "live";
    case "stopped":
      return "stopped";
    case "failed":
      return "failed";
    case "starting":
      return "starting";
    case "queued_for_capacity":
      return "queued";
    case "stopping":
      return "stopping";
    default:
      return "unknown";
  }
}

/**
 * Whether the page knows yet if the open workspace is a review copy
 * (`g5.governance.review-workspace.exclusions`): its review context has
 * answered, or the workspace list has answered and names it. A cached list
 * that does not name it yet (right after a copy is created) does not count.
 */
export function reviewIdentityKnownFrom(input: {
  reviewContextStatus: "loading" | "error" | "ok";
  listStatus: "loading" | "error" | "ok";
  listHasCurrent: boolean;
}): boolean {
  return (
    input.reviewContextStatus === "ok" ||
    (input.listStatus === "ok" && input.listHasCurrent)
  );
}

/** Whether Start is the valid lifecycle action for this kind. */
export function liveGateCanStart(kind: LiveGateKind): boolean {
  return kind === "stopped" || kind === "failed";
}

/** Whether the kind is a VM state the owner is waiting on, not a refusal. */
export function liveGateIsWaiting(kind: LiveGateKind): boolean {
  return (
    kind === "stopped" ||
    kind === "failed" ||
    kind === "starting" ||
    kind === "queued" ||
    kind === "stopping" ||
    kind === "unknown"
  );
}

/**
 * One sentence saying why `verb` (an infinitive phrase such as "send
 * messages" or "save") cannot happen now; `null` when it can.
 */
export function liveGateReason(
  kind: LiveGateKind,
  verb: string,
  options?: { workspaceNoun?: string },
): string | null {
  const noun = options?.workspaceNoun ?? "the workspace";
  switch (kind) {
    case "live":
      return null;
    case "base":
      return "Read-only workspace. Fork workspace to make edits.";
    case "reader":
      return `Only the owner of a running workspace can ${verb}.`;
    case "read_only":
      return `This workspace is read-only; make it editable in Workspace Details to ${verb}.`;
    case "freezing":
      return `This workspace is becoming read-only; wait for it to finish before you ${verb}.`;
    case "stopped":
      return `Start ${noun} to ${verb}.`;
    case "failed":
      return `${capitalize(noun)} failed to start. Start it again to ${verb}.`;
    case "starting":
      return `${capitalize(noun)} is starting; you can ${verb} once it is running.`;
    case "queued":
      return `${capitalize(noun)} is waiting for capacity; you can ${verb} once it is running.`;
    case "stopping":
      return `${capitalize(noun)} is stopping. Start it again to ${verb}.`;
    case "unknown":
      return `${capitalize(noun)}'s state is not known yet; ${verb} once it is.`;
  }
}

/** What the workspace banner says for a kind, and which action it offers. */
export function liveGateBanner(
  kind: LiveGateKind,
  options?: { waitReason?: string | null },
): { message: string; action: "start" | "stop" | null } | null {
  switch (kind) {
    case "live":
      return null;
    // Base, readers and a read-only policy keep the M25 text: the way to
    // an editable workspace from any of them is a fork (the owner of a
    // read-only workspace also has Make editable in Workspace Details).
    case "base":
    case "reader":
    case "read_only":
    case "freezing":
      return {
        message: "Read-only workspace. Fork workspace to make edits.",
        action: null,
      };
    case "stopped":
      return {
        message:
          "Workspace is stopped. Start it to edit; durable files remain available for browsing.",
        action: "start",
      };
    case "failed":
      return {
        message:
          "The workspace failed to start. Start it again to edit; durable files remain available for browsing.",
        action: "start",
      };
    case "starting":
      return { message: "Workspace is starting…", action: "stop" };
    case "queued":
      return {
        message: options?.waitReason
          ? `Workspace is waiting for capacity: ${options.waitReason}`
          : "Workspace is waiting for capacity…",
        action: "stop",
      };
    case "stopping":
      return { message: "Workspace is stopping…", action: null };
    case "unknown":
      return {
        message: "Workspace state is not known yet; editing opens once it is.",
        action: null,
      };
  }
}

function capitalize(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * The page's gate, read by leaf controls so no prop threads through the
 * page's tab machinery. `null` outside a workspace page: components then
 * behave as they did without the gate.
 */
export interface WorkspaceLiveGate {
  /** Authority generation for reads; a replaced runtime cannot reuse private cached bytes. */
  runtimeKey?: string;
  kind: LiveGateKind;
  /**
   * The kind from ownership and VM state alone, ignoring the mutation
   * policy, for the controls that need the running VM but no writable
   * policy: preview and the terminal (`g5.workspace.ui.gates`). Absent
   * (older callers), a policy kind is taken to mean the VM runs.
   */
  vmKind?: LiveGateKind;
  /** Durable authority: owner of a writable, non-Base workspace. */
  canWrite: boolean;
  /** Live attachment: `canWrite` and the VM is running. */
  live: boolean;
  /** Whether Start is the valid action now (owner, stopped or failed). */
  canStart: boolean;
  /**
   * A review copy publishes only through Push review
   * (`g5.governance.review-workspace.exclusions`); it also names itself
   * differently in reasons.
   */
  reviewCopy: boolean;
  /**
   * Whether the page has learned yet if this is a review copy (from the
   * workspace list or the review context). Until it has, the controls a
   * copy excludes are withheld rather than shown and taken away.
   */
  reviewIdentityKnown: boolean;
  /** The viewer is the current owner (Base included, where `canWrite` is false). */
  owner: boolean;
  /**
   * The viewer administers the workspace's organization. It grants only
   * what organization authority admits; see
   * {@link ownerOrBaseAdmin}.
   */
  orgAdmin?: boolean;
  workspaceNoun: string;
}

/** What a surface knows about the workspace to build its live gate. */
export interface WorkspaceLiveGateInputs {
  /** Identity of the runtime whose private reads this surface admits. */
  runtimeKey?: string;
  /** VM ownership evidence, when this surface has loaded it. */
  vmOwnerUserId?: string | null;
  /** The loaded detail of the workspace the surface shows; `null` while loading. */
  workspace: Pick<
    WorkspaceDetail,
    "isBase" | "kind" | "ownerUserId" | "readOnly" | "mutationPolicyTransition"
  > | null;
  /** The VM status; `null` while it has not been read. */
  vmStatus: WorkspaceVmStatus | string | null;
  /** The signed-in person. */
  userId: string | null | undefined;
  /** The person administers the workspace's organization. */
  orgAdmin: boolean;
  reviewCopy: boolean;
  reviewIdentityKnown: boolean;
}

/** The gate and the authority facts it was built from. */
export interface WorkspaceLiveGateValue {
  gate: WorkspaceLiveGate;
  isOwner: boolean;
  /** Owner of a workspace whose policy accepts writes (not read-only, not freezing). */
  canManageDurableOwnerState: boolean;
  /** Owner whose VM is running. */
  ownerHasRunningAttachment: boolean;
  /** Durable authority: `canManageDurableOwnerState` on a non-Base workspace. */
  writable: boolean;
  /** Live attachment: `writable` and the VM is running. */
  liveEditable: boolean;
  kind: LiveGateKind;
  vmKind: LiveGateKind;
  workspaceNoun: string;
}

/**
 * Build the live gate every control of a workspace surface reads
 * (`g5.workspace.ui.gates`). The workbench and Home's personal panel share
 * this one derivation, so the panel has no second gating path
 * (personal_workspace g5.personal.home).
 */
export function useWorkspaceLiveGateValue({
  runtimeKey,
  vmOwnerUserId,
  workspace,
  vmStatus,
  userId,
  orgAdmin,
  reviewCopy,
  reviewIdentityKnown,
}: WorkspaceLiveGateInputs): WorkspaceLiveGateValue {
  const isBase = workspace?.isBase === true;
  const isOwner = !!userId && workspace?.ownerUserId === userId;
  const readOnly = workspace?.readOnly === true;
  const freezing = workspace?.mutationPolicyTransition === "freezing";
  const personal = isPersonalWorkspace(workspace);
  const canManageDurableOwnerState = isOwner && !readOnly && !freezing;
  const isRuntimeOwner =
    isOwner && (vmOwnerUserId === undefined || vmOwnerUserId === userId);
  const ownerHasRunningAttachment = isRuntimeOwner && vmStatus === "running";
  const writable = canManageDurableOwnerState && !isBase;
  const liveEditable = writable && ownerHasRunningAttachment;
  const kind = liveGateKind({
    isOwner,
    isBase,
    readOnly,
    freezing,
    vmStatus,
  });
  // Preview and the terminal answer to ownership and the VM state only.
  const vmKind = liveGateKind({
    isOwner: isRuntimeOwner,
    isBase,
    readOnly: false,
    freezing: false,
    vmStatus,
  });
  const workspaceNoun = reviewCopy
    ? "your review workspace"
    : personal
      ? "your personal workspace"
      : "the workspace";
  const gate = useMemo<WorkspaceLiveGate>(
    () => ({
      runtimeKey,
      kind,
      vmKind,
      canWrite: writable,
      live: liveEditable,
      canStart: isOwner && liveGateCanStart(kind),
      reviewCopy,
      reviewIdentityKnown,
      owner: isOwner,
      orgAdmin,
      workspaceNoun,
    }),
    [
      isOwner,
      kind,
      liveEditable,
      orgAdmin,
      reviewCopy,
      reviewIdentityKnown,
      runtimeKey,
      vmKind,
      workspaceNoun,
      writable,
    ],
  );
  return {
    gate,
    isOwner,
    canManageDurableOwnerState,
    ownerHasRunningAttachment,
    writable,
    liveEditable,
    kind,
    vmKind,
    workspaceNoun,
  };
}

const WorkspaceLiveGateContext = createContext<WorkspaceLiveGate | null>(null);

export const WorkspaceLiveGateProvider = WorkspaceLiveGateContext.Provider;

/**
 * Whether the viewer may take the owner actions an organization
 * administrator shares on Base: pull-request refresh, close and merge
 * (`g5.governance.pr.base-admin`), Pull, auto-pull, Make Remote and Accept
 * provider history (`g5.workspace.remote.base-admin-pull`). The server
 * admits nothing else to a non-owner.
 */
export function ownerOrBaseAdmin(gate: WorkspaceLiveGate): boolean {
  return gate.owner || (gate.kind === "base" && gate.orgAdmin === true);
}

/**
 * Whether Make Remote is offered (`g5.workspace.ui.reader-publication`,
 * `g5.git.make-remote`): never on a review copy, nor before the page knows
 * whether this is one. Make Remote is an organization administrator's action, so
 * it always requires `orgAdmin`. Off Base it also needs the owner, so it is the
 * writable owner who also administers the organization (a non-admin owner and a
 * non-owner administrator are both hidden). On Base, never writable, it is any
 * organization administrator, as its owner (base-admin). Publish of an inherited
 * remote stays the owner's, in `PublishBranchMenu`. The trigger and the dialog's
 * submission read the same answer.
 */
export function makeRemoteOffered(gate: WorkspaceLiveGate): boolean {
  if (!gate.reviewIdentityKnown || gate.reviewCopy) return false;
  if (gate.orgAdmin !== true) return false;
  return gate.kind === "base" ? ownerOrBaseAdmin(gate) : gate.canWrite;
}

export function useWorkspaceLiveGate(): WorkspaceLiveGate | null {
  return useContext(WorkspaceLiveGateContext);
}

/**
 * The gate kind a policy-free control (preview, terminal) answers to: the
 * VM-state kind when the page supplies it, else the policy kinds are read
 * as a running VM (the only way they were produced before `vmKind`).
 */
export function policyFreeGateKind(gate: WorkspaceLiveGate): LiveGateKind {
  if (gate.vmKind) return gate.vmKind;
  return gate.kind === "read_only" || gate.kind === "freezing"
    ? "live"
    : gate.kind;
}

/** Whether Start is the valid action for a policy-free control now. */
export function policyFreeCanStart(gate: WorkspaceLiveGate): boolean {
  return gate.owner && liveGateCanStart(policyFreeGateKind(gate));
}

/**
 * Why the viewer cannot publish from this workspace now (create or update a
 * pull request, rebase and merge, publish a branch): a durable action that
 * needs writable authority on a workspace that is not a review copy
 * (`g5.workspace.ui.reader-publication`,
 * `g5.governance.review-workspace.exclusions`). `null` when they can, or
 * when no gate is provided. A dialog kept open through a gate change reads
 * this at submission and keeps its draft.
 */
export function publicationBlockedReason(
  gate: WorkspaceLiveGate | null,
): string | null {
  if (!gate) return null;
  if (!gate.reviewIdentityKnown) {
    return "Checking whether this is a review workspace…";
  }
  if (gate.reviewCopy) {
    return "A review workspace publishes only through Push review.";
  }
  if (gate.canWrite) return null;
  switch (gate.kind) {
    case "base":
      return "Base publishes nothing; fork it to make changes.";
    case "read_only":
      return "This workspace is read-only; make it editable in Workspace Details to publish.";
    case "freezing":
      return "This workspace is becoming read-only; wait for it to finish before you publish.";
    default:
      return "Only the owner can publish from this workspace.";
  }
}

/** `publicationBlockedReason` for the page's gate. */
export function usePublicationBlockedReason(): string | null {
  return publicationBlockedReason(useWorkspaceLiveGate());
}

/**
 * The reason `verb` cannot happen now, from the page's gate; `null` when it
 * can, or when no gate is provided. `policyFree` asks for the VM-state kind
 * only (preview, terminal).
 */
export function useLiveGateReason(
  verb: string,
  options?: { policyFree?: boolean },
): string | null {
  const gate = useWorkspaceLiveGate();
  if (!gate) return null;
  const kind = options?.policyFree ? policyFreeGateKind(gate) : gate.kind;
  return liveGateReason(kind, verb, { workspaceNoun: gate.workspaceNoun });
}

/**
 * Wraps a disabled control so that trying it (click, Enter or Space) reveals
 * why it is unavailable and, when Start is the valid action, the header's
 * Start (`g5.workspace.ui.unavailable`). Nothing shows until the viewer
 * tries the control; the reason stays on the control's `title` for hover.
 * When the action is available, or no gate is provided, the children render
 * unwrapped. `reason` overrides the gate's wording for controls whose reason
 * is computed elsewhere; `policyFree` answers to the VM state only.
 */
export function GatedControl({
  verb,
  reason: reasonOverride,
  policyFree = false,
  testId = "start-workspace-hint",
  className,
  align = "start",
  placement = "fixed",
  children,
}: {
  verb: string;
  reason?: string | null;
  policyFree?: boolean;
  testId?: string;
  className?: string;
  /**
   * Which edge of the control the popover hangs from. `start` (the default)
   * hangs it from the left edge and flips to the right edge when it would
   * leave the viewport (a dialog's or ribbon's rightmost control); `end`
   * always hangs it from the right edge.
   */
  align?: "start" | "end";
  /**
   * `fixed` (the default) positions the popover at the control's viewport
   * rectangle, so a row, panel or dialog that clips its overflow (the
   * ribbon's scrolling row, the sidebar, a modal) cannot cut it off, as the
   * ribbon's own menus do; it stays the control's DOM descendant, so test
   * ids and the outside-click boundary are unchanged, and it closes when
   * anything scrolls or the window resizes, where the taken position goes
   * stale. `absolute` keeps it in flow below the control.
   */
  placement?: "absolute" | "fixed";
  children: React.ReactNode;
}) {
  const gate = useWorkspaceLiveGate();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLSpanElement>(null);
  const kind = gate
    ? policyFree
      ? policyFreeGateKind(gate)
      : gate.kind
    : null;
  const reason =
    reasonOverride !== undefined
      ? reasonOverride
      : gate && kind
        ? liveGateReason(kind, verb, { workspaceNoun: gate.workspaceNoun })
        : null;
  const blocked = reason !== null && reason !== "";
  const canStart = gate
    ? policyFree
      ? policyFreeCanStart(gate)
      : gate.canStart
    : false;
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (rootRef.current?.contains(event.target as Node)) return;
      setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);
  useEffect(() => {
    if (!blocked) setOpen(false);
  }, [blocked]);
  const [fixedPos, setFixedPos] = useState<{
    top: number;
    left: number;
    right: number;
    fromEnd: boolean;
  } | null>(null);
  useLayoutEffect(() => {
    if (!open || placement !== "fixed") {
      setFixedPos(null);
      return;
    }
    const rect = rootRef.current?.getBoundingClientRect();
    if (!rect) return;
    // A control in the right half of the viewport (a dialog's or ribbon's
    // trailing button) hangs the popover from its right edge, so it reads
    // over the surface it belongs to instead of spilling past it; the
    // widest popover (20rem at 16px) must otherwise fit to the right.
    const fits = rect.left + 320 + 8 <= window.innerWidth;
    setFixedPos({
      top: rect.bottom + 4,
      left: Math.max(8, rect.left),
      right: Math.max(8, window.innerWidth - rect.right),
      fromEnd: align === "end" || !fits || rect.left > window.innerWidth / 2,
    });
    const close = () => setOpen(false);
    window.addEventListener("scroll", close, true);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("resize", close);
    };
  }, [align, open, placement]);
  const hintId = `${testId}-popover`;
  // The same wrapper renders in both states so a control keeps its DOM node
  // (focus, refs, test handles) when the gate changes under it.
  if (!blocked) {
    return (
      <span className={classNames("relative inline-flex", className)}>
        {children}
      </span>
    );
  }
  return (
    <span
      ref={rootRef}
      data-testid={`${testId}-trigger`}
      // A group, not a button: it holds the real (disabled) control and the
      // popover's Start, so it must not nest interactive roles or rename
      // the control. The group is named by the reason and is focusable so
      // keyboard users can reveal it, since a disabled control cannot take
      // focus itself.
      role="group"
      tabIndex={0}
      aria-label={reason}
      aria-describedby={open ? hintId : undefined}
      onClick={(event) => {
        // A working control inside the wrapper (Cancel beside a blocked
        // submit) keeps its own click.
        const target = event.target as HTMLElement;
        if (
          target !== event.currentTarget &&
          target.closest(
            "button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled)",
          )
        ) {
          return;
        }
        setOpen((current) => !current);
      }}
      onKeyDown={(event) => {
        if (event.target !== event.currentTarget) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          setOpen((current) => !current);
        }
      }}
      className={classNames(
        // The disabled control swallows no pointer events: the try lands
        // here and opens the reason.
        "relative inline-flex cursor-not-allowed [&_:disabled]:pointer-events-none",
        className,
      )}
    >
      {children}
      {open && (
        <span
          id={hintId}
          data-testid={testId}
          role="status"
          onClick={(event) => event.stopPropagation()}
          className={classNames(
            "z-30 flex min-w-[12rem] cursor-default flex-wrap items-center gap-1.5 rounded-md border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-2.5 py-2 text-left text-[11px] font-normal normal-case leading-snug tracking-normal text-[var(--g5-text1)] shadow-lg",
            fixedPos ? "fixed" : "absolute top-full mt-1",
            align === "end" ? "max-w-[16rem]" : "max-w-[20rem]",
            !fixedPos && (align === "end" ? "right-0" : "left-0"),
          )}
          style={
            fixedPos
              ? fixedPos.fromEnd
                ? { top: fixedPos.top, right: fixedPos.right }
                : { top: fixedPos.top, left: fixedPos.left }
              : undefined
          }
        >
          <span>{reason}</span>
          {canStart && (
            <button
              type="button"
              data-testid={`${testId}-start`}
              onClick={() => {
                setOpen(false);
                requestWorkspaceStart();
              }}
              title="Starts this workspace's VM, the same as Start in the header"
              className="rounded border border-[var(--g5-stroke)] px-1.5 py-0.5 text-[11px] font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
            >
              Start
            </button>
          )}
        </span>
      )}
    </span>
  );
}

/**
 * The reason a waiting action cannot happen now, with the header's Start
 * when that is the valid action, shown inline as a surface's own content
 * (the terminal panel's body). Controls use `GatedControl` instead, which
 * reveals the reason only when the control is tried
 * (`g5.workspace.ui.unavailable`). Renders nothing when the action is
 * available or the gate is absent. `policyFree` answers to the VM state
 * only (preview, terminal), so a read-only owner whose VM stopped is
 * offered Start rather than a policy change.
 */
export function StartWorkspaceHint({
  verb,
  className,
  testId = "start-workspace-hint",
  policyFree = false,
}: {
  verb: string;
  className?: string;
  testId?: string;
  policyFree?: boolean;
}) {
  const gate = useWorkspaceLiveGate();
  if (!gate) return null;
  const kind = policyFree ? policyFreeGateKind(gate) : gate.kind;
  const canStart = policyFree ? policyFreeCanStart(gate) : gate.canStart;
  const reason = liveGateReason(kind, verb, {
    workspaceNoun: gate.workspaceNoun,
  });
  if (!reason) return null;
  return (
    <span
      data-testid={testId}
      role="status"
      className={
        className ??
        "inline-flex flex-wrap items-center gap-1.5 text-[11px] text-[var(--g5-text2)]"
      }
    >
      <span>{reason}</span>
      {canStart && (
        <button
          type="button"
          data-testid={`${testId}-start`}
          onClick={requestWorkspaceStart}
          title="Starts this workspace's VM, the same as Start in the header"
          className="rounded border border-[var(--g5-stroke)] px-1.5 py-0.5 text-[11px] font-medium text-[var(--g5-text0)] hover:bg-[var(--g5-bg2)]"
        >
          Start
        </button>
      )}
    </span>
  );
}

/** Add authority to a read key before rendering, so invalidation need not wait for an effect. */
export function useWorkspaceReadAuthority(): string | undefined {
  const gate = useWorkspaceLiveGate();
  return gate
    ? JSON.stringify([gate.owner, policyFreeGateKind(gate), gate.runtimeKey])
    : undefined;
}
