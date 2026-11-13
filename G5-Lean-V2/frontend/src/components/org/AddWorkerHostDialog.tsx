// =============================================================================
// G5 Client — AddWorkerHostDialog
// =============================================================================
// Compact modal for provisioning a new org-scoped worker host. Opened from the
// WorkerHostsPage "[ + Add worker ]" control. Renders through the shared Modal
// shell (overlay, Escape/click-outside dismissal, focus trap) and provisions via
// the useProvisionWorkerHost mutation hook.
//
// SECURITY: the 201 response carries a one-time `hostJwt` host credential. The
// data layer (useProvisionWorkerHost) strips it before it ever reaches this
// component, and this dialog NEVER renders or persists any credential.
//
// @module org/AddWorkerHostDialog
// =============================================================================

import { useState, useEffect, useRef, useCallback } from "react";
import { Loader2, AlertCircle, Server, X } from "lucide-react";

import { Button, Modal } from "../ui/UI";
import { useProvisionWorkerHost } from "../../hooks/useWorkerHosts";
import {
  WORKER_HOST_REGIONS,
  WORKER_HOST_INSTANCE_TYPES,
  DEFAULT_WORKER_HOST_INSTANCE_TYPE,
} from "../../lib/types";
import type {
  ProvisionWorkerHostResult,
  WorkerHostBootDiskSizing,
} from "../../lib/types";

/* ------------------------------------------------------------------ */
/*  Props                                                              */
/* ------------------------------------------------------------------ */

export interface AddWorkerHostDialogProps {
  /** Whether the dialog is open */
  open: boolean;
  /** Called to close the dialog */
  onClose: () => void;
  /** Org ID to provision the worker host for */
  orgId: string;
  /** Called when the host is successfully provisioned (hostJwt already stripped) */
  onProvisioned?: (result: ProvisionWorkerHostResult) => void;
  /**
   * The org's own default base image ref (`orgDefaultBaseImageRef` from the
   * worker-hosts list envelope). When present, the base-image input opens
   * prefilled with it and remains fully editable. A blank submit resolves the
   * current org policy server-side or fails preflight when none exists.
   */
  defaultBaseImageRef?: string;
  /**
   * The server's boot-disk sizing coefficients (`bootDiskSizing` from the
   * worker-hosts list envelope). The disk-size field derives its placeholder,
   * hint and bounds from these — the server owns the sizing policy
   * (ontology `g5.runtime.placement.host-disk`). Absent against an older API:
   * the field then shows no derived default and leaves validation to the 422.
   */
  bootDiskSizing?: WorkerHostBootDiskSizing;
}

/**
 * Friendly image name for helper text: the path basename before the digest,
 * e.g. "…/g5/g5d-pairsoft-preview@sha256:…" → "g5d-pairsoft-preview".
 * Null when the ref has no digest separator (defensive; refs are digest-pinned).
 */
function imageNameFromRef(ref: string): string | null {
  const path = ref.split("@")[0];
  const name = path.split("/").pop();
  return name && name.length > 0 ? name : null;
}

/* ------------------------------------------------------------------ */
/*  Default region                                                     */
/* ------------------------------------------------------------------ */

/** Default cloud region pre-selected when the dialog opens. */
const DEFAULT_WORKER_HOST_REGION = "us-central1";

/**
 * Digest-pinned base-image grammar: `<registry>/<image>@sha256:<64 lowercase hex>`.
 * Used ONLY for a non-blocking client-side hint — the backend is authoritative
 * and rejects tag-only/non-digest refs with a `422`.
 */
const DIGEST_PINNED_RE = /^[^\s@]+\/[^\s@]+@sha256:[0-9a-f]{64}$/;

/**
 * Returns a soft hint for a non-digest-pinned base image ref, or null when the
 * input is blank (no opinion) or already digest-pinned. NEVER blocks submit.
 */
function baseImageRefHint(raw: string): string | null {
  const v = raw.trim();
  if (!v) return null;
  return DIGEST_PINNED_RE.test(v)
    ? null
    : "Must be digest-pinned: <registry>/<image>@sha256:<64 hex>. The server rejects tag-only refs.";
}

/**
 * vCPU count of an allowlisted instance type — the trailing hyphen-delimited
 * segment (mirrors the backend `parse_vcpus_from_instance_type`). Returns null
 * on an unexpected shape. Exported for the host-details Concurrency read-back
 * (ontology [D30]), which must derive the same default the dialog shows.
 */
