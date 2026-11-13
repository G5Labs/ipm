import { USER_SETTINGS_STORAGE_KEY } from "../hooks/useUserSettings";
import type { UserSettings } from "../lib/types";

/**
 * Merge a patch into the stored user settings so earlier seeds survive. The
 * center tab bar is hidden by default (workspaces g5.workspace.ui.center-tabs);
 * page suites that drive the tabs seed `{ showTabBar: true }`.
 */
export function seedUserSettings(patch: Partial<UserSettings>): void {
  let current: Record<string, unknown> = {};
  try {
    current =
      JSON.parse(
        window.localStorage.getItem(USER_SETTINGS_STORAGE_KEY) ?? "{}",
      ) ?? {};
  } catch {
    current = {};
  }
  window.localStorage.setItem(
    USER_SETTINGS_STORAGE_KEY,
    JSON.stringify({ ...current, ...patch }),
  );
}
