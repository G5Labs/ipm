import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  USER_GUIDE_DOCS,
  getGuideDocByHref,
  guideDocTab,
  stripGuideMarkdownComments,
} from "./userGuide";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const clientDir = path.resolve(__dirname, "..", "..");
const generationDir = process.env.G5_GENERATION_DIR
  ? path.resolve(process.env.G5_GENERATION_DIR)
  : path.resolve(clientDir, "..");

describe("user guide registry", () => {
  it("uses the canonical guide titles", () => {
    expect(USER_GUIDE_DOCS.map((guide) => guide.title)).toEqual([
      "Getting Started",
      "Using G5 to Build from Scratch",
      "Using G5 to Enhance an Existing Library",
    ]);
  });

  it("points every guide URL at an existing Markdown asset", () => {
    for (const guide of USER_GUIDE_DOCS) {
      expect(guide.url).toBe(`/user-guide/${guide.markdownFileName}`);
      expect(guide).not.toHaveProperty("htmlFileName");
      expect(
        existsSync(
          path.join(clientDir, "public", "user-guide", guide.markdownFileName),
        ),
      ).toBe(true);
      for (const alias of guide.aliases ?? []) {
        expect(
          existsSync(path.join(clientDir, "public", "user-guide", alias)),
        ).toBe(true);
      }
    }
  });

  it("resolves known guide Markdown links only", () => {
    expect(getGuideDocByHref("onboarding-build-from-scratch.md")?.id).toBe(
      "onboarding-build-from-scratch",
    );
    expect(getGuideDocByHref("./getting-started-latest.md")?.id).toBe(
      "getting-started",
    );
    expect(getGuideDocByHref("./getting-started.md")?.id).toBe(
      "getting-started",
    );
    expect(
      getGuideDocByHref("/user-guide/onboarding-PM-PoC-legacy.md#intro")?.id,
    ).toBe("onboarding-pm-poc-legacy");
    expect(getGuideDocByHref("other/getting-started.md")).toBe(null);
    expect(getGuideDocByHref("/user-guide/other/getting-started.md")).toBe(
      null,
    );
    expect(getGuideDocByHref("onboarding-build-from-scratch.html")).toBe(null);
    expect(getGuideDocByHref("https://example.com/getting-started.html")).toBe(
      null,
    );
  });

  it("creates read-only guide tab items", () => {
    expect(guideDocTab("getting-started")).toEqual({
      type: "guide_doc",
      guideId: "getting-started",
      name: "Getting Started",
    });
    expect(guideDocTab("missing-guide")).toBe(null);
  });

  it("keeps frame blocking without a guide iframe exception", () => {
    const caddyfile = readFileSync(
      path.join(generationDir, "infra", "docker", "Caddyfile"),
      "utf8",
    );

    expect(caddyfile).toContain("handle /user-guide/*");
    expect(caddyfile).toContain("X-Frame-Options DENY");
    expect(caddyfile).not.toContain("@guide_docs path /user-guide/*");
    expect(caddyfile).not.toContain("not path /user-guide/*");
    expect(caddyfile).not.toMatch(/^\s*X-Frame-Options SAMEORIGIN\s*$/m);
  });

  it("does not retain generated guide HTML, CSS, or build tooling", () => {
    for (const artifact of [
      "getting-started.html",
      "onboarding-build-from-scratch.html",
      "onboarding-PM-PoC-legacy.html",
      "guide.css",
    ]) {
      expect(
        existsSync(path.join(clientDir, "public", "user-guide", artifact)),
      ).toBe(false);
    }
    expect(
      existsSync(path.join(clientDir, "scripts", "build-user-guide.mjs")),
    ).toBe(false);

    const packageJson = JSON.parse(
      readFileSync(path.join(clientDir, "package.json"), "utf8"),
    );
    expect(packageJson.scripts).not.toHaveProperty("user-guide:build");
    expect(packageJson.scripts).not.toHaveProperty("user-guide:check");
    expect(packageJson.scripts.build).not.toContain("user-guide:check");
    expect(packageJson.devDependencies).not.toHaveProperty("marked");

    const justfile = readFileSync(path.join(generationDir, "Justfile"), "utf8");
    expect(justfile).not.toContain("user-guide-sync");
  });

  it("strips authored HTML comments from rendered guide Markdown", () => {
    expect(
      stripGuideMarkdownComments(
        "# Title\n<!-- SCREENSHOT: hidden note -->\nVisible",
      ),
    ).toBe("# Title\n\nVisible");
  });

  it("uses the latest Getting Started asset while keeping the old path as an alias", () => {
    const gettingStarted = USER_GUIDE_DOCS.find(
      (guide) => guide.id === "getting-started",
    );

    expect(gettingStarted?.markdownFileName).toBe("getting-started-latest.md");
    expect(gettingStarted?.url).toBe("/user-guide/getting-started-latest.md");
    expect(gettingStarted?.aliases).toContain("getting-started.md");

    const latestGuide = readFileSync(
      path.join(clientDir, "public", "user-guide", "getting-started-latest.md"),
      "utf8",
    );

    expect(latestGuide).toContain("# G5 Platform - Getting Started");
  });
});
