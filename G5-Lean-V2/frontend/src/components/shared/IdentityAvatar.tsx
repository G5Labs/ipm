// =============================================================================
// IdentityAvatar — small round avatar for a persisted actor identity
// =============================================================================
// Records store WHO did something as a bare string (an email, a user id, or a
// service actor like `system@g5.dev`). Surfaces that display provenance —
// intent traceability panels, answered question cards — need a consistent
// visual for that actor without every call site re-implementing lookup and
// fallback.
//
// Resolution order:
//   1. an explicit `avatarUrl` supplied by the caller (the record carried one),
//   2. an org member whose email matches the identity string,
//   3. a deterministic initial + tint derived from the identity string.
//
// The fallback is deterministic so the same actor keeps the same colour across
// renders and surfaces, which is what makes it readable as an identity at all.
// =============================================================================

import type { OrgMemberDTO } from "../../lib/types";

/** Design-token tints reused for the initial fallback. */
const TINTS = [
  "var(--g5-info)",
  "var(--g5-ok)",
  "var(--g5-warn)",
  "var(--g5-active)",
  "var(--g5-blocked)",
] as const;

/** Stable index from the identity string so an actor keeps one colour. */
function tintFor(identity: string): string {
  let hash = 0;
  for (let i = 0; i < identity.length; i++) {
    hash = (hash * 31 + identity.charCodeAt(i)) >>> 0;
  }
  return TINTS[hash % TINTS.length];
}

/** First alphanumeric character, uppercased; "?" when there is none. */
function initialFor(identity: string): string {
  const match = identity.match(/[a-z0-9]/i);
  return match ? match[0].toUpperCase() : "?";
}

export interface IdentityAvatarProps {
  /** The persisted actor string — an email, a user id, or a service actor. */
  identity: string;
  /** Avatar the record already carries; skips member lookup when present. */
  avatarUrl?: string | null;
  /** Optional roster used to resolve an email to a real avatar. */
  members?: OrgMemberDTO[];
  /** Square size in px. */
  size?: number;
  /** Hover label; the fallback initial otherwise names the raw identity. */
  title?: string;
  className?: string;
}

/**
 * Round avatar for a persisted actor identity, with a deterministic
 * initial-and-tint fallback when no image can be resolved.
 */
export function IdentityAvatar({
  identity,
  avatarUrl,
  members,
  size = 16,
  title,
  className,
}: IdentityAvatarProps) {
  const resolved =
    avatarUrl ??
    members?.find(
      (member) => member.email.toLowerCase() === identity.toLowerCase(),
    )?.avatarUrl ??
    null;

  const dimension = { width: size, height: size };

  if (resolved) {
    return (
      <img
        src={resolved}
        alt=""
        aria-hidden="true"
        data-testid="identity-avatar"
        title={title}
        // The avatar is decorative; a native image drag would otherwise
        // start on press and cancel gestures the avatar sits inside, such as
        // reordering a workspace tab by its owner avatar.
        draggable={false}
        style={dimension}
        className={classNamesLocal(
          "shrink-0 rounded-full object-cover",
          className,
        )}
      />
    );
  }

  return (
    <span
      aria-hidden="true"
      data-testid="identity-avatar"
      title={title ?? identity}
      style={{
        ...dimension,
        backgroundColor: `color-mix(in srgb, ${tintFor(identity)} 22%, transparent)`,
        color: tintFor(identity),
        fontSize: Math.max(8, Math.round(size * 0.55)),
      }}
      className={classNamesLocal(
        "inline-flex shrink-0 select-none items-center justify-center rounded-full font-medium leading-none",
        className,
      )}
    >
      {initialFor(identity)}
    </span>
  );
}

/** Local join to keep this leaf importable from anywhere without a cycle. */
function classNamesLocal(...parts: Array<string | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
