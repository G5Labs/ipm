# Subscription usage during agent work

The owner asked to detect subscription use for Codex and Claude, investigate
percentage meters, compare available approaches rather than simply adopting the
partial implementation, and minimize changes to existing code. Background checks
and persisted meters are unnecessary.

Interpretation: use the resolved launch's actual authentication route and passive
native usage observations during ordinary work. Keep display observations in
memory, separate from token/cost accounting and execution history. Missing
provider evidence remains unknown. See the [design](../../pr_ontology_diff_design/subscription-meters/ontology-plan.md).

Follow-up: the owner supplied a mock with a green ring and “99% left 7d” pill.
They requested a clickable button that expands the shown usage details, aligned
left on the configuration row above the chat input. The displayed percentage
continues to come from live observations; 99% is the mock's example value.

Follow-up: omit windows with no data and show only available usage. The owner
also asked to check Codex's `rateLimitsByLimitId[bucket].primary` and `secondary`
fields as possible sources for the missing five-hour observation.
