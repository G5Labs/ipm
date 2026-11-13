import { describe, expect, it } from "vitest";
import { defaultGenerateOptions } from "../../lib/types";
import type { GenerateOptionsV1, PendingSyncDocFile } from "../../lib/types";
import { generateChatPrompt } from "./generateChatPrompt";

const document: PendingSyncDocFile = {
  id: "requirements",
  targetKind: "doc",
  targetDocRef: "doc-ref",
  path: "docs/workspace-1/requirements.md",
  repoId: "docs-repo",
  repoName: "Docs repository",
  branchName: "feature",
  headCommit: "a".repeat(40),
  lastDownliftedCommitSha: null,
};
const render = (options = defaultGenerateOptions(), files = [document]) =>
  generateChatPrompt("workspace-1", files, options);

describe("Generate's initial chat message", () => {
  it("carries selected document paths and repository identities in the actual message", () => {
    const prompt = render();
    expect(prompt).toContain('"path":"docs/workspace-1/requirements.md"');
    expect(prompt).toContain('"repoId":"docs-repo"');
    expect(prompt).toContain('"documentRef":"doc-ref"');
    expect(prompt).toContain("current workspace checkout");
    expect(prompt).toContain("Stay on the current workspace branch");
    expect(prompt).toContain("Complete every required layer");
  });

  it.each([
    ["ontology", "Update the plan of record (ontology)"],
    ["designDoc", "Generate a design doc"],
    ["questionDoc", "Generate a question answering doc"],
    ["implPlan", "Generate an implementation plan"],
    ["testPlan", "Generate a test plan"],
    ["mocks", "Generate mocks"],
  ] as const)("includes only selected %s instructions", (option, title) => {
    const options = defaultGenerateOptions();
    options.code = "none";
    expect(render(options)).toContain(`## ${title}\n`);
    options.plans[option] = false;
    expect(render(options)).not.toContain(`## ${title}\n`);
  });

  it.each(["stubs", "full"] as const)(
    "%s requests ontology and the appropriate build",
    (code) => {
      const options = defaultGenerateOptions();
      options.code = code;
      options.plans.ontology = false;
      const prompt = render(options);
      expect(prompt).toContain("## Update the plan of record (ontology)");
      expect(prompt).toContain(
        code === "stubs"
          ? "## Build a working preview"
          : "## Build the full implementation",
      );
      expect(prompt).not.toContain(
        code === "stubs"
          ? "## Build the full implementation"
          : "## Build a working preview",
      );
    },
  );

  it("plans-only explicitly forbids code changes and excludes all implementation reports and preview setup", () => {
    const options = defaultGenerateOptions();
    options.code = "none";
    options.plans.ontology = false;
    const prompt = render(options);
    expect(prompt).toContain(
      "Do not create, edit, or delete source code, test code, configuration, or build files",
    );
    for (const heading of [
      "Build",
      "Update the plan",
      "Post-generation review",
      "Requirement coverage",
      "Refresh the status",
      "Maintain preview",
    ]) {
      expect(prompt).not.toContain(`## ${heading}`);
    }
  });

  it("coverage requests one review report; deselected reports have no instructions", () => {
    const options = defaultGenerateOptions();
    options.reports = { review: false, coverage: true, statusOverview: false };
    expect(render(options)).toContain("## Post-generation review report");
    expect(render(options)).toContain("## Requirement coverage");
    expect(render(options)).not.toContain("## Refresh the status overview");
    options.reports.coverage = false;
    expect(render(options)).not.toContain("## Post-generation review report");
    expect(render(options)).not.toContain("## Requirement coverage");
    options.reports.statusOverview = true;
    expect(render(options)).toContain("## Refresh the status overview");
  });

  it.each([
    ["concise", "Concise: executive-summary depth."],
    ["balanced", "Balanced: overview with selective detail."],
    ["indepth", "In-depth: a full walkthrough"],
  ] as const)(
    "preserves explainer style %s and both formats in the same session",
    (style, instruction) => {
      const options = defaultGenerateOptions();
      options.plans.explainer.style = style;
      expect(render(options)).toContain(instruction);
      expect(render(options)).toContain(
        "a self-contained HTML page at Project Docs/explainers/<feature-slug>/<YYYY-MM-DD-HHMM>-<style-slug>.html in this session",
      );
      options.plans.explainer.format = "slides";
      expect(render(options)).toContain(
        "a self-contained HTML slide presentation",
      );
      options.plans.explainer.enabled = false;
      expect(render(options)).not.toContain("## Generate an explainer");
      expect(render(options)).not.toContain("Project Docs/explainers/");
    },
  );

  it.each([
    ["raise_all", "Raise every dubious requirement or decision"],
    ["balanced", "Ask the user when a choice is irreversible"],
    ["autonomous", "Work as autonomously as you can"],
  ] as const)(
    "preserves %s autonomy",
    (autonomy: GenerateOptionsV1["autonomy"], instruction) => {
      expect(render({ ...defaultGenerateOptions(), autonomy })).toContain(
        instruction,
      );
    },
  );

  it("rejects empty or foreign selections rather than silently omitting a selected input", () => {
    expect(() => render(defaultGenerateOptions(), [])).toThrow(
      "Select Project Docs",
    );
    expect(() =>
      render(defaultGenerateOptions(), [
        document,
        { ...document, path: "docs/another-workspace/requirements.md" },
      ]),
    ).toThrow("current workspace");
  });
});
