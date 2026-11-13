// =============================================================================
// G5 Settings — OrgApiKeyManager
// =============================================================================
// API-key half of the former OrgCredentialManager. Lives on the workspace
// Admin page. Lets org admins mint scoped API keys
// (with optional expiry), reveals the plaintext key once on creation, and lists
// existing keys with a per-key revoke control.
//
// Worker registration tokens were retired with the worker-token routes; this
// manager owns only user-facing API credentials.
//
// Behavior/markup parity with the original API-keys card is preserved: the
// `ci-deploy` placeholder, the "Create API key" button label, the scope
// checkboxes, the reveal card ("New API key"), and the revoke flow all match so
// the existing e2e continues to pass.
// =============================================================================

import React, { useState } from "react";
import { KeyRound, Loader2, Trash2 } from "lucide-react";

import {
  useApiKeys,
  useCreateApiKey,
  useRevokeApiKey,
} from "../../hooks/useOrg";
import type { CreateApiKeyInput, CreateApiKeyResult } from "../../lib/types";
import { Button, SectionTitle, classNames } from "../ui/UI";
import { CredentialRevealCard } from "./CredentialRevealCard";

const API_SCOPE_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "org:read", label: "Org read" },
  { value: "org:admin", label: "Org admin" },
  { value: "workspaces:read", label: "Workspaces read" },
  { value: "workspaces:write", label: "Workspaces write" },
  { value: "workspaces:admin", label: "Workspaces admin" },
  { value: "tasks:read", label: "Tasks read" },
  { value: "tasks:execute", label: "Tasks execute" },
  { value: "sessions:read", label: "Sessions read" },
  { value: "sessions:write", label: "Sessions write" },
  { value: "users:read", label: "Users read" },
  { value: "workers:read", label: "Workers read" },
  { value: "workers:admin", label: "Workers admin" },
  { value: "settings:read", label: "Settings read" },
  { value: "settings:write", label: "Settings write" },
  { value: "billing:admin", label: "Billing admin" },
];

const EXPIRY_OPTIONS: Array<{ label: string; value: number | undefined }> = [
  { label: "No expiry", value: undefined },
  { label: "24 hours", value: 24 * 60 * 60 },
  { label: "7 days", value: 7 * 24 * 60 * 60 },
  { label: "30 days", value: 30 * 24 * 60 * 60 },
  { label: "90 days", value: 90 * 24 * 60 * 60 },
];

export interface OrgApiKeyManagerProps {
  orgId: string;
  orgName: string;
  canManage: boolean;
}

function formatTimestamp(value: string | null): string {
  return value ? new Date(value).toLocaleString() : "Never";
}

function expirySelectValue(value: number | undefined): string {
  return value == null ? "none" : String(value);
}

