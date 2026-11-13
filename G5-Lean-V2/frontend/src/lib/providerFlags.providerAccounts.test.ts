import { describe, expect, it } from "vitest";
import {
  CODEX_UI_FEATURE,
  PROVIDER_ACCOUNTS_FEATURE,
  isProviderAccountsEnabled,
} from "./providerFlags";

// Provider-accounts kill-switch resolver (external_agent_provider_contract
// D14; session_provider_controls D29; server carrier instance spec D16).
// The server advertises `provider_accounts` by default and drops it only
// when G5_ENABLE_PROVIDER_ACCOUNTS is exactly "false"; the client resolver is
// fail-closed on unloaded, errored, or malformed instance info regardless of
// that default-on direction.
describe("isProviderAccountsEnabled", () => {
  it("advertises the exact feature name the server pushes", () => {
    expect(PROVIDER_ACCOUNTS_FEATURE).toBe("provider_accounts");
  });

  it("is enabled when instance info advertises the provider_accounts feature", () => {
    expect(
      isProviderAccountsEnabled({
        features: ["chat", PROVIDER_ACCOUNTS_FEATURE],
      }),
    ).toBe(true);
    expect(
      isProviderAccountsEnabled({
        features: [PROVIDER_ACCOUNTS_FEATURE],
      }),
    ).toBe(true);
  });

  it("is hidden when the kill-switch dropped the feature from the list", () => {
    expect(
      isProviderAccountsEnabled({
        features: ["chat", CODEX_UI_FEATURE, "guided_journey"],
      }),
    ).toBe(false);
    expect(isProviderAccountsEnabled({ features: [] })).toBe(false);
  });

  it("fails closed while instance info is not yet loaded or unavailable", () => {
    expect(isProviderAccountsEnabled(null)).toBe(false);
    expect(isProviderAccountsEnabled(undefined)).toBe(false);
  });

  it("fails closed on malformed instance-info payloads instead of throwing", () => {
    expect(isProviderAccountsEnabled({})).toBe(false);
    expect(isProviderAccountsEnabled({ features: null })).toBe(false);
    expect(
      isProviderAccountsEnabled({
        features: "provider_accounts" as unknown as readonly string[],
      }),
    ).toBe(false);
  });

  it("does not match on prefixes or unrelated feature names", () => {
    expect(
      isProviderAccountsEnabled({
        features: ["provider_accounts_extra", "provider", "accounts"],
      }),
    ).toBe(false);
  });
});
