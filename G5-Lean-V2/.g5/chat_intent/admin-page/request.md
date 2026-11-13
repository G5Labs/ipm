# Organization default idle timeout on the Admin page

## Human instructions

On 2026-10-14 the user asked:

> I want add some configuration in the admin page where only user with admin role can access
> the first setting will be the `Default Workspace VM idle timeout (minutes)`, although user (workspace owner) can change it for their own workpsace in the settings page, admin can change this default so that all NEW workspace will get this default value that admin set

## Agent interpretation and decisions

- "Admin page" is the existing organization Admin hub (`OrgAdminPageContent`, tab `org_admin`), whose "Organization settings" section was left as the home for future org-level settings. G5 has no platform-wide admin role; "admin role" means an organization owner or admin, the authority that already gates organization Runtime Config.
- "All NEW workspaces get this default" means every workspace created after the change starts with the organization default, whichever workspace it was forked from. Forks therefore stop copying the idle timeout from their source; other copied settings are unchanged.
- Changing the default never changes an existing workspace, including Base; owners still override their own workspace in workspace settings.
- (Superseded by the owner answers below: no organization is seeded from its Base.)

## Follow-up: organization VM quota (2026-10-14)

> the next one is the organization VM quota, it is default to 10 today, please make it configurable in the admin page

### Agent interpretation and decisions

- The quota is the existing `max_running_vms` org key in the typed `configs` store, enforced at placement and defaulting to 10 when absent; nothing writes it today. The Admin page edits that key with the same administrator authority as the idle default.
- Both settings live in `configs` as registered org keys, so the idle default moves there from the previously proposed `orgs` column: one org-settings owner and one validator.
- A new quota applies to later placements. Lowering it stops no running VM; queued starts wait until the organization is below it. Raising it lets queued starts place on the next reconcile. The quota only bounds use of the org's own operator-provisioned hosts, so org admins may raise it.

## Owner answers (2026-10-14)

> Re: "Range: 0–100000," 100k is too crazy, make it 0-100
> Re: "Should the whole Admin page be hidden from non-admins" yes sure, as it is called admin page
> Re: "Is it right that a fork of a workspace with a custom timeout gets the org default rather than copying that custom value?" NO!, it should always get from the default that admin put
> do not need to worry about existing workspace, change only applicable to new workspace once delivery, no migration or stage rollout is needed

### Agent interpretation

- The quota range is 0–100, enforced by the API and the registered-key validator.
- The whole Admin page and its entry point are admin-only.
- The "NO!" answers the question as worded, but the instruction is the design: a fork always takes the admin's default, never its source's value.
- "No migration" means no data migration or seeding: organizations are not initialized from their Base, so new workspaces take the 8-hour platform default until an admin sets one. A schema-only migration that registers the new key and narrows the quota range is still required, because the `configs` validator rejects unregistered keys; it rewrites no rows. One PR, no staged rollout.

## Follow-up: managed capacity on the Admin page (2026-10-14)

Asked which other org-level settings admins should control, the agent proposed defaults for new workspaces, the egress allowlist, and the managed-capacity knobs from PR #514 (recommending the latter stay operator-only for hosted orgs). The owner chose:

> let's create a group (with a surrounding box so that user know they are a group) for Managed-capacity . please create the frontend first, i will verify the ui via preview frontend

### Agent interpretation

- The group holds the four knobs #514 hardcodes: automatic host growth on/off, the machine type for added hosts (the Workers page's C3 list), idle-host removal on/off, and the removal delay (proposed range 15–1440 minutes).
- Defaults equal today's behavior: growth on, `c3-standard-4`, removal on, 60 minutes.
- The group sits inside the existing settings card and shares its single Save. It is a bordered `fieldset` with the legend "Managed capacity".
- Frontend first: the API does not report or accept these fields yet. The card falls back to the defaults, and only the mock API (frontend-only preview) stores them. The backend design and ontology follow once the owner approves the UI.
