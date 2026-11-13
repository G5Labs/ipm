/**
 * OrgAdminSettingsCard — the organization settings administrators edit on the
 * Admin page: the default VM idle timeouts new workspaces and new personal
 * workspaces start with (`g5.lifecycle.idle.settings`,
 * `g5.personal.capacity`), the org VM quota
 * (`g5.runtime.placement.org-quota`), the per-owner limits on running VMs and
 * owned workspaces (`g5.organization.owner-limits`) and, grouped in their own
 * box, the
 * managed-capacity controls (automatic host growth, the free-slot floor and
 * idle-host retirement, `g5.architecture.host_demand_capacity` and
 * `g5.architecture.host_idle_retirement`).
 *
 * The Admin page renders this only for admins; the API refuses
 * everyone else regardless (`g5.security.org-admin-settings`).
 *
 * @module org/OrgAdminSettingsCard
 */

import React, { useEffect, useMemo, useState } from "react";
import { Loader2 } from "lucide-react";

import { Button, classNames } from "../ui/UI";
import { ApiError } from "../../lib/api";
import {
  useOrgAdminSettings,
  useUpdateOrgAdminSettings,
} from "../../hooks/useOrg";
import {
  DEFAULT_WORKER_HOST_INSTANCE_TYPE,
  WORKER_HOST_INSTANCE_TYPES,
} from "../../lib/types";
import type { OrgAdminSettingsDTO } from "../../lib/types";

export const IDLE_TIMEOUT_MINUTES_MIN = 1;
export const IDLE_TIMEOUT_MINUTES_MAX = 1440;
export const MAX_RUNNING_VMS_MIN = 0;
export const MAX_RUNNING_VMS_MAX = 100;
export const IDLE_HOST_RETIRE_MINUTES_MIN = 15;
export const IDLE_HOST_RETIRE_MINUTES_MAX = 1440;
export const MIN_FREE_SLOTS_MIN = 0;
export const MIN_FREE_SLOTS_MAX = 100;
export const MAX_RUNNING_VMS_PER_OWNER_MIN = 0;
export const MAX_RUNNING_VMS_PER_OWNER_MAX = 100;
export const MAX_WORKSPACES_PER_OWNER_MIN = 0;
export const MAX_WORKSPACES_PER_OWNER_MAX = 1000;

/**
 * Per-owner limits an organization has before any administrator changes
 * them. Also used when the API does not yet report a field.
 */
export const OWNER_LIMIT_DEFAULTS = {
  maxRunningVmsPerOwner: 5,
  maxWorkspacesPerOwner: 50,
} as const;
/** Personal idle default before any administrator changes it (30 minutes). */
export const DEFAULT_PERSONAL_IDLE_TIMEOUT_SECS = 1800;

/**
 * Managed-capacity values an organization has before any administrator
 * changes them: grow for waiting starts with the smallest host, keep one
 * free slot ready, retire hosts empty for an hour. Also used when the API
 * does not yet report a field.
 */
export const MANAGED_CAPACITY_DEFAULTS = {
  autoAddHosts: true,
  autoHostInstanceType: DEFAULT_WORKER_HOST_INSTANCE_TYPE,
  autoRetireIdleHosts: true,
  automaticContinuationEnabled: true,
  idleHostRetireSecs: 3600,
  minFreeSlots: 1,
} as const;

type FieldErrors = Partial<Record<keyof OrgAdminSettingsDTO, string>>;

interface Draft {
  hostDiskAllowanceGib: string;
  vmDiskAllowanceGib: string;
  idleMinutes: string;
  personalIdleMinutes: string;
  maxRunningVms: string;
  maxRunningVmsPerOwner: string;
  maxWorkspacesPerOwner: string;
  autoAddHosts: boolean;
  autoHostInstanceType: string;
  autoRetireIdleHosts: boolean;
  automaticContinuationEnabled: boolean;
  idleHostRetireMinutes: string;
  minFreeSlots: string;
}

