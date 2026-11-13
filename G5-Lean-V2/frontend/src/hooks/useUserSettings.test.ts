import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  CLAUDE_CHAT_DEFAULT_MODEL,
  CLAUDE_DIRECT_CHAT_DEFAULT_MODEL,
} from "../lib/claudeModels";
import { CODEX_CHAT_DEFAULT_MODEL } from "../lib/codexModels";
import { catalogDefaultChoiceFor } from "../lib/harnessPicker";
import { recordSettingChange } from "../lib/telemetry";
import { useUserSettings } from "./useUserSettings";

vi.mock("../lib/telemetry", () => ({ recordSettingChange: vi.fn() }));

const STORAGE_KEY = "g5-user-settings";

describe("useUserSettings", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(recordSettingChange).mockClear();
  });

  it("reports each changed preference's normalized value to telemetry (g5.platform.client-telemetry)", () => {
    const { result } = renderHook(() => useUserSettings());

    act(() => {
      result.current.update({ sidebarDisplayMode: "rail", showTabBar: true });
    });
    expect(vi.mocked(recordSettingChange).mock.calls).toEqual([
      ["sidebarDisplayMode", "rail", "collapse-to-rail"],
      ["showTabBar", "true", "false"],
    ]);

    // Unchanged values, unlisted fields and invalid patches report nothing:
    // an unknown style normalizes back to the stored value.
    vi.mocked(recordSettingChange).mockClear();
    act(() => {
      result.current.update({
        showTabBar: true,
        budgetWarnAt: 10,
        phaseBarStyle: "diagonal" as never,
      });
    });
    expect(recordSettingChange).not.toHaveBeenCalled();

    act(() => {
      result.current.update({
        editor: { ...result.current.settings.editor, editorTabSize: 2 },
      });
    });
    expect(recordSettingChange).toHaveBeenCalledWith(
      "editor.editorTabSize",
      "2",
      "4",
    );
  });

  it("uses catalog-default per-harness choices when no settings are stored", () => {
    const { result } = renderHook(() => useUserSettings());

    const choices = result.current.settings.defaultAgentProviderChoices;
    expect(choices.claude_code).toEqual(catalogDefaultChoiceFor("claude_code"));
    expect(choices.claude_code.model).toBe(CLAUDE_CHAT_DEFAULT_MODEL);
    expect(choices.claude_code.mode).toBe("default");
    expect(choices.claude_code.effort).toBe("high");
    expect(choices.codex).toEqual(catalogDefaultChoiceFor("codex"));
    expect(choices.codex.model).toBe(CODEX_CHAT_DEFAULT_MODEL);
    expect(choices.codex.mode).toBe("never/workspace-write");
    expect(choices.codex.effort).toBe("xhigh");
  });

  it("hides the center tab bar by default and keeps only boolean stored values", () => {
    const { result } = renderHook(() => useUserSettings());
    expect(result.current.settings.showTabBar).toBe(false);

    act(() => {
      result.current.update({ showTabBar: true });
    });
    expect(result.current.settings.showTabBar).toBe(true);
    expect(
      JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}").showTabBar,
    ).toBe(true);

    localStorage.setItem(STORAGE_KEY, JSON.stringify({ showTabBar: "yes" }));
    const { result: reread } = renderHook(() => useUserSettings());
    expect(reread.current.settings.showTabBar).toBe(false);
  });

  it("defaults the workbench header styles and drops unknown stored values (g5.workbench.header-styles)", () => {
    const { result } = renderHook(() => useUserSettings());
    expect(result.current.settings.phaseBarStyle).toBe("minimal");
    expect(result.current.settings.actionBarStyle).toBe("dynamic");

    act(() => {
      result.current.update({
        phaseBarStyle: "vertical",
        actionBarStyle: "simplified",
      });
    });
    expect(result.current.settings.phaseBarStyle).toBe("vertical");
    expect(result.current.settings.actionBarStyle).toBe("simplified");
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored.phaseBarStyle).toBe("vertical");
    expect(stored.actionBarStyle).toBe("simplified");

    act(() => {
      result.current.update({ actionBarStyle: "dynamic" });
    });
    expect(result.current.settings.actionBarStyle).toBe("dynamic");
    const { result: dynamicReread } = renderHook(() => useUserSettings());
    expect(dynamicReread.current.settings.actionBarStyle).toBe("dynamic");

    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ phaseBarStyle: "horizontal", actionBarStyle: 3 }),
    );
    const { result: reread } = renderHook(() => useUserSettings());
    expect(reread.current.settings.phaseBarStyle).toBe("minimal");
    expect(reread.current.settings.actionBarStyle).toBe("dynamic");
  });

  it("defaults the chat bubbles to five 3D heads and drops unknown stored values (g5.workspace.ui.chat-bubble-stack)", () => {
    const { result } = renderHook(() => useUserSettings());
    expect(result.current.settings.chatBubbleStyle).toBe("3d");
    expect(result.current.settings.chatBubbleLimit).toBe(5);

    act(() => {
      result.current.update({ chatBubbleStyle: "still", chatBubbleLimit: 8 });
    });
    expect(result.current.settings.chatBubbleStyle).toBe("still");
    expect(result.current.settings.chatBubbleLimit).toBe(8);
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored.chatBubbleStyle).toBe("still");
    expect(stored.chatBubbleLimit).toBe(8);

    // Out of range, fractional or non-numeric limits fall back to five; the
    // retired "column"/"collapsed" layout key no longer means anything.
    for (const limit of [0, 51, 2.5, "7", null]) {
      act(() => {
        result.current.update({ chatBubbleLimit: limit as number });
      });
      expect(result.current.settings.chatBubbleLimit).toBe(5);
    }
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        chatBubbleStyle: "gif",
        chatBubbleLimit: -3,
        chatBubbleStack: "collapsed",
      }),
    );
    const { result: reread } = renderHook(() => useUserSettings());
    expect(reread.current.settings.chatBubbleStyle).toBe("3d");
    expect(reread.current.settings.chatBubbleLimit).toBe(5);
  });

  it("defaults the left sidebar to collapse-to-rail and drops unknown stored modes (g5.ui.sidebar-default)", () => {
    const { result } = renderHook(() => useUserSettings());
    expect(result.current.settings.sidebarDisplayMode).toBe("collapse-to-rail");

    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ sidebarDisplayMode: "sideways" }),
    );
    const { result: reread } = renderHook(() => useUserSettings());
    expect(reread.current.settings.sidebarDisplayMode).toBe("collapse-to-rail");
  });

  it("hides the guided experience by default, persists turning it on and drops non-boolean values (g5.ui.guided-toggle)", () => {
    const { result } = renderHook(() => useUserSettings());
    expect(result.current.settings.showGuidedJourney).toBe(false);

    act(() => {
      result.current.update({ showGuidedJourney: true });
    });
    expect(result.current.settings.showGuidedJourney).toBe(true);
    expect(
      JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}").showGuidedJourney,
    ).toBe(true);

    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ showGuidedJourney: "yes" }),
    );
    const { result: reread } = renderHook(() => useUserSettings());
    expect(reread.current.settings.showGuidedJourney).toBe(false);
  });

  it("keeps a stored expand-on-hover choice (g5.ui.sidebar-default)", () => {
    const { result } = renderHook(() => useUserSettings());
    act(() => {
      result.current.update({ sidebarDisplayMode: "expand-on-hover" });
    });
    expect(result.current.settings.sidebarDisplayMode).toBe("expand-on-hover");
    const { result: reread } = renderHook(() => useUserSettings());
    expect(reread.current.settings.sidebarDisplayMode).toBe("expand-on-hover");
  });

  it("migrates a legacy defaultModel into the claude_code entry and drops the field", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        defaultModel: "claude-sonnet-4-6",
        editor: { editorFontSize: 18 },
      }),
    );

    const { result } = renderHook(() => useUserSettings());

    // Plan D-12: migrated mode is bypassPermissions; plan D-6: effort is the
    // migrated model's catalog defaultEffort (Sonnet → high, model-aware).
    const choices = result.current.settings.defaultAgentProviderChoices;
    expect(choices.claude_code).toEqual({
      model: "claude-sonnet-4-6",
      mode: "bypassPermissions",
      effort: "high",
    });
    expect(choices.codex).toEqual(catalogDefaultChoiceFor("codex"));
    expect(result.current.settings.editor.editorFontSize).toBe(18);
    expect("defaultModel" in result.current.settings).toBe(false);

    act(() => {
      result.current.update({ themeMode: "light" });
    });
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored.defaultModel).toBeUndefined();
    expect(stored.defaultAgentProviderChoices.claude_code.model).toBe(
      "claude-sonnet-4-6",
    );
    // default_agent_provider is server-side scoped config — never written here.
    expect(stored.default_agent_provider).toBeUndefined();
  });

  it("drops the retired notification delivery fields from stored settings", () => {
    // Lean P8.2 removed the dev-mode notification toggles and delivery
    // channel form; a store written by that UI must not carry the fields
    // forward, in memory or on the next persist.
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        themeMode: "light",
        notifyTaskComplete: false,
        notifyAgentBlocked: false,
        notifyChannels: [
          {
            id: "ch-1",
            type: "email",
            label: "Work Email",
            detail: "user@example.com",
            enabled: true,
          },
        ],
      }),
    );

    const { result } = renderHook(() => useUserSettings());

    expect(result.current.settings.themeMode).toBe("light");
    expect("notifyTaskComplete" in result.current.settings).toBe(false);
    expect("notifyAgentBlocked" in result.current.settings).toBe(false);
    expect("notifyChannels" in result.current.settings).toBe(false);

    act(() => {
      result.current.update({ themeMode: "dark" });
    });
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored.themeMode).toBe("dark");
    expect(stored.notifyTaskComplete).toBeUndefined();
    expect(stored.notifyAgentBlocked).toBeUndefined();
    expect(stored.notifyChannels).toBeUndefined();
  });

  it("migrates retired legacy ids to the canonical chat default", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ defaultModel: "claude-sonnet-4-20250514" }),
    );

    const { result } = renderHook(() => useUserSettings());

    expect(
      result.current.settings.defaultAgentProviderChoices.claude_code.model,
    ).toBe(CLAUDE_CHAT_DEFAULT_MODEL);
  });

  it("preserves custom model pins through migration with the legacy effort fallback", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ defaultModel: "custom/provider-model" }),
    );

    const { result } = renderHook(() => useUserSettings());

    const entry =
      result.current.settings.defaultAgentProviderChoices.claude_code;
    expect(entry.model).toBe("custom/provider-model");
    expect(entry.effort).toBe("xhigh");
  });

  it("preserves explicit direct Claude preferences and per-entry updates", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ defaultModel: CLAUDE_DIRECT_CHAT_DEFAULT_MODEL }),
    );

    const { result } = renderHook(() => useUserSettings());
    expect(
      result.current.settings.defaultAgentProviderChoices.claude_code.model,
    ).toBe(CLAUDE_DIRECT_CHAT_DEFAULT_MODEL);

    act(() => {
      const choices = result.current.settings.defaultAgentProviderChoices;
      result.current.update({
        defaultAgentProviderChoices: {
          ...choices,
          claude_code: { ...choices.claude_code, model: "claude-opus-4-8" },
        },
      });
    });

    expect(
      result.current.settings.defaultAgentProviderChoices.claude_code.model,
    ).toBe("claude-opus-4-8");
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored.defaultAgentProviderChoices.claude_code.model).toBe(
      "claude-opus-4-8",
    );
  });

  it("is idempotent: a migrated store reloads to the same choices", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ defaultModel: "claude-sonnet-4-6" }),
    );

    const first = renderHook(() => useUserSettings());
    act(() => {
      first.result.current.update({ themeMode: "light" });
    });
    const persisted = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    first.unmount();

    const second = renderHook(() => useUserSettings());
    expect(
      second.result.current.settings.defaultAgentProviderChoices.claude_code,
    ).toEqual({
      model: "claude-sonnet-4-6",
      mode: "bypassPermissions",
      effort: "high",
    });
    act(() => {
      second.result.current.update({ themeMode: "dark" });
    });
    const reStored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(reStored.defaultAgentProviderChoices).toEqual(
      persisted.defaultAgentProviderChoices,
    );
  });

  it("clamps a stored effort that the entry's model does not support (D-6)", () => {
    // Sonnet caps at "high"; a stored "xhigh" (valid for Opus, or persisted
    // before the model was switched) must not survive a read unclamped —
    // otherwise it reaches the server on the new chat's first turn.
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        defaultAgentProviderChoices: {
          claude_code: {
            model: "claude-sonnet-4-6",
            mode: "bypassPermissions",
            effort: "xhigh",
          },
          codex: catalogDefaultChoiceFor("codex"),
        },
      }),
    );

    const { result } = renderHook(() => useUserSettings());
    const claude =
      result.current.settings.defaultAgentProviderChoices.claude_code;
    expect(claude.model).toBe("claude-sonnet-4-6");
    expect(claude.effort).toBe("high");
    // An in-range stored effort is preserved verbatim.
    expect(
      result.current.settings.defaultAgentProviderChoices.codex.effort,
    ).toBe(catalogDefaultChoiceFor("codex").effort);
  });

  it("recovers corrupt entries to catalog defaults and never persists an empty model", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        defaultAgentProviderChoices: {
          claude_code: { model: "", mode: 42 },
          codex: "garbage",
        },
      }),
    );

    const { result } = renderHook(() => useUserSettings());
    const choices = result.current.settings.defaultAgentProviderChoices;
    expect(choices.claude_code.model).toBe(CLAUDE_CHAT_DEFAULT_MODEL);
    expect(choices.claude_code.mode).toBe("default");
    expect(choices.codex).toEqual(catalogDefaultChoiceFor("codex"));

    act(() => {
      result.current.update({
        defaultAgentProviderChoices: {
          ...choices,
          claude_code: { ...choices.claude_code, model: "  " },
        },
      });
    });

    expect(
      result.current.settings.defaultAgentProviderChoices.claude_code.model,
    ).toBe(CLAUDE_CHAT_DEFAULT_MODEL);
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored.defaultAgentProviderChoices.claude_code.model).toBe(
      CLAUDE_CHAT_DEFAULT_MODEL,
    );
  });

  it("defaults the subscription usage window to 7d and persists an off selection", () => {
    const { result } = renderHook(() => useUserSettings());
    // agent_interaction_ux D8: 7-day is the default window.
    expect(result.current.settings.subscriptionUsageWindow).toBe("7d");

    act(() => {
      result.current.update({ subscriptionUsageWindow: "off" });
    });

    expect(result.current.settings.subscriptionUsageWindow).toBe("off");
    const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    expect(stored.subscriptionUsageWindow).toBe("off");
  });

  it("switches the subscription usage window to 5h and reloads it verbatim", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ subscriptionUsageWindow: "5h" }),
    );

    const { result } = renderHook(() => useUserSettings());
    expect(result.current.settings.subscriptionUsageWindow).toBe("5h");
  });

  it("falls back an unknown stored subscription usage window to 7d", () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ subscriptionUsageWindow: "monthly" }),
    );

    const { result } = renderHook(() => useUserSettings());
    // D8: unknown/legacy values must not reach the chip as an unknown window.
    expect(result.current.settings.subscriptionUsageWindow).toBe("7d");
  });

  it("defaults Home rows per tile to 5, persists an offered size and drops others (g5.ui.home-paging)", () => {
    const { result } = renderHook(() => useUserSettings());
    expect(result.current.settings.homeRowsPerTile).toBe(5);

    act(() => {
      result.current.update({ homeRowsPerTile: 20 });
    });
    expect(result.current.settings.homeRowsPerTile).toBe(20);
    expect(
      JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}").homeRowsPerTile,
    ).toBe(20);
    expect(
      renderHook(() => useUserSettings()).result.current.settings
        .homeRowsPerTile,
    ).toBe(20);

    localStorage.setItem(STORAGE_KEY, JSON.stringify({ homeRowsPerTile: 7 }));
    expect(
      renderHook(() => useUserSettings()).result.current.settings
        .homeRowsPerTile,
    ).toBe(5);
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ homeRowsPerTile: "10" }),
    );
    expect(
      renderHook(() => useUserSettings()).result.current.settings
        .homeRowsPerTile,
    ).toBe(5);
  });
});
