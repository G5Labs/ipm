#!/usr/bin/env node
// =============================================================================
// Local OAuth Stub Server
// =============================================================================
// Lightweight OAuth 2.0 Authorization Code flow server that mimics GitHub's
// API surface. Used for E2E testing and local demo stacks so the full OAuth
// pipeline is exercised without hitting real providers.
//
// Endpoints:
//   GET  /login/oauth/authorize  — authorization page (auto-submits in test mode;
//                                  otherwise it offers each configured identity
//                                  in one click, or Email/Username fields: a
//                                  configured email keeps its configured id,
//                                  any other email gets a stable id derived
//                                  from it)
//   POST /login/oauth/access_token — token exchange
//   GET  /user                    — GitHub-format user info
//   GET  /user/emails             — GitHub-format emails list
//   GET  /__e2e/users             — configured roster (default + extra users)
//   POST /emails                  — Resend-compatible send (outbound email in
//                                  dev/CI): records the message, replays the
//                                  same id for a repeated Idempotency-Key and
//                                  refuses a reused key with another payload
//   GET  /__e2e/emails            — recorded messages, newest last (`?to=`
//                                  filters by recipient)
//   DELETE /__e2e/emails          — forget recorded messages
//   GET  /healthz                 — health check
//
// Environment:
//   OAUTH_PORT           — listen port (default: 9876)
//   OAUTH_AUTO_APPROVE   — skip consent screen, redirect immediately (default: true)
//   OAUTH_USER_ID        — numeric user id (default: 100001)
//   OAUTH_USER_LOGIN     — GitHub-style username (default: "e2e-test-user")
//   OAUTH_USER_EMAIL     — user email (default: "e2e-test@g5.dev")
//   OAUTH_USER_NAME      — display name (default: "E2E Test User")
//   OAUTH_USER_AVATAR    — avatar URL (default: "https://i.pravatar.cc/150?u=e2e")
//   OAUTH_USER_ROLE      — optional G5 seed role (admin|member|viewer),
//                          exposed only in /__e2e/users; seed defaults to Admin
//   OAUTH_EXTRA_USERS    — JSON array of additional {id, login, email, name,
//                          avatar_url, role} identities offered on the consent
//                          page (default: none; each needs a unique id and
//                          email). Optional `role` (admin|member|viewer)
//                          is G5 seed metadata for scripts/seed-dev-org.sh,
//                          never part of the GitHub-format user payload.
//   OAUTH_CLIENT_ID      — expected client_id (default: "local-oauth-client-id")
//   OAUTH_CLIENT_SECRET  — expected client_secret (default: "local-oauth-client-secret")
//   EMAIL_STUB_API_KEY   — bearer key POST /emails accepts (default: "re_stub_key")
// =============================================================================

const http = require('http');
const url = require('url');
const crypto = require('crypto');

const PORT = parseInt(process.env.OAUTH_PORT || '9876', 10);
const AUTO_APPROVE = (process.env.OAUTH_AUTO_APPROVE || 'true') !== 'false';
const CLIENT_ID = process.env.OAUTH_CLIENT_ID || 'local-oauth-client-id';
const CLIENT_SECRET = process.env.OAUTH_CLIENT_SECRET || 'local-oauth-client-secret';

// ---- Fine-grained PAT validation (SCM connect) ----------------------------
// The g5-api SCM connect flow validates a fine-grained PAT by calling
// `GET {G5_SCM_GITHUB_API_BASE_URL}/user`. In dev/CI that base URL points at
// this stub so the connect e2e is hermetic (no real github.com). The stub
// accepts a configured "valid" PAT (returns an identity + the
// `github-authentication-token-expiration` header) and a configured "expired"
// PAT (returns 401); any other unknown token is 401.
const STUB_PAT_VALID = process.env.OAUTH_STUB_PAT_VALID || 'github_pat_e2e_valid';
const STUB_PAT_EXPIRED =
  process.env.OAUTH_STUB_PAT_EXPIRED || 'github_pat_e2e_expired';
