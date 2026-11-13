// =============================================================================
// G5 Client — Platform operator console
// =============================================================================
// The operator organization's Home (g5.organization.platform-operator.home):
// every tenant organization's platform facts and, for operator members and
// above, creating an organization for a named Admin
// (g5.organization.platform-operator.create-org). Operators are not tenant
// members, so organizations are listed without links into them
// (g5.organization.platform-operator.tenant-boundary). The API enforces
// operator authority; the role here only hides what it would refuse.
// =============================================================================

import { AlertCircle, Loader2 } from "lucide-react";
import React, { useState } from "react";
import { Link } from "react-router-dom";

import { Button } from "../ui/UI";
import { HomeSectionHeader } from "../home/HomeTileParts";
import { useCreatePlatformOrg, usePlatformOrgs } from "../../hooks/usePlatform";
import { ApiError } from "../../lib/api";
import { formatDateTime } from "../../lib/dateTime";
import { orgSlugFromName } from "../../lib/orgSlug";
import type { CreatePlatformOrgResponse } from "../../lib/types";

const INPUT_CLASSES =
  "w-full px-3 py-2 bg-[var(--g5-bg0)] text-[var(--g5-text0)] border border-[var(--g5-stroke)] rounded-[var(--g5-radius-sm)] text-sm placeholder:text-[var(--g5-text2)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--g5-info)]";

/** Operator roles that may change platform state; viewers only observe. */
const OPERATING_ROLES = new Set(["admin", "member"]);

export interface PlatformHomeProps {
  /** The operator organization's slug (`operator`). */
  orgSlug: string;
  /** The caller's role in the operator organization. */
  role: string;
}

export function PlatformHome({ orgSlug, role }: PlatformHomeProps) {
  const canCreate = OPERATING_ROLES.has(role);
  const canManageOperators = role === "admin";

  return (
    <main
      data-testid="platform-home"
      className="mx-auto w-full max-w-[1400px] px-4 py-6 sm:px-6 lg:px-8"
    >
      <div className="mb-6 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold text-[var(--g5-text0)]">
            Platform operator
          </h1>
          <p className="mt-1 text-sm text-[var(--g5-text2)]">
            Organizations on this deployment. Operating the platform grants no
            access inside them.
          </p>
        </div>
        {canManageOperators && (
          <Link
            to={`/${orgSlug}/base?view=members`}
            data-testid="platform-manage-operators"
            className="text-sm text-[var(--g5-info)] hover:underline"
          >
            Manage operators →
          </Link>
        )}
      </div>
      <div
        className={
          canCreate
            ? "grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]"
            : "grid grid-cols-1 gap-6"
        }
      >
        <PlatformOrgsTable />
        {canCreate && <CreatePlatformOrgForm />}
      </div>
    </main>
  );
}

// ---------------------------------------------------------------------------
// Organizations table
// ---------------------------------------------------------------------------

