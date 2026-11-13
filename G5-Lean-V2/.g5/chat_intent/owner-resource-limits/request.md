# Per-owner workspace and running-VM limits

## Human instructions

On 2026-10-22 the owner asked:

> One additional protection about runaway workspace consumption would be to add some org defaults for maximum number of running VMs by owner and maximum number of workspaces owned by user. These would stop runaway agents before they fully consumed available resources (for some resources). Tell me what would be involved in enforcing these limits. It's fine if there are pathways where a user can exceed them if they aren't readily exploitable (eg lowering the limit doesn't cancel running VMs.

After the exploration the owner answered:

> I accept your recommendations on decisions 1-4
>
> for "which workspaces count" I don't understand your question about deactivated members. Presumably workspaces count for their owner, whether they are deactiviated or not - they definitely don't count for another user.

and then:

> keep it in this workspace - this is part of the safety mechanism we need to ship. Implement away.

The accepted recommendations were:

1. Defaults of 5 running VMs and 50 workspaces per owner (the organization-wide VM quota stays 10). 0 means the owner can start or create none; there is no "unlimited" value. Owners already over a new limit keep what they have but cannot add more.
2. A Start over the owner's limit is refused with a clear error rather than queued, and the owner's already-waiting Starts count toward the limit.
3. All live, non-Base workspaces an owner holds count, including ones still being created and review and formal approval-review workspaces; an approval review whose requester is at the limit fails cleanly.
4. No per-member overrides in the first version.

## Agent interpretation

- Workspaces count only for their current owner, whether that member is active or deactivated. Deactivation never reassigns ownership, so a reactivated member's workspaces count against their own limit again.
- The limits are admission checks. Lowering one never stops, deletes or transfers anything; it only refuses new Starts and new workspaces until the owner is back below it.
- An ownership transfer to a member at the workspace limit is refused like a fork, so another member or an administrator cannot hand a capped owner more workspaces.
