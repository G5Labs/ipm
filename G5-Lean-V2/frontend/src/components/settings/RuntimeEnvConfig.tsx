// =============================================================================
// RuntimeEnvConfig — the Settings "Defaults" card
// =============================================================================
// Ontology: software_architecture/client/settings-page D19 (this card: default
// agent; per visible agent a default-model select, an account select, and a
// resolver-fed resolution line; the custom-variable editor; NO built-in
// provider variable rows and NO Codex subscription checkbox — the default
// model plus the assigned account derive them), D2 (second section
// "Defaults"), D5 (custom-variable editor semantics: effective-first rows,
// client-side source badge with You > Workspace > Org precedence, Override
// drawer, minimal merge-patch Save with explicit removals), D6 (the
// `default_agent_provider` select), D8 (standard Codex availability),
// D9 (custom secret rows show the server hint only), D17
// (the account control and resolution line);
// system_invariants/external_agent_provider_contract D10 (standard Codex), D11
// (the resolver is the only source of route/auth/readiness; the card stages
// nothing for it except the in-flight assignment delta; an account-less
// resolution shows the D17 legacy credential hint, server-provided only —
// secret_custody D9 read-only), D14 (account
// controls hidden by the provider-accounts kill-switch; hiding never
// rewrites); config_service D3 (default agent provider), D6 (merge writes,
// explicit deletion). Scenario owners: test_contract/provider_accounts I7
// and I6 step 1.
//
// Backend semantics honored here:
//   - `settings` PUT is a partial MERGE into that scope's OWN settings object:
//     unspecified keys are preserved. A cleared key is deleted explicitly via
//     `removeSettingKeys` (omitting a key no longer deletes it).
//   - Secrets are deltas: `setSecrets` (add/update) + `removeSecretKeys`
//     (delete). Secret VALUES are never returned — only key names and hints.
//   - Effective value + winning scope are computed CLIENT-SIDE from each
//     scope's OWN values (the server `effectiveSettings` carries no
//     provenance).
//   - Built-in provider keys persisted by older Settings versions are never
//     rendered, never discovered as custom rows, never staged, and never
//     deleted by this card (D19).
// =============================================================================

