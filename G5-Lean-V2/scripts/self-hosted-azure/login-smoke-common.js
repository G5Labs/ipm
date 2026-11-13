// Shared Azure canary login smoke: provider-neutral browser helpers, the
// post-login API checks, worker-host provisioning and the real microVM chat
// turn. Provider drivers (github-login-smoke.js, entra-login-smoke.js) supply
// only the sign-in interaction and their credentials.
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const baseUrl = process.env.G5_BASE_URL;
const artifactDir = process.env.G5_PLAYWRIGHT_ARTIFACT_DIR || process.cwd();
const verifyChat = process.env.G5_VERIFY_CHAT === "true";
const chatMarker = process.env.G5_CHAT_MARKER || "";
const openrouterKey = process.env.OPENROUTER_API_KEY || "";
const provisionHostOutput = process.env.G5_PROVISION_HOST_OUTPUT || "";
const provisionHostBaseImage = process.env.G5_PROVISION_HOST_BASE_IMAGE || "";

fs.mkdirSync(artifactDir, { recursive: true });

function candidateLocator(page, candidate) {
  if (candidate.name) {
    return page.getByRole(candidate.role || "button", { name: candidate.name }).first();
  }
  if (candidate.text) {
    return page.getByText(candidate.text).first();
  }
  return page.locator(candidate.selector).first();
}

async function clickFirstVisible(page, candidates, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const candidate of candidates) {
      const locator = candidateLocator(page, candidate);
      const visible = await locator.isVisible({ timeout: 500 }).catch(() => false);
      const enabled = visible && (await locator.isEnabled({ timeout: 500 }).catch(() => false));
      if (enabled) {
        const clicked = await locator
          .click({ timeout: 2000 })
          .then(() => true)
          .catch(() => false);
        if (clicked) return true;
      }
    }
    await page.waitForTimeout(500);
  }
  return false;
}

function hasReturnedToApp(page, appOrigin) {
  try {
    const url = new URL(page.url());
    return url.origin === appOrigin && !url.pathname.includes("/login");
  } catch {
    return false;
  }
}

