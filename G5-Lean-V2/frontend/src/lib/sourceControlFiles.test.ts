import { expect, it } from "vitest";
import { countChangedFiles, changeFileSummary } from "./changeFiles";
import { groupSourcePatches } from "./sourceControlFiles";
import type { SourcePatch } from "./sourceControlSelection";

it("partitions canonical repository/file identities rather than patches or basenames", () => {
  const files = [
    { repoId: "code", path: "same.md", kind: "code" },
    { repoId: "code", path: "same.md", kind: "code" },
    { repoId: "docs", path: "same.md", kind: "docs" },
    { repoId: "docs", path: ".g5/ontology/a.g5.md", kind: "ontology" },
  ];
  const counts = countChangedFiles(files);
  expect(counts).toEqual({ code: 1, ontology: 1, docs: 1 });
  expect(changeFileSummary(counts, ["code", "ontology"])).toBe(
    "Code 1 · Ontology 1",
  );
  expect(changeFileSummary(counts, [])).toBe("");
});

it("groups a bounded large comparison once, using canonical destinations for earlier patches", () => {
  const patches = Array.from(
    { length: 2000 },
    (_, i): SourcePatch => ({
      repoId: "repo",
      repoName: "Repo",
      fileKey: `canonical/${Math.floor(i / 2)}.md`,
      kind: ["code", "ontology", "docs"][
        Math.floor(i / 2) % 3
      ] as SourcePatch["kind"],
      label: "Selected patch",
      before: "parent",
      after: "commit",
      file: {
        path: i % 2 ? `canonical/${Math.floor(i / 2)}.md` : `old/${i}.md`,
        oldPath: null,
        change: i % 2 ? "renamed" : "modified",
        diff: { status: "text", additions: 1, deletions: 2, hunks: [] },
      },
    }),
  );
  const grouped = groupSourcePatches(patches);
  expect(grouped.groups.size).toBe(1000);
  expect(
    [...grouped.groups.values()].every((group) => group.length === 2),
  ).toBe(true);
  expect(grouped.counts).toEqual({ code: 334, ontology: 333, docs: 333 });
  expect(grouped.additions).toBe(2000);
  expect(grouped.deletions).toBe(4000);
  expect(grouped.groups.get("repo\0canonical/0.md")).toEqual(
    patches.slice(0, 2),
  );
});
