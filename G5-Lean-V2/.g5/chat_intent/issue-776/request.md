# Platform operator organization (issue #776)

## Human instruction

The workspace owner asked (2026-10-24): "Work on G5Labs/G5-Lean#776: Platform operator:
a well-known 'operator' organization that operates the deployment".

## Source

[G5Labs/G5-Lean#776](https://github.com/G5Labs/G5-Lean/issues/776). Its "Original
intent (Ben)" section is the human requirement; its "Research and additions" section
and the comment assessments are agent proposals. Ben's comment: "app.g5labs.ai/admin/
would get to the platform admin org. I propose the Home page for admin/ is the admin
home page instead." Davide's comment asked about per-org operator tiers; the agent's
reply there recommends one roster (a flagged org).

## Owner decisions in chat (2026-10-24)

Asked as four choices; the owner answered:

1. Bootstrap (superseded by the design review below): **"First listed sign-in"**. Deployment config lists bootstrap operator
   emails; when the first listed person signs in and no operator org exists, G5 creates
   `operator` with them as owner and invites the other listed emails as admins. After
   that the list grants nothing; operators invite each other.
2. Cascade: **"Keep Base + docs"**. The operator org keeps the ordinary creation
   cascade, so the Members and Admin pages (which render inside the Base workspace
   view) manage the operator roster, and docs can hold runbooks. Only Home differs.
3. Owner of an operator-created org: the owner chose "Other" and wrote **"If the user
   doesn't exist, create it"**. The offered recommendation was "existing user by email;
   the operator never joins the tenant". Interpretation: the operator names the owner
   by email; an existing account becomes owner, otherwise G5 creates the account for
   that email and makes it owner.
4. URL: **"/operator only"**. Slug and URL are both `operator`; `admin` is reserved as
   a slug too so it can become an alias later.

## Design review (2026-10-24)

After reading the design the owner held implementation for review and said: "No
"operator" organizations currently exist - no need for this upgrade logic - fine to
just error." The design's slug-collision refusal and its log event were removed.

The owner then said: "Platform seeding scripts currently create an initial
organization. Update the design to ensure that platform seeding just provisions the
operator org". The platform seed is `scripts/seed-prod-org.sh`. It already writes org
rows through a non-login service user and invites `DEFAULT_ADMIN_EMAIL` as admin, so it
also takes over the first-operator bootstrap. That supersedes answer 1 ("First listed
sign-in"). Tenant organizations, including QA's `g5`, are created afterwards through the
operator API. Interpretation: the self-hosted installer is product setup for a single
customer org, not platform seeding, so it is unchanged.

## Self-serve creation and the allowlist

The owner then said (2026-10-24): "currently I think that successfully login for a user
without an organization offers to create an organization. With operators this route
shouldn't exist. The allowlist exists only for bootstrap. If we have a better bootstrap
mechanism then the allowlist could be retired?"

- Self-serve organization creation is removed (an instruction).
- Allowlist retirement was asked as a question. The agent's assessment is yes: with
  seeded operator invitations and operator-created organizations, admission by
  invitation or membership alone is stricter than every configured allowlist, and it
  keeps the 2026-10-23 production intent to refuse unlisted personal accounts. The
  design adopts retirement; the owner may reverse it.

## Implementation (2026-10-24)

After reading the updated design the owner said "complete implementation". The agent
takes this as acceptance of the design as written, including allowlist retirement and
the two-step `reset-dev-environment.sh`.

## Dev and preview seeding (2026-10-26)

The owner asked how `dev-up` and preview are seeded and whether an operator org is
provisioned. It was not: only CI minted the fixture operator persona. Offered either a
dedicated operator dev persona or matching prod, the owner said: "Let's match prod.
give Alex operator membership". Interpretation: Alex (`demo@g5labs.ai`), the dev
administrator persona, gets the administrator role in the operator org, as the platform
seed gives `DEFAULT_ADMIN_EMAIL`.

## Preview origin (2026-10-26)

The owner reported "Could not switch to Platform operator. Try again from the
organization switcher." on `/operator/home` in a full-stack preview. The preview's API
refused the `g5_refresh` cookie exchange from the preview origin, because only
`G5_EXTERNAL_URL` (`http://localhost:13000`) was trusted. Asked how previews should
trust their own origin, the owner chose **"Platform passes origin"**: the API puts the
preview's public origin in the preview module input, g5d exports it to the command, and
`dev-stack-up` trusts it. Asked where the fix goes, the owner chose **"Fold into #776"**.
Interpretation: the gap predates #776 (main, 2026-10-13) and affects every org switch
in a full-stack preview; it surfaced with the operator console.

The owner then asked to "restart the preview (down and up to try the from-scratch state
experience)" and reported "I still get the same error. Preview stack is still up for
investigation". The restarted stack trusted the origin, but the preview relay dropped
its `g5_refresh` Set-Cookie, so the cookie refresh answered 401. Interpretation: the
same request covers renaming the previewed stack's refresh cookie.

## Interpretation

- "Create it" means a G5 user row for the email, without a provider identity. The
  existing sign-in linking (`AuthService::handle_callback` matches an account by any
  verified email) attaches the person's provider identity at their first verified
  sign-in, so only someone proving that email can use the account.
- Open questions from the issue that the owner did not answer stay open (self-hosted
  mode, provisioning and monitoring scope, the license token). The design keeps current
  behavior for each.