export const OrgApiKeyManager: React.FC<OrgApiKeyManagerProps> = ({
  orgId,
  orgName,
  canManage,
}) => {
  const apiKeysState = useApiKeys(orgId, canManage);
  const createApiKey = useCreateApiKey(orgId);
  const revokeApiKey = useRevokeApiKey(orgId);

  const [apiKeyName, setApiKeyName] = useState("");
  const [apiKeyScopes, setApiKeyScopes] = useState<string[]>([
    "org:read",
    "workspaces:read",
    "tasks:read",
  ]);
  const [apiKeyExpiry, setApiKeyExpiry] = useState<number | undefined>(
    30 * 24 * 60 * 60,
  );
  const [revealKey, setRevealKey] = useState<CreateApiKeyResult | null>(null);

  const apiKeysError =
    apiKeysState.status === "error" ? apiKeysState.error.message : null;

  const toggleApiScope = (scope: string) => {
    setApiKeyScopes((prev) =>
      prev.includes(scope)
        ? prev.filter((value) => value !== scope)
        : [...prev, scope].sort(),
    );
  };

  const handleCreateApiKey = async () => {
    const payload: CreateApiKeyInput = {
      name: apiKeyName.trim(),
      scopes: apiKeyScopes,
      expiresInSecs: apiKeyExpiry,
    };
    const result = await createApiKey.mutateAsync(payload);
    setRevealKey(result);
    setApiKeyName("");
  };

  if (!orgId) {
    return null;
  }

  return (
    <div
      className={classNames(
        "rounded-[var(--g5-radius-md)] border border-[var(--g5-stroke)]",
        "bg-[var(--g5-bg1)] p-4 flex flex-col gap-4",
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <SectionTitle>API keys</SectionTitle>
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            Manage scoped API keys for {orgName}.
          </p>
        </div>
      </div>

      {!canManage && (
        <p className="text-sm text-[var(--g5-text2)]">
          Org admin access is required to manage credentials.
        </p>
      )}

      {canManage && (
        <>
          {revealKey && (
            <CredentialRevealCard
              title="New API key"
              value={revealKey.key}
              onDismiss={() => setRevealKey(null)}
            />
          )}

          <div className="flex flex-col gap-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg2)] p-3">
            <div className="flex items-center gap-2">
              <KeyRound className="h-4 w-4 text-[var(--g5-info)]" />
              <div>
                <p className="text-sm font-medium text-[var(--g5-text0)]">
                  API keys
                </p>
                <p className="text-xs text-[var(--g5-text2)]">
                  Use explicit scopes. Internal worker scopes are intentionally
                  excluded.
                </p>
              </div>
            </div>

            <div className="grid gap-2">
              <label className="text-xs font-medium text-[var(--g5-text1)]">
                Key name
              </label>
              <input
                type="text"
                className={classNames(
                  "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
                  "bg-[var(--g5-bg1)] px-3 py-2 text-sm text-[var(--g5-text0)]",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                )}
                value={apiKeyName}
                onChange={(event) => setApiKeyName(event.target.value)}
                placeholder="ci-deploy"
              />
            </div>

            <div className="grid gap-2">
              <label className="text-xs font-medium text-[var(--g5-text1)]">
                Expiry
              </label>
              <select
                data-testid="org-api-key-expiry-select"
                className={classNames(
                  "rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)]",
                  "bg-[var(--g5-bg1)] px-3 py-2 text-sm text-[var(--g5-text0)]",
                  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]",
                )}
                value={expirySelectValue(apiKeyExpiry)}
                onChange={(event) => {
                  setApiKeyExpiry(
                    event.target.value === "none"
                      ? undefined
                      : Number(event.target.value),
                  );
                }}
              >
                {EXPIRY_OPTIONS.map((option) => (
                  <option
                    key={option.label}
                    value={expirySelectValue(option.value)}
                  >
                    {option.label}
                  </option>
                ))}
              </select>
            </div>

            <div className="grid gap-2">
              <label className="text-xs font-medium text-[var(--g5-text1)]">
                Scopes
              </label>
              <div className="grid gap-2 sm:grid-cols-2">
                {API_SCOPE_OPTIONS.map((option) => (
                  <label
                    key={option.value}
                    className="flex items-center gap-2 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2 text-xs text-[var(--g5-text1)]"
                  >
                    <input
                      data-testid="org-api-key-scope-checkbox"
                      type="checkbox"
                      checked={apiKeyScopes.includes(option.value)}
                      onChange={() => toggleApiScope(option.value)}
                    />
                    <span>{option.label}</span>
                  </label>
                ))}
              </div>
            </div>

            {(createApiKey.error || apiKeysState.status === "error") && (
              <p className="text-xs text-[var(--g5-blocked)]">
                {createApiKey.error?.message ?? apiKeysError}
              </p>
            )}

            <div className="flex justify-end">
              <Button
                data-testid="org-api-key-create"
                kind="primary"
                disabled={
                  createApiKey.isLoading ||
                  !apiKeyName.trim() ||
                  apiKeyScopes.length === 0
                }
                onClick={() => {
                  void handleCreateApiKey();
                }}
              >
                {createApiKey.isLoading && (
                  <Loader2 className="h-4 w-4 animate-spin" />
                )}
                Create API key
              </Button>
            </div>

            <div className="flex flex-col gap-2">
              {apiKeysState.status === "loading" && (
                <div className="flex items-center gap-2 text-sm text-[var(--g5-text2)]">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Loading API keys…
                </div>
              )}
              {apiKeysState.status === "ok" &&
                apiKeysState.data.length === 0 && (
                  <p className="text-xs text-[var(--g5-text2)]">
                    No API keys issued yet.
                  </p>
                )}
              {apiKeysState.status === "ok" &&
                apiKeysState.data.map((key) => (
                  <div
                    key={key.id}
                    className="flex items-start justify-between gap-3 rounded-[var(--g5-radius-sm)] border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] px-3 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-sm font-medium text-[var(--g5-text0)]">
                          {key.name}
                        </span>
                        <span className="rounded-[var(--g5-radius-sm)] bg-[var(--g5-bg2)] px-2 py-0.5 text-[10px] text-[var(--g5-text2)]">
                          {key.keyPrefix}
                        </span>
                      </div>
                      <div className="mt-1 flex flex-wrap gap-1">
                        {key.scopes.map((scope) => (
                          <span
                            key={scope}
                            className="rounded-[var(--g5-radius-sm)] bg-[var(--g5-info)]/10 px-2 py-0.5 text-[10px] text-[var(--g5-info)]"
                          >
                            {scope}
                          </span>
                        ))}
                      </div>
                      <p className="mt-1 text-[10px] text-[var(--g5-text2)]">
                        Created {formatTimestamp(key.createdAt)}. Last used{" "}
                        {formatTimestamp(key.lastUsedAt)}. Expires{" "}
                        {formatTimestamp(key.expiresAt)}.
                      </p>
                    </div>
                    <Button
                      data-testid="org-api-key-revoke"
                      kind="ghost"
                      disabled={revokeApiKey.isLoading || !!key.revokedAt}
                      onClick={() => {
                        if (!window.confirm(`Revoke API key '${key.name}'?`)) {
                          return;
                        }
                        void revokeApiKey.mutateAsync({ keyId: key.id });
                      }}
                    >
                      <Trash2 className="h-4 w-4" />
                      {key.revokedAt ? "Revoked" : "Revoke"}
                    </Button>
                  </div>
                ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
};
