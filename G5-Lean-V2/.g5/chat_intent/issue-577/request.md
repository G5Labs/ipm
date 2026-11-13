# Workspace configuration authority

The user clarified that a workspace has a single writer by design, then requested:

> we should only allow workspace owner to change workspace scoped configuration,
> and we should remove the org scoped configuration in workspace completely and
> move them to admin page

Implementation interpretation: enforce current ownership for workspace runtime
configuration and provider-account mutations, with ownership held until commit.
Move organization runtime configuration, provider-account management and organization
API-key controls from workspace settings into Admin. Preserve inherited organization
defaults and selection of shared accounts. Personal configuration remains personal.
The prior credential-origin proposal was reverted; proxy removal was evaluated but
is not part of the implemented permission repair.

The user subsequently clarified the reason for evaluating removal:

> i understand Removing the LLM proxy is not necessary to fix that permission bug.
> but if LLM proxy brings no value to use, there is no reason to keep it

This asks whether the proxy earns its continued complexity independently of the
permission bug. The follow-up assessment and removal proposal are in the
[existing plan](../../pr_ontology_diff_design/issue-577/ontology-plan.md#llm-proxy-removal-assessment).
The user accepted the removal proposal on 2026-10-17 and requested implementation,
validation and a PR. Shipping requires green applicable GitHub checks, no merge
conflicts, and independent validation and inline responses for connector P0/P1
and CodeRabbit Critical/Major findings. Monitor reviews for up to 30 minutes after
each push; fix verified in-scope findings together and report unrelated issues
separately. The owner also reviews the PR.
