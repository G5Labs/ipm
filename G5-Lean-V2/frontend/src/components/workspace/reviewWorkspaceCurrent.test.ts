import { describe, expect, it } from "vitest";
import { isPublicationCurrent } from "./reviewWorkspace";

const vector = (repoId: string, sourceHead: string) => ({
  baseCommitVector: [{ repoId, sourceHead }],
});

describe("isPublicationCurrent", () => {
  it("is current while every repository of its vector is at that head", () => {
    expect(
      isPublicationCurrent(vector("repo-a", "abc"), { "repo-a": "ABC" }),
    ).toBe(true);
  });

  it("is outdated once the source moved on", () => {
    expect(
      isPublicationCurrent(vector("repo-a", "abc"), { "repo-a": "def" }),
    ).toBe(false);
  });

  it("is outdated when the source no longer reports the repository's head, like the server", () => {
    expect(isPublicationCurrent(vector("repo-a", "abc"), {})).toBe(false);
    expect(isPublicationCurrent(vector("repo-a", "abc"), null)).toBe(false);
  });

  it("counts an empty vector as current", () => {
    expect(isPublicationCurrent({ baseCommitVector: [] }, {})).toBe(true);
  });
});