export function vcpusOf(instanceType: string): number | null {
  const tail = instanceType.split("-").pop();
  const n = tail ? Number.parseInt(tail, 10) : NaN;
  return Number.isInteger(n) && n > 0 ? n : null;
}

/**
 * Server-side default budget when the field is left blank — mirrors the
 * backend `default_workspace_vm_limit` (ontology `provisioning_api` [D54]):
 * vCPUs/4 (floor 1) targets ≈16 GB RAM per VM on the 4 GB/vCPU SKUs.
 * Exported for the host-details Concurrency read-back (ontology [D30]).
 */
export function defaultWorkspaceVmLimit(vcpus: number): number {
  return Math.max(1, Math.floor(vcpus / 4));
}

/**
 * Advisory per-VM RAM estimate (GB) for the workspace-vm-limit hint, mirroring the
 * host reserve formula (`g5/g5d/host` [D23]) on the fixed 4 GB/vCPU allowlisted
 * SKUs. The host's live hardware read is authoritative; this is a `≈` guide.
 */
function perVmRamGb(vcpus: number, vmLimit: number): number {
  const totalMib = vcpus * 4096;
  const reserveMib = Math.min(
    Math.max(1536, Math.floor(totalMib / 10)),
    Math.floor(totalMib / 2),
  );
  return (totalMib - reserveMib) / vmLimit / 1024;
}

/**
 * The size the server would derive for `slots` slots, from the coefficients it
 * published. Mirrors `WorkerHostBootDiskSizing::default_gb`, but using only
 * server-supplied numbers — the rule's constants live in Rust alone.
 */
function derivedBootDiskGb(
  sizing: WorkerHostBootDiskSizing,
  slots: number,
): number {
  return sizing.baseGb + sizing.perSlotGb * slots;
}

/* ------------------------------------------------------------------ */
/*  Component                                                          */
/* ------------------------------------------------------------------ */

