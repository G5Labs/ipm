#!/usr/bin/env node
// Real full-stack preview data, owned by the dev-login Alex persona. All
// mutations use the same fork, terminal, Save and PR routes as the product.
// Existing workspaces are preserved: rerunning startup must not save a user's
// local edits or replace their selections. --verify is a read-only state check.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { pathToFileURL } from "node:url";

export const fixtures = [
  { name: "source-control-no-pr", pr: false },
  { name: "source-control-with-pr", pr: true },
];
const quote = (s) => `'${s.replaceAll("'", "'\\''")}'`;
const file = (path, content) =>
  `mkdir -p ${quote(path.slice(0, path.lastIndexOf("/")))}\nprintf %s ${quote(content)} > ${quote(path)}`;
const commit = (message) =>
  `git add -A\ngit -c user.name='Alex Preview' -c user.email=demo@g5labs.ai commit -qm ${quote(message)}`;

export function ontologyFixtureContent(local = false) {
  return `# Preview checkout\n\n[I · preview.checkout.shipping]\n\nCheckout keeps shipping and discounts explicit.\n\n${local ? "[I · preview.checkout.coupons]\n\nPreview coupons need review before publication.\n\n" : ""}## Metadata\n\n### preview.checkout.shipping\n- source: agent\n${local ? "\n### preview.checkout.coupons\n- source: agent\n" : ""}`;
}

// Code has four selectable pushed commits (including a category-changing
// rename), Docs has three. The local tail is
// deliberately added only AFTER Save and (for the second workspace) PR create.
export function pushedFixtureScript(codeRoot, docsRoot, workspaceId) {
  return `set -e\npreview_root="$PWD"\ncd ${quote(codeRoot)}\n${file("src/preview-cart.ts", "export const shipping = 5;\nexport const discount = 0;\n")}
${file("src/preview-legacy.ts", "export const legacyCheckout = true;\n")}
${commit("Add preview checkout and legacy adapter")}
${file("src/preview-cart.ts", "export const shipping = 3;\nexport const discount = 0;\n")}
${file(".g5/ontology/preview-checkout.md", ontologyFixtureContent())}
${commit("Lower shipping and describe checkout intent")}
${file("tests/preview-cart.test.ts", 'import { shipping } from "../src/preview-cart";\nconsole.assert(shipping === 3);\n')}
${commit("Cover the pushed shipping rate")}
git mv .g5/ontology/preview-checkout.md .g5/ontology/preview-checkout.g5.md
${commit("Promote checkout notes to supported ontology")}
cd "$preview_root"
cd ${quote(docsRoot)}
${file(`docs/${workspaceId}/preview-checkout.md`, "# Checkout plan\n\nOffer predictable shipping.\n")}
${commit("Draft the checkout plan")}
${file(`docs/${workspaceId}/preview-checkout.md`, "# Checkout plan\n\nOffer predictable shipping.\n\nThe pushed implementation charges 3.\n")}
${commit("Document the pushed shipping rate")}
${file(`docs/${workspaceId}/preview-review.md`, "# Review notes\n\nReview shipping before applying a discount.\n")}
${commit("Add checkout review notes")}`;
}

export function unpushedFixtureScript(codeRoot, docsRoot, workspaceId) {
  return `set -e\npreview_root="$PWD"\ncd ${quote(codeRoot)}
${file("src/preview-cart.ts", "export const shipping = 3;\nexport const discount = 10;\n")}
${commit("Try a local checkout discount (not pushed)")}
${file("src/preview-cart.ts", "export const shipping = 2;\nexport const discount = 10;\n")}
git add src/preview-cart.ts
${file("src/preview-cart.ts", "export const shipping = 2;\nexport const discount = 15;\n")}
rm src/preview-legacy.ts
${file("src/preview-coupon.ts", 'export const coupon = "PREVIEW15";\n')}
${file(".g5/ontology/preview-checkout.g5.md", ontologyFixtureContent(true))}
cd "$preview_root"
cd ${quote(docsRoot)}
${file(`docs/${workspaceId}/preview-review.md`, "# Review notes\n\nLocal commit: check the proposed discount.\n")}
${commit("Record local discount review (not pushed)")}
${file(`docs/${workspaceId}/preview-checkout.md`, "# Checkout plan\n\nUnpushed draft: try shipping 2 with a 15 percent discount.\n")}
${file(`docs/${workspaceId}/preview-follow-up.md`, "# Follow-up\n\nUntracked draft: validate the coupon before Save changes.\n")}`;
}

