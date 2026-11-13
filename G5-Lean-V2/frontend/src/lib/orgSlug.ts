// =============================================================================
// G5 Client — Organization slug derivation
// =============================================================================
// The slug the operator console's create form suggests from an organization
// name (g5.organization.platform-operator.create-org). The API validates the
// slug it receives, including reserved slugs; this only proposes one.
// =============================================================================

/** Lowercase, hyphen-separated slug for an organization name. */
export function orgSlugFromName(name: string): string {
  return name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9\s-]/g, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}
