# Account defaults — design

Request: [chat intent](../../chat_intent/account-defaults/request.md). Governing blocks:
[g5.chat.account-defaults](../../ontology/features/chat.g5.md#g5.chat.account-defaults) (new),
[g5.chat.account-continuity](../../ontology/features/chat.g5.md#g5.chat.account-continuity)
(removal now clears same-scope assignments),
[g5.chat.fork-start-account](../../ontology/features/chat.g5.md#g5.chat.fork-start-account)
(amended fallback), with
[g5.chat.inheritance](../../ontology/features/chat.g5.md#g5.chat.inheritance),
[g5.chat.accounts](../../ontology/features/chat.g5.md#g5.chat.accounts) and
[g5.ui.onboarding-tour](../../ontology/features/ui.g5.md#g5.ui.onboarding-tour) unchanged.

## What happens today

- An account is a row in `provider_accounts` (org, workspace or user scope). The default for
  an agent is a separate non-secret scoped setting, `claude_account_id` or `codex_account_id`,
  in `org_configs`, `workspace_configs` or `user_configs`; the launch resolver reads the
  effective value (user > workspace > org) when a launch says `inherit_configured`.
  Creating an account (`ProviderAccountService::create`) only inserts the row; nothing ever
  writes an assignment except the Default account select in Settings → Defaults and
  Admin → Defaults (`ProviderAccountAssignmentRow`).
- The chat composer (`ChatPanel`, `accountValue`) and the fork dialog (`CloneDialog`,
  `forkDefaultAccountValue`) seed their Account field from the effective assignment and fall
  back to the `runtime_config` sentinel, labelled "Runtime variables", which is always the
  first option. With no raw credential key in Runtime config that choice resolves to no
  credential: the launch preview shows "A credential must be configured before this launch
  can start" and Send fails with `missing_llm_credential`.
- The Defaults card already applies the rule the request asks for
  (`legacyProviderKeysPresent`): its unassigned option reads "Runtime variables" only while a
  raw key (`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`,
  `ANTHROPIC_BASE_URL`; `OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `OPENAI_BASE_URL`) exists in
  any scope in context, otherwise "No account". The three surfaces disagree.
- Consequence for a new person: the onboarding tour holds step 3 until an account exists,
  then step 5 has to say "Pick a model and account above the composer" because the composer
  would otherwise start on Runtime variables. Someone who skips the tour adds an account and
  sees every new chat fail until they find Default account in Defaults, two sections away.
  An organization account has the same gap: it serves nobody until an administrator also sets
  the organization's Default account in Admin → Defaults.
- Default agent "Auto (first available)" resolves against workers only. A person whose only
  account is Codex starts on Claude Code and fails for want of a credential.
- Deleting an account is refused while any assignment names it
  (`account_assigned`, [g5.chat.account-continuity](../../ontology/features/chat.g5.md#g5.chat.account-continuity)).

## Proposal

1. **The first account becomes the default, written by the server with the account.**
   `ProviderAccountService::create` inserts the row and, in the same transaction, for each
   agent the kind supports (`supports_agent`), sets the assignment at the account's own scope
   when that scope's own key is unset: org account → `org_configs`, workspace account →
   `workspace_configs`, user account → the creator's `user_configs` for that organization. It
   reuses `ConfigService::update_{org,workspace,user}_scope`, whose assignment validation sees
   the new row inside the transaction. The Accounts row shows "Default · Claude Code" from the
   refetched assignments, so the API shape is unchanged. An existing assignment at that scope is
   never replaced; other scopes are untouched. A person adding their own account therefore overrides
   the organization's default only for themselves, which is what the tour's "Add your own
   subscription here if you prefer to use it" promises.
2. **Removal clears what it made.** `delete` clears the assignments at the account's own scope
   that name the account, in the same transaction, instead of refusing; assignments at other
   scopes (a member's user settings naming an organization account) still refuse with the
   existing message. Without this, every one-account person would be told to "reassign first"
   before removing their only account, a regression the auto-default would introduce. The
   invariant the AccountAssignments model protected, that no assignment names a deleted
   account, still holds: the clear and the delete commit together under the same row lock.
3. **"Runtime variables" only while raw keys exist, on every surface.** The composer and the
   fork dialog adopt the Defaults card's `legacyProviderKeysPresent` rule, computed from the
   scoped configs the pages already load (`useAssignmentConfigs`). Without raw keys the
   unassigned choice reads "No account", is offered only when it is the current value (there
   is nothing else to fall back to), and the launch preview's warning becomes "Add an account
   in Settings → Accounts" with a link. The backend contract is unchanged: `runtime_config`
   remains a valid explicit selection for legacy data and for the g5d CLI.
4. **Auto default agent prefers an agent that can authenticate.** When no default agent is
   chosen, Auto picks the first available agent with an effective account assignment or raw
   keys for its family, else the first available agent as today. Pure derivation in the one
   place each client resolves Auto (`defaultAgentProviderFromConfig` and the backend callers of
   `default_agent_provider_from_settings`); no new setting.
5. **Existing accounts are left as they are.** No backfill: people who created accounts
   before this rule choose in Defaults, as today. (Owner decision; the alternative, a startup
   backfill of single-account scopes, was declined.)
6. **Settings copy.** The Defaults description mentions accounts ("the default agent, each
   agent's model and account, and custom variables"). Accounts rows carry a read-only "Default
   · Claude Code" pill when the page's scope assigns them; choosing stays in Defaults, as the
   settings-page intent placed it. Optional: the tour's step 5 copy becomes "Pick a model
   above the composer, then send this message" once the account is implied.

## Choices and alternatives considered

- **Server write, not a client follow-up write.** The Accounts section could call the config
  update after a successful create, but the g5d CLI and API callers would not, the two writes
  could fail apart, and the Remove guard and the Defaults card would briefly disagree with the
  row. The server owns both facts and already has the transaction.
- **An explicit assignment, not a derived "only account" default at resolve time.** Deriving
  the default from the visible accounts needs a tiebreak once a second account exists and
  makes the Defaults select show a value nobody chose; a confirmed launch would also have to
  compare a derived account. The written key reuses continuity, the delete guard, the
  confirmed-launch comparison and the Defaults UI without change. Existing unassigned
  accounts are the cost of this choice; the owner declined a backfill.
- **Not removing "Runtime variables".** Raw keys remain a supported route (CI, gateways, the
  g5d CLI); hiding the choice when no such key exists removes the dead option without
  removing the capability.
- **Not auto-assigning on every create.** Replacing an existing default when a second account
  arrives would surprise the organization (an administrator adding a backup key would switch
  everyone). First-only is what the request asks for.

## Owner decisions (2026-11-02)

- Q1 removal rule (2): accepted.
- Q2 Auto default-agent rule (4): included.
- Q3 backfill: declined; existing people choose in Defaults.

## Implementation and tests (2026-11-02)

- Backend: `provider_account_service.rs` create/delete over two `ConfigService` helpers
  (`assign_account_where_unset`, `clear_account_assignments_at`) that take the scope lock
  before the row lock, the order Settings writers use, so a concurrent assignment and a removal
  cannot deadlock. Route tests in `g5-api/tests/api_provider_accounts.rs`:
  `first_account_becomes_the_default_at_its_scope_and_removal_clears_it` (first create assigns;
  a second does not; a `both`-family account fills only the agent still unset; a person's own
  account wins for them alone; removal clears the own scope and a peer's user-scope pin still
  refuses it), plus the adjusted delete-guard and assignment-validation tests.
- Frontend: the raw-key rule and the unassigned option live in `lib/providerAccountSelection.ts`
  (`legacyProviderKeysPresent`, `unassignedAccountOption`, `authenticatableAgentProviders`);
  `useAssignmentConfigs` exposes `runtimeVariablesAvailable` so the composer (`ChatPanel`), the
  fork dialog (`CloneDialog`), the Defaults card and the Accounts section share one reading of
  the loaded configs. Auto goes through `getPreferredAvailableAgentProvider` and
  `getPreferredSyncAgentConfig`. The Accounts section shows the Default pill and blocks Remove
  only for other-scope assignments (`blockingAssignmentIdsFrom`); create and remove invalidate
  the scoped configs with the list. The launch warning reads "No account can authenticate this
  agent. Add one in Settings → Accounts." without a link, because the composer renders on
  pages whose settings destination differs (workspace, Home). Tests beside each file; the e2e
  provider-accounts spec reads the unassigned label before seeding and expects the seeded
  account to be the default.
- Ontology kept current in the same PR; the fork rule's "else Runtime variables" is amended.
