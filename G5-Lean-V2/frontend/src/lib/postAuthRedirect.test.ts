import { beforeEach, describe, expect, it } from "vitest";
import {
  clearPostAuthRedirect,
  getPostAuthRedirect,
  rememberPostAuthRedirect,
} from "./postAuthRedirect";

describe("postAuthRedirect", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  it("stores protected app routes for post-auth resume", () => {
    rememberPostAuthRedirect("/g5labs/base/doc/abc?mode=edit#comments");
    expect(getPostAuthRedirect()).toBe(
      "/g5labs/base/doc/abc?mode=edit#comments",
    );
  });

  it("ignores public entry routes", () => {
    rememberPostAuthRedirect("/landing");
    expect(getPostAuthRedirect()).toBeNull();

    rememberPostAuthRedirect("/coming-soon.html");
    expect(getPostAuthRedirect()).toBeNull();
  });

  it("rejects and clears the removed legacy /home return path", () => {
    rememberPostAuthRedirect("/home?from=legacy#resume");
    expect(getPostAuthRedirect()).toBeNull();

    sessionStorage.setItem("g5_post_auth_redirect", "/home");
    expect(getPostAuthRedirect()).toBeNull();
    expect(sessionStorage.getItem("g5_post_auth_redirect")).toBeNull();
  });

  it("clears the stored redirect explicitly", () => {
    rememberPostAuthRedirect("/g5labs/base");
    clearPostAuthRedirect();
    expect(getPostAuthRedirect()).toBeNull();
  });
});
