# Account defaults — request and interpretation

## User instructions (2026-11-02)

> Let's redesign how account defaults are set. Right now in the settings there are account
> defaults in this section [the Defaults section of Settings: Default agent, per-agent Default
> model and Default account, Other variables].
>
> "Runtime variables" should not be set as default unless runtime variables have been
> explicitly set. Instead once you configure your first account, that should automatically
> become your default.
>
> Research this UX and propose changes

## Owner decisions (2026-11-02, on the proposal)

> Accept removal clearing same-scope defaults; Include the Auto default-agent rule; leave
> existing people to choose in Defaults

## Agent interpretation

- "Runtime variables" is the unassigned choice of the Account field (chat composer, fork
  dialog, Defaults card): it runs the launch on the raw credential keys of Runtime config
  (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, …). It should appear only while such a key exists
  for the agent's family; the Defaults card already follows this rule, the composer and the
  fork dialog do not.
- "Configure your first account" is creating a provider account in Settings → Accounts or
  Admin → Accounts. "Become your default" is the existing per-agent assignment
  (`claude_account_id` / `codex_account_id`) at the account's own scope, which the Defaults
  card shows as Default account and every launch inherits.
- The answers settle the three open questions of the
  [design](../../pr_ontology_diff_design/account-defaults/ontology-plan.md) and authorize its
  implementation: no backfill, so accounts created earlier stay unassigned until chosen.
