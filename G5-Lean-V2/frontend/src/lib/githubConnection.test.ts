import { describe, expect, it } from "vitest";
import {
  githubConnectionPath,
  githubConnectionReturnPath,
} from "./githubConnection";

describe("GitHub connection return destination", () => {
  it.each([
    null,
    "https://evil.test",
    "//evil.test",
    "/\\evil.test",
    "/\nevil",
    "/login",
    "/account/github?returnTo=/work",
    "/account/x/../github",
  ])("refuses unsafe or recursive destination %s", (path) => {
    expect(githubConnectionReturnPath(path)).toBe("/");
  });
  it("preserves an application query and fragment through the connection route", () => {
    const path = "/acme/work?view=activity#comment";
    const url = new URL(githubConnectionPath(path), "https://g5.test");
    expect(url.pathname).toBe("/account/github");
    expect(url.searchParams.get("returnTo")).toBe(path);
  });
});