function draftFrom(settings: OrgAdminSettingsDTO): Draft {
  return {
    hostDiskAllowanceGib: String(settings.hostDiskAllowanceGib ?? 120),
    vmDiskAllowanceGib: String(settings.vmDiskAllowanceGib ?? 80),
    idleMinutes: String(Math.round(settings.defaultIdleTimeoutSecs / 60)),
    personalIdleMinutes: String(
      Math.round(
        (settings.defaultPersonalIdleTimeoutSecs ??
          DEFAULT_PERSONAL_IDLE_TIMEOUT_SECS) / 60,
      ),
    ),
    maxRunningVms: String(settings.maxRunningVms),
    maxRunningVmsPerOwner: String(
      settings.maxRunningVmsPerOwner ??
        OWNER_LIMIT_DEFAULTS.maxRunningVmsPerOwner,
    ),
    maxWorkspacesPerOwner: String(
      settings.maxWorkspacesPerOwner ??
        OWNER_LIMIT_DEFAULTS.maxWorkspacesPerOwner,
    ),
    autoAddHosts:
      settings.autoAddHosts ?? MANAGED_CAPACITY_DEFAULTS.autoAddHosts,
    autoHostInstanceType:
      settings.autoHostInstanceType ??
      MANAGED_CAPACITY_DEFAULTS.autoHostInstanceType,
    autoRetireIdleHosts:
      settings.autoRetireIdleHosts ??
      MANAGED_CAPACITY_DEFAULTS.autoRetireIdleHosts,
    automaticContinuationEnabled:
      settings.automaticContinuationEnabled ??
      MANAGED_CAPACITY_DEFAULTS.automaticContinuationEnabled,
    idleHostRetireMinutes: String(
      Math.round(
        (settings.idleHostRetireSecs ??
          MANAGED_CAPACITY_DEFAULTS.idleHostRetireSecs) / 60,
      ),
    ),
    minFreeSlots: String(
      settings.minFreeSlots ?? MANAGED_CAPACITY_DEFAULTS.minFreeSlots,
    ),
  };
}

interface Edit {
  orgId: string;
  /** Server values the draft started from. */
  base: OrgAdminSettingsDTO;
  draft: Draft;
}

const DRAFT_FIELD: Record<keyof Draft, keyof OrgAdminSettingsDTO> = {
  hostDiskAllowanceGib: "hostDiskAllowanceGib",
  vmDiskAllowanceGib: "vmDiskAllowanceGib",
  idleMinutes: "defaultIdleTimeoutSecs",
  personalIdleMinutes: "defaultPersonalIdleTimeoutSecs",
  maxRunningVms: "maxRunningVms",
  maxRunningVmsPerOwner: "maxRunningVmsPerOwner",
  maxWorkspacesPerOwner: "maxWorkspacesPerOwner",
  autoAddHosts: "autoAddHosts",
  autoHostInstanceType: "autoHostInstanceType",
  autoRetireIdleHosts: "autoRetireIdleHosts",
  automaticContinuationEnabled: "automaticContinuationEnabled",
  idleHostRetireMinutes: "idleHostRetireSecs",
  minFreeSlots: "minFreeSlots",
};

const DRAFT_KEYS = Object.keys(DRAFT_FIELD) as (keyof Draft)[];

function startEdit(orgId: string, base: OrgAdminSettingsDTO): Edit {
  return { orgId, base, draft: draftFrom(base) };
}

/**
 * Move an edit onto newer server values: fields the admin changed keep their
 * input, every other field takes the new value, so a save never sends a stale
 * value back over another administrator's change.
 */
function rebaseEdit(prev: Edit, base: OrgAdminSettingsDTO): Edit {
  const prevBase = draftFrom(prev.base);
  const draft: Draft = draftFrom(base);
  for (const key of DRAFT_KEYS) {
    if (prev.draft[key] !== prevBase[key]) {
      (draft as unknown as Record<string, unknown>)[key] = prev.draft[key];
    }
  }
  return { orgId: prev.orgId, base, draft };
}

function draftsEqual(a: Draft, b: Draft): boolean {
  return DRAFT_KEYS.every((key) => a[key] === b[key]);
}

