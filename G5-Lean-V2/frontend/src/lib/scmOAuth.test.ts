import { describe, expect, it } from "vitest";

import {
  buildScmOAuthReturnTo,
  clearScmOAuthParams,
  isScmSettingsReturn,
} from "./scmOAuth";

describe("SCM OAuth return helpers", () => {
  it("adds the settings SCM hint while preserving path, search, and hash", () => {
    const location = new URL(
      "https://dev.g5labs.ai/g5labs/base?tab=docs#node-1",
    ) as unknown as Location;

    expect(buildScmOAuthReturnTo(location)).toBe(
      "/g5labs/base?tab=docs&settings=scm#node-1",
    );
  });

  it("detects the SCM settings hint and clears all SCM return params", () => {
    const params = new URLSearchParams(
      "settings=scm&scm_connected=true&tab=docs",
    );

    expect(isScmSettingsReturn(params)).toBe(true);
    // The settings tab (WorkspacePage) is opened from `settings=scm` before the
    // params are consumed; clearing then removes every SCM return param —
    // including the `settings` hint — so the URL is left clean (only unrelated
    // params like `tab` survive). WorkspaceSettingsContent is the single mutator
    // of this URL, so no `settings=scm` is left to race WorkspacePage.
    expect(clearScmOAuthParams(params).toString()).toBe("tab=docs");
  });
});
