// =============================================================================
// providerAccountSecret — shape of a credential pasted into a provider account
//
// Mirrors `ProviderAccountKind::validate_secret` in g5-contracts so the
// Accounts form refuses an obviously wrong paste before the request: a
// trimmed single token (no spaces or line breaks) matching the kind's
// published shape. The server rule is authoritative; this only moves the
// message to the moment of typing. Ontology:
// chat.g5.md#g5.chat.account-credential-shape.
//
// Shapes follow what secret scanners agree on (gitleaks, TruffleHog, the
// Anthropic-supplied GitLab rule): `sk-ant-api03-`/`sk-ant-oat01-` + 95
// base64url chars, `sk-or-v1-` + 64 hex chars, `sk-`/`sk-proj-`/`sk-svcacct-`
// for OpenAI. Only what a wrong paste would violate is pinned: the prefix
// family with free version digits, the character class, and a body floor
// (20) far below the real lengths.
// =============================================================================

import type { ProviderAccountKind } from "./types";

/**
 * Published shape of each kind's credential; kinds without one are unchecked.
 * An Anthropic API key is any `sk-ant-<family>-` other than the OAuth access
 * (`oat`) and refresh (`ort`) tokens and the Admin key (`admin`), which cannot
 * call Messages.
 */
export const SECRET_SHAPE: Partial<Record<ProviderAccountKind, RegExp>> = {
  claude_subscription: /^sk-ant-oat\d+-[A-Za-z0-9_-]{20,}$/,
  anthropic_api_key:
    /^sk-ant-(?!(?:oat|ort|admin)\d*-)[a-z]+\d*-[A-Za-z0-9_-]{20,}$/,
  openrouter_api_key: /^sk-or-v\d+-[0-9a-fA-F]{20,}$/,
  openai_api_key: /^sk-[A-Za-z0-9_-]{20,}$/,
};

/** The provider's own host per API-key kind (`ProviderAccountKind::secret_shape_applies`). */
const NATIVE_HOST: Partial<Record<ProviderAccountKind, string>> = {
  anthropic_api_key: "api.anthropic.com",
  openai_api_key: "api.openai.com",
};

const SHAPE_HELP: Partial<Record<ProviderAccountKind, string>> = {
  claude_subscription:
    "A Claude subscription account takes the OAuth token printed by `claude setup-token`, which starts with sk-ant-oat01-. An Anthropic API key (sk-ant-api03-…) belongs in an Anthropic API key account.",
  anthropic_api_key:
    "An Anthropic API key starts with sk-ant-api03-. A `claude setup-token` value (sk-ant-oat01-…) belongs in a Claude subscription account.",
  openrouter_api_key:
    "An OpenRouter API key starts with sk-or-v1- followed by the key's hex digits.",
  openai_api_key:
    "An OpenAI API key starts with sk- (sk-proj-, sk-svcacct-, …).",
};

export const SECRET_WHITESPACE_MESSAGE =
  "The credential contains a space or line break. Paste it as one line.";

/**
 * The value the form submits: surrounding whitespace never counts. Trims
 * U+0085 as well, which Rust's `str::trim` strips and JavaScript's does not.
 */
export function normalizeSecret(raw: string): string {
  return raw.replace(/^[\s\u0085]+|[\s\u0085]+$/g, "");
}

/**
 * Whether the kind's shape applies at `baseUrl`: always for a kind with a
 * fixed host, and for an API-key kind only on the provider's own host. A
 * compatible gateway behind a custom base URL issues keys of its own shape.
 */
export function secretShapeApplies(
  kind: ProviderAccountKind,
  baseUrl: string | null | undefined,
): boolean {
  if (!SECRET_SHAPE[kind]) return false;
  const native = NATIVE_HOST[kind];
  if (!native) return true;
  const raw = (baseUrl ?? "").trim();
  if (raw.length === 0) return true;
  // Same scheme and host as the provider's own, on the default port only,
  // so an explicit `:443` still counts and a look-alike host does not.
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return false;
  }
  return (
    url.protocol === "https:" &&
    url.hostname.toLowerCase() === native &&
    (url.port === "" || url.port === "443")
  );
}

/**
 * Why a pasted credential cannot be saved for `kind`, or `null` when its
 * shape is acceptable. A blank value is not checked here: blank means "keep
 * the stored credential" on edit and "missing" on create, both handled by
 * the caller. `baseUrl` is the account's effective base URL (`null` for a
 * fixed-host kind).
 */
export function secretShapeError(
  kind: ProviderAccountKind,
  raw: string,
  baseUrl: string | null | undefined = null,
): string | null {
  const secret = normalizeSecret(raw);
  if (secret.length === 0) return null;
  // `\s` plus U+0085, which Rust's `char::is_whitespace` also rejects.
  if (/[\s\u0085]/.test(secret)) return SECRET_WHITESPACE_MESSAGE;
  const shape = SECRET_SHAPE[kind];
  if (shape && secretShapeApplies(kind, baseUrl) && !shape.test(secret)) {
    return (
      SHAPE_HELP[kind] ??
      "The credential does not look like one of this account type."
    );
  }
  return null;
}