function decodeBase32(secret) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  const normalized = secret.toUpperCase().replace(/[=\s-]/g, "");
  const bytes = [];
  let bits = 0;
  let value = 0;
  for (const char of normalized) {
    const index = alphabet.indexOf(char);
    if (index === -1) {
      throw new Error("TOTP secret contains a non-base32 character");
    }
    value = (value << 5) | index;
    bits += 5;
    if (bits >= 8) {
      bytes.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Buffer.from(bytes);
}

function generateTotp(secret, timestampMs = Date.now(), digits = 6) {
  const key = decodeBase32(secret);
  const counter = Math.floor(timestampMs / 1000 / 30);
  const counterBuffer = Buffer.alloc(8);
  counterBuffer.writeUInt32BE(Math.floor(counter / 0x100000000), 0);
  counterBuffer.writeUInt32BE(counter >>> 0, 4);
  const digest = crypto.createHmac("sha1", key).update(counterBuffer).digest();
  const offset = digest[digest.length - 1] & 0x0f;
  const binary =
    ((digest[offset] & 0x7f) << 24) |
    ((digest[offset + 1] & 0xff) << 16) |
    ((digest[offset + 2] & 0xff) << 8) |
    (digest[offset + 3] & 0xff);
  const modulus = 10 ** digits;
  return String(binary % modulus).padStart(digits, "0");
}

async function apiJson(request, method, url, jwt, data) {
  const response = await request[method](url, {
    headers: { Authorization: `Bearer ${jwt}` },
    ...(data === undefined ? {} : { data }),
  });
  if (!response.ok()) {
    throw new Error(
      `${method.toUpperCase()} ${url} failed with HTTP ${response.status()}: ${await response.text()}`,
    );
  }
  if (response.status() === 204) return null;
  const body = await response.json();
  return body.data ?? body;
}

async function pollWorkspaceRunning(request, jwt, workspaceId, timeoutMs = 900000) {
  const deadline = Date.now() + timeoutMs;
  let vm = null;
  while (Date.now() < deadline) {
    vm = await apiJson(request, "get", `${baseUrl}/api/v1/workspaces/${workspaceId}/vm`, jwt);
    if (vm.status === "running") return vm;
    if (vm.status === "lost" || vm.lastError) {
      throw new Error(`Workspace VM failed to start: ${JSON.stringify(vm)}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 5000));
  }
  throw new Error(`Timed out waiting for workspace VM ${workspaceId}: ${JSON.stringify(vm)}`);
}

async function pollWorkspacePreview(request, jwt, workspaceId, timeoutMs = 900000) {
  const deadline = Date.now() + timeoutMs;
  let preview = null;
  while (Date.now() < deadline) {
    preview = await apiJson(
      request,
      "get",
      `${baseUrl}/api/v1/workspaces/${workspaceId}/preview/`,
      jwt,
    );
    if (preview?.ready && preview.status === "running") return preview;
    if (preview && ["succeeded", "failed", "cancelled"].includes(preview.status)) {
      throw new Error(`Workspace preview ended before readiness: ${JSON.stringify(preview)}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 5000));
  }
  throw new Error(`Timed out waiting for workspace preview: ${JSON.stringify(preview)}`);
}

async function verifyExternalMicrovmWorkflow(page, request, jwt) {
  if (!chatMarker) throw new Error("G5_CHAT_MARKER is required when G5_VERIFY_CHAT=true");
  if (!openrouterKey) throw new Error("OPENROUTER_API_KEY is required when G5_VERIFY_CHAT=true");

  const orgs = await apiJson(request, "get", `${baseUrl}/api/v1/orgs`, jwt);
  const org = orgs[0];
  if (!org?.orgId) throw new Error(`No accessible organization found: ${JSON.stringify(orgs)}`);

  const workspaces = await apiJson(
    request,
    "get",
    `${baseUrl}/api/v1/orgs/${org.orgId}/workspaces`,
    jwt,
  );
  const workspace = workspaces.find((item) => item.isBase || item.name === "Base") || workspaces[0];
  if (!workspace?.id) throw new Error(`No accessible workspace found: ${JSON.stringify(workspaces)}`);

  const forkName = `canary-microvm-${chatMarker.toLowerCase().replace(/[^a-z0-9-]/g, "-")}`;
  const cloned = await apiJson(
    request,
    "post",
    `${baseUrl}/api/v1/workspaces/${workspace.id}/clone`,
    jwt,
    {
      name: forkName,
      idempotencyKey: crypto.randomUUID(),
      source: "durable",
      description: `Disposable Azure microVM canary ${chatMarker}`,
      readOnly: false,
      chatSessionIds: [],
    },
  );
  const fork = cloned.workspace || cloned;
  if (!fork?.id) throw new Error(`Fork did not return a workspace: ${JSON.stringify(cloned)}`);

  let preview = null;
  let previewStop = null;
  try {
    const runtimeConfig = await apiJson(
      request,
      "put",
      `${baseUrl}/api/v1/workspaces/${fork.id}/config?orgId=${encodeURIComponent(org.orgId)}`,
      jwt,
      {
        settings: { ANTHROPIC_BASE_URL: "https://openrouter.ai/api" },
        setSecrets: { ANTHROPIC_AUTH_TOKEN: openrouterKey },
      },
    );
    if (
      runtimeConfig.effectiveSettings?.ANTHROPIC_BASE_URL !== "https://openrouter.ai/api" ||
      !runtimeConfig.effectiveSecretKeys?.includes("ANTHROPIC_AUTH_TOKEN")
    ) {
      throw new Error(
        `Fork Runtime Config did not expose the OpenRouter route and write-only key name: ${JSON.stringify(runtimeConfig)}`,
      );
    }

    await apiJson(
      request,
      "post",
      `${baseUrl}/api/v1/workspaces/${fork.id}/resume`,
      jwt,
      { operationId: crypto.randomUUID() },
    );
    const vm = await pollWorkspaceRunning(request, jwt, fork.id);
    if (!vm.hostId || !vm.vmId || vm.epoch < 1) {
      throw new Error(`Running workspace VM lacks host/incarnation identity: ${JSON.stringify(vm)}`);
    }

    const docName = `canary-workspace-validation-${chatMarker.toLowerCase()}`;
    const doc = await apiJson(
      request,
      "post",
      `${baseUrl}/api/v1/workspaces/${fork.id}/docs`,
      jwt,
      {
        name: docName,
        docType: "design",
        filePath: `docs/${fork.id}/${docName}.md`,
        content: `# Azure microVM canary\n\nHarmless validation marker: ${chatMarker}\n`,
      },
    );
    if (!doc?.id || !doc?.filePath || !doc.repoId) {
      throw new Error(`Project-doc creation did not return repo identifiers: ${JSON.stringify(doc)}`);
    }

    const previewManifestPath = ".g5/workspace.yaml";
    const previewServerPath = "canary-preview-server.js";
    const previewPort = 4173;
    const previewManifest = [
      "preview:",
      `  command: \"node ${previewServerPath}\"`,
      `  port: ${previewPort}`,
      "",
    ].join("\n");
    const previewServer = [
      'const fs = require("fs");',
      'const http = require("http");',
      `const source = fs.readFileSync(${JSON.stringify(doc.filePath)}, "utf8");`,
      'http.createServer((_request, response) => {',
      '  response.writeHead(200, { "content-type": "text/plain; charset=utf-8" });',
      '  response.end(source);',
      `}).listen(${previewPort}, "0.0.0.0");`,
      "",
    ].join("\n");
    for (const [filePath, content] of [
      [previewManifestPath, previewManifest],
      [previewServerPath, previewServer],
    ]) {
      await apiJson(
        request,
        "put",
        `${baseUrl}/api/v1/workspaces/${fork.id}/files/${filePath}?repoId=${encodeURIComponent(doc.repoId)}`,
        jwt,
        { content, encoding: "utf-8" },
      );
    }

    const selectedPaths = [doc.filePath, previewManifestPath, previewServerPath];
    const changesDeadline = Date.now() + 30000;
    let changes = null;
    while (Date.now() < changesDeadline) {
      changes = await apiJson(
        request,
        "get",
        `${baseUrl}/api/v1/workspaces/${fork.id}/changes`,
        jwt,
      );
      const dirtyPaths = (changes.members || []).flatMap((member) =>
        (member.dirty || []).map((entry) => entry.path),
      );
      if (selectedPaths.every((filePath) => dirtyPaths.includes(filePath))) break;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
    const dirtyPaths = (changes?.members || []).flatMap((member) =>
      (member.dirty || []).map((entry) => entry.path),
    );
    if (changes?.source !== "live" || !changes.attached || !selectedPaths.every((p) => dirtyPaths.includes(p))) {
      throw new Error(`Live Changes did not include all saved files: ${JSON.stringify(changes)}`);
    }

    const previewStarted = await apiJson(
      request,
      "post",
      `${baseUrl}/api/v1/workspaces/${fork.id}/preview/`,
      jwt,
      {
        repoId: doc.repoId,
        profile: null,
        idempotencyKey: crypto.randomUUID(),
      },
    );
    if (!previewStarted?.taskId) {
      throw new Error(`Workspace preview did not return a task: ${JSON.stringify(previewStarted)}`);
    }
    preview = await pollWorkspacePreview(request, jwt, fork.id);
    const minted = await apiJson(
      request,
      "post",
      `${baseUrl}/api/v1/preview/session/${preview.taskId}`,
      jwt,
    );
    if (!minted?.previewUrl) {
      throw new Error(`Preview session did not return a URL: ${JSON.stringify(minted)}`);
    }
    const absolutePreviewUrl = new URL(minted.previewUrl, baseUrl).toString();
    const previewPage = await page.context().newPage();
    try {
      const response = await previewPage.goto(absolutePreviewUrl, {
        waitUntil: "domcontentloaded",
        timeout: 120000,
      });
      if (!response?.ok()) {
        throw new Error(`Preview render returned HTTP ${response?.status() ?? "unknown"}`);
      }
      const renderedText = await previewPage.locator("body").innerText({ timeout: 30000 });
      if (!renderedText.includes(chatMarker)) {
        throw new Error(`Preview did not render the live document marker: ${renderedText}`);
      }
      await previewPage.screenshot({
        path: path.join(artifactDir, "external-microvm-preview-success.png"),
        fullPage: true,
      });
    } finally {
      await previewPage.close();
    }
    previewStop = await apiJson(
      request,
      "post",
      `${baseUrl}/api/v1/workspaces/${fork.id}/preview/cancel`,
      jwt,
      {},
    );

    const started = await apiJson(
      request,
      "post",
      `${baseUrl}/api/v1/workspaces/${fork.id}/sessions/start`,
      jwt,
      {
        content: `This is an authorized end-to-end connectivity check. Do not read or modify files and do not invoke tools. Reply with exactly: ${chatMarker} g5-openrouter-microvm-ok`,
        title: "Azure ACR microVM end-to-end canary",
        scopeType: "workspace",
        agentProvider: "claude_code",
      },
    );
    const sessionId = started.session?.id;
    if (!sessionId || !started.taskId) {
      throw new Error(`Chat start did not return session/task identifiers: ${JSON.stringify(started)}`);
    }

    const deadline = Date.now() + 600000;
    let observed = null;
    while (Date.now() < deadline) {
      const messagePage = await apiJson(
        request,
        "get",
        `${baseUrl}/api/v1/sessions/${sessionId}/messages?tail=true&limit=100`,
        jwt,
      );
      const messages = Array.isArray(messagePage) ? messagePage : messagePage.messages || [];
      observed = messages.filter((message) => message.role === "assistant").at(-1) || null;
      if (["failed", "error"].includes(observed?.status)) {
        throw new Error(`Assistant response failed: ${JSON.stringify(observed)}`);
      }
      if (["complete", "completed"].includes(observed?.status)) {
        if (observed.content?.includes(chatMarker) && observed.content?.includes("g5-openrouter-microvm-ok")) {
          break;
        }
        throw new Error(`Assistant response completed without required markers: ${JSON.stringify(observed)}`);
      }
      await new Promise((resolve) => setTimeout(resolve, 3000));
    }
    if (
      !["complete", "completed"].includes(observed?.status) ||
      !observed?.content?.includes(chatMarker) ||
      !observed?.content?.includes("g5-openrouter-microvm-ok")
    ) {
      throw new Error(`Timed out waiting for real microVM response: ${JSON.stringify(observed)}`);
    }

    const evidence = {
      orgId: org.orgId,
      baseWorkspaceId: workspace.id,
      forkWorkspaceId: fork.id,
      forkName,
      runtimeConfigBaseUrl: runtimeConfig.effectiveSettings.ANTHROPIC_BASE_URL,
      runtimeConfigSecretKey: "ANTHROPIC_AUTH_TOKEN",
      hostId: vm.hostId,
      vmId: vm.vmId,
      vmEpoch: vm.epoch,
      docId: doc.id,
      docPath: doc.filePath,
      dirtyPaths,
      previewTaskId: preview.taskId,
      previewStatus: preview.status,
      previewReady: preview.ready,
      previewStopStatus: previewStop?.status,
      sessionId,
      taskId: started.taskId,
      marker: chatMarker,
      observedResponse: observed.content,
      status: observed.status,
    };
    fs.writeFileSync(
      path.join(artifactDir, "external-microvm-workflow-success.json"),
      `${JSON.stringify(evidence, null, 2)}\n`,
    );
    console.log(`External microVM workflow completed: ${JSON.stringify(evidence)}`);
  } finally {
    if (preview && !previewStop) {
      await apiJson(
        request,
        "post",
        `${baseUrl}/api/v1/workspaces/${fork.id}/preview/cancel`,
        jwt,
        {},
      ).catch((error) => console.warn(`Workspace preview cleanup failed: ${error}`));
    }
    await apiJson(
      request,
      "post",
      `${baseUrl}/api/v1/workspaces/${fork.id}/stop`,
      jwt,
      { operationId: crypto.randomUUID() },
    ).catch((error) => console.warn(`Workspace VM cleanup failed: ${error}`));
  }
}

async function provisionWorkspaceHost(request, jwt) {
  if (!provisionHostBaseImage) {
    throw new Error("G5_PROVISION_HOST_BASE_IMAGE is required when provisioning a host");
  }
  const orgs = await apiJson(request, "get", `${baseUrl}/api/v1/orgs`, jwt);
  const org = orgs[0];
  if (!org?.orgId) throw new Error(`No accessible organization found: ${JSON.stringify(orgs)}`);
  const host = await apiJson(
    request,
    "post",
    `${baseUrl}/api/v1/orgs/${org.orgId}/worker-hosts`,
    jwt,
    {
      region: "us-central1",
      instanceType: "c3-standard-4",
      baseImageRef: provisionHostBaseImage,
      workspaceVmLimit: 1,
    },
  );
  if (!host?.hostId || !host?.hostJwt) {
    throw new Error(`Host provisioning omitted its bound credential: ${JSON.stringify(host)}`);
  }
  fs.writeFileSync(
    provisionHostOutput,
    `${JSON.stringify({ orgId: org.orgId, hostId: host.hostId, hostJwt: host.hostJwt })}\n`,
    { mode: 0o600 },
  );
}

async function saveDiagnosticScreenshot(page, name) {
  const screenshotPath = path.join(artifactDir, name);
  try {
    await page.screenshot({ path: screenshotPath, fullPage: true });
  } catch (error) {
    console.warn(
      `Failed to save diagnostic screenshot ${screenshotPath}: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  }
}

/**
 * Drives one provider login end to end: sign in through `login`, prove the
 * session with /auth/refresh and /users/me, then either provision a worker
 * host (G5_PROVISION_HOST_OUTPUT) or run the real microVM chat turn
 * (G5_VERIFY_CHAT=true).
 */
async function runLoginSmoke({ providerLabel, email, password, login, screenshotPrefix }) {
  if (!baseUrl || !email || !password) {
    throw new Error(`G5_BASE_URL and the ${providerLabel} test credentials are required`);
  }

  const { chromium } = require("playwright");
  const browser = await chromium.launch();
  const context = await browser.newContext({
    ignoreHTTPSErrors: false,
    recordVideo: { dir: artifactDir },
  });
  const page = await context.newPage();

  try {
    await page.goto(`${baseUrl}/login`, { waitUntil: "domcontentloaded" });

    await login(page, new URL(baseUrl).origin);
    await page.waitForLoadState("networkidle", { timeout: 90000 }).catch(() => {});

    const refresh = await context.request.post(`${baseUrl}/api/v1/auth/refresh`);
    if (!refresh.ok()) {
      throw new Error(
        `POST /api/v1/auth/refresh failed at ${page.url()} with HTTP ${refresh.status()}: ${await refresh.text()}`,
      );
    }
    const refreshBody = await refresh.json();
    const jwt = refreshBody.data?.jwt || refreshBody.jwt;
    if (!jwt) {
      throw new Error(
        `POST /api/v1/auth/refresh did not return a JWT: ${JSON.stringify(refreshBody)}`,
      );
    }

    const me = await context.request.get(`${baseUrl}/api/v1/users/me`, {
      headers: {
        Authorization: `Bearer ${jwt}`,
      },
    });
    if (!me.ok()) {
      throw new Error(`GET /api/v1/users/me failed with HTTP ${me.status()}: ${await me.text()}`);
    }
    const body = await me.json();
    const user = body.data || body;
    if (String(user.email || "").toLowerCase() !== email.toLowerCase()) {
      throw new Error(
        `logged-in user did not look like expected ${providerLabel} user: ${JSON.stringify(user)}`,
      );
    }

    if (provisionHostOutput) {
      await provisionWorkspaceHost(context.request, jwt);
    } else if (verifyChat) {
      await verifyExternalMicrovmWorkflow(page, context.request, jwt);
    }

    await page.screenshot({ path: path.join(artifactDir, `${screenshotPrefix}-success.png`), fullPage: true });
    await browser.close();
    process.exit(0);
  } catch (error) {
    await page.screenshot({ path: path.join(artifactDir, `${screenshotPrefix}-failure.png`), fullPage: true }).catch(() => {});
    await browser.close();
    throw error;
  }
}

function runTotpSelfTest() {
  const rfcSecret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
  const expected = [
    [59000, "94287082"],
    [1111111109000, "07081804"],
    [1111111111000, "14050471"],
    [1234567890000, "89005924"],
    [2000000000000, "69279037"],
    [20000000000000, "65353130"],
  ];
  for (const [timestampMs, code] of expected) {
    const actual = generateTotp(rfcSecret, timestampMs, 8);
    if (actual !== code) {
      throw new Error(`TOTP self-test failed for ${timestampMs}: expected ${code}, got ${actual}`);
    }
  }
}

module.exports = {
  clickFirstVisible,
  decodeBase32,
  generateTotp,
  hasReturnedToApp,
  runLoginSmoke,
  runTotpSelfTest,
  saveDiagnosticScreenshot,
};
