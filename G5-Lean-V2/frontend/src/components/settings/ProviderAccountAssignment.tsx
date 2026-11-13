// =============================================================================
// ProviderAccountAssignment — per-agent Default account select for the
// Defaults card
// =============================================================================
// Ontology: software_architecture/client/settings-page D17 (this control:
// label, helper text, one select — no source badge, per-scope pills,
// Override drawer, or resolution line), D19 (the Defaults card that hosts
// one control per visible agent next to that agent's default-model select;
// "Runtime variables" is offered only while legacy provider keys exist for
// the family), D16 (the Accounts section reads the assignment through
// `useAssignmentConfigs` for its Remove guard only);
// system_invariants/external_agent_provider_contract D14 (assignment keys
// `claude_account_id` / `codex_account_id`, scope-at-or-above write rule,
// kill-switch never rewrites), D11 (the account is an input to the one
// resolver; Settings does not preview the resolved route), D10 (Codex is
// a standard provider);
// system_invariants/config_safe_defaults D5 (`inherit_configured` resolves
// the effective assignment); config_service D3/D6 (merge writes, explicit
// removal), D12/D13 (account projection and assignment validation).
// Scenario owner: test_contract/provider_accounts I7.
//
// The control persists immediately to the page's scope: user in workspace
// settings, organization in Admin. Writes are minimal deltas (clears use
// `removeSettingKeys`) with an optimistic overlay rolled back on rejection.
// Workspace settings still display effective inherited assignments; Admin
// edits only organization defaults.
// =============================================================================

import React, { useCallback, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { ApiError } from "../../lib/api";
import type {
  AsyncState,
  PickerHarness,
  ProviderAccountDTO,
  ProviderAccountScope,
  ScopedConfig,
  UpdateScopedConfigInput,
} from "../../lib/types";
import {
  useOrgScopedConfig,
  useUserScopedConfig,
  useWorkspaceScopedConfig,
} from "../../hooks/useSettings";
import { ThemedSelect } from "../ui/UI";
import {
  NO_ACCOUNT_LABEL,
  RUNTIME_CONFIG_ACCOUNT_LABEL,
  legacyProviderKeysPresent,
  runtimeVariablesAvailableFrom,
} from "../../lib/providerAccountSelection";

export { legacyProviderKeysPresent };

// ---------------------------------------------------------------------------
// Assignment keys and scopes (external_agent_provider_contract D14)
// ---------------------------------------------------------------------------

type Scope = ProviderAccountScope;

const SCOPES: Scope[] = ["org", "workspace", "user"];

const SCOPE_LABEL: Record<Scope, string> = {
  org: "Org",
  workspace: "Workspace",
  user: "You",
};

/** Non-secret scoped setting key holding each agent's assignment. */
export const ASSIGNMENT_KEYS: Record<PickerHarness, string> = {
  claude_code: "claude_account_id",
  codex: "codex_account_id",
};

const HARNESS_LABEL: Record<PickerHarness, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
};

const HELPER_TEXT: Record<PickerHarness, string> = {
  claude_code:
    "Account (subscription or API key) Claude Code authenticates with.",
  codex: "Account (subscription or API key) Codex authenticates with.",
};

/** Value of the unassigned option: no assignment at that scope. */
const UNASSIGNED = "";
/** Unassigned label while legacy provider keys exist for the family (D19). */
export const RUNTIME_VARIABLES_LABEL = RUNTIME_CONFIG_ACCOUNT_LABEL;
export { NO_ACCOUNT_LABEL };

export type ScopeTriple = Record<Scope, string>;

/** A scope's OWN value for a key; a missing or non-string value is unset. */
function ownValue(
  config: ScopedConfig | null | undefined,
  key: string,
): string {
  const raw = config?.settings?.[key];
  return typeof raw === "string" ? raw : "";
}

// ---------------------------------------------------------------------------
// Scoped-config access (settings-page D17)
// ---------------------------------------------------------------------------

/**
 * Derive the per-harness per-scope assignment triples from each scope's OWN
 * `settings`. Precedence is applied by the caller (User > Workspace > Org),
 * never taken from the server's `effectiveSettings`.
 */
