import { describe, expect, it } from "vitest";

import { agentAuthModeLabel } from "./agentLaunchPresentation";

describe("agentAuthModeLabel", () => {
  it("names the typed direct-Anthropic bearer mode", () => {
    expect(agentAuthModeLabel("anthropic_bearer")).toBe(
      "Anthropic bearer token",
    );
  });
});