const STUB_PAT_IDENTITY = {
  id: Number.parseInt(process.env.OAUTH_STUB_PAT_USER_ID || '424242', 10),
  login: process.env.OAUTH_STUB_PAT_LOGIN || 'octo-dev',
  name: process.env.OAUTH_STUB_PAT_NAME || 'Octo Dev',
  email: process.env.OAUTH_STUB_PAT_EMAIL || 'octo-dev@example.com',
};
const STUB_PAT_EXPIRY = process.env.OAUTH_STUB_PAT_EXPIRY || '2099-12-31 23:59:59 UTC';

function normalizeUser(user) {
  const parsedId = Number.parseInt(String(user?.id ?? '100001'), 10);
  return {
    id: Number.isFinite(parsedId) ? parsedId : 100001,
    login: user?.login || 'e2e-test-user',
    email: user?.email || 'e2e-test@g5.dev',
    name: user?.name || 'E2E Test User',
    avatar_url: user?.avatar_url || 'https://i.pravatar.cc/150?u=e2e',
  };
}

const DEFAULT_USER = normalizeUser({
  id: process.env.OAUTH_USER_ID || '100001',
  login: process.env.OAUTH_USER_LOGIN,
  email: process.env.OAUTH_USER_EMAIL,
  name: process.env.OAUTH_USER_NAME,
  avatar_url: process.env.OAUTH_USER_AVATAR,
});

const SEED_ROLES = ['admin', 'member', 'viewer'];
const DEFAULT_USER_ROLE = process.env.OAUTH_USER_ROLE;
if (DEFAULT_USER_ROLE !== undefined && !SEED_ROLES.includes(DEFAULT_USER_ROLE)) {
  throw new Error(`OAUTH_USER_ROLE must be one of ${SEED_ROLES.join('|')}`);
}

// Extra identities are only selectable on the manual consent page; the
// default user stays the auto-approve and `/__e2e/identity/reset` identity.
// A malformed roster fails startup instead of silently hiding users, and
// distinct ids keep the API from linking two people to one account.
function parseExtraUsers(raw) {
  if (!raw || !raw.trim()) return [];
  const parsed = JSON.parse(raw);
  if (!Array.isArray(parsed)) {
    throw new Error('OAUTH_EXTRA_USERS must be a JSON array');
  }
  const users = parsed.map((entry) => {
    if (!entry?.email || entry.id === undefined) {
      throw new Error(`OAUTH_EXTRA_USERS entry needs id and email: ${JSON.stringify(entry)}`);
    }
    if (entry.role !== undefined && !SEED_ROLES.includes(entry.role)) {
      throw new Error(`OAUTH_EXTRA_USERS role must be one of ${SEED_ROLES.join('|')}: ${entry.email}`);
    }
    return entry.role === undefined
      ? normalizeUser(entry)
      : { ...normalizeUser(entry), role: entry.role };
  });
  const seenIds = new Set([DEFAULT_USER.id]);
  const seenEmails = new Set([DEFAULT_USER.email.toLowerCase()]);
  for (const user of users) {
    if (seenIds.has(user.id) || seenEmails.has(user.email.toLowerCase())) {
      throw new Error(`OAUTH_EXTRA_USERS duplicates an id or email: ${user.email}`);
    }
    seenIds.add(user.id);
    seenEmails.add(user.email.toLowerCase());
  }
  return users;
}

const ROSTER = [
  DEFAULT_USER_ROLE === undefined ? DEFAULT_USER : { ...DEFAULT_USER, role: DEFAULT_USER_ROLE },
  ...parseExtraUsers(process.env.OAUTH_EXTRA_USERS),
];

const E2E_IDENTITY_COOKIE = 'g5_e2e_oauth_identity';
let activeUser = { ...DEFAULT_USER };