export function assertFixtureMenu(menu) {
  assert.equal(menu.private, true, "Alex must see live owner data");
  assert.equal(
    menu.repos.length,
    2,
    "expected the inherited Code and Docs repositories",
  );
  for (const repo of menu.repos) {
    assert.equal(repo.error, null, `${repo.name}: repository read failed`);
    assert.equal(repo.truncated, false, `${repo.name}: incomplete history`);
    assert.ok(
      repo.base && repo.pushed !== repo.base,
      `${repo.name}: real fork base and pushed history`,
    );
    assert.equal(
      repo.commits.length,
      repo.role === "code" ? 4 : 3,
      `${repo.name}: pushed commit list`,
    );
    assert.equal(
      repo.commits[0].sha,
      repo.pushed,
      `${repo.name}: pushed tip matches history`,
    );
    assert.deepEqual(
      repo.live,
      { selectable: true },
      `${repo.name}: local work is read only on Show`,
    );
  }
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(label, read, ready, timeout = 120_000) {
  const deadline = Date.now() + timeout;
  do {
    const value = await read();
    if (ready(value)) return value;
    await delay(500);
  } while (Date.now() < deadline);
  throw new Error(`Timed out waiting for ${label}`);
}

// Ticket-authenticated production PTY; no host filesystem or database edits.
export async function terminal(api, apiUrl, workspaceId) {
  const { ticket } = await api("/auth/ws-ticket", "POST", { workspaceId });
  const url = new URL(`/api/v1/workspaces/${workspaceId}/terminal`, apiUrl);
  url.protocol = "ws:";
  url.search = new URLSearchParams({ token: ticket, cols: "240", rows: "40" });
  const socket = new WebSocket(url);
  let output = "";
  let messages = Promise.resolve();
  let ended = false;
  let transportError = false;
  socket.addEventListener("message", ({ data }) => {
    // Blob decoding is async. Queue frames so adjacent control/output frames
    // cannot overwrite each other or reorder a split completion marker.
    messages = messages.then(async () => {
      const text = typeof data === "string" ? data : await data.text();
      output += text;
      if (output.length > 1024 * 1024) output = output.slice(-512 * 1024);
    });
  });
  socket.addEventListener("close", () => {
    ended = true;
  });
  socket.addEventListener("error", () => {
    transportError = true;
  });
  try {
    await until(
      "workspace terminal",
      () => output,
      (value) => {
        if (ended || transportError || value.includes('"type":"rejected"'))
          throw new Error("Preview terminal unavailable");
        return value.includes('"type":"opened"');
      },
      30_000,
    );
  } catch (error) {
    socket.close();
    throw error;
  }
  return {
    async run(script) {
      const nonce = randomUUID().replaceAll("-", "");
      const marker = `PREVIEW_DONE_${nonce}:`;
      const encoded = Buffer.from(script).toString("base64");
      // Split marker literals so command echo cannot satisfy the completion.
      socket.send(
        JSON.stringify({
          type: "input",
          data: `stty -echo; printf %s '${encoded}' | base64 -d | bash; printf '\\n%s%s:%s\\n' PREVIEW_DONE_ ${nonce} "$?"\n`,
        }),
      );
      const result = await until(
        "fixture Git command",
        () => output,
        (value) => {
          if (ended || transportError)
            throw new Error("Preview terminal disconnected");
          return value.includes(marker);
        },
        45_000,
      );
      assert.equal(
        result
          .split(marker)
          .at(-1)
          .split(/[\r\n]/)[0],
        "0",
        `fixture Git command failed: ${result.slice(-4000)}`,
      );
    },
    close() {
      if (socket.readyState === WebSocket.OPEN)
        socket.send(JSON.stringify({ type: "close" }));
      socket.close();
    },
  };
}

export async function main() {
  const apiUrl = new URL(
    process.env.G5_SOURCE_PREVIEW_API_URL ??
      `http://127.0.0.1:${process.env.G5_PORT_API ?? 7500}`,
  );
  assert.ok(
    ["localhost", "127.0.0.1", "[::1]"].includes(apiUrl.hostname) &&
      apiUrl.protocol === "http:",
    "seed only a local dev/preview API",
  );
  let token;
  async function api(path, method = "GET", body) {
    const response = await fetch(new URL(`/api/v1${path}`, apiUrl), {
      method,
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      signal: AbortSignal.timeout(60_000),
    });
    assert.ok(response.ok, `${method} ${path} failed (${response.status})`);
    return (await response.json()).data;
  }
  token = (await api("/auth/test-token", "POST", { email: "demo@g5labs.ai" }))
    .jwt;
  assert.ok(token, "development auth required");
  const org = (await api("/orgs")).find((o) => o.orgSlug === "g5labs");
  assert.ok(org, "run just dev-up first");
  const workspaces = await api(`/orgs/${org.orgId}/workspaces`);
  const base = workspaces.find((w) => w.isBase);
  assert.ok(base, "Base workspace required");
  const verify = process.argv.includes("--verify");
  for (const fixture of fixtures) {
    let workspace = workspaces.find((w) => w.name === fixture.name);
    if (workspace && !verify) {
      assert.ok(
        !workspace.description?.startsWith(
          "Initializing Source Control preview",
        ),
        `/g5labs/${fixture.name} has an incomplete seed; preserve it for inspection or recreate the preview`,
      );
      console.log(
        `Preserved existing /g5labs/${fixture.name}; no edits or Save performed. Use --verify to check initial fixture state.`,
      );
      continue;
    }
    if (!verify) {
      const clone = await api(`/workspaces/${base.id}/clone`, "POST", {
        name: fixture.name,
        source: "durable",
        idempotencyKey: `source-control-preview-v1:${base.id}:${fixture.name}`,
        description:
          "Initializing Source Control preview; startup will verify the Git and PR state before marking this example ready.",
      });
      workspace = clone.workspace;
      assert.equal(
        workspace.name,
        fixture.name,
        "fixture name must not silently change",
      );
      console.log(`Preparing /g5labs/${fixture.name} from Base`);
      await api(`/workspaces/${workspace.id}/resume`, "POST", {
        operationId: randomUUID(),
      });
      await until(
        "running workspace",
        () => api(`/workspaces/${workspace.id}/vm`),
        (v) => v.status === "running",
      );
      const live = await until(
        "live repository checkouts",
        () => api(`/workspaces/${workspace.id}/changes`),
        (v) => v.source === "live" && v.attached,
      );
      const detail = await api(`/workspaces/${workspace.id}`);
      const docs = live.members.find((m) => m.repo_id === detail.docsRepoId);
      const code = live.members.find((m) => m.repo_id !== detail.docsRepoId);
      assert.equal(live.members.length, 2);
      assert.ok(
        code && docs && live.members.every((m) => m.errors.length === 0),
        "healthy inherited Code and Docs checkouts",
      );
      // Register the managed project documents through their normal API.
      for (const name of ["checkout", "review"]) {
        await api(`/workspaces/${workspace.id}/docs`, "POST", {
          name: `Preview ${name}`,
          docType: "design",
          filePath: `docs/${workspace.id}/preview-${name}.md`,
          content: `# Preview ${name}\n`,
        });
      }
      const pty = await terminal(api, apiUrl, workspace.id);
      try {
        await pty.run(
          pushedFixtureScript(code.root_path, docs.root_path, workspace.id),
        );
        const saved = await api(
          `/workspaces/${workspace.id}/source-control/save`,
          "POST",
          {},
        );
        assert.equal(saved.saved, true, "Save must publish actual commits");
        if (fixture.pr) {
          const result = await api(
            `/workspaces/${workspace.id}/pull-requests`,
            "POST",
            {
              targetWorkspaceId: base.id,
              title: "Preview checkout improvements",
              description:
                "Pushed checkout and shipping changes. Further discount work remains local for Source Control selection and Save changes.",
            },
          );
          assert.equal(result.pullRequest.status, "open", "PR must be open");
        }
        await pty.run(
          unpushedFixtureScript(code.root_path, docs.root_path, workspace.id),
        );
      } finally {
        pty.close();
      }
    }
    assert.ok(workspace, `Missing /g5labs/${fixture.name}`);
    const menu = await api(`/workspaces/${workspace.id}/source-control/menu`);
    assertFixtureMenu(menu);
    // Exercise the production comparison boundary too: menu counts alone can
    // look plausible even if Save published the local tail or a tree is empty.
    for (const mode of ["pushed", "unpushed", "all"]) {
      const comparison = await api(
        `/workspaces/${workspace.id}/source-control/apply`,
        "POST",
        {
          fingerprint: menu.fingerprint,
          repos: menu.repos.map((repo) => ({
            repoId: repo.repoId,
            base: repo.base,
            pushed: repo.pushed,
            commits: mode === "unpushed" ? [] : repo.commits.map((c) => c.sha),
            includeUnpushed: mode !== "pushed",
          })),
        },
      );
      assert.equal(
        comparison.fileCount,
        mode === "pushed" ? 6 : 7,
        `${mode}: exact unique changed files`,
      );
      const expected = menu.repos.flatMap((repo) => {
        const paths =
          repo.role === "code"
            ? [
                "src/preview-cart.ts",
                ".g5/ontology/preview-checkout.g5.md",
                ...(mode === "all" ? [] : ["src/preview-legacy.ts"]),
                ...(mode === "unpushed" ? [] : ["tests/preview-cart.test.ts"]),
                ...(mode === "pushed" ? [] : ["src/preview-coupon.ts"]),
              ]
            : [
                `docs/${workspace.id}/preview-checkout.md`,
                `docs/${workspace.id}/preview-review.md`,
                ...(mode === "pushed"
                  ? []
                  : [`docs/${workspace.id}/preview-follow-up.md`]),
              ];
        return paths.map((path) => `${repo.repoId}:${path}`);
      });
      assert.deepEqual(
        new Set(comparison.patches.map((p) => `${p.repoId}:${p.fileKey}`)),
        new Set(expected),
        `${mode}: exact repository/file identities`,
      );
      assert.deepEqual(
        new Set(comparison.patches.map((p) => p.kind)),
        new Set(["code", "ontology", "docs"]),
        `${mode}: all three file categories`,
      );
      const bytes = JSON.stringify(comparison.patches);
      if (mode === "pushed") {
        assert.ok(
          !bytes.includes("PREVIEW15") && !bytes.includes("discount = 15"),
          "pushed comparison excludes local bytes",
        );
      } else {
        assert.ok(
          bytes.includes("PREVIEW15") && bytes.includes("discount = 15"),
          `${mode}: real local diffs`,
        );
      }
    }
    const prs = await api(
      `/workspaces/${workspace.id}/pull-requests?role=source&status=all`,
    );
    assert.equal(prs.length, fixture.pr ? 1 : 0, "fixture outgoing PR count");
    if (fixture.pr) {
      assert.equal(prs[0].status, "open");
      assert.equal(prs[0].targetWorkspaceId, base.id);
      assert.deepEqual(
        new Map(prs[0].sourceHeads.map((h) => [h.repoId, h.headCommit])),
        new Map(menu.repos.map((r) => [r.repoId, r.pushed])),
        "PR captures pushed tips, excluding the local commits",
      );
    }
    if (!verify)
      await api(`/workspaces/${workspace.id}`, "PATCH", {
        description: `Source Control preview: four pushed Code commits, three pushed Docs commits and one local commit per repository, staged/unstaged/untracked edits; ${fixture.pr ? "open PR to Base" : "no PR"}. Save changes publishes the local work.`,
      });
    console.log(
      `Verified /g5labs/${fixture.name}: 7 pushed commits, 2 local commits, dirty Code + Docs, ${prs.length} PR.`,
    );
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
