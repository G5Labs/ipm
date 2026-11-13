// =============================================================================
// ProviderAccountsSection — named provider accounts as expandable rows
// =============================================================================
// Ontology: software_architecture/client/settings-page D16 (section contract:
// the accounts LIST ONLY — the per-agent assignment controls live in the
// Defaults card, D19; this section reads the assignment through
// `useAssignmentConfigs` only so the config-derived assigned ids feed the
// Remove guard, D17), D18 (per-window usage rings on rows whose list DTO
// carries an account-keyed snapshot; "No usage data" otherwise), D2
// (placement between Defaults and Access, gated on the `provider_accounts`
// instance feature), D5 (inline dashed drawer idiom);
// system_invariants/external_agent_provider_contract D14 (closed kind
// catalog, scope fixed at creation, kill-switch never deletes);
// security_policy/secret_custody_and_redaction D9 (the row shows only the
// server's bounded hint; the client never derives one from typed input);
// user_experience_model/agent_interaction_ux D2 (controls disable with an
// explanation). Scenario owners: test_contract/provider_accounts I6 (rows)
// and I11 (usage rings).
//
// Collapsed row: chevron · name · type pill (Subscription / API) · scope pill
// (You / Workspace / Org) · [5h / 7d usage cells] · monospace hint. Clicking
// the row toggles an
// inline dashed drawer with a two-column form; Save sends only the changed
// fields and omits a blank credential; kind and scope are immutable after
// create. Editable drawers carry a per-kind setup guide (plain steps to get
// the credential, then the provider's official documentation link) under
// Type. "+ Add account" appends one expanded new row with Name focused.
// =============================================================================

import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ChevronRight, ExternalLink, Loader2 } from "lucide-react";

import { ApiError } from "../../lib/api";
import type {
  CreateProviderAccountInput,
  PickerHarness,
  ProviderAccountDTO,
  ProviderAccountKind,
  ProviderAccountScope,
  ProviderAccountUsage,
  UpdateProviderAccountInput,
} from "../../lib/types";
import {
  useCreateProviderAccount,
  useDeleteProviderAccount,
  useProviderAccounts,
  useUpdateProviderAccount,
} from "../../hooks/useProviderAccounts";
import { useProviderAccountsEnabled } from "../../hooks/useSettings";
import {
  normalizeSecret,
  secretShapeError,
} from "../../lib/providerAccountSecret";
import { Button, SectionTitle, ThemedSelect, classNames } from "../ui/UI";
import { formatAgo, formatReset } from "../chat/SubscriptionUsageChip";
import {
  blockingAssignmentIdsFrom,
  effectiveAssignment,
  useAssignmentConfigs,
} from "./ProviderAccountAssignment";
import { UsageRing, usagePercent } from "./UsageRing";

// ---------------------------------------------------------------------------
// Kind catalog presentation (external_agent_provider_contract D14)
// ---------------------------------------------------------------------------

type TypePill = "Subscription" | "API";
type BaseUrlMode = "fixed" | "input" | "preset";

interface KindMeta {
  label: string;
  typePill: TypePill;
  /** Credential field label; `null` = no stored credential for this kind. */
  credentialLabel: string | null;
  baseUrl: BaseUrlMode;
}

const KIND_META: Record<ProviderAccountKind, KindMeta> = {
  claude_subscription: {
    label: "Claude subscription",
    typePill: "Subscription",
    credentialLabel: "OAuth token",
    baseUrl: "fixed",
  },
  anthropic_api_key: {
    label: "Anthropic API key",
    typePill: "API",
    credentialLabel: "API key",
    baseUrl: "input",
  },
  anthropic_compatible_bearer: {
    label: "Anthropic-compatible endpoint (GLM, …)",
    typePill: "API",
    credentialLabel: "Bearer token",
    baseUrl: "preset",
  },
  openrouter_api_key: {
    label: "OpenRouter API key",
    typePill: "API",
    credentialLabel: "API key",
    baseUrl: "fixed",
  },
  openai_api_key: {
    label: "OpenAI API key",
    typePill: "API",
    credentialLabel: "API key",
    baseUrl: "input",
  },
  chatgpt_subscription: {
    label: "ChatGPT subscription",
    typePill: "Subscription",
    credentialLabel: null,
    baseUrl: "fixed",
  },
};

const KIND_ORDER: ProviderAccountKind[] = [
  "claude_subscription",
  "anthropic_api_key",
  "anthropic_compatible_bearer",
  "openrouter_api_key",
  "openai_api_key",
  "chatgpt_subscription",
];

/** Anthropic-compatible endpoint presets offered for `anthropic_compatible_bearer`. */
const COMPATIBLE_BASE_URL_PRESETS: ReadonlyArray<{
  value: string;
  label: string;
}> = [
  {
    value: "https://api.z.ai/api/anthropic",
    label: "api.z.ai — Anthropic-compatible (global)",
  },
  {
    value: "https://open.bigmodel.cn/api/anthropic",
    label: "open.bigmodel.cn — Anthropic-compatible (China mainland)",
  },
];

const CUSTOM_BASE_URL = "custom";

// ---------------------------------------------------------------------------
// Per-kind setup guide (settings-page D16): plain-language steps for getting
// the credential each kind stores, then a link to the provider's own
// documentation. Steps describe the provider's UI as of 2026-10-30; the
// linked page is the authority when they drift.
// ---------------------------------------------------------------------------

interface KindGuide {
  /** Short, numbered steps anyone can follow to obtain the credential. */
  steps: ReadonlyArray<string>;
  /** Official provider documentation, opened in a new tab. */
  docs: ReadonlyArray<{ label: string; href: string }>;
}

