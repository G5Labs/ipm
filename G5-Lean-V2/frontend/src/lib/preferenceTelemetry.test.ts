// Tests for the preference telemetry encoder (operations
// g5.platform.client-telemetry): offered choices only, never free text.

import { describe, expect, it } from "vitest";

import { DEFAULT_SETTINGS } from "./defaults";
import {
  changedPreferences,
  PREFERENCE_TELEMETRY,
  preferenceTelemetryValues,
  UNLISTED_PREFERENCE_VALUE,
} from "./preferenceTelemetry";
import type { UserSettings } from "./types";

describe("preferenceTelemetryValues", () => {
  it("reports every listed Settings-page preference with its encoded default", () => {
    const values = Object.fromEntries(
      preferenceTelemetryValues(DEFAULT_SETTINGS),
    );
    expect(Object.keys(values)).toHaveLength(PREFERENCE_TELEMETRY.length);
    expect(values).toMatchObject({
      themeName: "editor",
      themeMode: "dark",
      sidebarDisplayMode: "collapse-to-rail",
      homeRowsPerTile: "5",
      phaseBarStyle: "minimal",
      actionBarStyle: "dynamic",
      showGuidedJourney: "false",
      showTabBar: "false",
      subscriptionUsageWindow: "7d",
      "editor.editorFontSize": "14",
      "editor.editorTabSize": "4",
      "editor.wordWrap": "off",
      "editor.renderWhitespace": "selection",
      "editor.showMinimap": "false",
    });
  });

  it("never reports the per-harness model choices", () => {
    const keys = PREFERENCE_TELEMETRY.map((spec) => spec.key);
    expect(keys.some((key) => /model|harness|Provider/i.test(key))).toBe(false);
  });

  it("reports values outside the offered choices as `other`, never verbatim", () => {
    const corrupt = {
      ...DEFAULT_SETTINGS,
      themeName: "alice@example.com typed this",
      homeRowsPerTile: 7,
      showTabBar: "yes",
      editor: {
        ...DEFAULT_SETTINGS.editor,
        editorFontSize: 1e9,
        wordWrap: "<script>",
      },
    } as unknown as UserSettings;
    const values = Object.fromEntries(preferenceTelemetryValues(corrupt));
    expect(values.themeName).toBe(UNLISTED_PREFERENCE_VALUE);
    expect(values.homeRowsPerTile).toBe(UNLISTED_PREFERENCE_VALUE);
    expect(values.showTabBar).toBe(UNLISTED_PREFERENCE_VALUE);
    expect(values["editor.editorFontSize"]).toBe(UNLISTED_PREFERENCE_VALUE);
    expect(values["editor.wordWrap"]).toBe(UNLISTED_PREFERENCE_VALUE);
    expect(JSON.stringify(values)).not.toContain("alice");
    expect(JSON.stringify(values)).not.toContain("<script>");
  });

  it("encodes every offered theme and header style", () => {
    for (const theme of [
      "signal-studio",
      "classic-98",
      "aero-glass",
    ] as const) {
      const values = Object.fromEntries(
        preferenceTelemetryValues({ ...DEFAULT_SETTINGS, themeName: theme }),
      );
      expect(values.themeName).toBe(theme);
    }
    const styled = Object.fromEntries(
      preferenceTelemetryValues({
        ...DEFAULT_SETTINGS,
        phaseBarStyle: "vertical",
        actionBarStyle: "extended-icons",
      }),
    );
    expect(styled.phaseBarStyle).toBe("vertical");
    expect(styled.actionBarStyle).toBe("extended-icons");
  });
});

describe("changedPreferences", () => {
  it("lists only the preferences whose encoded value changed, with the previous value", () => {
    const next: UserSettings = {
      ...DEFAULT_SETTINGS,
      sidebarDisplayMode: "rail",
      showTabBar: true,
      editor: { ...DEFAULT_SETTINGS.editor, editorTabSize: 2 },
    };
    expect(changedPreferences(DEFAULT_SETTINGS, next)).toEqual([
      {
        key: "sidebarDisplayMode",
        value: "rail",
        previous: "collapse-to-rail",
      },
      { key: "showTabBar", value: "true", previous: "false" },
      { key: "editor.editorTabSize", value: "2", previous: "4" },
    ]);
  });

  it("ignores unlisted fields and unchanged patches", () => {
    const next: UserSettings = {
      ...DEFAULT_SETTINGS,
      budgetWarnAt: 50,
      defaultAgentProviderChoices: {
        ...DEFAULT_SETTINGS.defaultAgentProviderChoices,
        claude_code: {
          ...DEFAULT_SETTINGS.defaultAgentProviderChoices.claude_code,
          model: "my-private-model",
        },
      },
    };
    expect(changedPreferences(DEFAULT_SETTINGS, next)).toEqual([]);
    expect(changedPreferences(DEFAULT_SETTINGS, DEFAULT_SETTINGS)).toEqual([]);
  });
});