// In-memory stores:
// - code → { redirect_uri, client_id, code_challenge, created_at, user }
// - access_token → { created_at, user }
const codes = new Map();
const tokens = new Map();

// Clean up expired auth codes and stub access tokens.
setInterval(() => {
  const now = Date.now();
  for (const [code, meta] of codes) {
    if (now - meta.created_at > 5 * 60 * 1000) codes.delete(code);
  }
  for (const [token, meta] of tokens) {
    if (now - meta.created_at > 60 * 60 * 1000) tokens.delete(token);
  }
}, 60_000);

// Consent choices: the request's identity first, then the roster,
// deduplicated by email.
function consentChoices(user) {
  const choices = [user];
  for (const candidate of ROSTER) {
    if (!choices.some((c) => c.email.toLowerCase() === candidate.email.toLowerCase())) {
      choices.push(candidate);
    }
  }
  return choices;
}

function json(res, status, data) {
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify(data));
}

function bearerToken(req) {
  const authHeader = req.headers.authorization;
  if (!authHeader || !authHeader.startsWith('Bearer ')) {
    return null;
  }
  return authHeader.slice('Bearer '.length).trim();
}

function parseBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const body = Buffer.concat(chunks).toString();
      // Support both JSON and form-urlencoded
      if (req.headers['content-type']?.includes('json')) {
        try { resolve(JSON.parse(body)); } catch { resolve({}); }
      } else {
        resolve(Object.fromEntries(new URLSearchParams(body)));
      }
    });
    req.on('error', reject);
  });
}

// A repeated query parameter parses as an array; the stub uses the first.
function firstParam(value) {
  return Array.isArray(value) ? value[0] : value;
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (ch) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  })[ch]);
}

function parseCookies(req) {
  const header = req.headers.cookie || '';
  const cookies = {};
  for (const part of header.split(';')) {
    const trimmed = part.trim();
    if (!trimmed) continue;
    const index = trimmed.indexOf('=');
    if (index < 0) continue;
    const name = decodeURIComponent(trimmed.slice(0, index));
    const value = trimmed.slice(index + 1);
    cookies[name] = value;
  }
  return cookies;
}

function requestUser(req) {
  const cookieValue = parseCookies(req)[E2E_IDENTITY_COOKIE];
  if (!cookieValue) {
    return activeUser;
  }

  try {
    const decoded = Buffer.from(cookieValue, 'base64url').toString('utf8');
    return normalizeUser(JSON.parse(decoded));
  } catch (error) {
    console.warn(`[oauth-stub] Ignoring malformed ${E2E_IDENTITY_COOKIE} cookie: ${error}`);
    return activeUser;
  }
}

// ---- Resend-compatible outbound email (dev/CI) ------------------------------
// g5-api's `resend` email transport points G5_EMAIL_RESEND_API_URL here so
// invitation emails are hermetic and inspectable (backend email module,
// .g5/ontology/software_architecture/email.g5.md).
const EMAIL_STUB_API_KEY = process.env.EMAIL_STUB_API_KEY || 're_stub_key';
const EMAIL_LIMIT = 500;
const emails = [];
const emailsByKey = new Map();

function readRawBody(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => resolve(Buffer.concat(chunks).toString()));
    req.on('error', reject);
  });
}

function resendError(res, status, name, message) {
  return json(res, status, { statusCode: status, name, message });
}