const KIND_GUIDE: Record<ProviderAccountKind, KindGuide> = {
  claude_subscription: {
    steps: [
      "On a computer with Claude Code installed, sign in to the Claude Pro, Max, Team or Enterprise plan you want to use.",
      "Run `claude setup-token` in a terminal and approve the request in the browser window it opens.",
      "Copy the token printed in the terminal (it starts with sk-ant-oat01) and paste it below. It is valid for one year.",
    ],
    docs: [
      {
        label: "Claude Code authentication — long-lived token",
        href: "https://code.claude.com/docs/en/authentication#generate-a-long-lived-token",
      },
    ],
  },
  anthropic_api_key: {
    steps: [
      "Sign in to the Claude Console at platform.claude.com, or create an account there.",
      "Open Settings → API keys and click Create key.",
      "Copy the key (it starts with sk-ant-) and paste it below. Leave Base URL empty unless your organization routes requests through its own gateway.",
    ],
    docs: [
      {
        label: "Get your Claude API key",
        href: "https://platform.claude.com/docs/en/get-api-key",
      },
    ],
  },
  anthropic_compatible_bearer: {
    steps: [
      "Sign in to the provider's platform. For GLM, use z.ai (global) or bigmodel.cn (China mainland) and subscribe to a Coding Plan.",
      "Create an API key on your plan's overview page and copy it.",
      "Paste it below as the bearer token and pick the matching Base URL, or choose Custom… for another Anthropic-compatible endpoint.",
    ],
    docs: [
      {
        label: "GLM Coding Plan with Claude Code (z.ai)",
        href: "https://docs.z.ai/devpack/tool/claude",
      },
      {
        label: "GLM Coding Plan with Claude Code (bigmodel.cn)",
        href: "https://docs.bigmodel.cn/cn/coding-plan/tool/claude",
      },
    ],
  },
  openrouter_api_key: {
    steps: [
      "Sign in at openrouter.ai and add credits or set up billing.",
      "Open Keys and click Create API key.",
      "Copy the key (it starts with sk-or-) and paste it below. One OpenRouter key serves both Claude and Codex agents.",
    ],
    docs: [
      {
        label: "OpenRouter authentication",
        href: "https://openrouter.ai/docs/api-reference/authentication",
      },
    ],
  },
  openai_api_key: {
    steps: [
      "Sign in to the OpenAI platform at platform.openai.com and make sure billing is set up.",
      "Open API keys and click Create new secret key.",
      "Copy the key (it starts with sk-) and paste it below. Leave Base URL empty unless your organization routes requests through its own gateway.",
    ],
    docs: [
      {
        label: "OpenAI developer quickstart",
        href: "https://developers.openai.com/api/docs/quickstart",
      },
    ],
  },
  chatgpt_subscription: {
    steps: [
      "Nothing to paste here: you sign in inside the workspace's own VM, and the login never leaves it.",
      "In ChatGPT, turn on device code login under Settings → Security (personal plan) or in workspace permissions (ask your ChatGPT workspace admin).",
      "The first Codex turn in a workspace shows a link and a one-time code in chat. Open the link, sign in with your ChatGPT account and enter the code within five minutes.",
    ],
    docs: [
      {
        label: "Codex authentication",
        href: "https://learn.chatgpt.com/docs/auth",
      },
    ],
  },
};

const SCOPE_LABEL: Record<ProviderAccountScope, string> = {
  user: "You",
  workspace: "Workspace",
  org: "Org",
};

const NO_STORED_CREDENTIAL_TEXT =
  "Resolved for each execution from the current workspace owner; nothing stored in session history.";

/** Typed error codes rendered inline under the row (settings-page D16). */
const ERROR_MESSAGES: Record<string, string> = {
  account_kind_mismatch:
    "This account's type does not match the agent it is assigned to.",
  account_base_url_invalid: "Base URL must be an https:// URL.",
  account_secret_invalid:
    "The credential is missing or not valid for this account type.",
  account_scope_forbidden: "You cannot create or edit accounts at this scope.",
  account_name_invalid: "Enter a valid, non-empty name.",
  account_name_conflict:
    "An account with this name already exists at this scope.",
  account_assigned:
    "This account is assigned to an agent at another scope. Clear that assignment first, then remove.",
};

const FORBIDDEN_MESSAGE = "You cannot edit accounts at this scope";

/**
 * Extract the typed account token from the exact wire carriers the routes
 * emit (settings-page D16). Validation failures: HTTP 422,
 * `error.code = "VALIDATION_ERROR"`, token in `error.details.code`.
 * Conflicts: HTTP 409, `error.code = "CONFLICT"`, `error.message` starts
 * with the token followed by ": text". `error.code` itself is never the token.
 */
function accountErrorToken(error: ApiError): string | null {
  if (error.status === 422 && error.code === "VALIDATION_ERROR") {
    const token = error.details.code;
    return typeof token === "string" && ERROR_MESSAGES[token] ? token : null;
  }
  if (error.status === 409 && error.code === "CONFLICT") {
    // The envelope message is "Conflict: <token>: <text>" (the generic
    // "Conflict:" prefix comes from the error's Display impl), so the token
    // is the first colon segment that names a known account error.
    const token = error.message
      .split(":")
      .map((segment) => segment.trim())
      .find((segment) => Boolean(ERROR_MESSAGES[segment]));
    return token ?? null;
  }
  return null;
}

function errorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    const token = accountErrorToken(error);
    if (token === "account_secret_invalid") {
      // The server names the expected shape (prefix, one line); prefer it.
      const detail = error.details.error;
      if (typeof detail === "string" && detail) return detail;
    }
    if (token) return ERROR_MESSAGES[token];
    if (error.status === 403) return FORBIDDEN_MESSAGE;
    const detail = error.details.message;
    if (typeof detail === "string" && detail) return detail;
    return error.message || "The request failed.";
  }
  if (error instanceof Error && error.message) return error.message;
  return "The request failed.";
}