export function assignmentTriplesFrom(
  orgData: ScopedConfig | null | undefined,
  workspaceData: ScopedConfig | null | undefined,
  userData: ScopedConfig | null | undefined,
  hasWorkspace: boolean,
): Record<PickerHarness, ScopeTriple> {
  const build = (key: string): ScopeTriple => ({
    org: ownValue(orgData, key),
    workspace: hasWorkspace ? ownValue(workspaceData, key) : "",
    user: ownValue(userData, key),
  });
  return {
    claude_code: build(ASSIGNMENT_KEYS.claude_code),
    codex: build(ASSIGNMENT_KEYS.codex),
  };
}

export interface AssignmentConfigs {
  org: AsyncState<ScopedConfig>;
  workspace: AsyncState<ScopedConfig>;
  user: AsyncState<ScopedConfig>;
  /** Per-harness per-scope assignment, each scope's own value only. */
  triples: Record<PickerHarness, ScopeTriple>;
  /**
   * Raw credential keys exist for the harness's family in a scope in
   * context (chat g5.chat.account-defaults): "Runtime variables" is a real
   * choice only then.
   */
  runtimeVariablesAvailable: Record<PickerHarness, boolean>;
  /** True once every scope in context has loaded. */
  ready: boolean;
  loadError: Error | null;
}

/**
 * Read the three scoped configs and derive the per-scope assignment triples
 * (the Accounts section's Remove guard and the chat composer use this; the
 * Defaults card derives the same triples from the configs it already holds).
 */
export function useAssignmentConfigs(
  orgId: string,
  workspaceId?: string | null,
): AssignmentConfigs {
  const org = useOrgScopedConfig(orgId);
  const workspace = useWorkspaceScopedConfig(workspaceId ?? "", orgId);
  const user = useUserScopedConfig(orgId);
  const hasWorkspace = !!workspaceId;

  const orgData = org.status === "ok" ? org.data : null;
  const workspaceData = workspace.status === "ok" ? workspace.data : null;
  const userData = user.status === "ok" ? user.data : null;

  const triples = useMemo(
    () => assignmentTriplesFrom(orgData, workspaceData, userData, hasWorkspace),
    [orgData, workspaceData, userData, hasWorkspace],
  );
  const runtimeVariablesAvailable = useMemo(
    () =>
      runtimeVariablesAvailableFrom([
        orgData,
        hasWorkspace ? workspaceData : null,
        userData,
      ]),
    [orgData, workspaceData, userData, hasWorkspace],
  );

  const ready =
    orgData !== null &&
    userData !== null &&
    (!hasWorkspace || workspaceData !== null);
  const loadError =
    org.status === "error"
      ? org.error
      : user.status === "error"
        ? user.error
        : hasWorkspace && workspace.status === "error"
          ? workspace.error
          : null;

  return {
    org,
    workspace,
    user,
    triples,
    runtimeVariablesAvailable,
    ready,
    loadError,
  };
}

/**
 * Account ids whose removal the client already knows the server would
 * refuse: assignments at a scope other than the account's own
 * (chat g5.chat.account-defaults lets an account's own scope release it).
 * Only the configs in context are known here; the server's guard also sees
 * other members' user settings.
 */
export function blockingAssignmentIdsFrom(
  configs: AssignmentConfigs,
  accounts: ReadonlyArray<Pick<ProviderAccountDTO, "id" | "scope">>,
): string[] {
  const ids = new Set<string>();
  for (const account of accounts) {
    for (const triple of Object.values(configs.triples)) {
      for (const scope of SCOPES) {
        if (scope !== account.scope && triple[scope] === account.id) {
          ids.add(account.id);
        }
      }
    }
  }
  return [...ids];
}

/** Every account id any scope's own assignment references (Remove guard). */
export function assignedAccountIdsFrom(configs: AssignmentConfigs): string[] {
  const ids = new Set<string>();
  for (const triple of Object.values(configs.triples)) {
    for (const scope of SCOPES) {
      if (triple[scope]) ids.add(triple[scope]);
    }
  }
  return [...ids];
}

/** Effective value + winning scope, client-side (User > Workspace > Org). */
export function effectiveAssignment(triple: ScopeTriple): {
  value: string;
  source: Scope | "none";
} {
  if (triple.user) return { value: triple.user, source: "user" };
  if (triple.workspace) return { value: triple.workspace, source: "workspace" };
  if (triple.org) return { value: triple.org, source: "org" };
  return { value: UNASSIGNED, source: "none" };
}

