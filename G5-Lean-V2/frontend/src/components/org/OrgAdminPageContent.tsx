/**
 * OrgAdminPageContent — the organization's settings page, one scroll with an
 * index on the left (ui g5.ui.settings-page).
 *
 * Holds the org-level settings themselves (identity and the active-org
 * switch, the settings administrators edit, Issues, workbench actions,
 * organization defaults, accounts and API keys) as sections of one sheet, and links the
 * configuration surfaces that stay separate tabs (Members, Approval
 * Definitions, Workers, GitHub App) from the index.
 *
 * Renders WITHOUT standalone `<AppShell>` chrome: it is a center-panel tab
 * inside `WorkspacePage` (`?view=admin`, tab type `org_admin`). Like the other
 * org admin surfaces it reads the `:org` slug from `useParams` and resolves the
 * org from the authenticated user, so it re-opens cleanly from tab persistence
 * and takes no org props.
 *
 * The whole page is for the org's admins (`g5.settings.admin-page`):
 * anyone else sees an "Administrators only" notice, and the Account flyout
 * offers the page only to them. Its organization settings (default idle
 * timeout, VM quota) are also refused by the API for anyone else.
 *
 * Each page link points at the canonical `/:org/settings/*` route, which
 * redirects into this org's base workspace with the matching `?view=`
 * (mirrors the long-standing Account-flyout navigation pattern). The
 * individual surfaces keep their own authorization; the links are shown
 * except where an instance feature gates the surface entirely (GitHub App).
 *
 * @module org/OrgAdminPageContent
 */

