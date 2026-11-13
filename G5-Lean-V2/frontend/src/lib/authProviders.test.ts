import { describe, expect, it } from "vitest";

import {
  configuredAuthProviders,
  getAuthProviderHref,
  getAuthProviderLabel,
} from "./authProviders";

describe("auth provider helpers", () => {
  it("shows only the providers the instance reports", () => {
    expect(configuredAuthProviders([])).toEqual([]);
    expect(configuredAuthProviders(["github", "github"])).toEqual(["github"]);
    expect(configuredAuthProviders(["microsoft"])).toEqual(["microsoft"]);
  });

  it("keeps product order with GitHub before Microsoft", () => {
    expect(configuredAuthProviders(["microsoft", "github"])).toEqual([
      "github",
      "microsoft",
    ]);
  });

  it("ignores retired and unknown login providers", () => {
    expect(configuredAuthProviders(["google", "gitlab", "unknown"])).toEqual(
      [],
    );
    expect(
      configuredAuthProviders(["google", "gitlab", "microsoft", "github"]),
    ).toEqual(["github", "microsoft"]);
  });

  it("links to each platform login", () => {
    expect(getAuthProviderLabel("github")).toBe("GitHub");
    expect(getAuthProviderHref("github")).toBe("/api/v1/auth/login/github");
    expect(getAuthProviderLabel("microsoft")).toBe("Microsoft");
    expect(getAuthProviderHref("microsoft")).toBe(
      "/api/v1/auth/login/microsoft",
    );
  });
});