async function handleSendEmail(req, res) {
  if (bearerToken(req) !== EMAIL_STUB_API_KEY) {
    return resendError(res, 401, 'missing_api_key', 'Missing or unknown API key');
  }
  const raw = await readRawBody(req);
  let body;
  try {
    body = JSON.parse(raw);
  } catch {
    return resendError(res, 422, 'validation_error', 'Body must be JSON');
  }
  if (!body || typeof body !== 'object') {
    return resendError(res, 422, 'validation_error', 'Body must be a JSON object');
  }
  const to = (Array.isArray(body.to) ? body.to : [body.to]).filter(
    (address) => typeof address === 'string' && address,
  );
  if (!body.from || to.length === 0 || !body.subject) {
    return resendError(res, 422, 'missing_required_field', 'from, to and subject are required');
  }
  const key = req.headers['idempotency-key'];
  if (key) {
    const previous = emailsByKey.get(key);
    if (previous && previous.raw !== raw) {
      return resendError(res, 409, 'invalid_idempotent_request',
        'This idempotency key has already been used with a different payload');
    }
    if (previous) {
      return json(res, 200, { id: previous.message.id });
    }
  }
  const message = {
    id: crypto.randomUUID(),
    idempotencyKey: key || null,
    from: body.from,
    to,
    subject: body.subject,
    text: body.text || '',
    html: body.html || '',
    createdAt: new Date().toISOString(),
  };
  emails.push(message);
  if (key) emailsByKey.set(key, { raw, message });
  while (emails.length > EMAIL_LIMIT) {
    const dropped = emails.shift();
    if (dropped.idempotencyKey) emailsByKey.delete(dropped.idempotencyKey);
  }
  console.log(`[oauth-stub] Recorded email ${message.id} to ${to.join(', ')}: ${message.subject}`);
  return json(res, 200, { id: message.id });
}

