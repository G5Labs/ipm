// =============================================================================
// scopedConfigMerge.test.ts — MSW scoped-config PUT mirrors ConfigService D6
// =============================================================================
// The shared MSW handler for `PUT /config` stands in for ConfigService in every
// frontend test that touches scoped runtime config. If it diverged from the
// server's merge contract (config_service D1/D6), those tests could pass against
// behavior that never ships. These tests pin the handler to the server's
// semantics: a partial settings PUT deep-merges (nested objects merge
// key-by-key, siblings preserved), and `removeSettingKeys` deletes top-level
// keys after the merge — the same guarantees asserted by the Rust
// `merge_json_overlays_nested_objects` / `settings_write_*` unit tests.
// =============================================================================

import { describe, it, expect } from "vitest";

import { resetMockScopedConfig } from "./handlers";

async function putOrgConfig(body: unknown): Promise<Record<string, unknown>> {
  const res = await fetch("/api/v1/orgs/org-1/config", {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  expect(res.ok).toBe(true);
  const json = (await res.json()) as {
    data: { settings: Record<string, unknown> };
  };
  return json.data.settings;
}

async function getOrgSettings(): Promise<Record<string, unknown>> {
  const res = await fetch("/api/v1/orgs/org-1/config");
  const json = (await res.json()) as {
    data: { settings: Record<string, unknown> };
  };
  return json.data.settings;
}

describe("MSW scoped-config PUT mirrors ConfigService merge semantics", () => {
  it("deep-merges a nested settings object instead of replacing it", async () => {
    // Seed a scope whose value at `llm` is itself an object with two children.
    resetMockScopedConfig({
      org: {
        settings: {
          llm: { provider: "openai", model: "gpt-4o-mini" },
          TOP_LEVEL: "keep-me",
        },
      },
    });

    // Overlay changes ONLY llm.model and adds a new nested branch.
    const returned = await putOrgConfig({
      settings: {
        llm: { model: "claude-sonnet-4-6" },
        featureFlags: { smartHttp: true },
      },
    });

    // llm.provider survives the nested merge — a shallow spread would have
    // dropped it by replacing the whole `llm` object.
    expect(returned.llm).toEqual({
      provider: "openai",
      model: "claude-sonnet-4-6",
    });
    expect(returned.featureFlags).toEqual({ smartHttp: true });
    expect(returned.TOP_LEVEL).toBe("keep-me");

    // The follow-up GET reflects the same persisted state.
    expect(await getOrgSettings()).toEqual({
      llm: { provider: "openai", model: "claude-sonnet-4-6" },
      featureFlags: { smartHttp: true },
      TOP_LEVEL: "keep-me",
    });
  });

  it("applies removeSettingKeys as a top-level delete after the merge", async () => {
    resetMockScopedConfig({
      org: {
        settings: {
          llm: { provider: "openai" },
          DROP_ME: "gone",
          KEEP_ME: "stays",
        },
      },
    });

    const returned = await putOrgConfig({
      settings: { llm: { model: "claude-sonnet-4-6" } },
      removeSettingKeys: ["DROP_ME"],
    });

    expect(returned).toEqual({
      llm: { provider: "openai", model: "claude-sonnet-4-6" },
      KEEP_ME: "stays",
    });
    expect(returned).not.toHaveProperty("DROP_ME");
  });
});
