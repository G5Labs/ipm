// =============================================================================
// Preference telemetry — which Settings-page preference values people use
// =============================================================================
// Pure module (no React). Encodes the client-local Settings-page preferences
// (`g5-user-settings`, hooks/useUserSettings.ts) into value-carrying telemetry
// entries for the UI telemetry stream (lib/telemetry.ts, kind `setting`), so
// usage can be measured from the `g5_audit` stream (operations
// g5.platform.client-telemetry). Click telemetry alone cannot: a native
// <select> change is no click, and a toggle click says nothing about the
// value it produced.
//
// Privacy rule: a reported value is one of the preference's offered choices, a
// boolean or a bounded integer. Anything else — a corrupt store, a value from
// a newer client — is reported as `other`, so free text can never travel in a
// preference value. The per-harness model choices are deliberately not
// listed: a model id can be typed by the person.

import { THEME_OPTIONS } from "./defaults";
import { HOME_ROWS_PER_TILE_OPTIONS, type UserSettings } from "./types";
import { ACTION_BAR_STYLES, PHASE_BAR_STYLES } from "./workbenchDefaults";

/** Reported in place of any value outside the preference's offered choices. */
export const UNLISTED_PREFERENCE_VALUE = "other";

/** One `[preference key, encoded value]` pair. */
export type PreferenceEntry = readonly [key: string, value: string];

/** One preference whose stored value changed. */
export interface PreferenceChange {
  key: string;
  value: string;
  previous: string;
}

type Encoder = (value: unknown) => string;

function oneOf(options: readonly (string | number)[]): Encoder {
  const allowed = new Set(options.map(String));
  return (value) =>
    (typeof value === "string" || typeof value === "number") &&
    allowed.has(String(value))
      ? String(value)
      : UNLISTED_PREFERENCE_VALUE;
}

const boolean: Encoder = (value) =>
  typeof value === "boolean" ? String(value) : UNLISTED_PREFERENCE_VALUE;

/** Small non-negative integers only (the editor font size is 10–24). */
const smallInteger: Encoder = (value) =>
  typeof value === "number" &&
  Number.isInteger(value) &&
  value >= 0 &&
  value < 1000
    ? String(value)
    : UNLISTED_PREFERENCE_VALUE;

interface PreferenceSpec {
  /** Telemetry key: the settings field, `editor.<field>` for editor ones. */
  key: string;
  read: (settings: UserSettings) => unknown;
  encode: Encoder;
}

const editor =
  (field: keyof UserSettings["editor"]) => (settings: UserSettings) =>
    settings.editor?.[field];

/**
 * The reported preferences, in report order. Every Settings-page preference
 * that lives only in localStorage and whose values are enumerable is listed;
 * keep this in step with the Settings page's Appearance, Layout and Editor
 * rows.
 */
export const PREFERENCE_TELEMETRY: readonly PreferenceSpec[] = [
  {
    key: "themeName",
    read: (s) => s.themeName,
    encode: oneOf(THEME_OPTIONS.map((theme) => theme.name)),
  },
  {
    key: "themeMode",
    read: (s) => s.themeMode,
    encode: oneOf(["dark", "light"]),
  },
  {
    key: "sidebarDisplayMode",
    read: (s) => s.sidebarDisplayMode,
    encode: oneOf(["expanded", "collapse-to-rail", "expand-on-hover", "rail"]),
  },
  {
    key: "homeRowsPerTile",
    read: (s) => s.homeRowsPerTile,
    encode: oneOf(HOME_ROWS_PER_TILE_OPTIONS),
  },
  {
    key: "phaseBarStyle",
    read: (s) => s.phaseBarStyle,
    encode: oneOf(PHASE_BAR_STYLES),
  },
  {
    key: "actionBarStyle",
    read: (s) => s.actionBarStyle,
    encode: oneOf(ACTION_BAR_STYLES),
  },
  {
    key: "showGuidedJourney",
    read: (s) => s.showGuidedJourney,
    encode: boolean,
  },
  { key: "showTabBar", read: (s) => s.showTabBar, encode: boolean },
  {
    key: "subscriptionUsageWindow",
    read: (s) => s.subscriptionUsageWindow,
    encode: oneOf(["7d", "5h", "off"]),
  },
  {
    key: "editor.editorFontSize",
    read: editor("editorFontSize"),
    encode: smallInteger,
  },
  {
    key: "editor.editorTabSize",
    read: editor("editorTabSize"),
    encode: oneOf([2, 4, 8]),
  },
  {
    key: "editor.rainbowBrackets",
    read: editor("rainbowBrackets"),
    encode: boolean,
  },
  { key: "editor.codeFolding", read: editor("codeFolding"), encode: boolean },
  { key: "editor.showMinimap", read: editor("showMinimap"), encode: boolean },
  {
    key: "editor.wordWrap",
    read: editor("wordWrap"),
    encode: oneOf(["off", "on", "wordWrapColumn", "bounded"]),
  },
  { key: "editor.indentGuides", read: editor("indentGuides"), encode: boolean },
  {
    key: "editor.bracketPairGuides",
    read: editor("bracketPairGuides"),
    encode: boolean,
  },
  { key: "editor.ligatures", read: editor("ligatures"), encode: boolean },
  { key: "editor.stickyScroll", read: editor("stickyScroll"), encode: boolean },
  {
    key: "editor.renderWhitespace",
    read: editor("renderWhitespace"),
    encode: oneOf(["none", "boundary", "selection", "trailing", "all"]),
  },
];

/** Every reported preference's current encoded value (the snapshot). */
export function preferenceTelemetryValues(
  settings: UserSettings,
): PreferenceEntry[] {
  return PREFERENCE_TELEMETRY.map((spec) => [
    spec.key,
    spec.encode(spec.read(settings)),
  ]);
}

/** The reported preferences whose encoded value differs between two states. */
export function changedPreferences(
  previous: UserSettings,
  next: UserSettings,
): PreferenceChange[] {
  const changes: PreferenceChange[] = [];
  for (const spec of PREFERENCE_TELEMETRY) {
    const before = spec.encode(spec.read(previous));
    const after = spec.encode(spec.read(next));
    if (before !== after) {
      changes.push({ key: spec.key, value: after, previous: before });
    }
  }
  return changes;
}