// ---------------------------------------------------------------------------
// Option filtering (family + scope-at-or-above, contract D14)
// ---------------------------------------------------------------------------

function matchesFamily(
  account: ProviderAccountDTO,
  harness: PickerHarness,
): boolean {
  if (account.family === "both") return true;
  return harness === "claude_code"
    ? account.family === "claude"
    : account.family === "codex";
}

/**
 * Whether a write at `scope` may name `account`: org accounts everywhere;
 * a workspace account only at that workspace's scope; a user account only at
 * the caller's user scope.
 */
export function assignableAt(
  account: ProviderAccountDTO,
  scope: Scope,
  workspaceId: string | null | undefined,
): boolean {
  if (account.scope === "org") return true;
  if (account.scope === "workspace") {
    return (
      scope === "workspace" &&
      !!workspaceId &&
      account.workspaceId === workspaceId
    );
  }
  return scope === "user";
}

function accountOptionLabel(account: ProviderAccountDTO): string {
  return `${account.name} · ${SCOPE_LABEL[account.scope]}`;
}

// ---------------------------------------------------------------------------
// Errors (settings-page D17 inline tokens)
// ---------------------------------------------------------------------------

const ERROR_MESSAGES: Record<string, string> = {
  account_not_visible:
    "This account is not visible from here or no longer exists.",
  account_kind_mismatch: "This account's type does not match this agent.",
  account_scope_forbidden: "This account cannot be assigned at that scope.",
};

const FORBIDDEN_MESSAGE = "You cannot change the assignment at this scope";

function assignmentErrorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.status === 422 && error.code === "VALIDATION_ERROR") {
      const token = error.details.code;
      if (typeof token === "string" && ERROR_MESSAGES[token]) {
        return ERROR_MESSAGES[token];
      }
    }
    if (error.status === 403) return FORBIDDEN_MESSAGE;
    const detail = error.details.message;
    if (typeof detail === "string" && detail) return detail;
    return error.message || "The assignment could not be saved.";
  }
  if (error instanceof Error && error.message) return error.message;
  return "The assignment could not be saved.";
}

// ---------------------------------------------------------------------------
// Row component (one per visible agent, hosted by the Defaults card — D19)
// ---------------------------------------------------------------------------

interface PendingWrite {
  value: string;
  requestId: number;
}

export interface ProviderAccountAssignmentRowProps {
  harness: PickerHarness;
  orgId: string;
  /** Visible accounts (org + workspace + own user rows). */
  accounts: AsyncState<ProviderAccountDTO[]>;
  /** This harness's per-scope assignment (each scope's own value). */
  triple: ScopeTriple;
  /** True once every scoped config in context has loaded. */
  configsReady: boolean;
  /** Legacy provider keys exist for this family (D19): offer Runtime variables. */
  runtimeVariablesAvailable: boolean;
  scope?: "org" | "user";
  updateConfig: (input: UpdateScopedConfigInput) => Promise<unknown>;
}

export const ProviderAccountAssignmentRow: React.FC<
  ProviderAccountAssignmentRowProps
