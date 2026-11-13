/** Codex is a standard G5 Lean provider, advertised for API clients. */
export const CODEX_UI_FEATURE = "codex_ui";

// Runtime visibility kill-switch for the provider-accounts surfaces
// (ontology: system_invariants/external_agent_provider_contract D14;
// server carrier instance spec D16; client resolver
// session_provider_controls D29).
//
// This is DEFAULT-ON: g5-api advertises the
// `provider_accounts` instance feature unless `G5_ENABLE_PROVIDER_ACCOUNTS` is
// exactly "false" (config_safe_defaults D6). The client still fails closed
// while instance info is unloaded, errored, or malformed — the default-on
// direction lives on the server read, never in a client fallback. While the
// feature is absent, the Settings accounts section, the per-agent assignment
// controls, and the chat Account field are hidden; persisted accounts and
// assignments are neither deleted nor rewritten and keep resolving server-side.
//
// Every provider-accounts surface must derive from `isProviderAccountsEnabled`
// (directly or through a threaded prop). Playwright specs gate on the
// `G5_E2E_PROVIDER_ACCOUNTS_ENABLED` process env instead of importing this
// module's resolution.

/** Instance feature name g5-api advertises while provider accounts are enabled. */
export const PROVIDER_ACCOUNTS_FEATURE = "provider_accounts";

/**
 * Resolve the provider-accounts kill-switch from instance-info features.
 * Fail-closed: missing/unloaded instance info (`null`/`undefined`) means hidden.
 */
export function isProviderAccountsEnabled(
  instanceInfo: { features?: readonly string[] | null } | null | undefined,
): boolean {
  const features = instanceInfo?.features;
  // Array.isArray guards malformed/legacy instance-info payloads (the API
  // client performs no runtime validation) so the gate fails closed instead
  // of throwing during render.
  return (
    Array.isArray(features) && features.includes(PROVIDER_ACCOUNTS_FEATURE)
  );
}