// ---------------------------------------------------------------------------
// Drafts
// ---------------------------------------------------------------------------

interface RowDraft {
  name: string;
  secret: string;
  /** Preset select value for `preset` kinds; `custom` reveals the input. */
  preset: string;
  /** Free-form base URL (input kinds, or the Custom… preset). */
  baseUrl: string;
}

interface NewDraft extends RowDraft {
  kind: ProviderAccountKind;
  scope: ProviderAccountScope;
}

const NEW_ROW_ID = "new";

function presetFor(baseUrl: string | null): string {
  if (!baseUrl) return COMPATIBLE_BASE_URL_PRESETS[0].value;
  return COMPATIBLE_BASE_URL_PRESETS.some((p) => p.value === baseUrl)
    ? baseUrl
    : CUSTOM_BASE_URL;
}

function draftFromAccount(account: ProviderAccountDTO): RowDraft {
  const preset = presetFor(account.baseUrl);
  return {
    name: account.name,
    secret: "",
    preset,
    baseUrl: account.baseUrl ?? "",
  };
}

/** Effective base URL the draft would submit for the given kind (`null` = none). */
function effectiveBaseUrl(
  kind: ProviderAccountKind,
  draft: RowDraft,
): string | null {
  const mode = KIND_META[kind].baseUrl;
  if (mode === "fixed") return null;
  if (mode === "preset") {
    return draft.preset === CUSTOM_BASE_URL
      ? draft.baseUrl.trim()
      : draft.preset;
  }
  return draft.baseUrl.trim();
}

function computePatch(
  account: ProviderAccountDTO,
  draft: RowDraft,
): UpdateProviderAccountInput {
  const patch: UpdateProviderAccountInput = {};
  const name = draft.name.trim();
  if (name !== account.name) patch.name = name;
  const nextBaseUrl = effectiveBaseUrl(account.kind, draft);
  if (nextBaseUrl !== null && nextBaseUrl !== (account.baseUrl ?? "")) {
    patch.baseUrl = nextBaseUrl;
  }
  // A blank credential means "keep the stored one": never send `secret`.
  const secret = normalizeSecret(draft.secret);
  if (secret.length > 0) patch.secret = secret;
  return patch;
}

function formatAdded(iso: string): string {
  return iso.length >= 10 ? iso.slice(0, 10) : iso;
}

// ---------------------------------------------------------------------------
// Usage rings (settings-page D18; provider quota snapshot D13; UX D8)
// ---------------------------------------------------------------------------

type UsageWindow = "5h" | "7d";

const USAGE_WINDOWS: ReadonlyArray<{ tag: UsageWindow; name: string }> = [
  { tag: "5h", name: "5-hour" },
  { tag: "7d", name: "7-day" },
];

/** Tick interval for the capture-age / reset clock the rings render against. */
const USAGE_CLOCK_TICK_MS = 60_000;

interface UsageWindowView {
  percent: number | null;
  /** "62% used" | "no data" */
  usedText: string;
  /** "resets in 2h · as of 5m ago" (reset omitted when unknown) */
  detail: string;
  /** Hover title: "<window>: <usedText> · <detail>" */
  note: string;
}

function usageWindowView(
  usage: ProviderAccountUsage,
  window: { tag: UsageWindow; name: string },
  nowMs: number,
): UsageWindowView {
  const utilization =
    window.tag === "5h" ? usage.fiveHourUtilization : usage.sevenDayUtilization;
  const resetAt =
    window.tag === "5h" ? usage.fiveHourResetAt : usage.sevenDayResetAt;
  const percent = usagePercent(utilization);
  const usedText = percent == null ? "no data" : `${percent}% used`;
  const reset = formatReset(resetAt, nowMs);
  const age = formatAgo(usage.capturedAt, nowMs) ?? "unknown";
  const detail = [reset ? reset.text : null, `as of ${age}`]
    .filter((part): part is string => part !== null)
    .join(" · ");
  return {
    percent,
    usedText,
    detail,
    note: `${window.name}: ${usedText} · ${detail}`,
  };
}

// ---------------------------------------------------------------------------
// Small presentational pieces
// ---------------------------------------------------------------------------

const HARNESS_LABEL: Record<PickerHarness, string> = {
  claude_code: "Claude Code",
  codex: "Codex",
};

const Pill: React.FC<{
  children: React.ReactNode;
  tone?: "info" | "neutral";
  testId?: string;
}> = ({ children, tone = "neutral", testId }) => (
  <span
    className={classNames(
      "rounded-full border px-1.5 text-[10px] leading-4 whitespace-nowrap",
      tone === "info"
        ? "border-[color-mix(in_srgb,var(--g5-info)_40%,transparent)] text-[var(--g5-info)]"
        : "border-[var(--g5-stroke)] text-[var(--g5-text2)]",
    )}
    data-testid={testId}
  >
    {children}
  </span>
);

const fieldInputClass = classNames(
  "w-full rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
  "bg-[var(--g5-bg1)] px-2 py-1.5 text-xs text-[var(--g5-text0)]",
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
  "disabled:opacity-60",
);

const Field: React.FC<{
  label: string;
  htmlFor?: string;
  children: React.ReactNode;
}> = ({ label, htmlFor, children }) => (
  <div className="flex min-w-0 flex-col gap-1">
    <label htmlFor={htmlFor} className="text-[11px] text-[var(--g5-text2)]">
      {label}
    </label>
    {children}
  </div>
);

const ReadOnlyValue: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => <div className="py-1.5 text-xs text-[var(--g5-text0)]">{children}</div>;

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface ProviderAccountsSectionProps {
  orgId: string;
  context?: "workspace" | "organization";
  workspaceId?: string | null;
  canEditOrg: boolean;
  canEditWorkspace: boolean;
  /** Ids currently referenced by an assignment; Remove is disabled for them. */
  assignedAccountIds?: readonly string[];
}