import React, { useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { CheckCircle2, UserPlus } from "lucide-react";

import { Button, Chip } from "../ui/UI";
import type { State } from "../ui/UI";
import { useAuth } from "../../hooks/useAuth";
import { useInstanceInfo } from "../../hooks/useSettings";
import { isGithubAppEnabled } from "../../lib/githubAppFlags";
import {
  SettingsPageShell,
  SettingsRow,
  SettingsSection,
  type SettingsIndexLink,
} from "../settings/SettingsPageShell";
import { InviteDialog } from "./InviteDialog";
import { OrgAdminSettingsCard } from "./OrgAdminSettingsCard";
import { OrgIssuesSettingsSection } from "./OrgIssuesSettingsSection";
import { OrgRuntimeConfigSection } from "./OrgRuntimeConfigSection";
import { OrgWorkbenchActionsSection } from "./OrgWorkbenchActionsSection";

const ROLE_STATE_MAP: Record<string, State> = {
  admin: "info",
  member: "ok",
  viewer: "muted",
};

export function OrgAdminPageContent() {
  const { org, workspace } = useParams<{ org: string; workspace?: string }>();
  const { user, switchOrg } = useAuth();
  const instanceInfo = useInstanceInfo();
  const info = instanceInfo.status === "ok" ? instanceInfo.data : null;

  const githubAppEnabled = isGithubAppEnabled(info);

  // Resolve the route org from the authenticated user's org list (slug → org),
  // the same derivation every org center-panel surface uses.
  const currentOrg = useMemo(
    () => user?.orgs.find((candidate) => candidate.orgSlug === org),
    [user, org],
  );
  const orgName = currentOrg?.orgName ?? org ?? "";
  const orgSlug = currentOrg?.orgSlug ?? org ?? "";
  const role = currentOrg?.role ?? "viewer";
  const roleLabel = role.charAt(0).toUpperCase() + role.slice(1);
  // The route org's role decides; JWT scopes describe the selected org, which
  // need not be this one.
  const isAdmin = role === "admin";
  const isActiveOrg = !!currentOrg && currentOrg.orgId === user?.activeOrgId;

  const [inviteOpen, setInviteOpen] = useState(false);

  // The route named an org this user is not a member of (or auth is mid-load).
  // Mirror the sibling surfaces' simple not-found gate rather than guessing.
  if (!currentOrg) {
    return (
      <div
        className="mx-auto w-full max-w-5xl p-6"
        data-testid="org-admin-page-content"
      >
        <p className="text-sm text-[var(--g5-text2)]">
          Organization not found.
        </p>
      </div>
    );
  }

  if (!isAdmin) {
    return (
      <div
        className="mx-auto w-full max-w-5xl p-6"
        data-testid="org-admin-page-content"
      >
        <p
          className="text-sm text-[var(--g5-text2)]"
          data-testid="org-admin-forbidden"
        >
          Administrators only. Ask an admin of {orgName} for access.
        </p>
      </div>
    );
  }

  const pageLinks: SettingsIndexLink[] = [
    {
      id: "members",
      label: "Members",
      href: `/${orgSlug}/settings/members`,
      testId: "org-admin-link-members",
    },
    {
      id: "approvals",
      label: "Approval definitions",
      href: `/${orgSlug}/settings/approvals`,
      testId: "org-admin-link-approvals",
    },
    {
      id: "workers",
      label: "Workers",
      href: `/${orgSlug}/settings/workers`,
      testId: "org-admin-link-workers",
    },
    ...(githubAppEnabled
      ? [
          {
            id: "github-app",
            label: "GitHub App",
            href: `/${orgSlug}/settings/github-app`,
            testId: "org-admin-link-github-app",
          } satisfies SettingsIndexLink,
        ]
      : []),
  ];

  return (
    <SettingsPageShell
      testId="org-admin-page-content"
      view="admin"
      crumbs={[orgSlug, workspace]}
      title="Admin"
      scopeLine={`${orgName} · Organization settings · ${roleLabel}`}
      headerAside={
        <>
          <Chip state={ROLE_STATE_MAP[role] ?? "muted"} text={roleLabel} />
          {isActiveOrg && <Chip state="ok" text="Active" />}
          <Button
            kind="primary"
            data-testid="org-admin-invite"
            onClick={() => setInviteOpen(true)}
          >
            <UserPlus className="mr-1 inline h-4 w-4" />
            Invite teammate
          </Button>
        </>
      }
      indexTitle="Organization settings"
      links={{
        title: "Pages",
        items: pageLinks,
        testId: "org-admin-config-section",
      }}
      indexFooter={
        <>
          Your own settings are in{" "}
          <a
            href={`/${orgSlug}/settings`}
            data-testid="org-admin-link-settings"
            className="underline underline-offset-2 hover:text-[var(--g5-text0)]"
          >
            Settings
          </a>
          .
        </>
      }
    >
      {/* Identity and the active-org switch. */}
      <SettingsSection
        id="organization"
        title="Organization"
        description="Who this organization is and whether it is the one you are working in."
        testId="org-admin-organization-section"
      >
        <SettingsRow
          title="Identity"
          description="The name people see and the slug in every URL."
        >
          <dl className="grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-2">
            <div>
              <dt className="text-xs text-[var(--g5-text2)]">Name</dt>
              <dd className="text-sm text-[var(--g5-text0)]">{orgName}</dd>
            </div>
            <div>
              <dt className="text-xs text-[var(--g5-text2)]">Slug</dt>
              <dd className="font-mono text-sm text-[var(--g5-text0)]">
                {orgSlug}
              </dd>
            </div>
            <div>
              <dt className="text-xs text-[var(--g5-text2)]">Your role</dt>
              <dd className="text-sm text-[var(--g5-text0)]">{roleLabel}</dd>
            </div>
            <div>
              <dt className="text-xs text-[var(--g5-text2)]">Active org</dt>
              <dd className="flex items-center gap-2 text-sm text-[var(--g5-text0)]">
                {isActiveOrg ? (
                  <>
                    <CheckCircle2 className="h-4 w-4 text-[var(--g5-ok)]" />
                    This organization is active
                  </>
                ) : (
                  <button
                    type="button"
                    data-testid="org-admin-switch-active"
                    className="text-[var(--g5-info)] hover:underline"
                    onClick={() => switchOrg(currentOrg.orgId)}
                  >
                    Switch to this organization
                  </button>
                )}
              </dd>
            </div>
          </dl>
        </SettingsRow>
      </SettingsSection>

      {/* The settings administrators edit (default idle timeout, VM quota,
          managed capacity). */}
      <SettingsSection
        id="workspaces"
        title="Workspaces and hosts"
        description="What new workspaces start with, how many VMs may run at once, and how G5 adds and removes cloud hosts."
        testId="org-admin-settings-section"
      >
        <div className="rounded-[var(--g5-radius-md)] bg-[var(--g5-bg2)] p-4">
          <OrgAdminSettingsCard orgId={currentOrg.orgId} />
        </div>
      </SettingsSection>

      {/* Issues: the switch and issue domains, with or without the GitHub
          App (g5.issue.native.domain). Renders nothing unless offered. */}
      <OrgIssuesSettingsSection orgId={currentOrg.orgId} orgSlug={orgSlug} />

      {/* Workbench action switches (ui g5.workbench.review-prompts). */}
      <OrgWorkbenchActionsSection orgId={currentOrg.orgId} />

      {/* Organization defaults, accounts and API keys. */}
      <OrgRuntimeConfigSection orgId={currentOrg.orgId} orgName={orgName} />

      <InviteDialog
        open={inviteOpen}
        onClose={() => setInviteOpen(false)}
        orgId={currentOrg.orgId}
        onInviteSent={() => setInviteOpen(false)}
      />
    </SettingsPageShell>
  );
}