import React, { useEffect, useMemo, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import type {
  AsyncState,
  PickerHarness,
  ScopedConfig,
  UpdateScopedConfigInput,
} from "../../lib/types";
import {
  CLAUDE_CHAT_DEFAULT_MODEL,
  CLAUDE_CHAT_DEFAULT_MODEL_LABEL,
  CLAUDE_CHAT_MODEL_OPTIONS,
  claudeFriendlyModelLabelFor,
} from "../../lib/claudeModels";
import {
  CODEX_CHAT_DEFAULT_MODEL,
  CODEX_CHAT_DEFAULT_MODEL_LABEL,
  CODEX_CHAT_MODEL_OPTIONS,
  codexFriendlyModelLabelFor,
} from "../../lib/codexModels";
import { useProviderAccountsEnabled } from "../../hooks/useSettings";
import { useProviderAccounts } from "../../hooks/useProviderAccounts";
import {
  ProviderAccountAssignmentRow,
  assignmentTriplesFrom,
  legacyProviderKeysPresent,
} from "./ProviderAccountAssignment";
import { Button, SectionTitle, classNames } from "../ui/UI";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

type Scope = "org" | "workspace" | "user";
type VarType = "secret" | "custom";

const SCOPES: Scope[] = ["org", "workspace", "user"];
const SCOPE_LABEL: Record<Scope, string> = {
  org: "Org",
  workspace: "Workspace",
  user: "You",
};

// Non-secret scoped-config key persisting the default agent provider
// (codex_support I11/D3; settings-page D6). Surfaced as the first control.
const KEY_DEFAULT_AGENT_PROVIDER = "default_agent_provider";

/**
 * Built-in provider runtime variables that older Settings versions edited as
 * rows. Since chat 994e615b (settings-page D19) the default model plus the
 * assigned account derive them, so they are never rendered, never discovered
 * as custom variables, never addable, and never touched by Save. Persisted
 * values keep resolving server-side unchanged.
 */
const HIDDEN_PROVIDER_KEYS = new Set([
  "ANTHROPIC_AUTH_TOKEN",
  "CLAUDE_CODE_OAUTH_TOKEN",
  "ANTHROPIC_BASE_URL",
  "ANTHROPIC_MODEL",
  "CLAUDE_CODE_SUBAGENT_MODEL",
  "OPENROUTER_API_KEY",
  "OPENAI_API_KEY",
  "OPENAI_BASE_URL",
  "OPENAI_MODEL",
]);

/**
 * Non-secret scoped settings that have their own control (or none) and must
 * never fall through to the discovered-custom list: the default agent
 * provider (own select), the Codex subscription preference (no control since
 * D19; the account decides), and the per-agent assignments (Account selects).
 */
const NON_ROW_SETTING_KEYS = new Set([
  KEY_DEFAULT_AGENT_PROVIDER,
  "codex_subscription_enabled",
  "claude_account_id",
  "codex_account_id",
]);

const CUSTOM_VALUE = "__other__";

function isRetiredRuntimeConfigKey(key: string): boolean {
  return key.trim() === "CODEX_AUTH_JSON";
}

function isHiddenProviderKey(key: string): boolean {
  return HIDDEN_PROVIDER_KEYS.has(key) || NON_ROW_SETTING_KEYS.has(key);
}

// ---------------------------------------------------------------------------
// Local staged model
// ---------------------------------------------------------------------------

/** Per-variable, per-scope staged value for a single scope. */
type ScopeTriple = Record<Scope, string>;

/**
 * Staged settings state: variable key -> per-scope string value.
 * For secrets, the value is the SENTINEL `__set__` when a secret is present
 * (server never returns the plaintext) or a real plaintext string staged for a
 * SET, and `''` when unset/cleared. We track secret SETs separately so we know
 * the plaintext to send.
 */
type SettingsState = Record<string, ScopeTriple>;

const SECRET_SET_SENTINEL = "__set__";

interface CustomVar {
  key: string;
  type: VarType;
}

/** Snapshot used for dirty-checking and discard. */
interface Snapshot {
  settings: SettingsState;
  secretPlain: Record<string, Partial<Record<Scope, string>>>;
  custom: CustomVar[];
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface RuntimeEnvConfigProps {
  orgId: string;
  context?: "workspace" | "organization";
  workspaceId?: string | null;
  org: AsyncState<ScopedConfig>;
  workspace?: AsyncState<ScopedConfig>;
  user?: AsyncState<ScopedConfig>;
  updateOrg?: (input: UpdateScopedConfigInput) => Promise<unknown>;
  updateWorkspace?: (input: UpdateScopedConfigInput) => Promise<unknown>;
  updateUser?: (input: UpdateScopedConfigInput) => Promise<unknown>;
  savingOrg?: boolean;
  savingWorkspace?: boolean;
  savingUser?: boolean;
  canEditOrg: boolean;
  canEditWorkspace: boolean;
  /** Per-provider personal default models (one per agent row). Each provider
   * reads and writes only its own UserSettings namespace. */
  defaultAgentModels?: Partial<Record<PickerHarness, string>>;
  onDefaultAgentModelChange?: (harness: PickerHarness, value: string) => void;
  /** When false, only the per-agent default-model selects are shown; the
   * default-agent select, the Default account selects, and the
   * custom-variable editor are hidden (e.g. non-cloud builds where Runtime
   * Config is gated off). Default true. */
  showScopedConfig?: boolean;
  /** Agent providers offered by the default-agent selector (codex_support
   * I11/D3). Defaults to Claude Code + Codex. The selected default is persisted
   * to the non-secret `default_agent_provider` user-scoped config value. */
  defaultAgentProviderOptions?: Array<{ value: string; label: string }>;
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

function isSecretType(type: VarType): boolean {
  return type === "secret";
}

/** UPPER_SNAKE_CASE env-name validation. */
function isValidEnvName(key: string): boolean {
  return /^[A-Z_][A-Z0-9_]*$/.test(key);
}

function isReservedPrefix(key: string): boolean {
  return /^G5D?_/.test(key);
}

function emptyTriple(): ScopeTriple {
  return { org: "", workspace: "", user: "" };
}

const HARNESS_LABEL: Record<PickerHarness, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const RuntimeEnvConfig: React.FC<RuntimeEnvConfigProps> = ({
  orgId,
  context = "workspace",
  workspaceId,
  org,
  workspace = { status: "loading" },
  user = { status: "loading" },
  updateOrg,
  updateWorkspace,
  updateUser,
  savingOrg = false,
  savingWorkspace = false,
  savingUser = false,
  canEditOrg,
  canEditWorkspace,
  defaultAgentModels,
  onDefaultAgentModelChange,
  showScopedConfig = true,
  // Both standard providers are offered unless the caller supplies options.
  defaultAgentProviderOptions = [
    { value: "claude_code", label: "Claude Code" },
    { value: "codex", label: "Codex" },
  ],
}) => {
  const organizationContext = context === "organization";
  const hasWorkspace = !organizationContext && Boolean(workspaceId);
  const defaultScope = organizationContext ? "org" : "user";
  const editableScopes: Scope[] = organizationContext
    ? ["org"]
    : hasWorkspace
      ? ["workspace", "user"]
      : ["user"];
  const updateDefault = organizationContext ? updateOrg : updateUser;
  // Provider-accounts kill-switch (contract D14): the account selects render
  // only while it is on; hiding never rewrites a persisted assignment.
  const providerAccountsEnabled = useProviderAccountsEnabled();
  const showAccountControls =
    showScopedConfig && providerAccountsEnabled && !!orgId;
  const accounts = useProviderAccounts(
    showAccountControls ? orgId : "",
    workspaceId,
  );

  // Workspace settings include inherited org defaults; Admin reads only the
  // organization scope and must not be shadowed by personal/workspace values.
  const orgData = org.status === "ok" ? org.data : null;
  const workspaceData = workspace.status === "ok" ? workspace.data : null;
  const userData =
    !organizationContext && user.status === "ok" ? user.data : null;

  const ready =
    orgData !== null &&
    (organizationContext || userData !== null) &&
    (!hasWorkspace || workspaceData !== null);

  const loadError =
    org.status === "error"
      ? org.error
      : !organizationContext && user.status === "error"
        ? user.error
        : hasWorkspace && workspace.status === "error"
          ? workspace.error
          : null;

  // Per-agent assignment triples and the legacy-key rule (settings-page D17
  // / D19), derived from the same scoped configs this card already holds.
  const scopedConfigsInContext = useMemo(
    () => [orgData, hasWorkspace ? workspaceData : null, userData],
    [orgData, workspaceData, userData, hasWorkspace],
  );
  const assignmentTriples = useMemo(
    () =>
      assignmentTriplesFrom(
        orgData,
        hasWorkspace ? workspaceData : null,
        userData,
        hasWorkspace,
      ),
    [orgData, workspaceData, userData, hasWorkspace],
  );
  const runtimeVariablesAvailable = useMemo<Record<PickerHarness, boolean>>(
    () => ({
      claude_code: legacyProviderKeysPresent(
        "claude_code",
        scopedConfigsInContext,
      ),
      codex: legacyProviderKeysPresent("codex", scopedConfigsInContext),
    }),
    [scopedConfigsInContext],
  );

  // -------------------------------------------------------------------------
  // Local staged state (custom variables + the default-agent triple)
  // -------------------------------------------------------------------------

  const [settings, setSettings] = useState<SettingsState>({});
  // secret plaintext staged for SET, per var/scope. Absence ⇒ no staged set.
  const [secretPlain, setSecretPlain] = useState<
    Record<string, Partial<Record<Scope, string>>>
  >({});
  const [custom, setCustom] = useState<CustomVar[]>([]);

  // The last server-derived snapshot. Dirty = current != snapshot.
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const savedSettingsRef = useRef<SettingsState | null>(null);

  // Drawer / inline-override UI state.
  const [openKey, setOpenKey] = useState<string | null>(null);
  const [drawerScope, setDrawerScope] = useState<Record<string, Scope>>({});
  const [drawerDraft, setDrawerDraft] = useState<Record<string, string>>({});

  const [saveError, setSaveError] = useState<string | null>(null);

  // -------------------------------------------------------------------------
  // Build a snapshot from the resolved server states. Custom secrets read
  // their per-scope "present" flag from each scope's OWN `secretKeys`;
  // non-secret custom vars read their per-scope value from each scope's OWN
  // `settings`. Built-in provider keys and non-row settings are skipped
  // entirely (D19): never rendered, never staged, never deleted.
  // -------------------------------------------------------------------------

  const serverSnapshot = useMemo<Snapshot | null>(() => {
    if (!ready || !orgData) {
      return null;
    }
    const perScope: Record<Scope, ScopedConfig | null> = {
      org: orgData,
      workspace: hasWorkspace ? workspaceData : null,
      user: userData,
    };

    const nextSettings: SettingsState = {
      [KEY_DEFAULT_AGENT_PROVIDER]: emptyTriple(),
    };
    const discoveredCustom: CustomVar[] = [];
    const seen = new Set<string>();

    for (const scope of SCOPES) {
      const cfg = perScope[scope];
      if (!cfg) {
        continue;
      }
      // Non-secret settings.
      for (const [k, rawVal] of Object.entries(cfg.settings)) {
        if (isRetiredRuntimeConfigKey(k)) {
          continue;
        }
        if (k === KEY_DEFAULT_AGENT_PROVIDER) {
          nextSettings[k][scope] =
            rawVal === null || rawVal === undefined ? "" : String(rawVal);
          continue;
        }
        if (isHiddenProviderKey(k)) {
          continue; // derived from model + account, or has its own control
        }
        if (!nextSettings[k]) {
          nextSettings[k] = emptyTriple();
        }
        nextSettings[k][scope] =
          rawVal === null || rawVal === undefined ? "" : String(rawVal);
        if (!seen.has(k)) {
          seen.add(k);
          discoveredCustom.push({ key: k, type: "custom" });
        }
      }
      // Secret keys (presence only).
      for (const k of cfg.secretKeys) {
        if (isRetiredRuntimeConfigKey(k) || isHiddenProviderKey(k)) {
          continue;
        }
        if (!nextSettings[k]) {
          nextSettings[k] = emptyTriple();
        }
        nextSettings[k][scope] = SECRET_SET_SENTINEL;
        if (!seen.has(k)) {
          seen.add(k);
          discoveredCustom.push({ key: k, type: "secret" });
        }
      }
    }

    return {
      settings: nextSettings,
      secretPlain: {},
      custom: discoveredCustom,
    };
  }, [ready, orgData, workspaceData, userData, hasWorkspace]);

  // Reset local staged state whenever the server snapshot changes (initial load
  // and after a successful save triggers a refetch).
  const snapshotKey = useMemo(
    () => (serverSnapshot ? JSON.stringify(serverSnapshot) : null),
    [serverSnapshot],
  );

  useEffect(() => {
    if (!serverSnapshot) {
      return;
    }
    savedSettingsRef.current = serverSnapshot.settings;
    setSettings(serverSnapshot.settings);
    setSecretPlain({});
    setCustom(serverSnapshot.custom);
    setSnapshot(serverSnapshot);
    setOpenKey(null);
    setDrawerScope({});
    setDrawerDraft({});
    setSaveError(null);
    // snapshotKey changes iff the structural content changed.
  }, [snapshotKey, serverSnapshot]);

  // -------------------------------------------------------------------------
  // Derived: custom variable list, effective value/source.
  // -------------------------------------------------------------------------

  const customVars = useMemo<CustomVar[]>(
    () => custom.filter((c) => !isRetiredRuntimeConfigKey(c.key)),
    [custom],
  );

  /** Effective value + winning scope for a variable, computed client-side. */
  const effectiveOf = useMemo(() => {
    return function eff(key: string): {
      value: string;
      source: Scope | "default";
    } {
      const triple = settings[key] ?? emptyTriple();
      if (triple.user) {
        return { value: triple.user, source: "user" };
      }
      if (triple.workspace) {
        return { value: triple.workspace, source: "workspace" };
      }
      if (triple.org) {
        return { value: triple.org, source: "org" };
      }
      return { value: "", source: "default" };
    };
  }, [settings]);

  // Server-provided bounded secret display hints per scope (settings-page D9;
  // sanctioned by security_policy secret_custody_and_redaction D9): key →
  // first 12 chars of that scope's stored value, present only for
  // hint-eligible values. Read-only display metadata — never derived
  // client-side from typed input, so a staged-but-unsaved secret keeps the
  // masked display until the server returns its hint.
  const hintsByScope = useMemo<Record<Scope, Record<string, string>>>(
    () => ({
      org: orgData?.secretKeyHints ?? {},
      workspace: workspaceData?.secretKeyHints ?? {},
      user: userData?.secretKeyHints ?? {},
    }),
    [orgData, workspaceData, userData],
  );

  // -------------------------------------------------------------------------
  // Dirty tracking
  // -------------------------------------------------------------------------

  const dirty = useMemo(() => {
    if (!snapshot) {
      return false;
    }
    const current: Snapshot = { settings, secretPlain, custom };
    return JSON.stringify(current) !== JSON.stringify(snapshot);
  }, [settings, secretPlain, custom, snapshot]);

  // -------------------------------------------------------------------------
  // Staging mutations (do NOT call the API)
  // -------------------------------------------------------------------------

  function stageSet(vd: CustomVar, scope: Scope, value: string): void {
    setSettings((prev) => {
      const triple = prev[vd.key] ? { ...prev[vd.key] } : emptyTriple();
      if (isSecretType(vd.type)) {
        triple[scope] = value ? SECRET_SET_SENTINEL : "";
      } else {
        triple[scope] = value;
      }
      return { ...prev, [vd.key]: triple };
    });
    if (isSecretType(vd.type)) {
      setSecretPlain((prev) => {
        const forVar = { ...(prev[vd.key] ?? {}) };
        if (value) {
          forVar[scope] = value;
        } else {
          delete forVar[scope];
        }
        return { ...prev, [vd.key]: forVar };
      });
    }
  }

  function stageClear(vd: CustomVar, scope: Scope): void {
    setSettings((prev) => {
      const triple = prev[vd.key] ? { ...prev[vd.key] } : emptyTriple();
      triple[scope] = "";
      return { ...prev, [vd.key]: triple };
    });
    if (isSecretType(vd.type)) {
      setSecretPlain((prev) => {
        if (!prev[vd.key]) {
          return prev;
        }
        const forVar = { ...prev[vd.key] };
        delete forVar[scope];
        return { ...prev, [vd.key]: forVar };
      });
    }
  }

  // -------------------------------------------------------------------------
  // Drawer helpers
  // -------------------------------------------------------------------------

  function scopeForKey(key: string): Scope {
    return drawerScope[key] ?? defaultScope;
  }

  function openOverride(key: string): void {
    setOpenKey((prev) => (prev === key ? null : key));
    setDrawerDraft((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
    setDrawerScope((prev) =>
      prev[key] ? prev : { ...prev, [key]: defaultScope },
    );
  }

  function clearDraft(key: string): void {
    setDrawerDraft((prev) => {
      const copy = { ...prev };
      delete copy[key];
      return copy;
    });
  }

  function scopeSelectable(scope: Scope): boolean {
    if (!editableScopes.includes(scope)) return false;
    if (scope === "org") {
      return canEditOrg && !!updateOrg;
    }
    if (scope === "workspace") {
      return hasWorkspace && canEditWorkspace && !!updateWorkspace;
    }
    return !!updateUser;
  }

  // -------------------------------------------------------------------------
  // Add variable
  // -------------------------------------------------------------------------

  function handleAddVariable(): void {
    const raw = window.prompt("New variable name (UPPER_SNAKE_CASE):");
    if (!raw) {
      return;
    }
    const key = raw.trim();
    if (!isValidEnvName(key)) {
      window.alert(
        "Invalid: keys must match [A-Z_][A-Z0-9_]* (letters, digits, underscore).",
      );
      return;
    }
    if (isReservedPrefix(key)) {
      window.alert("Reserved prefix: G5_ / G5D_ keys are blocked.");
      return;
    }
    if (isRetiredRuntimeConfigKey(key)) {
      window.alert(
        "CODEX_AUTH_JSON is retired; sign in to Codex in chat using device authorization.",
      );
      return;
    }
    if (isHiddenProviderKey(key)) {
      // Built-in provider keys are derived from the default model and the
      // assigned account (settings-page D19); they are not addable rows.
      window.alert(
        `${key} is a built-in provider runtime variable. It is derived from the default model and account chosen above, so it cannot be added as a custom variable.`,
      );
      return;
    }
    if (settings[key]) {
      window.alert("That custom variable already exists.");
      return;
    }
    const asSecret = window.confirm(
      `Store ${key} as a write-only secret?  OK = secret · Cancel = visible setting`,
    );
    setSettings((prev) => ({ ...prev, [key]: emptyTriple() }));
    setCustom((prev) => [
      ...prev,
      { key, type: asSecret ? "secret" : "custom" },
    ]);
    setOpenKey(key);
    setDrawerScope((prev) => ({ ...prev, [key]: defaultScope }));
  }

  // -------------------------------------------------------------------------
  // Save
  // -------------------------------------------------------------------------

  function secretKeySetFor(vars: CustomVar[]): Set<string> {
    return new Set<string>(
      vars.filter((c) => c.type === "secret").map((c) => c.key),
    );
  }

  function cloneSettingsState(
    state: SettingsState | null | undefined,
  ): SettingsState {
    const cloned: SettingsState = {};
    if (!state) {
      return cloned;
    }
    for (const [key, triple] of Object.entries(state)) {
      cloned[key] = { ...triple };
    }
    return cloned;
  }

  async function handleSave(): Promise<void> {
    if (!snapshot || !dirty) {
      return;
    }
    setSaveError(null);

    const secretKeys = secretKeySetFor(custom);

    const tasks: Array<() => Promise<unknown>> = [];

    for (const scope of editableScopes) {
      const prevTriple = (k: string): string =>
        snapshot.settings[k]?.[scope] ?? "";
      const nextTriple = (k: string): string => settings[k]?.[scope] ?? "";

      // Build a MINIMAL settings patch: only the keys the user actually
      // changed at this scope. The server merges (config_service D6), so
      // re-sending untouched keys would ship their fetched — possibly stale —
      // value and clobber a concurrent edit to that sibling made elsewhere.
      // Cleared keys are never put in the patch; they go through
      // `removeSettingKeys` alone.
      const settingsPatch: Record<string, unknown> = {};
      const setSecrets: Record<string, string> = {};
      const removeSecretKeys: string[] = [];
      const removeSettingKeys: string[] = [];

      const allKeys = new Set<string>([
        ...Object.keys(snapshot.settings),
        ...Object.keys(settings),
      ]);

      for (const key of allKeys) {
        const before = prevTriple(key);
        const after = nextTriple(key);
        // A rotated secret keeps the same `__set__` sentinel in `settings`
        // (the replacement value lives only in `secretPlain`), so
        // `before === after`. Without this guard the rotation is silently
        // dropped: no `setSecrets` mutation is sent yet the dirty state clears.
        const stagedSecretPlain =
          secretKeys.has(key) && after === SECRET_SET_SENTINEL
            ? secretPlain[key]?.[scope]
            : undefined;
        if (before === after && !stagedSecretPlain) {
          continue;
        }
        if (secretKeys.has(key)) {
          // Secret delta.
          if (after === SECRET_SET_SENTINEL) {
            const plain = secretPlain[key]?.[scope];
            if (plain) {
              setSecrets[key] = plain;
            }
          } else {
            // cleared at this scope
            removeSecretKeys.push(key);
          }
        } else if (after === "") {
          // Cleared: request explicit deletion only (the server merges, so
          // omission no longer deletes the key). Never echoed into the patch.
          removeSettingKeys.push(key);
        } else {
          // Changed to a non-empty value: this key alone enters the patch.
          settingsPatch[key] = after;
        }
      }

      const hasSettingChange =
        Object.keys(settingsPatch).length > 0 || removeSettingKeys.length > 0;
      const hasSecretChange =
        Object.keys(setSecrets).length > 0 || removeSecretKeys.length > 0;
      if (!hasSettingChange && !hasSecretChange) {
        continue;
      }
      if (!scopeSelectable(scope)) {
        setSaveError(
          `You no longer have permission to save ${scope} changes. Your edits are still unsaved.`,
        );
        return;
      }

      const input: UpdateScopedConfigInput = {};
      if (Object.keys(settingsPatch).length > 0) {
        input.settings = settingsPatch;
      }
      if (removeSettingKeys.length > 0) {
        input.removeSettingKeys = removeSettingKeys;
      }
      if (Object.keys(setSecrets).length > 0) {
        input.setSecrets = setSecrets;
      }
      if (removeSecretKeys.length > 0) {
        input.removeSecretKeys = removeSecretKeys;
      }

      const mutate =
        scope === "org"
          ? updateOrg
          : scope === "workspace"
            ? updateWorkspace
            : updateUser;
      if (mutate) tasks.push(() => mutate(input));
    }

    try {
      await Promise.all(tasks.map((save) => save()));
      // Re-snapshot from the now-current local state. The refetched server
      // states will also flow back in via serverSnapshot and re-sync.
      const nextSnapshot = { settings, secretPlain: {}, custom };
      savedSettingsRef.current = settings;
      setSnapshot(nextSnapshot);
      setSecretPlain({});
      setOpenKey(null);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Save failed.";
      setSaveError(msg);
    }
  }

  function handleDiscard(): void {
    if (!snapshot) {
      return;
    }
    setSettings(snapshot.settings);
    setSecretPlain(snapshot.secretPlain);
    setCustom(snapshot.custom);
    setOpenKey(null);
    setDrawerScope({});
    setDrawerDraft({});
    setSaveError(null);
  }

  const saving = savingOrg || savingWorkspace || savingUser;

  // -------------------------------------------------------------------------
  // Render helpers
  // -------------------------------------------------------------------------

  const selectClass = classNames(
    "min-w-[16rem] rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
    "bg-[var(--g5-bg1)] px-2 py-1.5 text-xs text-[var(--g5-text0)]",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
  );
  const inputClass = classNames(
    "min-w-[16rem] rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
    "bg-[var(--g5-bg1)] px-2 py-1.5 text-xs font-mono text-[var(--g5-text0)]",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
  );
  const cardClass = classNames(
    "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
    "bg-[var(--g5-bg1)] px-3 py-3",
  );

  function sourceBadge(source: Scope | "default"): React.ReactNode {
    if (source === "default") {
      return (
        <span className="text-[10px] text-[var(--g5-text2)]">default</span>
      );
    }
    const token =
      source === "org"
        ? "--g5-active"
        : source === "workspace"
          ? "--g5-info"
          : "--g5-ok";
    return (
      <span
        className="rounded-full px-1.5 text-[10px] border"
        style={{
          color: `var(${token})`,
          borderColor: `color-mix(in srgb, var(${token}) 40%, transparent)`,
        }}
      >
        {SCOPE_LABEL[source]}
      </span>
    );
  }

  function scopePillValue(vd: CustomVar, scope: Scope): string {
    const raw = settings[vd.key]?.[scope] ?? "";
    if (!raw) {
      return "—";
    }
    if (isSecretType(vd.type) || raw === SECRET_SET_SENTINEL) {
      // Show the scope's own server-provided hint for a SAVED secret
      // (settings-page D9). A staged-but-unsaved secret (raw plaintext, not
      // the sentinel) and an ineligible value keep the bare `set` marker.
      if (raw === SECRET_SET_SENTINEL) {
        const hint = hintsByScope[scope]?.[vd.key];
        if (hint) {
          return `${hint}…`;
        }
      }
      return "set";
    }
    return raw;
  }

  function effectiveDisplay(vd: CustomVar): React.ReactNode {
    const e = effectiveOf(vd.key);
    let body: React.ReactNode;
    if (e.value === "") {
      body = <span className="italic text-[var(--g5-text2)]">unset</span>;
    } else if (isSecretType(vd.type)) {
      // Beginning-of-key hint for the winning scope's SAVED secret
      // (settings-page D9): server-provided only, masked fallback otherwise.
      const winner = e.source === "default" ? null : e.source;
      const hint =
        e.value === SECRET_SET_SENTINEL && winner
          ? hintsByScope[winner]?.[vd.key]
          : undefined;
      body = hint ? (
        <span>
          <span
            className="font-mono text-[var(--g5-text0)]"
            data-testid={`runtime-env-hint-${vd.key}`}
          >
            {hint}…
          </span>{" "}
          <span className="text-[var(--g5-text2)]">configured</span>
        </span>
      ) : (
        <span className="text-[var(--g5-text2)]">•••• configured</span>
      );
    } else {
      body = (
        <span className="font-mono text-[var(--g5-text0)]">{e.value}</span>
      );
    }
    return (
      <span className="text-xs">
        {body} {sourceBadge(e.source)}
      </span>
    );
  }

  /** The value control inside the override drawer for a custom variable. */
  function renderValueControl(vd: CustomVar): React.ReactNode {
    const key = vd.key;
    const scope = scopeForKey(key);
    const stagedRaw = settings[key]?.[scope] ?? "";
    const draft = drawerDraft[key] !== undefined ? drawerDraft[key] : stagedRaw;

    if (isSecretType(vd.type)) {
      return (
        <input
          type="password"
          className={inputClass}
          placeholder="new value (write-only)"
          value={drawerDraft[key] ?? ""}
          onChange={(e) =>
            setDrawerDraft((prev) => ({ ...prev, [key]: e.target.value }))
          }
          aria-label={`${key} value`}
          data-testid={`runtime-env-secret-input-${key}`}
        />
      );
    }
    return (
      <input
        type="text"
        className={inputClass}
        placeholder="value"
        value={draft}
        onChange={(e) =>
          setDrawerDraft((prev) => ({ ...prev, [key]: e.target.value }))
        }
        aria-label={`${key} value`}
        data-testid={`runtime-env-text-input-${key}`}
      />
    );
  }

  // Default-agent provider dropdown (codex_support I11/D3; settings-page D6).
  // Reads the effective `default_agent_provider` non-secret scoped value and
  // persists to this page's default scope: personal in workspace settings,
  // organization in Admin, using the same precedence as other scoped settings.
  //
  const visibleDefaultAgentProviderOptions = defaultAgentProviderOptions;

  function renderDefaultAgentRow(): React.ReactNode {
    const eff = effectiveOf(KEY_DEFAULT_AGENT_PROVIDER);
    const value = eff.value;
    const known = visibleDefaultAgentProviderOptions.some(
      (o) => o.value === value,
    );
    const opts: Array<[string, string]> = [
      ["", "Auto (first available)"],
      ...visibleDefaultAgentProviderOptions.map<[string, string]>((o) => [
        o.value,
        o.label,
      ]),
    ];
    if (value && !known) {
      opts.push([value, value]);
    }
    return (
      <div data-testid="runtime-env-default-agent" className={cardClass}>
        <div className="flex items-start justify-between gap-3">
          <div className="flex flex-col gap-1 min-w-0">
            <div className="text-xs font-semibold text-[var(--g5-text0)]">
              Default agent
            </div>
            <div className="text-[11px] text-[var(--g5-text2)]">
              Agent provider new chats and tasks start on (resolved against
              available workers).
            </div>
          </div>
          <select
            className={selectClass}
            value={value}
            aria-label="Default agent provider"
            data-testid="runtime-env-default-agent-select"
            disabled={saving || !scopeSelectable(defaultScope)}
            onChange={(e) => {
              const next = e.target.value;
              // Send only this key's delta. The server merges `settings`, so
              // "Auto" must ask for an explicit delete via `removeSettingKeys`
              // — omitting the key would leave the stored value in place and it
              // would reappear on the next refetch. Re-sending the whole saved
              // user settings object would also let a stale sibling value
              // clobber a concurrent edit made elsewhere.
              const input: UpdateScopedConfigInput =
                next === ""
                  ? { removeSettingKeys: [KEY_DEFAULT_AGENT_PROVIDER] }
                  : { settings: { [KEY_DEFAULT_AGENT_PROVIDER]: next } };
              const nextSavedSettings = cloneSettingsState(
                savedSettingsRef.current ?? snapshot?.settings,
              );
              nextSavedSettings[KEY_DEFAULT_AGENT_PROVIDER] = {
                ...(nextSavedSettings[KEY_DEFAULT_AGENT_PROVIDER] ??
                  emptyTriple()),
                [defaultScope]: next,
              };
              // Optimistically reflect the change in local state so the row and
              // effective badge update before the refetch lands.
              setSettings((prev) => ({
                ...prev,
                [KEY_DEFAULT_AGENT_PROVIDER]: {
                  ...(prev[KEY_DEFAULT_AGENT_PROVIDER] ?? emptyTriple()),
                  [defaultScope]: next,
                },
              }));
              void Promise.resolve(updateDefault?.(input))
                .then(() => {
                  savedSettingsRef.current = nextSavedSettings;
                  setSnapshot((prev) =>
                    prev ? { ...prev, settings: nextSavedSettings } : prev,
                  );
                })
                .catch((err: unknown) => {
                  const msg =
                    err instanceof Error ? err.message : "Save failed.";
                  setSaveError(msg);
                });
            }}
          >
            {opts.map(([v, l]) => (
              <option key={`${v}::${l}`} value={v}>
                {l}
              </option>
            ))}
          </select>
        </div>
      </div>
    );
  }

  // One row per visible agent (settings-page D19): the user-scoped default
  // model select, the account select (D17; only while accounts are enabled
  // and the scoped config is in context), and the resolver-fed resolution
  // line. Catalog and route data stay provider-specific.
  function renderAgentRow(harness: PickerHarness): React.ReactNode {
    const isCodex = harness === "codex";
    const providerLabel = HARNESS_LABEL[harness];
    const defaultModel = isCodex
      ? CODEX_CHAT_DEFAULT_MODEL
      : CLAUDE_CHAT_DEFAULT_MODEL;
    const defaultModelLabel = isCodex
      ? CODEX_CHAT_DEFAULT_MODEL_LABEL
      : CLAUDE_CHAT_DEFAULT_MODEL_LABEL;
    const value = defaultAgentModels?.[harness] ?? defaultModel;
    const modelOptions: Array<[string, string]> = isCodex
      ? CODEX_CHAT_MODEL_OPTIONS.map((option) => [option.value, option.label])
      : CLAUDE_CHAT_MODEL_OPTIONS.map((option) => [option.value, option.label]);
    const baseOpts: Array<[string, string]> = [
      [defaultModel, defaultModelLabel],
      ...modelOptions.filter(([model]) => model !== defaultModel),
      [CUSTOM_VALUE, "Custom…"],
    ];
    const known = baseOpts.some((o) => o[0] === value);
    const opts = [...baseOpts];
    if (!known && value) {
      const friendlyLabel = isCodex
        ? codexFriendlyModelLabelFor(value)
        : claudeFriendlyModelLabelFor(value);
      opts.splice(opts.length - 1, 0, [value, friendlyLabel]);
    }
    return (
      <div
        data-testid={`runtime-env-default-model-${harness}`}
        className={classNames(cardClass, "flex flex-col gap-3")}
      >
        <div className="text-xs font-semibold text-[var(--g5-text0)]">
          {providerLabel}
        </div>
        {onDefaultAgentModelChange && (
          <div className="flex items-start justify-between gap-3">
            <div className="flex flex-col gap-1 min-w-0">
              <div className="text-xs text-[var(--g5-text1)]">
                Default model
              </div>
              <div className="text-[11px] text-[var(--g5-text2)]">
                Personal model preference when {providerLabel} is selected;
                per-launch source choices still apply.
              </div>
            </div>
            <select
              className={selectClass}
              value={value}
              aria-label={`${providerLabel} default model`}
              data-testid={`runtime-env-default-model-select-${harness}`}
              onChange={(e) => {
                let v = e.target.value;
                if (v === CUSTOM_VALUE) {
                  const c = window.prompt("Custom value:");
                  if (c == null) return;
                  v = c.trim();
                }
                onDefaultAgentModelChange?.(harness, v);
              }}
            >
              {opts.map(([v, l]) => (
                <option key={`${v}::${l}`} value={v}>
                  {l}
                </option>
              ))}
            </select>
          </div>
        )}
        {showAccountControls && updateDefault && (
          <ProviderAccountAssignmentRow
            harness={harness}
            orgId={orgId}
            accounts={accounts}
            triple={assignmentTriples[harness]}
            configsReady={ready}
            runtimeVariablesAvailable={runtimeVariablesAvailable[harness]}
            scope={defaultScope}
            updateConfig={updateDefault}
          />
        )}
      </div>
    );
  }

  function renderCustomRow(vd: CustomVar): React.ReactNode {
    const isOpen = openKey === vd.key;
    const scope = scopeForKey(vd.key);
    return (
      <div
        key={vd.key}
        data-testid={`runtime-env-row-${vd.key}`}
        className={cardClass}
      >
        <div className="flex items-start justify-between gap-3">
          <div className="flex flex-col gap-1 min-w-0">
            <div className="font-mono text-xs text-[var(--g5-text1)]">
              {vd.key}
              {isSecretType(vd.type) && (
                <span
                  className={classNames(
                    "ml-1.5 rounded-full px-1.5 text-[10px]",
                    "border border-[var(--g5-warn)]/40 text-[var(--g5-warn)]",
                  )}
                >
                  secret
                </span>
              )}
            </div>
            <div>{effectiveDisplay(vd)}</div>
            <div className="flex flex-wrap gap-1.5 mt-0.5">
              {editableScopes.map((s) => (
                <span
                  key={s}
                  className={classNames(
                    "rounded-full px-1.5 text-[10px]",
                    "border border-[var(--g5-stroke)] text-[var(--g5-text2)]",
                  )}
                >
                  {SCOPE_LABEL[s]}: {scopePillValue(vd, s)}
                </span>
              ))}
            </div>
          </div>
          <Button
            kind="secondary"
            onClick={() => openOverride(vd.key)}
            data-testid={`runtime-env-override-${vd.key}`}
          >
            {isOpen ? "Close" : "Override"}
          </Button>
        </div>

        {isOpen && (
          <div
            className={classNames(
              "mt-3 rounded-[var(--g5-radius-sm)] border border-dashed",
              "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] p-3",
              "flex flex-col gap-2",
            )}
            data-testid={`runtime-env-drawer-${vd.key}`}
          >
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-[11px] text-[var(--g5-text2)]">
                Set at:
              </span>
              <select
                className={classNames(
                  "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
                  "bg-[var(--g5-bg1)] px-2 py-1.5 text-xs text-[var(--g5-text0)]",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                )}
                value={scope}
                aria-label={`Set ${vd.key} at scope`}
                data-testid={`runtime-env-scope-${vd.key}`}
                onChange={(e) => {
                  const next = e.target.value as Scope;
                  setDrawerScope((prev) => ({ ...prev, [vd.key]: next }));
                  clearDraft(vd.key);
                }}
              >
                {editableScopes.map((s) => (
                  <option key={s} value={s} disabled={!scopeSelectable(s)}>
                    {SCOPE_LABEL[s]}
                  </option>
                ))}
              </select>

              {renderValueControl(vd)}

              <Button
                kind="primary"
                disabled={!scopeSelectable(scope)}
                data-testid={`runtime-env-apply-${vd.key}`}
                onClick={() => {
                  if (!scopeSelectable(scope)) {
                    return;
                  }
                  const value = isSecretType(vd.type)
                    ? (drawerDraft[vd.key] ?? "")
                    : drawerDraft[vd.key] !== undefined
                      ? drawerDraft[vd.key]
                      : (settings[vd.key]?.[scope] ?? "");
                  if (isSecretType(vd.type) && value === "") {
                    // No new secret typed — Apply is a no-op. Removing an
                    // existing secret requires the explicit Clear button, so
                    // a token is never deleted just by opening/closing the
                    // drawer.
                    setOpenKey(null);
                    clearDraft(vd.key);
                    return;
                  }
                  stageSet(vd, scope, value);
                  setOpenKey(null);
                  clearDraft(vd.key);
                }}
              >
                Apply
              </Button>
              <Button
                kind="secondary"
                disabled={!scopeSelectable(scope)}
                data-testid={`runtime-env-clear-${vd.key}`}
                onClick={() => {
                  stageClear(vd, scope);
                  clearDraft(vd.key);
                }}
              >
                Clear at {SCOPE_LABEL[scope]}
              </Button>
            </div>
          </div>
        )}
      </div>
    );
  }

  // -------------------------------------------------------------------------
  // Render
  // -------------------------------------------------------------------------

  return (
    <div
      data-testid="runtime-env-config"
      className={classNames(
        "rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)]",
        "bg-[var(--g5-bg1)] p-4 flex flex-col gap-3",
      )}
    >
      {showScopedConfig && loadError && (
        <p className="text-sm text-[var(--g5-blocked)]">{loadError.message}</p>
      )}

      {showScopedConfig && !ready && !loadError && (
        <div className="flex items-center gap-2 text-sm text-[var(--g5-text2)]">
          <Loader2 className="w-4 h-4 animate-spin" />
          Loading configuration…
        </div>
      )}

      {/* 1. Default agent (scoped config; immediate persist). */}
      {showScopedConfig && ready && renderDefaultAgentRow()}

      {/* 2. One row per visible agent: default model (personal preference,
          always visible), account (kill-switch + scoped config gated), and
          the resolver-fed resolution line. */}
      {(onDefaultAgentModelChange ||
        (organizationContext && showAccountControls)) && (
        <div className="flex flex-col gap-2.5">
          {renderAgentRow("claude_code")}
          {renderAgentRow("codex")}
        </div>
      )}

      {/* 3. Other variables: the custom-variable editor (D5), unchanged. */}
      {showScopedConfig && ready && (
        <>
          <SectionTitle>Other variables</SectionTitle>

          {customVars.length > 0 && (
            <div className="flex flex-col gap-2.5">
              {customVars.map(renderCustomRow)}
            </div>
          )}

          <div>
            <Button
              kind="secondary"
              onClick={handleAddVariable}
              data-testid="runtime-env-add-var"
            >
              + Add custom variable
            </Button>
          </div>

          {saveError && (
            <p className="text-xs text-[var(--g5-blocked)]">{saveError}</p>
          )}

          <div className="flex items-center gap-3 pt-3 border-t border-[var(--g5-stroke)]">
            <span
              className="text-xs"
              style={{
                color: dirty ? "var(--g5-warn)" : "var(--g5-ok)",
              }}
              data-testid="runtime-env-status"
            >
              {dirty ? "● Unsaved changes" : "✓ All changes saved"}
            </span>
            <span className="flex-1" />
            <Button
              kind="secondary"
              onClick={handleDiscard}
              disabled={!dirty || saving}
              data-testid="runtime-env-discard"
            >
              Discard changes
            </Button>
            <Button
              kind="primary"
              onClick={handleSave}
              disabled={!dirty || saving}
              data-testid="runtime-env-save"
            >
              {saving ? "Saving…" : "Save changes"}
            </Button>
          </div>
        </>
      )}
    </div>
  );
};
