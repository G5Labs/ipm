// =============================================================================
// Native G5 tracker helpers (g5.issue.native): the issue key suggested from
// an organization slug, the key rule, people's names, the native issue page
// path and the `issues` instance feature.
// =============================================================================

import { describe, expect, it } from "vitest";

import {
  isExternalIssuesEnabled,
  isGithubIssuesEnabled,
} from "./externalIssuesFlags";
import { isNativeIssue, issuePagePath, nativeIssueHref } from "./issuePage";
import {
  isValidNativeIssueKey,
  issueUserLabel,
  issueUserName,
  suggestNativeIssueKey,
} from "./nativeIssues";

describe("suggestNativeIssueKey", () => {
  it.each([
    ["acme", "ACME"],
    ["acme-corp", "ACMECORP"],
    ["g5labs", "G5LABS"],
    ["42-widgets", "WIDGETS"],
    ["very-long-organization", "VERYLONGOR"],
    ["123", "G5"],
    ["-", "G5"],
    ["x", "G5"],
  ])("suggests a key for %s", (slug, key) => {
    expect(suggestNativeIssueKey(slug)).toBe(key);
    expect(isValidNativeIssueKey(suggestNativeIssueKey(slug))).toBe(true);
  });

  it("accepts only 2–10 letters and digits starting with a letter", () => {
    expect(isValidNativeIssueKey("AB")).toBe(true);
    expect(isValidNativeIssueKey("A")).toBe(false);
    expect(isValidNativeIssueKey("1AB")).toBe(false);
    expect(isValidNativeIssueKey("ABCDEFGHIJK")).toBe(false);
    expect(isValidNativeIssueKey("ab")).toBe(false);
  });
});

describe("issue people", () => {
  it("names a G5 member by display name and a GitHub account by login", () => {
    const member = {
      login: "maya@g5labs.dev",
      avatarUrl: null,
      userId: "user-maya",
      displayName: "Maya Patel",
    };
    const github = { login: "octocat", avatarUrl: null };
    expect(issueUserName(member)).toBe("Maya Patel");
    expect(issueUserLabel(member)).toBe("Maya Patel");
    expect(issueUserName(github)).toBe("octocat");
    expect(issueUserLabel(github)).toBe("@octocat");
    expect(issueUserLabel(null)).toBeNull();
  });
});

describe("native issue page", () => {
  it("builds the native page path and keeps GitHub's", () => {
    expect(issuePagePath("acme", "g5", 12)).toBe("/acme/issues/g5/12");
    expect(issuePagePath("acme", "11", 12)).toBe("/acme/issues/github/11/12");
  });

  it("recognizes native issues by provider, else by locator", () => {
    expect(isNativeIssue({ provider: "g5" })).toBe(true);
    expect(isNativeIssue({ provider: "github" })).toBe(false);
    expect(isNativeIssue({ locator: { repositoryId: "g5", number: 1 } })).toBe(
      true,
    );
    expect(isNativeIssue({ locator: { repositoryId: "11", number: 1 } })).toBe(
      false,
    );
  });

  it("navigates to a native page URL within the app", () => {
    expect(
      nativeIssueHref("https://g5.example.com/acme/issues/g5/12#comment-4"),
    ).toBe("/acme/issues/g5/12#comment-4");
  });
});

describe("issue features", () => {
  it("offers issues on `issues` (or an older server's `external_issues`) and GitHub only on `external_issues`", () => {
    expect(isExternalIssuesEnabled({ features: ["issues"] })).toBe(true);
    expect(isExternalIssuesEnabled({ features: ["external_issues"] })).toBe(
      true,
    );
    expect(isExternalIssuesEnabled({ features: ["chat"] })).toBe(false);
    expect(isExternalIssuesEnabled(null)).toBe(false);
    expect(isGithubIssuesEnabled({ features: ["issues"] })).toBe(false);
    expect(isGithubIssuesEnabled({ features: ["external_issues"] })).toBe(true);
  });
});
