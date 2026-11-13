# Revoke live event access

[Issue #610](https://github.com/G5Labs/G5-Lean/issues/610) changes
[current authority](../../ontology/concepts/security.g5.md#g5.security.current-authority)
to bound event-stream revocation by the 30-second heartbeat interval. Workspace
read access remains governed by current membership, visibility, invitations,
ownership and the existing organization administrator bypass.

The WebSocket hub owns connection closure and records the resolved organization
and workspace beside subscriptions. Organization removal evicts the user's
connections in that scope after commit. Workspace removal, visibility restriction
and completed transfer recheck affected readers after commit and evict only those
who lost access. Transfer hooks belong at the service commit, including background
completion; routes own commits for the other mutations. A connection with revoked
subscriptions closes as a whole; clients may reconnect to resources still allowed.

Every connection periodically rechecks its subscriptions with the same database
authorization used at admission. Organization channels must check current membership,
not merely the ticket's organization claim. A nonempty ticket organization also
requires current membership, including sockets subscribed only to their user channel;
account sockets with no selected organization remain valid.

Successful checks grant at most one heartbeat interval measured from the start of
the check, with renewal attempted halfway through that interval. Subscription
admission and renewal serialize per connection, so a concurrent new subscription
cannot escape renewal. Fan-out and the writer enforce the deadline independently
of database progress. Closure uses a separate signal that takes priority over the
bounded event queue, discarding queued data and interrupting blocked sends. Database
errors or slow checks fail closed; already transmitted bytes cannot be recalled.

This reuses the existing hub and access rules without a durable revocation log or
cross-pod notification system. Post-commit hooks accelerate local revocation; leases
bound races, missed hooks and connections on another API instance. Reauthorization
cost grows with active subscriptions, within the existing connection limit; overload
may close connections rather than extend authorization. Retained authorized readers
are not evicted merely because an authority row changed.

Validation exercises real API/WebSocket and PostgreSQL boundaries for organization
and workspace removal, visibility restriction, transfer, retained readers, stale
tickets and direct database revocation without a hook. Hub tests cover full writer
queues, expired authority, scope matching and subscription/eviction races. No
terminal, preview, ticket-expiry or unrelated authorization policy changes are included.

Authorized branch: `issue-610-security-removed-members-keep-receiving-live`.