function parseWhole(value: string, min: number, max: number): number | null {
  if (!/^\d+$/.test(value.trim())) return null;
  const parsed = Number(value.trim());
  return parsed >= min && parsed <= max ? parsed : null;
}

/**
 * Turn the draft into the changed fields, or field errors. Exported for
 * tests; the card is its only caller.
 */
export function buildAdminSettingsPatch(
  draft: Draft,
  current: OrgAdminSettingsDTO,
): { patch: Partial<OrgAdminSettingsDTO>; errors: FieldErrors } {
  const errors: FieldErrors = {};
  const patch: Partial<OrgAdminSettingsDTO> = {};
  const currentDraft = draftFrom(current);
  for (const [key, min] of [
    ["hostDiskAllowanceGib", 30],
    ["vmDiskAllowanceGib", 1],
  ] as const) {
    const value = parseWhole(draft[key], min, 2000);
    if (value === null)
      errors[key] = `Enter a whole number from ${min} to 2000 GiB.`;
    else if (String(value) !== currentDraft[key].trim()) patch[key] = value;
  }

  const minutes = parseWhole(
    draft.idleMinutes,
    IDLE_TIMEOUT_MINUTES_MIN,
    IDLE_TIMEOUT_MINUTES_MAX,
  );
  if (minutes === null) {
    errors.defaultIdleTimeoutSecs = `Enter whole minutes from ${IDLE_TIMEOUT_MINUTES_MIN} to ${IDLE_TIMEOUT_MINUTES_MAX}.`;
  } else if (minutes * 60 !== current.defaultIdleTimeoutSecs) {
    patch.defaultIdleTimeoutSecs = minutes * 60;
  }

  const personalMinutes = parseWhole(
    draft.personalIdleMinutes,
    IDLE_TIMEOUT_MINUTES_MIN,
    IDLE_TIMEOUT_MINUTES_MAX,
  );
  if (personalMinutes === null) {
    errors.defaultPersonalIdleTimeoutSecs = `Enter whole minutes from ${IDLE_TIMEOUT_MINUTES_MIN} to ${IDLE_TIMEOUT_MINUTES_MAX}.`;
  } else if (
    String(personalMinutes) !== currentDraft.personalIdleMinutes.trim()
  ) {
    patch.defaultPersonalIdleTimeoutSecs = personalMinutes * 60;
  }

  const quota = parseWhole(
    draft.maxRunningVms,
    MAX_RUNNING_VMS_MIN,
    MAX_RUNNING_VMS_MAX,
  );
  if (quota === null) {
    errors.maxRunningVms = `Enter a whole number from ${MAX_RUNNING_VMS_MIN} to ${MAX_RUNNING_VMS_MAX}.`;
  } else if (quota !== current.maxRunningVms) {
    patch.maxRunningVms = quota;
  }

  const ownerVms = parseWhole(
    draft.maxRunningVmsPerOwner,
    MAX_RUNNING_VMS_PER_OWNER_MIN,
    MAX_RUNNING_VMS_PER_OWNER_MAX,
  );
  if (ownerVms === null) {
    errors.maxRunningVmsPerOwner = `Enter a whole number from ${MAX_RUNNING_VMS_PER_OWNER_MIN} to ${MAX_RUNNING_VMS_PER_OWNER_MAX}.`;
  } else if (String(ownerVms) !== currentDraft.maxRunningVmsPerOwner.trim()) {
    patch.maxRunningVmsPerOwner = ownerVms;
  }

  const ownerWorkspaces = parseWhole(
    draft.maxWorkspacesPerOwner,
    MAX_WORKSPACES_PER_OWNER_MIN,
    MAX_WORKSPACES_PER_OWNER_MAX,
  );
  if (ownerWorkspaces === null) {
    errors.maxWorkspacesPerOwner = `Enter a whole number from ${MAX_WORKSPACES_PER_OWNER_MIN} to ${MAX_WORKSPACES_PER_OWNER_MAX}.`;
  } else if (
    String(ownerWorkspaces) !== currentDraft.maxWorkspacesPerOwner.trim()
  ) {
    patch.maxWorkspacesPerOwner = ownerWorkspaces;
  }

  if (draft.autoAddHosts !== currentDraft.autoAddHosts) {
    patch.autoAddHosts = draft.autoAddHosts;
  }
  if (draft.autoHostInstanceType !== currentDraft.autoHostInstanceType) {
    patch.autoHostInstanceType = draft.autoHostInstanceType;
  }
  if (draft.autoRetireIdleHosts !== currentDraft.autoRetireIdleHosts) {
    patch.autoRetireIdleHosts = draft.autoRetireIdleHosts;
  }
  if (
    draft.automaticContinuationEnabled !==
    currentDraft.automaticContinuationEnabled
  ) {
    patch.automaticContinuationEnabled = draft.automaticContinuationEnabled;
  }
  const retireMinutes = parseWhole(
    draft.idleHostRetireMinutes,
    IDLE_HOST_RETIRE_MINUTES_MIN,
    IDLE_HOST_RETIRE_MINUTES_MAX,
  );
  if (retireMinutes === null) {
    errors.idleHostRetireSecs = `Enter whole minutes from ${IDLE_HOST_RETIRE_MINUTES_MIN} to ${IDLE_HOST_RETIRE_MINUTES_MAX}.`;
  } else if (
    String(retireMinutes) !== currentDraft.idleHostRetireMinutes.trim()
  ) {
    patch.idleHostRetireSecs = retireMinutes * 60;
  }
  const floor = parseWhole(
    draft.minFreeSlots,
    MIN_FREE_SLOTS_MIN,
    MIN_FREE_SLOTS_MAX,
  );
  if (floor === null) {
    errors.minFreeSlots = `Enter a whole number from ${MIN_FREE_SLOTS_MIN} to ${MIN_FREE_SLOTS_MAX}.`;
  } else if (String(floor) !== currentDraft.minFreeSlots.trim()) {
    patch.minFreeSlots = floor;
  }

  return { patch, errors };
}

