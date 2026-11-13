# Idle managed-host feasibility request

Human request, 2026-10-14, with the subsequent clarification: evaluate feasibility
and difficulty of preferring a host with at least one running VM for new VM starts,
and automatically removing an empty host after over one hour when at least two
healthy hosts have free slots, including the candidate. Removing the candidate
leaves **one** healthy host with free slots. This supersedes the initial ambiguous
"more than two" wording and the assessment's two-remaining-host interpretation.
The motivation is human-added hosts remaining unused outside office hours while
image rotation keeps replacing them.

The human explicitly accepts that an unused but running VM prevents host removal.
Also propose automatic scale-up from the existing capacity-waiting Start flow:
provision the smallest host, `c3-standard-4`, with the normal defaults for concurrent
jobs, disk size and other settings. Propose better alternatives where useful.

Further human direction: automatic provisioning for queued VM Starts should reuse
as much as possible of the existing Workers page Add Host logic. The human also
asked whether the organization VM quota already exists; this asks for clarification,
not removal or alteration of that quota.

The human explicitly excluded proactive spare replenishment from the current
scope. Automatic growth requires valid queued VM Starts with unmet capacity;
consuming the last free slot alone does not trigger host creation. The rule that
idle removal leaves one healthy host with free slots remains unchanged.

The human subsequently requested implementation of this design. The separate request
to raise the default organization VM quota was completely reverted at their request;
the default remains ten and quota configurability is outside this implementation.

The [assessment](../../pr_ontology_diff_design/idle-host-recycle/ontology-plan.md)
separately identifies proposals for pool scope, placement tie-breaking, demand
accounting and automatic provisioning bounds. The
one-remaining-host threshold and acceptance of running VMs blocking removal are
human decisions; those additional proposals are not.

Follow-up, 2026-10-14 after PR #514: the human observed QA's hourly load-balancer
tunnel reconnect clearing `idle_since`, making retirement depend on a narrow
sweep timing window. Fix hello's idle continuity inside its existing transaction;
do not raise the load-balancer timeout. First reproduce through real database
host control, then cover unchanged empty reconnects, live inventory, sizing
changes, stale heartbeats, readiness promotion, delayed old-tunnel heartbeats,
reservations between sweeps, and sweeps during a short reconnect gap. Explicitly
decide whether the existing 30-second freshness boundary remains. Run capacity,
worker-host and stale-host/rotation tests plus workspace/all-target Clippy, commit
and open a follow-up PR. The QA observations are human-provided evidence, not
an independently repeated live experiment by this agent.
