import { describe, expect, it } from "vitest";
import {
  supportedCloudAgentProviders,
  getAvailableSyncAgentConfigs,
} from "../pages/WorkspacePage";

describe("standard agent providers", () => {
  it("offers Codex without an instance-info response or available worker", () => {
    expect(supportedCloudAgentProviders()).toEqual(["claude_code", "codex"]);
    expect(getAvailableSyncAgentConfigs().map((c) => c.agentProvider)).toEqual([
      "claude_code",
      "codex",
    ]);
  });
});
