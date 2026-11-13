# Keep a free worker-host slot ready

## Human instructions

On 2026-10-16 the owner, restating managed capacity as it stands, asked:

> hosts now scales up and down automatically, logic is
> remove host when idle (no VM running) for 30 minutes AND even after removal, there are at least one free slot
> add host when user hit start VM but no slots available
> Admin can configure those in the admin page these logic, can you add another to make sure that we probably can add to make sure there is always a free(not stale) slot

## Agent interpretation and decisions

- "Always a free slot" is a floor on free slots, not a wait for a Start: when the last free slot in the organization's region is taken, managed capacity adds a host before the next Start arrives, instead of only after one queues. "Not stale" means a free slot on a host whose runtime no longer matches the control plane does not count, exactly as placement already refuses it.
- The floor is an organization setting beside the existing managed-capacity knobs on the Admin page: `minFreeSlots`, default 1, range 0–100. 0 turns the floor off and restores today's "grow only for a waiting Start". The default is 1 because the owner asked for a free slot to always exist, not for an opt-in.
- The floor tops up a fleet; it does not create one. A region with no host at all (a new organization, or one whose administrators removed every host) gets its first host from a Start or from Add Host, as today. Otherwise every organization, including dormant ones, would pay for a permanently idle host.
- The floor needs automatic growth on; with growth off the floor only stops removal from going below it. Removal already keeps one free slot elsewhere; it now keeps the floor (at least one).
- A floor host is an ordinary automatic claim with no Start behind it. It is launched, retried and reconciled on the same path as a Start's claim, and is abandoned when it is no longer needed, so the floor adds no second lifecycle.
- Default 1 changes behavior for hosted organizations that already run hosts: taking the last free slot now provisions another host. The owner asked for this; it is the point of the change.
