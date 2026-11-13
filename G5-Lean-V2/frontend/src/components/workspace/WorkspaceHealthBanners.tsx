// =============================================================================
// G5 Workspace Page — WorkspaceHealthBanners
// =============================================================================
// A full-width warning banner rendered by the shell directly
// below the workbench header. They make
// an execution prerequisite visible before a task fails on it:
//
//   1. No LLM credentials configured — the workspace's effective scoped-config
//      has neither a provider credential key nor enabled Codex subscription.
//
// Rendering is fail-closed toward silence: banners derive only from
// successfully loaded (`status === "ok"`) queries, so loading or errored data
// never flashes a false warning.
//
// Links are real anchors (`?view=workers` / `?view=settings`) so copy and
// middle-click yield the bookmarkable admin deep links; a plain click is
// intercepted and opens the matching admin center tab in-app, after which the
// shell's tab → URL sync stamps `?view=` itself.
//
// @ref software_architecture/client/workspace-page/workspace-page-shell#D40
// =============================================================================

import { AlertTriangle } from "lucide-react";
import React from "react";
import type { AsyncState, ScopedConfig } from "../../lib/types";
import { classNames } from "../ui/UI";

// ---------------------------------------------------------------------------
// Credential key vocabulary
// ---------------------------------------------------------------------------

/**
 * Union of the server credential gate's per-provider secret key sets
 * (`agent_dispatch::provider_required_credential_keys` — Claude Code + Codex).
 * Any one of these present in the workspace's effective secret keys means an
 * LLM credential is configured and the credentials banner stays hidden.
 */
export const LLM_CREDENTIAL_SECRET_KEYS: readonly string[] = [
  "ANTHROPIC_AUTH_TOKEN",
  "CLAUDE_CODE_OAUTH_TOKEN",
  "ANTHROPIC_API_KEY",
  "OPENROUTER_API_KEY",
  "OPENAI_API_KEY",
];

/**
 * Whether the effective scoped-config carries at least one provided-agent LLM
 * credential path. Pure so the gate is unit-testable.
 *
 * A stored provider secret key always counts. The Codex ChatGPT subscription
 * also counts as a configured credential when the effective `OPENAI_BASE_URL` selects the direct OpenAI provider route,
 * AND the effective `codex_subscription_enabled` preference is absent or
 * boolean `true`. This mirrors the resolver's semantics (external_agent_
 * provider_contract D11 / settings-page D6 / shell D40): an ABSENT value
 * inherits default-on, so a fresh Codex-enabled workspace is not simultaneously
 * shown the checkbox checked in Settings and a "No LLM credentials configured"
 * banner. A PRESENT non-boolean value (legacy `"false"` string, `null`) does
 * NOT count: the resolver rejects it with a validation error instead of
 * defaulting it, so the banner must not report a credential the launch would
 * refuse. On a non-direct route (OpenRouter or another compatible endpoint)
 * subscription auth does not exist and only stored keys count. When the value is explicitly `false`, subscription
 * does not count.
 *
 * Tolerates unexpected shapes: an async query's "ok" only means the request
 * resolved — E2E catch-all fixtures serve `[]` for unmocked object endpoints
 * and older backends may omit the field, so the input can be undefined or a
 * non-array/non-object. A malformed input reports `false` instead of throwing,
 * and the caller separately suppresses the banner for malformed shapes (shell
 * D40: fail-closed toward silence, never a render throw).
 */