export const ProviderAccountsSection: React.FC<
  ProviderAccountsSectionProps
> = ({
  orgId,
  context = "workspace",
  workspaceId,
  canEditOrg,
  canEditWorkspace,
  assignedAccountIds,
}) => {
  const enabled = useProviderAccountsEnabled();
  if (!enabled) return null;
  return (
    <ProviderAccountsSectionBody
      orgId={orgId}
      context={context}
      workspaceId={workspaceId}
      canEditOrg={canEditOrg}
      canEditWorkspace={canEditWorkspace}
      assignedAccountIds={assignedAccountIds}
    />
  );
};

const ProviderAccountsSectionBody: React.FC<ProviderAccountsSectionProps> = ({
  orgId,
  context = "workspace",
  workspaceId,
  canEditOrg,
  canEditWorkspace,
  assignedAccountIds,
}) => {
  const accounts = useProviderAccounts(orgId, workspaceId);
  const createAccount = useCreateProviderAccount(orgId);
  const updateAccount = useUpdateProviderAccount(orgId);
  const deleteAccount = useDeleteProviderAccount(orgId);
  // Per-agent assignment (settings-page D17): read here ONLY for the Remove
  // guard below; the assignment controls themselves live in the Defaults
  // card (D19).
  const assignmentConfigs = useAssignmentConfigs(orgId, workspaceId);

  const [openIds, setOpenIds] = useState<Set<string>>(() => new Set());
  const [drafts, setDrafts] = useState<Record<string, RowDraft>>({});
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [busyId, setBusyId] = useState<string | null>(null);
  // Ids the server refused to delete with `account_assigned` (kept disabled
  // with the reason until the list refetches a change).
  const [serverAssigned, setServerAssigned] = useState<Set<string>>(
    () => new Set(),
  );
  const [newDraft, setNewDraft] = useState<NewDraft | null>(null);
  const newNameRef = useRef<HTMLInputElement>(null);
  // Minute-ticking clock so the usage rings' capture age and reset text stay
  // live (settings-page D18) while render stays pure.
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(
      () => setNowMs(Date.now()),
      USAGE_CLOCK_TICK_MS,
    );
    return () => window.clearInterval(timer);
  }, []);

  const rows = useMemo(
    () =>
      accounts.status === "ok"
        ? accounts.data.filter((account) =>
            context === "organization"
              ? account.scope === "org"
              : account.scope !== "org",
          )
        : [],
    [accounts, context],
  );

  // Remove is refused only for assignments outside the account's own scope
  // (chat g5.chat.account-defaults): the own scope's assignments go with
  // the row, so a person's only account can always be removed.
  const assignedSet = useMemo(() => {
    const set = new Set<string>(assignedAccountIds ?? []);
    blockingAssignmentIdsFrom(assignmentConfigs, rows).forEach((id) =>
      set.add(id),
    );
    serverAssigned.forEach((id) => set.add(id));
    return set;
  }, [assignedAccountIds, assignmentConfigs, rows, serverAssigned]);

  // The agents this account is the default for, as this page sees them:
  // Admin shows the organization's own assignment, Settings the effective
  // one (chat g5.chat.account-defaults).
  const defaultFor = (accountId: string): PickerHarness[] =>
    (["claude_code", "codex"] as const).filter((harness) => {
      const triple = assignmentConfigs.triples[harness];
      const assigned =
        context === "organization"
          ? triple.org
          : effectiveAssignment(triple).value;
      return assigned === accountId;
    });

  // Focus Name once when the new row appears; later draft edits do not refocus.
  const hasNewRow = newDraft !== null;
  useEffect(() => {
    if (hasNewRow) newNameRef.current?.focus();
  }, [hasNewRow]);

  const canEditScope = useCallback(
    (scope: ProviderAccountScope): boolean => {
      if (context === "organization") return scope === "org" && canEditOrg;
      if (scope === "org") return false;
      if (scope === "user") return true;
      if (scope === "workspace") return canEditWorkspace && !!workspaceId;
      return canEditOrg;
    },
    [canEditOrg, canEditWorkspace, workspaceId, context],
  );

  const setError = (id: string, message: string | null) =>
    setErrors((prev) => {
      const next = { ...prev };
      if (message) next[id] = message;
      else delete next[id];
      return next;
    });

  const toggleRow = (account: ProviderAccountDTO) => {
    setOpenIds((prev) => {
      const next = new Set(prev);
      if (next.has(account.id)) {
        next.delete(account.id);
      } else {
        next.add(account.id);
      }
      return next;
    });
    setDrafts((prev) =>
      prev[account.id]
        ? prev
        : { ...prev, [account.id]: draftFromAccount(account) },
    );
  };

  const closeRow = (id: string) => {
    setOpenIds((prev) => {
      const next = new Set(prev);
      next.delete(id);
      return next;
    });
    setDrafts((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
    setError(id, null);
  };

  const updateDraft = (id: string, patch: Partial<RowDraft>) =>
    setDrafts((prev) => ({ ...prev, [id]: { ...prev[id], ...patch } }));

  const saveRow = async (account: ProviderAccountDTO) => {
    const draft = drafts[account.id];
    if (!draft) return;
    const patch = computePatch(account, draft);
    if (Object.keys(patch).length === 0) return;
    const shapeError = secretShapeError(
      account.kind,
      draft.secret,
      effectiveBaseUrl(account.kind, draft),
    );
    if (shapeError) {
      setError(account.id, shapeError);
      return;
    }
    setBusyId(account.id);
    setError(account.id, null);
    try {
      await updateAccount.mutateAsync({ accountId: account.id, patch });
      closeRow(account.id);
    } catch (error) {
      setError(account.id, errorMessage(error));
    } finally {
      setBusyId(null);
    }
  };

  const removeRow = async (account: ProviderAccountDTO) => {
    setBusyId(account.id);
    setError(account.id, null);
    try {
      await deleteAccount.mutateAsync(account.id);
      closeRow(account.id);
    } catch (error) {
      if (
        error instanceof ApiError &&
        accountErrorToken(error) === "account_assigned"
      ) {
        setServerAssigned((prev) => new Set(prev).add(account.id));
      }
      setError(account.id, errorMessage(error));
    } finally {
      setBusyId(null);
    }
  };

  const startAdd = () => {
    if (newDraft) {
      newNameRef.current?.focus();
      return;
    }
    setNewDraft({
      name: "",
      secret: "",
      preset: COMPATIBLE_BASE_URL_PRESETS[0].value,
      baseUrl: "",
      kind: "claude_subscription",
      scope: context === "organization" ? "org" : "user",
    });
    setError(NEW_ROW_ID, null);
  };

  const cancelAdd = () => {
    setNewDraft(null);
    setError(NEW_ROW_ID, null);
  };

  const saveNew = async () => {
    if (!newDraft || !canEditScope(newDraft.scope)) return;
    const name = newDraft.name.trim();
    if (!name) {
      setError(NEW_ROW_ID, ERROR_MESSAGES.account_name_invalid);
      return;
    }
    const input: CreateProviderAccountInput = {
      name,
      kind: newDraft.kind,
      scope: newDraft.scope,
    };
    if (newDraft.scope === "workspace" && workspaceId) {
      input.workspaceId = workspaceId;
    }
    const baseUrl = effectiveBaseUrl(newDraft.kind, newDraft);
    if (baseUrl) input.baseUrl = baseUrl;
    const shapeError = secretShapeError(
      newDraft.kind,
      newDraft.secret,
      baseUrl,
    );
    if (shapeError) {
      setError(NEW_ROW_ID, shapeError);
      return;
    }
    const secret = normalizeSecret(newDraft.secret);
    if (secret.length > 0) input.secret = secret;
    setBusyId(NEW_ROW_ID);
    setError(NEW_ROW_ID, null);
    try {
      await createAccount.mutateAsync(input);
      setNewDraft(null);
    } catch (error) {
      setError(NEW_ROW_ID, errorMessage(error));
    } finally {
      setBusyId(null);
    }
  };

  // -------------------------------------------------------------------------
  // Render helpers
  // -------------------------------------------------------------------------

  const renderBaseUrlField = (
    id: string,
    kind: ProviderAccountKind,
    draft: RowDraft,
    onChange: (patch: Partial<RowDraft>) => void,
    disabled: boolean,
  ) => {
    const mode = KIND_META[kind].baseUrl;
    if (mode === "fixed") return null;
    if (mode === "input") {
      return (
        <Field label="Base URL" htmlFor={`provider-account-base-url-${id}`}>
          <input
            id={`provider-account-base-url-${id}`}
            className={fieldInputClass}
            type="url"
            inputMode="url"
            placeholder="https://"
            value={draft.baseUrl}
            disabled={disabled}
            data-testid={`provider-account-base-url-${id}`}
            onChange={(e) => onChange({ baseUrl: e.target.value })}
          />
        </Field>
      );
    }
    const presetOptions = [
      ...COMPATIBLE_BASE_URL_PRESETS.map((p) => ({
        value: p.value,
        label: p.label,
      })),
      { value: CUSTOM_BASE_URL, label: "Custom…" },
    ];
    return (
      <Field label="Base URL">
        <ThemedSelect
          value={draft.preset}
          options={presetOptions}
          disabled={disabled}
          size="sm"
          testId={`provider-account-base-url-preset-${id}`}
          onChange={(value) => onChange({ preset: value })}
        />
        {draft.preset === CUSTOM_BASE_URL && (
          <input
            className={fieldInputClass}
            type="url"
            inputMode="url"
            placeholder="https://"
            aria-label="Custom base URL"
            value={draft.baseUrl}
            disabled={disabled}
            data-testid={`provider-account-base-url-${id}`}
            onChange={(e) => onChange({ baseUrl: e.target.value })}
          />
        )}
      </Field>
    );
  };

  // Per-kind setup steps and the provider's documentation link, spanning the
  // drawer's two columns right under Type so the steps are read before the
  // credential is pasted.
  const renderKindGuide = (id: string, kind: ProviderAccountKind) => {
    const guide = KIND_GUIDE[kind];
    return (
      <div
        className="sm:col-span-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2.5"
        data-testid={`provider-account-guide-${id}`}
        data-kind={kind}
      >
        <div className="text-[11px] font-medium text-[var(--g5-text0)]">
          How to get this credential
        </div>
        <ol className="mt-1.5 flex list-decimal flex-col gap-1 pl-4 text-[11px] leading-4 text-[var(--g5-text2)]">
          {guide.steps.map((step, index) => (
            <li key={index}>{step}</li>
          ))}
        </ol>
        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-[var(--g5-text2)]">
          <span>Full documentation:</span>
          {guide.docs.map((doc) => (
            <a
              key={doc.href}
              href={doc.href}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-[var(--g5-info)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]"
              data-testid={`provider-account-guide-link-${id}`}
            >
              {doc.label}
              <ExternalLink aria-hidden="true" className="h-3 w-3" />
            </a>
          ))}
        </div>
      </div>
    );
  };

  const renderCredentialField = (
    id: string,
    kind: ProviderAccountKind,
    hint: string | null,
    draft: RowDraft,
    onChange: (patch: Partial<RowDraft>) => void,
    disabled: boolean,
  ) => {
    const label = KIND_META[kind].credentialLabel;
    if (label === null) {
      return (
        <Field label="Auth">
          <ReadOnlyValue>{NO_STORED_CREDENTIAL_TEXT}</ReadOnlyValue>
        </Field>
      );
    }
    const placeholder = hint
      ? `${hint}…  (leave blank to keep)`
      : id === NEW_ROW_ID
        ? "Paste the credential"
        : "(leave blank to keep)";
    return (
      <Field label={label} htmlFor={`provider-account-secret-input-${id}`}>
        <input
          id={`provider-account-secret-input-${id}`}
          className={classNames(fieldInputClass, "font-mono")}
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={placeholder}
          value={draft.secret}
          disabled={disabled}
          data-testid={`provider-account-secret-input-${id}`}
          onChange={(e) => onChange({ secret: e.target.value })}
        />
      </Field>
    );
  };

  const renderHint = (account: ProviderAccountDTO, pushRight: boolean) =>
    account.hasSecret && account.secretHint ? (
      <span
        className={classNames(
          "min-w-[7.5rem] text-right font-mono text-[11px] text-[var(--g5-text2)] whitespace-nowrap",
          pushRight ? "ml-auto" : "ml-3",
        )}
        data-testid={`provider-account-hint-${account.id}`}
      >
        {account.secretHint}…
      </span>
    ) : (
      <span
        className={classNames(
          "min-w-[7.5rem] text-right text-[11px] italic text-[var(--g5-text2)] whitespace-nowrap",
          pushRight ? "ml-auto" : "ml-3",
        )}
        data-testid={`provider-account-hint-${account.id}`}
      >
        {account.hasSecret ? "configured" : "nothing stored"}
      </span>
    );

  // Collapsed-row usage cells (settings-page D18): one ring + label + percent
  // per window, right-aligned before the hint; `usage: null` renders nothing.
  const renderUsageCells = (account: ProviderAccountDTO) => {
    const usage = account.usage;
    if (!usage) return null;
    return (
      <span
        className="ml-auto inline-flex items-center gap-3"
        data-testid={`provider-account-usage-${account.id}`}
      >
        {USAGE_WINDOWS.map((window) => {
          const view = usageWindowView(usage, window, nowMs);
          return (
            <span
              key={window.tag}
              className="inline-flex w-[4.5rem] items-center gap-1.5 whitespace-nowrap"
              data-testid={`provider-account-usage-${window.tag}-${account.id}`}
              data-percent={view.percent ?? ""}
            >
              <UsageRing
                utilization={
                  window.tag === "5h"
                    ? usage.fiveHourUtilization
                    : usage.sevenDayUtilization
                }
                label={window.tag}
                note={view.note}
                testId={`usage-ring-${window.tag}`}
              />
              <span className="text-[10px] text-[var(--g5-text2)]">
                {window.tag}
              </span>
              <span className="tabular-nums text-[11px] text-[var(--g5-text0)]">
                {view.percent == null ? "—" : `${view.percent}%`}
              </span>
            </span>
          );
        })}
      </span>
    );
  };

  // Expanded-row Usage field (settings-page D18): one line per window, or
  // "No usage data" when the list DTO carries no snapshot.
  const renderUsageField = (account: ProviderAccountDTO) => {
    const usage = account.usage;
    return (
      <div className="sm:col-span-2">
        <Field label="Usage">
          {usage ? (
            <div className="flex flex-col gap-1.5 py-1">
              {USAGE_WINDOWS.map((window) => {
                const view = usageWindowView(usage, window, nowMs);
                return (
                  <div
                    key={window.tag}
                    className="flex items-center gap-2 text-xs"
                    data-testid={`provider-account-usage-line-${window.tag}-${account.id}`}
                    data-percent={view.percent ?? ""}
                  >
                    <UsageRing
                      utilization={
                        window.tag === "5h"
                          ? usage.fiveHourUtilization
                          : usage.sevenDayUtilization
                      }
                      label={window.tag}
                      note={view.note}
                      testId={`usage-ring-line-${window.tag}`}
                    />
                    <span className="min-w-[2.25rem] text-[var(--g5-text2)]">
                      {window.tag}
                    </span>
                    <span className="min-w-[4.25rem] tabular-nums text-[var(--g5-text0)]">
                      {view.usedText}
                    </span>
                    <span className="text-[11px] text-[var(--g5-text2)]">
                      {view.detail}
                    </span>
                  </div>
                );
              })}
            </div>
          ) : (
            <ReadOnlyValue>
              <span
                className="text-[var(--g5-text2)]"
                data-testid={`provider-account-usage-none-${account.id}`}
              >
                No usage data
              </span>
            </ReadOnlyValue>
          )}
        </Field>
      </div>
    );
  };

  const renderRow = (account: ProviderAccountDTO) => {
    const id = account.id;
    const meta = KIND_META[account.kind];
    const isOpen = openIds.has(id);
    const draft = drafts[id] ?? draftFromAccount(account);
    const editable = canEditScope(account.scope);
    const patch = computePatch(account, draft);
    const dirty = Object.keys(patch).length > 0;
    const busy = busyId === id;
    const assigned = assignedSet.has(id);
    const error = errors[id];
    const headerName = draft.name.trim() || account.name;

    return (
      <div
        key={id}
        className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
        data-testid={`provider-account-row-${id}`}
        data-open={isOpen ? "true" : "false"}
      >
        <button
          type="button"
          className={classNames(
            "flex w-full min-h-9 items-center gap-2 px-3 py-2 text-left",
            "hover:bg-[var(--g5-bg2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
          )}
          aria-expanded={isOpen}
          data-testid={`provider-account-toggle-${id}`}
          onClick={() => toggleRow(account)}
        >
          <ChevronRight
            className={classNames(
              "h-3.5 w-3.5 shrink-0 text-[var(--g5-text2)] transition-transform",
              isOpen && "rotate-90",
            )}
            aria-hidden="true"
          />
          <span
            className="truncate text-[13px] font-medium text-[var(--g5-text0)]"
            data-testid={`provider-account-name-${id}`}
          >
            {headerName}
          </span>
          <Pill tone="info" testId={`provider-account-type-pill-${id}`}>
            {meta.typePill}
          </Pill>
          <Pill testId={`provider-account-scope-pill-${id}`}>
            {SCOPE_LABEL[account.scope]}
          </Pill>
          {defaultFor(id).map((harness) => (
            <Pill
              key={harness}
              tone="info"
              testId={`provider-account-default-pill-${id}-${harness}`}
            >
              Default · {HARNESS_LABEL[harness]}
            </Pill>
          ))}
          {renderUsageCells(account)}
          {renderHint(account, account.usage === null)}
        </button>

        {isOpen && (
          <div
            className={classNames(
              "mx-3 mb-3 rounded-[var(--g5-radius-sm)] border border-dashed",
              "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] p-3 flex flex-col gap-3",
            )}
            data-testid={`provider-account-drawer-${id}`}
          >
            {!editable && (
              <div
                className="text-[11px] text-[var(--g5-text2)]"
                data-testid={`provider-account-readonly-${id}`}
              >
                {SCOPE_LABEL[account.scope]} accounts are read-only for you —
                you cannot edit accounts at this scope.
              </div>
            )}
            <div className="grid grid-cols-1 gap-x-4 gap-y-2.5 sm:grid-cols-2">
              <Field label="Name" htmlFor={`provider-account-name-input-${id}`}>
                <input
                  id={`provider-account-name-input-${id}`}
                  className={fieldInputClass}
                  type="text"
                  value={draft.name}
                  placeholder="e.g. Personal Max plan"
                  disabled={!editable || busy}
                  data-testid={`provider-account-name-input-${id}`}
                  onChange={(e) => updateDraft(id, { name: e.target.value })}
                />
              </Field>
              <Field label="Type">
                <ReadOnlyValue>{meta.label}</ReadOnlyValue>
              </Field>
              {editable && renderKindGuide(id, account.kind)}
              {renderCredentialField(
                id,
                account.kind,
                account.hasSecret ? account.secretHint : null,
                draft,
                (p) => updateDraft(id, p),
                !editable || busy,
              )}
              {renderBaseUrlField(
                id,
                account.kind,
                draft,
                (p) => updateDraft(id, p),
                !editable || busy,
              )}
              <Field label="Scope">
                <ReadOnlyValue>{SCOPE_LABEL[account.scope]}</ReadOnlyValue>
              </Field>
              <Field label="Added">
                <ReadOnlyValue>{formatAdded(account.createdAt)}</ReadOnlyValue>
              </Field>
              {renderUsageField(account)}
            </div>

            {error && (
              <div
                className="text-[11px] text-[var(--g5-warn)]"
                role="alert"
                data-testid={`provider-account-error-${id}`}
              >
                ⚠ {error}
              </div>
            )}

            {editable && (
              <div className="flex flex-wrap items-center gap-2 border-t border-[var(--g5-stroke)] pt-2.5">
                <Button
                  kind="primary"
                  disabled={!dirty || busy}
                  data-testid={`provider-account-save-${id}`}
                  onClick={() => void saveRow(account)}
                >
                  {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
                  Save
                </Button>
                <Button
                  kind="secondary"
                  disabled={busy}
                  data-testid={`provider-account-cancel-${id}`}
                  onClick={() => closeRow(id)}
                >
                  Cancel
                </Button>
                {dirty && (
                  <span
                    className="text-[11px] text-[var(--g5-warn)]"
                    data-testid={`provider-account-dirty-${id}`}
                  >
                    Unsaved changes
                  </span>
                )}
                <span className="flex-1" />
                <span
                  title={
                    assigned
                      ? ERROR_MESSAGES.account_assigned
                      : "Remove this account"
                  }
                  className="inline-flex"
                >
                  <Button
                    kind="secondary"
                    disabled={assigned || busy}
                    aria-disabled={assigned || busy}
                    title={
                      assigned
                        ? ERROR_MESSAGES.account_assigned
                        : "Remove this account"
                    }
                    data-testid={`provider-account-remove-${id}`}
                    onClick={() => void removeRow(account)}
                  >
                    Remove
                  </Button>
                </span>
              </div>
            )}
          </div>
        )}
      </div>
    );
  };

  const renderNewRow = () => {
    if (!newDraft) return null;
    const id = NEW_ROW_ID;
    const meta = KIND_META[newDraft.kind];
    const busy = busyId === id;
    const error = errors[id];
    const kindOptions = KIND_ORDER.map((kind) => ({
      value: kind,
      label: KIND_META[kind].label,
    }));
    const scopeOptions: Array<{
      value: ProviderAccountScope;
      label: string;
      disabled?: boolean;
    }> = [
      { value: "user", label: SCOPE_LABEL.user },
      {
        value: "workspace",
        label: SCOPE_LABEL.workspace,
        disabled: !canEditScope("workspace"),
      },
      { value: "org", label: SCOPE_LABEL.org, disabled: !canEditScope("org") },
    ];
    const canSave =
      newDraft.name.trim().length > 0 && !busy && canEditScope(newDraft.scope);

    return (
      <div
        className="rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]"
        data-testid={`provider-account-row-${id}`}
        data-open="true"
      >
        <div className="flex min-h-9 items-center gap-2 px-3 py-2">
          <ChevronRight
            className="h-3.5 w-3.5 shrink-0 rotate-90 text-[var(--g5-text2)]"
            aria-hidden="true"
          />
          <span
            className="truncate text-[13px] font-medium text-[var(--g5-text0)]"
            data-testid={`provider-account-name-${id}`}
          >
            {newDraft.name.trim() || "New account"}
          </span>
          <Pill tone="info">{meta.typePill}</Pill>
          <Pill>{SCOPE_LABEL[newDraft.scope]}</Pill>
          <span className="ml-auto text-[11px] italic text-[var(--g5-text2)]">
            not saved yet
          </span>
        </div>
        <div
          className={classNames(
            "mx-3 mb-3 rounded-[var(--g5-radius-sm)] border border-dashed",
            "border-[var(--g5-stroke)] bg-[var(--g5-bg2)] p-3 flex flex-col gap-3",
          )}
          data-testid={`provider-account-drawer-${id}`}
        >
          <div className="grid grid-cols-1 gap-x-4 gap-y-2.5 sm:grid-cols-2">
            <Field label="Name" htmlFor={`provider-account-name-input-${id}`}>
              <input
                ref={newNameRef}
                id={`provider-account-name-input-${id}`}
                className={fieldInputClass}
                type="text"
                value={newDraft.name}
                placeholder="e.g. Personal Max plan"
                disabled={busy}
                data-testid={`provider-account-name-input-${id}`}
                onChange={(e) =>
                  setNewDraft((prev) =>
                    prev ? { ...prev, name: e.target.value } : prev,
                  )
                }
              />
            </Field>
            <Field label="Type">
              <ThemedSelect
                value={newDraft.kind}
                options={kindOptions}
                disabled={busy}
                size="sm"
                testId="provider-account-kind-select"
                onChange={(value) =>
                  setNewDraft((prev) =>
                    prev
                      ? {
                          ...prev,
                          kind: value as ProviderAccountKind,
                          secret: "",
                          baseUrl: "",
                          preset: COMPATIBLE_BASE_URL_PRESETS[0].value,
                        }
                      : prev,
                  )
                }
              />
            </Field>
            {renderKindGuide(id, newDraft.kind)}
            {renderCredentialField(
              id,
              newDraft.kind,
              null,
              newDraft,
              (p) => setNewDraft((prev) => (prev ? { ...prev, ...p } : prev)),
              busy,
            )}
            {renderBaseUrlField(
              id,
              newDraft.kind,
              newDraft,
              (p) => setNewDraft((prev) => (prev ? { ...prev, ...p } : prev)),
              busy,
            )}
            <Field label="Scope">
              <ThemedSelect
                value={newDraft.scope}
                options={scopeOptions.filter((option) =>
                  context === "organization"
                    ? option.value === "org"
                    : option.value !== "org",
                )}
                disabled={busy}
                size="sm"
                testId="provider-account-scope-select"
                onChange={(value) =>
                  setNewDraft((prev) =>
                    prev
                      ? { ...prev, scope: value as ProviderAccountScope }
                      : prev,
                  )
                }
              />
            </Field>
          </div>

          {error && (
            <div
              className="text-[11px] text-[var(--g5-warn)]"
              role="alert"
              data-testid={`provider-account-error-${id}`}
            >
              ⚠ {error}
            </div>
          )}

          <div className="flex flex-wrap items-center gap-2 border-t border-[var(--g5-stroke)] pt-2.5">
            <Button
              kind="primary"
              disabled={!canSave}
              data-testid={`provider-account-save-${id}`}
              onClick={() => void saveNew()}
            >
              {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
              Save
            </Button>
            <Button
              kind="secondary"
              disabled={busy}
              data-testid={`provider-account-cancel-${id}`}
              onClick={cancelAdd}
            >
              Cancel
            </Button>
            <span className="text-[11px] text-[var(--g5-warn)]">
              Unsaved changes
            </span>
          </div>
        </div>
      </div>
    );
  };

  const addButton = (
    <Button
      kind="secondary"
      data-testid="provider-account-add"
      onClick={startAdd}
      disabled={busyId === NEW_ROW_ID}
    >
      + Add account
    </Button>
  );

  const renderBody = () => {
    if (accounts.status === "loading") {
      return (
        <div
          className="flex flex-col gap-2"
          aria-busy="true"
          data-testid="provider-accounts-loading"
        >
          {[0, 1].map((i) => (
            <div
              key={i}
              className="h-9 animate-pulse rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg2)]"
            />
          ))}
        </div>
      );
    }
    if (accounts.status === "error") {
      return (
        <div
          className="text-[11px] text-[var(--g5-warn)]"
          role="alert"
          data-testid="provider-accounts-error"
        >
          ⚠ Could not load accounts: {accounts.error.message}
        </div>
      );
    }
    if (rows.length === 0 && !newDraft) {
      return (
        <div
          className="flex flex-col items-start gap-3 rounded-[var(--g5-radius-sm)] border border-dashed border-[var(--g5-stroke)] p-4"
          data-testid="provider-accounts-empty"
        >
          <p className="text-xs text-[var(--g5-text2)]">
            No accounts yet. Add a subscription or an API key to give your
            agents a named credential.
          </p>
          {addButton}
        </div>
      );
    }
    return (
      <div className="flex flex-col gap-2">
        {rows.map(renderRow)}
        {renderNewRow()}
        <div>{addButton}</div>
      </div>
    );
  };

  return (
    <div
      className="flex flex-col gap-3"
      data-testid="provider-accounts-section"
    >
      <div className="flex flex-col gap-1">
        <SectionTitle>Accounts</SectionTitle>
        <p className="text-xs text-[var(--g5-text2)]">
          Subscriptions and API keys, each with a name. Click a row to expand
          and edit it. Your first account becomes the default for its agent;
          change defaults in Defaults.
        </p>
      </div>
      {renderBody()}
    </div>
  );
};