export function AddWorkerHostDialog({
  open,
  onClose,
  orgId,
  onProvisioned,
  defaultBaseImageRef,
  bootDiskSizing,
}: AddWorkerHostDialogProps) {
  const [region, setRegion] = useState<string>(DEFAULT_WORKER_HOST_REGION);
  const [instanceType, setInstanceType] = useState<string>(
    DEFAULT_WORKER_HOST_INSTANCE_TYPE,
  );
  const [baseImageRef, setBaseImageRef] = useState<string>("");
  // Blank ⇒ omit from the POST body; the server resolves the RAM-targeted
  // default (≈16 GB/VM, [D54]). Kept as a string so the field can be empty
  // vs. a real 0.
  const [workspaceVmLimit, setWorkspaceVmLimit] = useState<string>("");
  // Blank ⇒ omit from the POST body; the server derives the size from the
  // host's VM budget and stores nothing (g5.runtime.placement.host-disk).
  const [bootDiskGb, setBootDiskGb] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const provisionHost = useProvisionWorkerHost(orgId);
  const regionSelectRef = useRef<HTMLSelectElement>(null);

  // Snapshot the org default through a ref so the open-reset effect stays
  // keyed to `open` alone: a mid-open list refetch changing the default must
  // not re-run the reset and clobber in-progress edits.
  const defaultBaseImageRefLatest = useRef(defaultBaseImageRef);
  defaultBaseImageRefLatest.current = defaultBaseImageRef;

  /* ---- reset state when dialog opens ---- */
  useEffect(() => {
    if (open) {
      setRegion(DEFAULT_WORKER_HOST_REGION);
      setInstanceType(DEFAULT_WORKER_HOST_INSTANCE_TYPE);
      setBaseImageRef(defaultBaseImageRefLatest.current ?? "");
      setWorkspaceVmLimit("");
      setBootDiskGb("");
      setError(null);
      setIsSubmitting(false);
    }
  }, [open]);

  /* ---- submit handler ---- */
  // vCPU count of the selected instance type — computed once and reused by the
  // workspace-vm-limit input bounds, placeholder, and per-VM RAM hint below.
  const instanceVcpus = vcpusOf(instanceType);
  // The slot count the disk default follows: the typed workspace-vm-limit value
  // when valid, else the same default the server would resolve. The disk and
  // the VM budget are sized from one number, as they are server-side.
  const resolvedVmLimit = (() => {
    if (instanceVcpus === null) return null;
    const typed = workspaceVmLimit.trim();
    if (!typed) return defaultWorkspaceVmLimit(instanceVcpus);
    const n = Number(typed);
    return Number.isInteger(n) && n >= 1 && n <= instanceVcpus ? n : null;
  })();


  const handleSubmit = useCallback(async () => {
    const trimmedRegion = region.trim();
    if (!trimmedRegion) {
      setError("Please choose a region.");
      return;
    }

    // Workspace VM limit: blank ⇒ omit (server resolves the ≈16 GB/VM default,
    // [D54], saved with the host allocation). When
    // present it must be a positive integer within 1..=vCPUs(instanceType); the
    // backend is authoritative (422) but we block an obviously-bad value here.
    const trimmedVmLimit = workspaceVmLimit.trim();
    let workspaceVmLimitValue: number | undefined;
    if (trimmedVmLimit) {
      const n = Number(trimmedVmLimit);
      const maxVms = vcpusOf(instanceType);
      if (!Number.isInteger(n) || n < 1 || (maxVms !== null && n > maxVms)) {
        setError(
          maxVms !== null
            ? `Workspace VM limit must be a whole number between 1 and ${maxVms} (the instance type's vCPUs).`
            : "Workspace VM limit must be a positive whole number.",
        );
        return;
      }
      workspaceVmLimitValue = n;
    }

    // Disk size: blank ⇒ omit (the server derives it from the budget). When
    // present it must be a whole number inside the published bounds; the
    // backend is authoritative (422) but an obviously-bad value is blocked
    // here. With no published coefficients there is nothing to check against,
    // so the value goes to the server and the 422 governs.
    const minimumDisk =
      bootDiskSizing && resolvedVmLimit !== null
        ? derivedBootDiskGb(bootDiskSizing, resolvedVmLimit)
        : bootDiskSizing?.minGb;
    if (
      bootDiskSizing &&
      minimumDisk !== undefined &&
      minimumDisk > bootDiskSizing.maxGb
    ) {
      setError(
        `This configuration requires ${minimumDisk} GiB, above the supported ${bootDiskSizing.maxGb} GiB limit. Reduce the slot count or organization disk allowances.`,
      );
      return;
    }
    const trimmedDisk = bootDiskGb.trim();
    let bootDiskGbValue: number | undefined;
    if (trimmedDisk) {
      const n = Number(trimmedDisk);
      if (
        !Number.isInteger(n) ||
        // A non-positive size is nonsense at any configuration, so it is
        // rejected even with no published coefficients to range-check against.
        n < 1 ||
        (bootDiskSizing &&
          !(
            n >= (minimumDisk ?? bootDiskSizing.minGb) &&
            n <= bootDiskSizing.maxGb
          ))
      ) {
        setError(
          bootDiskSizing
            ? `Disk size must be a whole number between ${minimumDisk} and ${bootDiskSizing.maxGb} GiB.`
            : "Disk size must be a positive whole number of GiB.",
        );
        return;
      }
      bootDiskGbValue = n;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      // The mutation result has `hostJwt` stripped by the data layer; nothing
      // here reads or stores a credential. `baseImageRef` (prefilled from the
      // org default, editable) and `workspaceVmLimit` are omitted when blank so
      // the server resolves the org image policy / RAM-targeted concurrency
      // defaults. A non-digest-pinned ref is not blocked here — the backend is
      // authoritative and 422s it. `workerProfile` is intentionally not sent
      // (unused today; server defaults fat-default).
      const trimmedRef = baseImageRef.trim();
      const result = await provisionHost.mutateAsync({
        region: trimmedRegion,
        instanceType,
        ...(trimmedRef ? { baseImageRef: trimmedRef } : {}),
        ...(workspaceVmLimitValue !== undefined
          ? { workspaceVmLimit: workspaceVmLimitValue }
          : {}),
        ...(bootDiskGbValue !== undefined
          ? { bootDiskGb: bootDiskGbValue }
          : {}),
      });
      onProvisioned?.(result);
      onClose();
    } catch (err: unknown) {
      const status =
        typeof err === "object" && err !== null && "status" in err
          ? (err as { status?: number }).status
          : undefined;
      if (status === 403) {
        setError(
          "You don't have permission to provision worker hosts for this organization.",
        );
      } else if (status === 422) {
        setError(
          "Invalid region, instance type, workspace VM limit, disk size, or base image ref.",
        );
      } else {
        setError(
          err instanceof Error
            ? err.message
            : "Failed to provision worker host. Please try again.",
        );
      }
    } finally {
      setIsSubmitting(false);
    }
  }, [
    region,
    instanceType,
    baseImageRef,
    workspaceVmLimit,
    bootDiskGb,
    bootDiskSizing,
    resolvedVmLimit,
    provisionHost,
    onProvisioned,
    onClose,
  ]);

  /* ---- render ---- */
  if (!open) return null;


  return (
    <Modal
      onClose={onClose}
      ariaLabel="Add worker host"
      size="sm"
      closeDisabled={isSubmitting}
      initialFocusRef={regionSelectRef}
      className="p-6"
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-[var(--g5-text0)] flex items-center gap-2">
          <Server className="w-5 h-5" />
          Add worker host
        </h2>
        <button
          data-testid="add-worker-host-close"
          onClick={onClose}
          disabled={isSubmitting}
          className="p-1 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] disabled:opacity-50"
          title="Close"
          aria-label="Close dialog"
        >
          <X className="w-5 h-5" />
        </button>
      </div>

      <div className="space-y-4">
        {/* Region selector */}
        <div>
          <label
            htmlFor="worker-host-region"
            className="block text-sm text-[var(--g5-text1)] mb-1"
          >
            Region
          </label>
          <select
            data-testid="add-worker-host-region-select"
            ref={regionSelectRef}
            id="worker-host-region"
            value={region}
            onChange={(e) => {
              setRegion(e.target.value);
              if (error) setError(null);
            }}
            className="
                w-full px-3 py-2
                bg-[var(--g5-bg0)] text-[var(--g5-text0)]
                border border-[var(--g5-stroke)]
                rounded-[var(--g5-radius-sm)]
                focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]
              "
          >
            {WORKER_HOST_REGIONS.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </div>

        {/* Instance type selector */}
        <div>
          <label
            htmlFor="worker-host-instance-type"
            className="block text-sm text-[var(--g5-text1)] mb-1"
          >
            Instance type
          </label>
          <select
            data-testid="add-worker-host-instance-type-select"
            id="worker-host-instance-type"
            value={instanceType}
            onChange={(e) => {
              setInstanceType(e.target.value);
              if (error) setError(null);
            }}
            className="
                w-full px-3 py-2
                bg-[var(--g5-bg0)] text-[var(--g5-text0)]
                border border-[var(--g5-stroke)]
                rounded-[var(--g5-radius-sm)]
                focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]
              "
          >
            {WORKER_HOST_INSTANCE_TYPES.map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </div>

        {/* Workspace VM limit (optional; host VM budget → per-VM sizing) */}
        <div>
          <label
            htmlFor="worker-host-workspace-vm-limit"
            className="block text-sm text-[var(--g5-text1)] mb-1"
          >
            Workspace VM limit (optional)
          </label>
          <input
            id="worker-host-workspace-vm-limit"
            type="number"
            min={1}
            max={instanceVcpus ?? undefined}
            step={1}
            value={workspaceVmLimit}
            placeholder={
              instanceVcpus !== null
                ? String(defaultWorkspaceVmLimit(instanceVcpus))
                : undefined
            }
            onChange={(e) => {
              setWorkspaceVmLimit(e.target.value);
              if (error) setError(null);
            }}
            className="
                w-full px-3 py-2
                bg-[var(--g5-bg0)] text-[var(--g5-text0)]
                border border-[var(--g5-stroke)]
                rounded-[var(--g5-radius-sm)]
                focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]
              "
          />
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            {(() => {
              const vcpus = instanceVcpus;
              if (vcpus === null) {
                return "Leave blank for the server default.";
              }
              const isBlank = !workspaceVmLimit.trim();
              const vmLimit = isBlank
                ? defaultWorkspaceVmLimit(vcpus)
                : Number(workspaceVmLimit);
              const validVmLimit =
                Number.isInteger(vmLimit) && vmLimit >= 1 && vmLimit <= vcpus;
              const ram = validVmLimit ? perVmRamGb(vcpus, vmLimit) : null;
              // The default callout renders only while blank: beside a RAM
              // figure computed from a typed value it reads as contradictory.
              return (
                <>
                  {isBlank && `Defaults to ${defaultWorkspaceVmLimit(vcpus)}. `}
                  {ram !== null && `≈${ram.toFixed(1)} GB RAM per VM.`}
                </>
              );
            })()}
          </p>
        </div>

        {/* Disk size (optional; the host's shared boot disk) */}
        <div>
          <label
            htmlFor="worker-host-boot-disk-gb"
            className="block text-sm text-[var(--g5-text1)] mb-1"
          >
            Disk size in GiB (optional)
          </label>
          <input
            id="worker-host-boot-disk-gb"
            data-testid="add-worker-host-boot-disk-input"
            type="number"
            min={
              bootDiskSizing && resolvedVmLimit !== null
                ? derivedBootDiskGb(bootDiskSizing, resolvedVmLimit)
                : undefined
            }
            max={bootDiskSizing?.maxGb}
            step={1}
            value={bootDiskGb}
            placeholder={
              bootDiskSizing && resolvedVmLimit !== null
                ? String(derivedBootDiskGb(bootDiskSizing, resolvedVmLimit))
                : undefined
            }
            onChange={(e) => {
              setBootDiskGb(e.target.value);
              if (error) setError(null);
            }}
            className="
                w-full px-3 py-2
                bg-[var(--g5-bg0)] text-[var(--g5-text0)]
                border border-[var(--g5-stroke)]
                rounded-[var(--g5-radius-sm)]
                focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]
              "
          />
          {(() => {
            // Every number here comes from the server's coefficients: the
            // dialog never restates the sizing rule
            // (g5.runtime.placement.host-disk).
            if (!bootDiskSizing) {
              return (
                <p className="mt-1 text-xs text-[var(--g5-text2)]">
                  Leave blank for the server default.
                </p>
              );
            }
            const typed = bootDiskGb.trim();
            if (!typed) {
              return (
                <p
                  className="mt-1 text-xs text-[var(--g5-text2)]"
                  data-testid="boot-disk-hint"
                >
                  {resolvedVmLimit !== null
                    ? `Defaults to ${derivedBootDiskGb(bootDiskSizing, resolvedVmLimit)} GiB (minimum).`
                    : "Leave blank for the server default."}
                </p>
              );
            }
            const n = Number(typed);
            const minimum =
              resolvedVmLimit === null
                ? bootDiskSizing.minGb
                : derivedBootDiskGb(bootDiskSizing, resolvedVmLimit);
            return Number.isInteger(n) && n < minimum ? (
              <p
                className="mt-1 text-xs text-[var(--g5-warn)]"
                data-testid="boot-disk-hint"
              >
                Requires at least {minimum} GiB for this host's VM slots.
              </p>
            ) : (
              <p
                className="mt-1 text-xs text-[var(--g5-text2)]"
                data-testid="boot-disk-hint"
              >
                Shared by every VM on the host.
              </p>
            );
          })()}
        </div>

        {/* Base image ref (optional, digest-pinned) */}
        <div>
          <label
            htmlFor="worker-host-base-image-ref"
            className="block text-sm text-[var(--g5-text1)] mb-1"
          >
            Base image ref (optional)
          </label>
          <input
            id="worker-host-base-image-ref"
            type="text"
            value={baseImageRef}
            placeholder="registry.example.com/img@sha256:…"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            onChange={(e) => {
              setBaseImageRef(e.target.value);
              if (error) setError(null);
            }}
            className="
                w-full px-3 py-2
                bg-[var(--g5-bg0)] text-[var(--g5-text0)]
                border border-[var(--g5-stroke)]
                rounded-[var(--g5-radius-sm)]
                font-mono text-xs
                focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]
              "
          />
          {baseImageRefHint(baseImageRef) && (
            <p
              className="mt-1 text-xs text-[var(--g5-warn)]"
              data-testid="base-image-hint"
            >
              {baseImageRefHint(baseImageRef)}
            </p>
          )}
          {defaultBaseImageRef ? (
            <p
              className="mt-1 text-xs text-[var(--g5-text2)]"
              data-testid="base-image-org-default-note"
            >
              Org default
              {imageNameFromRef(defaultBaseImageRef) && (
                <>
                  {" "}
                  <code>{imageNameFromRef(defaultBaseImageRef)}</code>
                </>
              )}{" "}
              — edit to override; a blank field also uses the org policy.
            </p>
          ) : (
            <p className="mt-1 text-xs text-[var(--g5-text2)]">
              Leave blank to use the organization image policy.
            </p>
          )}
        </div>

        {/* Error message */}
        {error && (
          <div className="flex items-center gap-2 text-sm text-[var(--g5-blocked)]">
            <AlertCircle className="w-4 h-4 flex-shrink-0" />
            {error}
          </div>
        )}

        {/* Action buttons */}
        <div className="flex justify-end gap-2 pt-2">
          <Button
            data-testid="add-worker-host-cancel"
            kind="secondary"
            onClick={onClose}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
          <Button
            data-testid="add-worker-host-submit"
            kind="primary"
            disabled={isSubmitting || !region.trim()}
            onClick={handleSubmit}
          >
            {isSubmitting ? (
              <span className="flex items-center gap-2">
                <Loader2 className="w-4 h-4 animate-spin" />
                Provisioning…
              </span>
            ) : (
              "Provision host"
            )}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
