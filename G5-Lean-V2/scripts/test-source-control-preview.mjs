import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, basename } from "node:path";
import { test } from "node:test";
import {
  pushedFixtureScript,
  unpushedFixtureScript,
  terminal,
} from "./seed-source-control-preview.mjs";

test("terminal preserves adjacent binary frames and a split command completion", async () => {
  const original = globalThis.WebSocket;
  class Socket extends EventTarget {
    static OPEN = 1;
    readyState = 1;
    constructor() {
      super();
      queueMicrotask(() => {
        this.frame('{"type":"opened"}\n', 20);
        this.frame("shell prompt\n", 0);
      });
    }
    frame(text, milliseconds) {
      this.dispatchEvent(
        new MessageEvent("message", {
          data: {
            text: () =>
              new Promise((resolve) =>
                setTimeout(() => resolve(text), milliseconds),
              ),
          },
        }),
      );
    }
    send(message) {
      const input = JSON.parse(message);
      if (input.type !== "input") return;
      const nonce = input.data.match(/PREVIEW_DONE_ ([a-f0-9]+) /)[1];
      this.frame("\nPREVIEW_DONE_", 20);
      this.frame(`${nonce}:0\n`, 0);
    }
    close() {
      this.readyState = 3;
      this.dispatchEvent(new Event("close"));
    }
  }
  globalThis.WebSocket = Socket;
  try {
    const pty = await terminal(
      async () => ({ ticket: "fixture" }),
      "http://127.0.0.1:7500",
      "fixture",
    );
    try {
      await pty.run("true");
    } finally {
      pty.close();
    }
  } finally {
    globalThis.WebSocket = original;
  }
});

for (const relative of [false, true])
  test(`preview history separates pushed/local work with ${relative ? "relative" : "absolute"} member roots`, () => {
    const root = mkdtempSync(join(tmpdir(), "source-preview-"));
    const git = (dir, ...args) =>
      execFileSync("git", ["-C", dir, ...args], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      }).trim();
    try {
      const code = join(root, "Code's checkout");
      const docs = join(root, "Docs checkout");
      const base = new Map();
      const pushed = new Map();
      for (const dir of [code, docs]) {
        execFileSync("git", ["init", "-q", "-b", "preview", dir]);
        git(
          dir,
          "-c",
          "user.name=Fixture",
          "-c",
          "user.email=fixture@g5.dev",
          "commit",
          "-qm",
          "Base",
          "--allow-empty",
        );
        base.set(dir, git(dir, "rev-parse", "HEAD"));
      }
      execFileSync(
        "bash",
        [
          "-c",
          pushedFixtureScript(
            relative ? basename(code) : code,
            relative ? basename(docs) : docs,
            "preview-workspace",
          ),
        ],
        { cwd: root },
      );
      for (const dir of [code, docs]) {
        pushed.set(dir, git(dir, "rev-parse", "HEAD"));
        assert.equal(
          git(dir, "rev-list", "--count", `${base.get(dir)}..HEAD`),
          dir === code ? "4" : "3",
        );
        assert.equal(git(dir, "status", "--porcelain"), "");
      }
      execFileSync(
        "bash",
        [
          "-c",
          unpushedFixtureScript(
            relative ? basename(code) : code,
            relative ? basename(docs) : docs,
            "preview-workspace",
          ),
        ],
        { cwd: root },
      );
      for (const dir of [code, docs]) {
        assert.equal(
          git(dir, "rev-list", "--count", `${pushed.get(dir)}..HEAD`),
          "1",
        );
        assert.equal(
          git(
            dir,
            "rev-list",
            "--count",
            `${base.get(dir)}..${pushed.get(dir)}`,
          ),
          dir === code ? "4" : "3",
        );
      }
      const codeStatus = git(code, "status", "--porcelain");
      assert.match(codeStatus, /MM src\/preview-cart.ts/);
      assert.match(codeStatus, / D src\/preview-legacy.ts/);
      assert.match(codeStatus, /\?\? src\/preview-coupon.ts/);
      assert.match(codeStatus, /M \.g5\/ontology\/preview-checkout.g5.md/);
      assert.match(git(code, "diff", "--cached"), /shipping = 2/);
      assert.match(git(code, "diff"), /discount = 15/);
      assert.match(
        git(docs, "status", "--porcelain"),
        /M docs\/preview-workspace\/preview-checkout.md/,
      );
      assert.match(
        git(docs, "status", "--porcelain"),
        /\?\? docs\/preview-workspace\/preview-follow-up.md/,
      );
      // A Save-like commit must collect the entire local tail, while the PR's
      // previously captured pushed vector remains the three-commit revision.
      for (const dir of [code, docs]) {
        git(dir, "add", "-A");
        git(
          dir,
          "-c",
          "user.name=Fixture",
          "-c",
          "user.email=fixture@g5.dev",
          "commit",
          "-qm",
          "Save changes",
        );
        assert.equal(git(dir, "status", "--porcelain"), "");
        assert.equal(
          git(dir, "rev-list", "--count", `${pushed.get(dir)}..HEAD`),
          "2",
        );
        assert.equal(
          git(
            dir,
            "rev-list",
            "--count",
            `${base.get(dir)}..${pushed.get(dir)}`,
          ),
          dir === code ? "4" : "3",
        );
      }
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