function PlatformOrgsTable() {
  const orgs = usePlatformOrgs();
  const count = orgs.status === "ok" ? orgs.data.length : undefined;

  return (
    <section data-testid="platform-orgs" className="min-w-0 space-y-2">
      <HomeSectionHeader
        title="Organizations"
        count={count}
        countLabel="organizations"
        countTestId="platform-orgs-count"
      />
      <div className="overflow-x-auto rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)]">
        {orgs.status === "loading" && (
          <p role="status" className="px-4 py-5 text-sm text-[var(--g5-text2)]">
            Loading organizations…
          </p>
        )}
        {orgs.status === "error" && (
          <p
            role="alert"
            data-testid="platform-orgs-error"
            className="px-4 py-5 text-sm text-[var(--g5-blocked)]"
          >
            {orgs.error instanceof ApiError && orgs.error.status === 403
              ? "Only active platform operators can list organizations."
              : "Organizations are temporarily unavailable."}
          </p>
        )}
        {orgs.status === "ok" && orgs.data.length === 0 && (
          <p className="px-4 py-5 text-sm text-[var(--g5-text2)]">
            No organizations yet.
          </p>
        )}
        {orgs.status === "ok" && orgs.data.length > 0 && (
          <table
            data-testid="platform-orgs-table"
            className="w-full text-left text-sm"
          >
            <thead className="border-b border-[var(--g5-stroke)] text-xs uppercase tracking-wide text-[var(--g5-text2)]">
              <tr>
                <th scope="col" className="px-4 py-2 font-medium">
                  Name
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  Slug
                </th>
                <th scope="col" className="px-4 py-2 font-medium">
                  Created
                </th>
                <th scope="col" className="px-4 py-2 text-right font-medium">
                  Members
                </th>
              </tr>
            </thead>
            <tbody>
              {orgs.data.map((org) => (
                <tr
                  key={org.id}
                  data-testid="platform-org-row"
                  data-org-slug={org.slug}
                  className="border-b border-[var(--g5-stroke)] last:border-b-0"
                >
                  <td className="px-4 py-2 text-[var(--g5-text0)]">
                    {org.name}
                  </td>
                  <td className="px-4 py-2 font-mono text-xs text-[var(--g5-text1)]">
                    {org.slug}
                  </td>
                  <td className="px-4 py-2 text-[var(--g5-text2)]">
                    {formatDateTime(org.createdAt)}
                  </td>
                  <td className="px-4 py-2 text-right tabular-nums text-[var(--g5-text1)]">
                    {org.memberCount}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Create organization form
// ---------------------------------------------------------------------------

function createErrorMessage(error: unknown, slug: string): string {
  if (error instanceof ApiError) {
    if (error.status === 409) {
      return `An organization with the slug “${slug}” already exists.`;
    }
    if (error.status === 403) {
      return "Your operator role cannot create organizations.";
    }
  }
  return error instanceof Error && error.message
    ? error.message
    : "Could not create the organization.";
}

function CreatePlatformOrgForm() {
  const createOrg = useCreatePlatformOrg();
  const [name, setName] = useState("");
  // The slug follows the name until the operator edits it.
  const [slugOverride, setSlugOverride] = useState<string | null>(null);
  const [adminEmail, setAdminEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<CreatePlatformOrgResponse | null>(
    null,
  );

  const slug = slugOverride ?? orgSlugFromName(name);
  const canSubmit =
    !!name.trim() &&
    !!slug.trim() &&
    !!adminEmail.trim() &&
    !createOrg.isLoading;

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!canSubmit) return;
    setError(null);
    setCreated(null);
    const submittedSlug = slug.trim();
    try {
      const result = await createOrg.mutateAsync({
        name: name.trim(),
        slug: submittedSlug,
        adminEmail: adminEmail.trim(),
      });
      setCreated(result);
      setName("");
      setSlugOverride(null);
      setAdminEmail("");
    } catch (err) {
      setError(createErrorMessage(err, submittedSlug));
    }
  };

  return (
    <section data-testid="platform-create-org" className="min-w-0 space-y-2">
      <HomeSectionHeader title="Create organization" />
      <form
        onSubmit={(event) => void handleSubmit(event)}
        className="space-y-4 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg1)] p-4"
      >
        <div>
          <label
            htmlFor="platform-org-name"
            className="mb-1 block text-sm text-[var(--g5-text1)]"
          >
            Organization name
          </label>
          <input
            id="platform-org-name"
            data-testid="platform-create-org-name"
            type="text"
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
            placeholder="Acme Corp"
            className={INPUT_CLASSES}
          />
        </div>
        <div>
          <label
            htmlFor="platform-org-slug"
            className="mb-1 block text-sm text-[var(--g5-text1)]"
          >
            Slug
          </label>
          <input
            id="platform-org-slug"
            data-testid="platform-create-org-slug"
            type="text"
            value={slug}
            onChange={(event) => {
              setSlugOverride(event.target.value);
              setError(null);
            }}
            placeholder="acme-corp"
            className={`${INPUT_CLASSES} font-mono`}
          />
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            The organization&apos;s URL segment. It cannot be a reserved name.
          </p>
        </div>
        <div>
          <label
            htmlFor="platform-org-admin-email"
            className="mb-1 block text-sm text-[var(--g5-text1)]"
          >
            Admin email
          </label>
          <input
            id="platform-org-admin-email"
            data-testid="platform-create-org-admin-email"
            type="email"
            value={adminEmail}
            onChange={(event) => {
              setAdminEmail(event.target.value);
              setError(null);
            }}
            placeholder="admin@example.com"
            className={INPUT_CLASSES}
          />
          <p className="mt-1 text-xs text-[var(--g5-text2)]">
            They become an Admin. You do not join the organization.
          </p>
        </div>

        {error && (
          <div
            role="alert"
            data-testid="platform-create-org-error"
            className="flex items-center gap-2 text-sm text-[var(--g5-blocked)]"
          >
            <AlertCircle className="h-4 w-4 flex-none" aria-hidden="true" />
            {error}
          </div>
        )}

        {created && (
          <div
            role="status"
            data-testid="platform-create-org-result"
            className="space-y-1 rounded-[var(--g5-radius-sm)] border border-[var(--g5-ok)]/40 bg-[color-mix(in_srgb,var(--g5-ok)_10%,transparent)] px-3 py-2 text-sm text-[var(--g5-text1)]"
          >
            <p>
              Created{" "}
              <span className="font-semibold text-[var(--g5-text0)]">
                {created.org.name}
              </span>{" "}
              (<span className="font-mono">{created.org.slug}</span>).
            </p>
            <p data-testid="platform-create-org-admin">
              Admin {created.admin.email}:{" "}
              {created.admin.accountCreated
                ? "new account — they sign in with this email."
                : "existing account."}
            </p>
          </div>
        )}

        <div className="flex justify-end">
          <Button
            type="submit"
            kind="primary"
            data-testid="platform-create-org-submit"
            disabled={!canSubmit}
          >
            {createOrg.isLoading ? (
              <span className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                Creating…
              </span>
            ) : (
              "Create organization"
            )}
          </Button>
        </div>
      </form>
    </section>
  );
}