export function hasLlmCredential(
  effectiveSecretKeys: readonly string[] | undefined,
  effectiveSettings: Record<string, unknown> | undefined,
): boolean {
  if (
    Array.isArray(effectiveSecretKeys) &&
    effectiveSecretKeys.some((key) => LLM_CREDENTIAL_SECRET_KEYS.includes(key))
  ) {
    return true;
  }
  // Object guard preserves shell D40's malformed-shape safety: a non-object
  // effectiveSettings (e.g. an array catch-all fixture) has no explicit value.
  const settingsObject =
    effectiveSettings &&
    typeof effectiveSettings === "object" &&
    !Array.isArray(effectiveSettings)
      ? effectiveSettings
      : undefined;
  // Route mirror of the resolver's `codex_route_is_openai`: subscription
  // auth exists only on the direct OpenAI route — an absent/empty/non-string
  // OPENAI_BASE_URL, or a URL equal to or under https://api.openai.com. Any
  // other URL (OpenRouter / compatible endpoint) requires a stored API key.
  const baseUrlRaw = settingsObject?.OPENAI_BASE_URL;
  const baseUrl = typeof baseUrlRaw === "string" ? baseUrlRaw.trim() : "";
  const normalizedBaseUrl = baseUrl.replace(/\/+$/, "").toLowerCase();
  const openaiDirectRoute =
    baseUrl === "" ||
    normalizedBaseUrl === "https://api.openai.com" ||
    normalizedBaseUrl.startsWith("https://api.openai.com/");
  const raw = settingsObject?.codex_subscription_enabled;
  // Default-on, boolean-only (resolver mirror): absent/undefined inherits
  // enabled; boolean `true` is enabled; `false` opts out; a present
  // non-boolean is rejected by the resolver, so it never counts as enabled.
  return openaiDirectRoute && (raw === undefined || raw === true);
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface WorkspaceHealthBannersProps {
  /** Workspace scoped config — `effectiveSecretKeys` drives the credential banner. */
  workspaceConfig: AsyncState<ScopedConfig>;
  /** Opens the workspace Settings center tab in-app. */
  onOpenSettings: () => void;
  /** The Settings surface's URL for copy and new-tab opens: the workspace
   *  root with `?view=settings` (g5.workbench.tabs names singleton surfaces
   *  there; a relative `?view=` beside a content path would be dropped). */
  settingsHref?: string;
}

// ---------------------------------------------------------------------------
// Banner row
// ---------------------------------------------------------------------------

const BannerRow: React.FC<{
  testId: string;
  message: string;
  linkText: string;
  href: string;
  onNavigate: () => void;
}> = ({ testId, message, linkText, href, onNavigate }) => (
  <div
    data-testid={testId}
    role="status"
    className={classNames(
      "flex items-center gap-2 px-4 py-1.5",
      "border-b border-[var(--g5-warn)]/40 bg-[var(--g5-warn)]/10",
      "text-xs text-[var(--g5-text1)]",
    )}
  >
    <AlertTriangle size={14} className="shrink-0 text-[var(--g5-warn)]" />
    <span>
      {message}{" "}
      <a
        href={href}
        className="text-[var(--g5-info)] hover:underline"
        onClick={(event) => {
          // Plain click opens the admin tab in-app; modified clicks and
          // middle-click keep native anchor behavior for the deep link.
          if (
            event.defaultPrevented ||
            event.button !== 0 ||
            event.metaKey ||
            event.ctrlKey ||
            event.shiftKey ||
            event.altKey
          ) {
            return;
          }
          event.preventDefault();
          onNavigate();
        }}
      >
        {linkText}
      </a>
    </span>
  </div>
);

// ---------------------------------------------------------------------------
// WorkspaceHealthBanners
// ---------------------------------------------------------------------------

export const WorkspaceHealthBanners: React.FC<WorkspaceHealthBannersProps> = ({
  workspaceConfig,
  onOpenSettings,
  settingsHref = "?view=settings",
}) => {
  const configOk =
    workspaceConfig.status === "ok" ? workspaceConfig.data : null;

  // Shape guards (shell D40): "ok" only means the request resolved — the body
  // can be an array (E2E catch-all fixtures) or predate a field. A malformed
  // payload renders nothing; it must never throw, or the whole WorkspacePage
  // unmounts through the shell error boundary.
  const noLlmCredentials =
    configOk !== null &&
    Array.isArray(configOk.effectiveSecretKeys) &&
    !hasLlmCredential(configOk.effectiveSecretKeys, configOk.effectiveSettings);

  if (!noLlmCredentials) return null;

  return (
    <div data-testid="workspace-health-banners">
      {noLlmCredentials && (
        <BannerRow
          testId="workspace-health-banner-no-llm-credentials"
          message="No LLM credentials configured."
          linkText="Configure them here"
          href={settingsHref}
          onNavigate={onOpenSettings}
        />
      )}
    </div>
  );
};