function serverFieldErrors(error: unknown): FieldErrors {
  if (!(error instanceof ApiError)) return {};
  const errors: FieldErrors = {};
  for (const key of Object.values(DRAFT_FIELD)) {
    const message = error.details[key];
    if (typeof message === "string") errors[key] = message;
  }
  return errors;
}

export function OrgAdminSettingsCard({ orgId }: { orgId: string }) {
  const settingsState = useOrgAdminSettings(orgId);
  const update = useUpdateOrgAdminSettings();
  const current = settingsState.status === "ok" ? settingsState.data : null;

  // The draft remembers the org and server values it started from, so a
  // refetch refreshes untouched fields but never the admin's unsaved edits.
  // Inputs are disabled while a save is pending, so its response cannot
  // replace newer input.
  const [edit, setEdit] = useState<Edit | null>(null);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [saved, setSaved] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    if (!current) return;
    setEdit((prev) =>
      !prev || prev.orgId !== orgId
        ? startEdit(orgId, current)
        : rebaseEdit(prev, current),
    );
  }, [current, orgId]);

  const draft = edit?.orgId === orgId ? edit.draft : null;
  const dirty = useMemo(
    () => !!current && !!draft && !draftsEqual(draft, draftFrom(current)),
    [current, draft],
  );

  if (settingsState.status === "loading") {
    return <p className="text-sm text-[var(--g5-text2)]">Loading settings…</p>;
  }
  if (settingsState.status === "error" || !current || !draft) {
    return (
      <p
        className="text-sm text-[var(--g5-blocked)]"
        data-testid="org-admin-settings-load-error"
      >
        Failed to load organization settings.
      </p>
    );
  }

  const setField = <K extends keyof Draft>(key: K, value: Draft[K]) => {
    setEdit((prev) =>
      prev ? { ...prev, draft: { ...prev.draft, [key]: value } } : prev,
    );
    // Editing a field clears only that field's stale error.
    setErrors((prev) => {
      const { [DRAFT_FIELD[key]]: _cleared, ...rest } = prev;
      return rest;
    });
    setSaved(false);
    setSaveError(null);
  };

  const handleSave = async () => {
    const { patch, errors: localErrors } = buildAdminSettingsPatch(
      draft,
      current,
    );
    setErrors(localErrors);
    setSaved(false);
    setSaveError(null);
    if (Object.keys(localErrors).length > 0) return;
    if (Object.keys(patch).length === 0) return;
    try {
      const savedSettings = await update.mutateAsync({
        orgId,
        settings: patch,
      });
      setEdit(startEdit(orgId, savedSettings));
      setSaved(true);
    } catch (error) {
      const fieldErrors = serverFieldErrors(error);
      setErrors(fieldErrors);
      if (Object.keys(fieldErrors).length === 0) {
        setSaveError(
          error instanceof Error && error.message
            ? error.message
            : "Failed to save organization settings.",
        );
      }
    }
  };

  const inputCls = (field: keyof OrgAdminSettingsDTO) =>
    classNames(
      "w-40 px-3 py-2 text-sm",
      "rounded-[var(--g5-radius-sm)]",
      "bg-[var(--g5-bg2)] text-[var(--g5-text0)]",
      "border",
      errors[field]
        ? "border-[var(--g5-blocked)]"
        : "border-[var(--g5-stroke)]",
      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
      "disabled:cursor-not-allowed disabled:opacity-60",
    );
  const labelCls = "text-xs font-medium text-[var(--g5-text1)]";
  const hintCls = "text-xs text-[var(--g5-text2)]";
  const errorCls = "text-xs text-[var(--g5-blocked)]";

  return (
    <div className="flex flex-col gap-4" data-testid="org-admin-settings-card">
      <div className="flex flex-col gap-1">
        <label htmlFor="org-admin-default-idle-timeout" className={labelCls}>
          Default workspace VM idle timeout (minutes)
        </label>
        <input
          id="org-admin-default-idle-timeout"
          data-testid="org-admin-default-idle-timeout"
          type="number"
          inputMode="numeric"
          min={IDLE_TIMEOUT_MINUTES_MIN}
          max={IDLE_TIMEOUT_MINUTES_MAX}
          step={1}
          className={inputCls("defaultIdleTimeoutSecs")}
          value={draft.idleMinutes}
          disabled={update.isLoading}
          onChange={(e) => setField("idleMinutes", e.target.value)}
        />
        {errors.defaultIdleTimeoutSecs && (
          <span className={errorCls}>{errors.defaultIdleTimeoutSecs}</span>
        )}
        <span className={hintCls}>
          Every new workspace starts with this timeout; owners can change their
          own workspace in its settings. Existing workspaces keep theirs.
        </span>
      </div>

      <div className="flex flex-col gap-1">
        <label
          htmlFor="org-admin-default-personal-idle-timeout"
          className={labelCls}
        >
          Default personal workspace VM idle timeout (minutes)
        </label>
        <input
          id="org-admin-default-personal-idle-timeout"
          data-testid="org-admin-default-personal-idle-timeout"
          type="number"
          inputMode="numeric"
          min={IDLE_TIMEOUT_MINUTES_MIN}
          max={IDLE_TIMEOUT_MINUTES_MAX}
          step={1}
          className={inputCls("defaultPersonalIdleTimeoutSecs")}
          value={draft.personalIdleMinutes}
          disabled={update.isLoading}
          onChange={(e) => setField("personalIdleMinutes", e.target.value)}
        />
        {errors.defaultPersonalIdleTimeoutSecs && (
          <span className={errorCls}>
            {errors.defaultPersonalIdleTimeoutSecs}
          </span>
        )}
        <span className={hintCls}>
          Every new personal workspace starts with this timeout; owners can
          change their own in its settings. Existing personal workspaces keep
          theirs.
        </span>
      </div>

      <div className="flex flex-col gap-1">
        <label htmlFor="org-admin-max-running-vms" className={labelCls}>
          Organization VM quota
        </label>
        <input
          id="org-admin-max-running-vms"
          data-testid="org-admin-max-running-vms"
          type="number"
          inputMode="numeric"
          min={MAX_RUNNING_VMS_MIN}
          max={MAX_RUNNING_VMS_MAX}
          step={1}
          className={inputCls("maxRunningVms")}
          value={draft.maxRunningVms}
          disabled={update.isLoading}
          onChange={(e) => setField("maxRunningVms", e.target.value)}
        />
        {errors.maxRunningVms && (
          <span className={errorCls}>{errors.maxRunningVms}</span>
        )}
        <span className={hintCls}>
          Most workspace VMs the organization runs at once. Lowering it stops no
          running VM; new starts wait until the organization is below it.
          {draft.maxRunningVms.trim() === "0" &&
            " A quota of 0 blocks every workspace start."}
        </span>
      </div>

      <div className="flex flex-col gap-1">
        <label
          htmlFor="org-admin-max-running-vms-per-owner"
          className={labelCls}
        >
          Running workspace VMs per owner
        </label>
        <input
          id="org-admin-max-running-vms-per-owner"
          data-testid="org-admin-max-running-vms-per-owner"
          type="number"
          inputMode="numeric"
          min={MAX_RUNNING_VMS_PER_OWNER_MIN}
          max={MAX_RUNNING_VMS_PER_OWNER_MAX}
          step={1}
          className={inputCls("maxRunningVmsPerOwner")}
          value={draft.maxRunningVmsPerOwner}
          disabled={update.isLoading}
          onChange={(e) => setField("maxRunningVmsPerOwner", e.target.value)}
        />
        {errors.maxRunningVmsPerOwner && (
          <span className={errorCls}>{errors.maxRunningVmsPerOwner}</span>
        )}
        <span className={hintCls}>
          Most workspace VMs one person&apos;s workspaces run or wait to start
          at once, so a runaway agent cannot take the whole quota. A start over
          it is refused, not queued. Lowering it stops no running VM.
          {draft.maxRunningVmsPerOwner.trim() === "0" &&
            " A limit of 0 blocks every workspace start."}
        </span>
      </div>

      <div className="flex flex-col gap-1">
        <label
          htmlFor="org-admin-max-workspaces-per-owner"
          className={labelCls}
        >
          Workspaces per owner
        </label>
        <input
          id="org-admin-max-workspaces-per-owner"
          data-testid="org-admin-max-workspaces-per-owner"
          type="number"
          inputMode="numeric"
          min={MAX_WORKSPACES_PER_OWNER_MIN}
          max={MAX_WORKSPACES_PER_OWNER_MAX}
          step={1}
          className={inputCls("maxWorkspacesPerOwner")}
          value={draft.maxWorkspacesPerOwner}
          disabled={update.isLoading}
          onChange={(e) => setField("maxWorkspacesPerOwner", e.target.value)}
        />
        {errors.maxWorkspacesPerOwner && (
          <span className={errorCls}>{errors.maxWorkspacesPerOwner}</span>
        )}
        <span className={hintCls}>
          Most workspaces one person may own, including review workspaces.
          Creating, forking or transferring a workspace to someone at the limit
          is refused. Lowering it removes no workspace.
          {draft.maxWorkspacesPerOwner.trim() === "0" &&
            " A limit of 0 blocks every new workspace."}
        </span>
      </div>

      {/* Managed capacity — one bordered group so its controls read together. */}
      <fieldset
        data-testid="org-admin-managed-capacity"
        className="flex flex-col gap-4 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] px-4 pb-4 pt-2"
      >
        <legend className="px-1 text-sm font-semibold text-[var(--g5-text0)]">
          Managed capacity
        </legend>
        <p className={hintCls}>
          How G5 adds and removes this organization&apos;s cloud worker hosts.
          Self-hosted workers are never removed automatically.
        </p>

        <div className="flex flex-col gap-1">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <label htmlFor="org-admin-auto-add-hosts" className={labelCls}>
              Add hosts automatically
            </label>
            <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text1)]">
              <input
                id="org-admin-auto-add-hosts"
                data-testid="org-admin-auto-add-hosts"
                type="checkbox"
                role="switch"
                checked={draft.autoAddHosts}
                disabled={update.isLoading}
                onChange={(e) => setField("autoAddHosts", e.target.checked)}
                className="h-4 w-4 accent-[var(--g5-info)]"
              />
              {draft.autoAddHosts ? "On" : "Off"}
            </label>
          </div>
          <span className={hintCls}>
            When workspace starts are waiting for a free slot, G5 adds a host
            for them, within the organization VM quota. Off: starts wait until a
            slot frees up or you add a host.
          </span>
        </div>

        <div className="flex flex-col gap-1">
          <label
            htmlFor="org-admin-auto-host-instance-type"
            className={labelCls}
          >
            Machine type for added hosts
          </label>
          <select
            id="org-admin-auto-host-instance-type"
            data-testid="org-admin-auto-host-instance-type"
            className={inputCls("autoHostInstanceType")}
            value={draft.autoHostInstanceType}
            disabled={update.isLoading || !draft.autoAddHosts}
            onChange={(e) => setField("autoHostInstanceType", e.target.value)}
          >
            {WORKER_HOST_INSTANCE_TYPES.map((type) => (
              <option key={type} value={type}>
                {type}
              </option>
            ))}
          </select>
          {errors.autoHostInstanceType && (
            <span className={errorCls}>{errors.autoHostInstanceType}</span>
          )}
          <span className={hintCls}>
            Larger machines hold more workspace VMs per host and cost more.
          </span>
        </div>

        <div className="flex flex-col gap-1">
          <label htmlFor="org-admin-min-free-slots" className={labelCls}>
            Free slots to keep ready
          </label>
          <input
            id="org-admin-min-free-slots"
            data-testid="org-admin-min-free-slots"
            type="number"
            inputMode="numeric"
            min={MIN_FREE_SLOTS_MIN}
            max={MIN_FREE_SLOTS_MAX}
            step={1}
            className={inputCls("minFreeSlots")}
            value={draft.minFreeSlots}
            disabled={update.isLoading}
            onChange={(e) => setField("minFreeSlots", e.target.value)}
          />
          {errors.minFreeSlots && (
            <span className={errorCls}>{errors.minFreeSlots}</span>
          )}
          <span className={hintCls}>
            Once the organization has a host, G5 adds another (with adding on)
            whenever fewer slots than this are free, so the next start does not
            wait for a boot, and never removes a host that would leave fewer.
            Slots on hosts awaiting a runtime update do not count. 0 keeps only
            what starts need.
          </span>
        </div>

        <div className="flex flex-col gap-1">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <label
              htmlFor="org-admin-auto-retire-idle-hosts"
              className={labelCls}
            >
              Remove empty hosts automatically
            </label>
            <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text1)]">
              <input
                id="org-admin-auto-retire-idle-hosts"
                data-testid="org-admin-auto-retire-idle-hosts"
                type="checkbox"
                role="switch"
                checked={draft.autoRetireIdleHosts}
                disabled={update.isLoading}
                onChange={(e) =>
                  setField("autoRetireIdleHosts", e.target.checked)
                }
                className="h-4 w-4 accent-[var(--g5-info)]"
              />
              {draft.autoRetireIdleHosts ? "On" : "Off"}
            </label>
          </div>
          <span className={hintCls}>
            A cloud host is removed once it has had no workspace VM for the time
            below (a running VM keeps it, even if unused), as long as the other
            healthy hosts in the same region together keep the free slots set
            below (at least one).
          </span>
        </div>

        <div className="flex flex-col gap-1">
          <label htmlFor="org-admin-idle-host-retire" className={labelCls}>
            Remove a host after it is empty for (minutes)
          </label>
          <input
            id="org-admin-idle-host-retire"
            data-testid="org-admin-idle-host-retire"
            type="number"
            inputMode="numeric"
            min={IDLE_HOST_RETIRE_MINUTES_MIN}
            max={IDLE_HOST_RETIRE_MINUTES_MAX}
            step={1}
            className={inputCls("idleHostRetireSecs")}
            value={draft.idleHostRetireMinutes}
            disabled={update.isLoading || !draft.autoRetireIdleHosts}
            onChange={(e) => setField("idleHostRetireMinutes", e.target.value)}
          />
          {errors.idleHostRetireSecs && (
            <span className={errorCls}>{errors.idleHostRetireSecs}</span>
          )}
        </div>
      </fieldset>

      <fieldset className="flex flex-col gap-4 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] px-4 pb-4 pt-2">
        <legend className="px-1 text-sm font-semibold text-[var(--g5-text0)]">
          Managed host disks
        </legend>
        {(
          [
            ["hostDiskAllowanceGib", "Host disk allowance H (GiB)", 30],
            ["vmDiskAllowanceGib", "VM disk allowance V (GiB)", 1],
          ] as const
        ).map(([key, label, min]) => (
          <div key={key} className="flex flex-col gap-1">
            <label htmlFor={`org-admin-${key}`} className={labelCls}>
              {label}
            </label>
            <input
              id={`org-admin-${key}`}
              data-testid={`org-admin-${key}`}
              type="number"
              min={min}
              max={2000}
              step={1}
              className={inputCls(key)}
              value={draft[key]}
              disabled={update.isLoading}
              onChange={(e) => setField(key, e.target.value)}
            />
            {errors[key] && <span className={errorCls}>{errors[key]}</span>}
          </div>
        ))}
        <span className={hintCls}>
          New hosts require H + V × VM slots. H covers shared system, images and
          caches; each VM receives V. Existing hosts keep their recorded
          capacity. 1 GiB = 1024 MiB.
        </span>
      </fieldset>

      {/* Interrupted chats — its own group: not a capacity control. */}
      <fieldset
        data-testid="org-admin-interrupted-chats"
        className="flex flex-col gap-4 rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)] px-4 pb-4 pt-2"
      >
        <legend className="px-1 text-sm font-semibold text-[var(--g5-text0)]">
          Interrupted chats
        </legend>
        <div className="flex flex-col gap-1">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <label
              htmlFor="org-admin-automatic-continuation"
              className={labelCls}
            >
              Continue interrupted chats automatically
            </label>
            <label className="inline-flex cursor-pointer items-center gap-2 text-sm text-[var(--g5-text1)]">
              <input
                id="org-admin-automatic-continuation"
                data-testid="org-admin-automatic-continuation"
                type="checkbox"
                role="switch"
                checked={draft.automaticContinuationEnabled}
                disabled={update.isLoading}
                onChange={(e) =>
                  setField("automaticContinuationEnabled", e.target.checked)
                }
                className="h-4 w-4 accent-[var(--g5-info)]"
              />
              {draft.automaticContinuationEnabled ? "On" : "Off"}
            </label>
          </div>
          <span className={hintCls}>
            When a host drain or a lost VM interrupts a running chat turn, the
            workspace is started again for its owner and the turn continues.
            Stops by the owner or an administrator are never undone, and a
            workspace that keeps losing its VM is started again at most twice
            between the owner&apos;s own Starts.
          </span>
        </div>
      </fieldset>

      <div className="flex items-center gap-3">
        <Button
          kind="primary"
          data-testid="org-admin-settings-save"
          disabled={!dirty || update.isLoading}
          onClick={handleSave}
        >
          {update.isLoading ? (
            <>
              <Loader2 className="h-4 w-4 animate-spin" />
              Saving…
            </>
          ) : (
            "Save"
          )}
        </Button>
        {saved && !dirty && (
          <span
            className="text-xs text-[var(--g5-ok)]"
            data-testid="org-admin-settings-saved"
          >
            Saved
          </span>
        )}
        {saveError && (
          <span
            className="text-xs text-[var(--g5-blocked)]"
            data-testid="org-admin-settings-save-error"
          >
            {saveError}
          </span>
        )}
      </div>
    </div>
  );
}
