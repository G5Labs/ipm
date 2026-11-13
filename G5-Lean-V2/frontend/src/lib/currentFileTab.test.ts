import { describe, expect, it } from "vitest";
import { canonicalFilePath, currentFileTab } from "./currentFileTab";
import type { DocNode, TabItem } from "./types";

const docs = [
  {
    id: "opaque-doc-id",
    repoId: "repo-a",
    filePath: "docs/workspace/design.md",
  },
] as DocNode[];
describe("current file identities", () => {
  it("resolves a document to the same repository/path as Files", () => {
    expect(
      currentFileTab(
        { type: "doc", docId: "opaque-doc-id", name: "Design" },
        docs,
      ),
    ).toEqual(
      currentFileTab(
        {
          type: "file",
          branchId: "repo-a",
          filePath: "./docs/workspace/../workspace/design.md",
          name: "Design",
        },
        docs,
      ),
    );
  });
  it("retains the live authority marker when restoring a document as a file", () => {
    const item: TabItem = {
      type: "doc",
      docId: "opaque-doc-id",
      name: "Design",
      liveAuthorityKey: "previous-owner-and-vm",
    };
    expect(currentFileTab(item, docs)).toMatchObject({
      type: "file",
      branchId: "repo-a",
      filePath: "docs/workspace/design.md",
      liveAuthorityKey: "previous-owner-and-vm",
    });
  });
  it("keeps repositories, unresolved documents and historical tabs distinct", () => {
    const first = currentFileTab(
      { type: "doc", docId: "opaque-doc-id", name: "Design" },
      docs,
    );
    const second = currentFileTab(
      {
        type: "file",
        branchId: "repo-b",
        filePath: "docs/workspace/design.md",
        name: "Design",
      },
      docs,
    );
    expect(first).not.toEqual(second);
    const unresolved = {
      type: "doc",
      docId: "missing",
      name: "Unknown",
    } as const;
    expect(currentFileTab(unresolved, docs, "repo-a")).toBe(unresolved);
    const revision = {
      type: "commit-diff",
      commitHash: "old",
      name: "History",
    } as unknown as TabItem;
    expect(currentFileTab(revision, docs)).toBe(revision);
    expect(
      currentFileTab(
        { type: "doc", docId: "opaque-doc-id", name: "Design" },
        [{ ...docs[0], repoId: undefined }],
        "repo-b",
      ),
    ).toMatchObject({ type: "doc" });
  });
  it("resolves restored repository handles outside the Docs listing only in their workspace", () => {
    const workspace = "12345678-1234-4123-8123-123456789abc";
    const repo = "87654321-4321-4321-8321-cba987654321";
    const item = {
      type: "doc",
      docId: `overlay:${workspace}:${repo}:${btoa("docs/archive.md")}`,
      name: "Archive",
    } as const;
    expect(currentFileTab(item, [], workspace)).toEqual({
      type: "file",
      branchId: repo,
      filePath: "docs/archive.md",
      name: "Archive",
    });
    expect(currentFileTab(item, [], "another-workspace")).toBe(item);
  });
  it("keeps an unqualified file unresolved instead of guessing its repository", () => {
    expect(
      currentFileTab(
        { type: "file", filePath: "./shared.md", name: "shared" },
        docs,
        "workspace",
      ),
    ).toEqual({ type: "file", filePath: "shared.md", name: "shared" });
  });
  it("normalizes repository paths without escaping their root", () => {
    expect(canonicalFilePath("docs/./a/../b.md")).toBe("docs/b.md");
    for (const path of ["/etc/passwd", "../a", "a/../../b", "a\0b", ""])
      expect(canonicalFilePath(path)).toBeNull();
  });
});