> = ({
  harness,
  orgId,
  accounts,
  triple: serverTriple,
  configsReady,
  runtimeVariablesAvailable,
  scope: writeScope = "user",
  updateConfig,
}) => {
  const queryClient = useQueryClient();

  const [pending, setPending] = useState<PendingWrite | null>(null);
  const [error, setError] = useState<string | null>(null);
  const requestSeq = useRef(0);

  const accountRows = useMemo<readonly ProviderAccountDTO[]>(
    () => (accounts.status === "ok" ? accounts.data : []),
    [accounts],
  );
  const accountsReady = accounts.status === "ok";
  const accountsError = accounts.status === "error" ? accounts.error : null;

  /** Server triple overlaid with the in-flight write at this page's scope. */
  const triple = useMemo<ScopeTriple>(
    () =>
      pending ? { ...serverTriple, [writeScope]: pending.value } : serverTriple,
    [serverTriple, pending, writeScope],
  );
  const effective = effectiveAssignment(triple);

  const unassignedLabel = runtimeVariablesAvailable
    ? RUNTIME_VARIABLES_LABEL
    : NO_ACCOUNT_LABEL;

  // Highest non-user scope that sets the key (Workspace > Org): the value
  // the user-scope select falls back to when the user's own assignment is
  // cleared. Clearing the user scope can never make an inherited assignment
  // go away, so the unassigned option is truthful only relative to it.
  const inherited = useMemo(() => {
    if (writeScope === "org") return null;
    const scope: Scope | null = triple.workspace
      ? "workspace"
      : triple.org
        ? "org"
        : null;
    if (!scope) return null;
    const id = triple[scope];
    const name =
      accountRows.find((a) => a.id === id)?.name ?? "Unavailable account";
    return { scope, id, name };
  }, [accountRows, triple, writeScope]);

  // The select lists accounts assignable at this page's write scope
  // (scope-at-or-above, contract D14) plus, when the effective
  // account was set at Workspace or Org, that account so the effective value
  // is always displayable. With an inherited assignment the unassigned
  // option is offered only while the user holds an own override, and it
  // reads as reverting to the inherited default (settings-page D17).
  const options = useMemo(() => {
    const current = effective.value;
    const eligible = accountRows.filter(
      (a) => matchesFamily(a, harness) && assignableAt(a, writeScope, null),
    );
    const list: Array<{ value: string; label: string }> = [];
    if (!inherited) {
      list.push({ value: UNASSIGNED, label: unassignedLabel });
    } else if (triple.user) {
      list.push({
        value: UNASSIGNED,
        label: `Use ${SCOPE_LABEL[inherited.scope]} default (${inherited.name})`,
      });
    }
    list.push(
      ...eligible.map((a) => ({ value: a.id, label: accountOptionLabel(a) })),
    );
    if (current && !eligible.some((a) => a.id === current)) {
      const known = accountRows.find((a) => a.id === current);
      list.push({
        value: current,
        label: known ? accountOptionLabel(known) : "Unavailable account",
      });
    }
    return list;
  }, [
    accountRows,
    effective.value,
    harness,
    inherited,
    triple.user,
    unassignedLabel,
    writeScope,
  ]);

  const configQueryKey = useMemo<readonly string[]>(
    () => ["scoped-config", writeScope, orgId],
    [orgId, writeScope],
  );

  const persist = useCallback(
    (value: string) => {
      const key = ASSIGNMENT_KEYS[harness];
      // Minimal delta only: the server merges `settings`, so a clear must go
      // through the explicit delete channel or the stored id would survive.
      const input: UpdateScopedConfigInput =
        value === UNASSIGNED
          ? { removeSettingKeys: [key] }
          : { settings: { [key]: value } };
      const requestId = ++requestSeq.current;
      setPending({ value, requestId });
      setError(null);
      const isCurrent = () => requestSeq.current === requestId;
      void Promise.resolve(updateConfig(input))
        .then(() =>
          // Keep the optimistic overlay until the refetched scope carries the
          // written value, so the row never flashes the pre-write value.
          queryClient.invalidateQueries({ queryKey: configQueryKey }),
        )
        .then(
          () => {
            if (!isCurrent()) return;
            setPending(null);
          },
          (reason: unknown) => {
            if (!isCurrent()) return;
            // Rollback: drop the overlay so the row shows the server value.
            setPending(null);
            setError(assignmentErrorMessage(reason));
          },
        );
    },
    [harness, queryClient, updateConfig, configQueryKey],
  );

  const busy = pending !== null;
  const disabled = !configsReady || !accountsReady || busy;

  return (
    <div
      className="flex flex-col gap-2"
      data-testid={`provider-account-assignment-${harness}`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <div className="text-xs text-[var(--g5-text1)]">Default account</div>
          <div className="text-[11px] text-[var(--g5-text2)]">
            {HELPER_TEXT[harness]}
          </div>
        </div>
        <ThemedSelect
          value={effective.value}
          options={options}
          disabled={disabled}
          size="sm"
          className="min-w-[16rem] shrink-0"
          testId={`provider-account-assignment-select-${harness}`}
          onChange={(value) => {
            if (value === effective.value) return;
            persist(value);
          }}
        />
      </div>

      {accountsError && (
        <div
          className="text-[11px] text-[var(--g5-warn)]"
          role="alert"
          data-testid={`provider-account-assignment-accounts-error-${harness}`}
        >
          ⚠ Could not load the accounts: {accountsError.message}
        </div>
      )}

      {error && (
        <div
          className="text-[11px] text-[var(--g5-warn)]"
          role="alert"
          data-testid={`provider-account-assignment-error-${harness}`}
        >
          ⚠ {error}
        </div>
      )}
    </div>
  );
};
