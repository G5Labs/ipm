// Auto default agent (chat g5.chat.account-defaults): with no configured
// default, prefer the first available agent that can authenticate, then the
// first available agent.
import { describe, expect, it } from "vitest";

import { getPreferredSyncAgentConfig } from "./agentLaunchDefaults";
import {
  getPreferredAvailableAgentProvider,
  hasLiveAssistantTurn,
} from "./chatTurns";

describe("hasLiveAssistantTurn", () => {
  it.each(["pending", "streaming"])(
    "keeps a %s assistant live through later notices and follow-ups",
    (status) => {
      expect(
        hasLiveAssistantTurn([
          { role: "assistant", status },
          { role: "user", status: "complete" },
          { role: "system", status: "complete" },
          { role: "system", status: "complete" },
        ]),
      ).toBe(true);
    },
  );

  it.each(["complete", "stopped", "error"])(
    "ends with the latest assistant's %s state despite trailing notices",
    (status) => {
      expect(
        hasLiveAssistantTurn([
          { role: "assistant", status: "streaming" },
          { role: "assistant", status },
          { role: "system", status: "complete" },
        ]),
      ).toBe(false);
    },
  );

  it("does not start a turn for a notice or input alone", () => {
    expect(hasLiveAssistantTurn([])).toBe(false);
    expect(
      hasLiveAssistantTurn([
        { role: "user", status: "complete" },
        { role: "system", status: "complete" },
      ]),
    ).toBe(false);
  });
});

describe("getPreferredAvailableAgentProvider", () => {
  it("keeps an available configured default over an authenticatable agent", () => {
    expect(
      getPreferredAvailableAgentProvider(
        ["claude_code", "codex"],
        "claude_code",
        ["codex"],
      ),
    ).toBe("claude_code");
  });

  it("prefers the first available agent that can authenticate when Auto", () => {
    expect(
      getPreferredAvailableAgentProvider(["claude_code", "codex"], null, [
        "codex",
      ]),
    ).toBe("codex");
    expect(
      getPreferredAvailableAgentProvider(["claude_code", "codex"], undefined, [
        "codex",
        "claude_code",
      ]),
    ).toBe("claude_code");
  });

  it("falls back to the first available agent when none can authenticate", () => {
    expect(
      getPreferredAvailableAgentProvider(["claude_code", "codex"], null, []),
    ).toBe("claude_code");
    expect(getPreferredAvailableAgentProvider(["codex"], "claude_code")).toBe(
      "codex",
    );
    expect(getPreferredAvailableAgentProvider([], null)).toBe("claude_code");
  });
});

describe("getPreferredSyncAgentConfig", () => {
  it("applies the same Auto preference to the launch config", () => {
    expect(
      getPreferredSyncAgentConfig(null, null, ["codex"]).agentProvider,
    ).toBe("codex");
    expect(
      getPreferredSyncAgentConfig("claude_code", null, ["codex"]).agentProvider,
    ).toBe("claude_code");
    expect(getPreferredSyncAgentConfig(null, null, []).agentProvider).toBe(
      "claude_code",
    );
  });
});