const server = http.createServer(async (req, res) => {
  const parsed = url.parse(req.url, true);
  const path = parsed.pathname;
  const method = req.method;

  // ---- Health check ----
  if (path === '/healthz') {
    return json(res, 200, { status: 'ok' });
  }

  // ---- E2E identity controls ----
  if (path === '/__e2e/identity' && method === 'GET') {
    return json(res, 200, { user: activeUser });
  }

  if (path === '/__e2e/identity' && method === 'POST') {
    const body = await parseBody(req);
    activeUser = normalizeUser({
      id: body.id,
      login: body.login,
      email: body.email,
      name: body.name,
      avatar_url: body.avatar_url,
    });
    console.log(`[oauth-stub] Active user switched to ${activeUser.login} <${activeUser.email}>`);
    return json(res, 200, { user: activeUser });
  }

  if (path === '/__e2e/users' && method === 'GET') {
    return json(res, 200, { users: ROSTER });
  }

  // ---- Outbound email ----
  if (path === '/emails' && method === 'POST') {
    return handleSendEmail(req, res);
  }

  if (path === '/__e2e/emails' && method === 'GET') {
    const to = firstParam(parsed.query.to);
    const matching = to
      ? emails.filter((message) => message.to.some((address) => address.toLowerCase() === String(to).toLowerCase()))
      : emails;
    return json(res, 200, { emails: matching });
  }

  if (path === '/__e2e/emails' && method === 'DELETE') {
    emails.length = 0;
    emailsByKey.clear();
    return json(res, 200, { emails: [] });
  }

  if (path === '/__e2e/identity/reset' && method === 'POST') {
    activeUser = { ...DEFAULT_USER };
    console.log(`[oauth-stub] Active user reset to ${activeUser.login} <${activeUser.email}>`);
    return json(res, 200, { user: activeUser });
  }

  // ---- Authorization endpoint ----
  if (path === '/login/oauth/authorize' && method === 'GET') {
    const { client_id, redirect_uri, state, scope, code_challenge, code_challenge_method } =
      parsed.query;

    if (client_id !== CLIENT_ID) {
      return json(res, 400, { error: 'invalid_client_id' });
    }
    // PKCE, as GitHub and GitLab apply it: a code issued with an S256
    // challenge is exchanged only with the matching verifier.
    if (code_challenge && code_challenge_method !== 'S256') {
      return json(res, 400, { error: 'invalid_request', error_description: 'S256 required' });
    }

    const user = requestUser(req);
    const choices = consentChoices(user);

    // Generate authorization code
    const code = crypto.randomBytes(20).toString('hex');
    codes.set(code, {
      redirect_uri,
      client_id,
      code_challenge: code_challenge || null,
      created_at: Date.now(),
      user: { ...user },
      choices,
    });

    if (AUTO_APPROVE) {
      // Auto-approve: redirect immediately with code
      const sep = redirect_uri.includes('?') ? '&' : '?';
      const location = `${redirect_uri}${sep}code=${code}&state=${state || ''}`;
      res.writeHead(302, { Location: location });
      return res.end();
    }

    // Manual approve: render a consent page offering each configured identity
    // in one click, plus Email/Username fields for any other identity.
    res.writeHead(200, { 'Content-Type': 'text/html' });

    const hidden = `
    <input type="hidden" name="code" value="${escapeHtml(code)}" />
    <input type="hidden" name="state" value="${escapeHtml(state || '')}" />`;
    const quickPicks = choices.map((choice) => `
  <form method="GET" action="/login/oauth/authorize/callback" style="margin: 8px 0;">${hidden}
    <input type="hidden" name="email" value="${escapeHtml(choice.email)}" />
    <button type="submit" style="padding: 6px 16px;">${escapeHtml(choice.name)} &lt;${escapeHtml(choice.email)}&gt;</button>
  </form>`).join('');

    return res.end(`<!DOCTYPE html>
<html>
<head><title>OAuth Stub — Authorize</title></head>
<body style="font-family: sans-serif; max-width: 400px; margin: 60px auto;">
  <h2>Authorize Application</h2>
  <p><strong>${escapeHtml(client_id)}</strong> wants to access your account.</p>
  <p>Scopes: <code>${escapeHtml(scope || 'none')}</code></p>
  <h3>Sign in as</h3>${quickPicks}
  <h3>Or another identity</h3>
  <form method="GET" action="/login/oauth/authorize/callback">${hidden}
    <label>Email: <input name="email" value="${escapeHtml(user.email)}" /></label><br/><br/>
    <label>Username: <input name="login" value="${escapeHtml(user.login)}" /></label><br/><br/>
    <button type="submit" style="padding: 8px 24px; font-size: 16px;">Authorize</button>
  </form>
</body>
</html>`);
  }

  // ---- Manual authorize callback (from consent page form) ----
  if (path === '/login/oauth/authorize/callback' && method === 'GET') {
    const code = firstParam(parsed.query.code);
    const state = firstParam(parsed.query.state);
    const email = firstParam(parsed.query.email);
    const login = firstParam(parsed.query.login);
    const codeMeta = codes.get(code);
    if (!codeMeta) {
      return json(res, 400, { error: 'invalid_code' });
    }
    const current = codeMeta.user;
    const nextEmail = (email || '').trim() || current.email;
    const typedLogin = (login || '').trim();
    const loginChanged = Boolean(typedLogin) && typedLogin !== current.login;
    // A configured identity (the request's own or a roster persona) keeps its
    // stable id and profile; a typed username only overrides its login.
    const configured = codeMeta.choices.find(
      (choice) => choice.email.toLowerCase() === nextEmail.toLowerCase(),
    );
    if (configured) {
      codeMeta.user = loginChanged ? { ...configured, login: typedLogin } : { ...configured };
    } else {
      // Honor any other identity typed on the consent page. A different email
      // is a different GitHub account, so it gets its own stable id: G5 keys
      // users by (provider, oauth id), and reusing the default id would sign
      // in as the default user regardless of email.
      const digest = crypto.createHash('sha256').update(nextEmail.toLowerCase()).digest();
      const nextLogin = loginChanged ? typedLogin : nextEmail.split('@')[0];
      codeMeta.user = normalizeUser({
        ...current,
        id: 200000 + (digest.readUInt32BE(0) % 100000000),
        email: nextEmail,
        login: nextLogin,
        name: nextLogin,
      });
    }
    // Redirect only to the redirect_uri bound to this code at authorize time.
    const redirectUri = codeMeta.redirect_uri;
    const sep = redirectUri.includes('?') ? '&' : '?';
    const location = `${redirectUri}${sep}code=${encodeURIComponent(code)}&state=${encodeURIComponent(state || '')}`;
    res.writeHead(302, { Location: location });
    return res.end();
  }

  // ---- Token exchange endpoint ----
  if (path === '/login/oauth/access_token' && method === 'POST') {
    const body = await parseBody(req);
    const { client_id, client_secret, code, grant_type, code_verifier } = body;

    // Validate client credentials
    if (client_id !== CLIENT_ID || client_secret !== CLIENT_SECRET) {
      return json(res, 401, { error: 'invalid_client' });
    }

    // Validate code
    const codeMeta = codes.get(code);
    if (!codeMeta) {
      return json(res, 400, { error: 'invalid_code', error_description: 'Code not found or expired' });
    }
    codes.delete(code);
    if (codeMeta.code_challenge) {
      const challenge = code_verifier
        ? crypto.createHash('sha256').update(code_verifier).digest('base64url')
        : null;
      if (challenge !== codeMeta.code_challenge) {
        console.log('[oauth-stub] Token exchange refused: PKCE verifier does not match');
        return json(res, 400, { error: 'invalid_grant', error_description: 'PKCE verification failed' });
      }
    }

    // Mint a simple bearer token (random hex; not a real JWT)
    const access_token = crypto.randomBytes(32).toString('hex');
    tokens.set(access_token, {
      created_at: Date.now(),
      user: { ...codeMeta.user },
    });

    console.log(`[oauth-stub] Token exchanged for code=${code.substring(0, 8)}... → token=${access_token.substring(0, 8)}...`);

    return json(res, 200, {
      access_token,
      token_type: 'bearer',
      scope: 'user:email read:user',
    });
  }

  // ---- User info endpoint (GitHub format) ----
  if (path === '/user' && method === 'GET') {
    const token = bearerToken(req);

    // Fine-grained PAT validation path (SCM connect). Checked before the
    // OAuth-issued token map so a configured test PAT validates hermetically.
    if (token === STUB_PAT_VALID) {
      res.setHeader('github-authentication-token-expiration', STUB_PAT_EXPIRY);
      console.log('[oauth-stub] PAT validation: accepted valid test PAT');
      return json(res, 200, { ...STUB_PAT_IDENTITY });
    }
    if (token === STUB_PAT_EXPIRED) {
      console.log('[oauth-stub] PAT validation: rejected expired test PAT');
      return json(res, 401, { message: 'Bad credentials (token expired)' });
    }

    const tokenMeta = token ? tokens.get(token) : null;
    if (!tokenMeta) {
      return json(res, 401, { message: 'Requires authentication' });
    }

    console.log(`[oauth-stub] User info requested`);

    const user = tokenMeta.user;
    return json(res, 200, {
      id: user.id,
      login: user.login,
      name: user.name,
      email: user.email,
      avatar_url: user.avatar_url,
    });
  }

  // ---- User emails endpoint (GitHub format) ----
  if (path === '/user/emails' && method === 'GET') {
    const token = bearerToken(req);
    const tokenMeta = token ? tokens.get(token) : null;
    if (!tokenMeta) {
      return json(res, 401, { message: 'Requires authentication' });
    }

    const user = tokenMeta.user;
    return json(res, 200, [
      { email: user.email, primary: true, verified: true, visibility: 'public' },
    ]);
  }

  // ---- 404 ----
  json(res, 404, { error: 'not_found', path });
});

server.listen(PORT, '0.0.0.0', () => {
  console.log(`[oauth-stub] Local OAuth server listening on :${PORT}`);
  console.log(`[oauth-stub] Auto-approve: ${AUTO_APPROVE}`);
  console.log(`[oauth-stub] Default user: ${DEFAULT_USER.login} <${DEFAULT_USER.email}>`);
  console.log(`[oauth-stub] Client ID: ${CLIENT_ID}`);
});
