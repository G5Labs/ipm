import { describe, expect, it } from "vitest";
import { AgentProviderSchema } from "./api-schemas";

describe("agent provider wire contract", () => {
  it("enumerates exactly the two CLI providers", () => {
    expect(AgentProviderSchema.options).toEqual(["claude_code", "codex"]);
    for (const provider of AgentProviderSchema.options) {
      expect(AgentProviderSchema.parse(provider)).toBe(provider);
    }
    expect(AgentProviderSchema.safeParse("unsupported").success).toBe(false);
  });
});
