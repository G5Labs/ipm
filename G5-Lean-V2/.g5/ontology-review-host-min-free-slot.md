# Free-slot floor implementation review

Review record for the `kaihui/host-min-free-slot` change (PR #598) against the saved
[design](pr_ontology_diff_design/host-min-free-slot/ontology-plan.md) and
[chat intent](chat_intent/host-min-free-slot/request.md). The same agent implemented the
change; independent rounds follow the self-review.

## Self-review before the first commit

| Commitment | Check | Result |
|---|---|---|
| `g5.architecture.host_demand_capacity` (floor) | Taking the last free slot claims one host with no Start behind it; the claim launches on the shared claim path; in flight it covers the floor; a region with no host is not grown; growth off and floor 0 claim nothing; stale slots count for nothing. | `api_host_capacity.rs`: `taking_the_last_free_slot_claims_a_floor_host_once`, `the_floor_follows_growth_and_its_value_and_ignores_stale_slots`, `a_stale_fleet_gets_a_current_floor_host_and_the_empty_stale_host_retires`, `floor_off_no_proactive_spare_and_manual_inflight_capacity_covers_demand`. Passing. |
| Claim lifecycle shared | A floor claim is abandoned when other supply covers the floor; a Start claim and a floor claim made together resolve to exactly one survivor once the Start is cancelled; a stockout retries on the same row and blocks a second claim. | `floor_claims_are_abandoned_or_kept_by_what_the_floor_needs`, `a_failed_floor_claim_retries_on_its_row_and_blocks_a_second_claim`. Passing. |
| `g5.architecture.host_idle_retirement` (reserve) | With a floor of 2 the small host retires and the large one stays; a floor above the remaining supply stops removal and claims instead. | `removal_keeps_the_floor_of_free_slots`. Passing. |
| `g5.data.configuration.org-admin-settings` | `minFreeSlots` round-trips, 101 and -1 are refused without writing; migration 086 applied by the real migrator, replays as a no-op, rewrites no row, bounds hold. | `api_org_admin_settings`, `migration_086_min_free_slots`. Passing. |

Defects found and repaired before commit: a Start on a full fleet claimed a Start host and a floor host in the same sweep without counting the Start claim's spare slots (now `claimed_spare`); a dead Start claim could make a region "present" for the floor (presence now needs a cloud instance or a registration); eight existing growth tests pinned to floor 0.

## Independent review round 1 (Codex CLI 0.155.1, `gpt-6-astra`, effort `xhigh`, read-only sandbox) plus CodeRabbit

Verdict: changes requested. Dispositions:

| Finding | Disposition |
|---|---|
| P1 · The manual cross-region stockout retry (`provision_next_retry_at`, `stale_host::retry_stockout_rows`) excludes only Start claims, so it could retry an exhausted floor claim a fourth time and move it to a fallback region without `drive_claim`'s locks or necessity check. | Fixed: migration 086 replaces the 082 capacity guard so it clears the schedule for floor claims too, and all three selectors in `stale_host.rs` exclude `capacity_floor_claim`. Tests: `a_failed_floor_claim_retries_on_its_row_and_blocks_a_second_claim` (fallback regions configured; a forced schedule reads back NULL; the region stays) and the migration test (guard clears a floor claim's schedule, keeps a plain host's). |
| P1 · Retiring the last stale host in the sweep that claims its floor successor left no "present" host for `drive_claim`, which abandoned the claim; the region never recovered. | Fixed: presence gates claim creation and the keeping of a Start's claim, not a floor claim's recovery (`floor_short` takes `require_present`, `!h.floor_claim` in `drive_claim`). Rejected alternatives: keep the stale host until a successor is established (would hold the `quota + 1` bound, `a_spent_stale_claim_and_a_stale_fleet_free_the_resource_bound`); drop presence for every claim (a cancelled Start's claim in an empty org became a floor host, `quota_only_wait_cancellation_and_configuration_do_not_launch_hosts`). Test: the stale-fleet test runs through launch. |
| P1 · A floor claim that demand is assigned to in a later sweep was never launched (the assignment branch scheduled only Start claims, the recovery loop skips assigned hosts). | Fixed: the assignment branch schedules `h.claim && h.retry_due`. Test: `a_floor_claim_that_demand_is_assigned_to_still_launches`. |
| P1 · `drive_claim` shadowed the claim's persisted region with the demand region, so a region change could bring the host up in the wrong region under the old row; the floor check never required a region match. | Fixed: the demand region is `floor_region`, the claim's own region drives lookup and bring-up, and floor need requires `h.region == floor_region`. Test: `a_floor_claim_outside_the_demand_region_is_abandoned_not_moved`. |
| CodeRabbit · A Start claim from an earlier sweep covered the floor. | Fixed: only unassigned young claims cover the floor. Test: `a_start_claim_from_an_earlier_sweep_does_not_cover_the_floor`. |
| CodeRabbit · `resources` not incremented after a floor claim; removal hint and the design's "rejected per-host marker" sentence contradicted the code. | Hint and plan fixed. The increment is not applied: nothing reads `resources` after the floor claim and clippy (`-D warnings`) rejects the unread assignment; the floor claim is the sweep's last claim by construction. |
| Consequence · With a Start's claim no longer covering the floor, ten fully used claims gain an eleventh floor host within the `quota + 1` bound. | By design; `burst_claims_are_durable_deduplicated_and_capped_by_unchanged_quota` pins the floor to 0 like the other Start-growth tests. |

## Independent review round 2 (same reviewer; round-1 repairs listed for verification)

Verdict: changes requested, one finding.

| Finding | Disposition |
|---|---|
| P1 · With the `quota + 1` bound filled by empty stale automatic hosts, the sweep cannot claim a floor host, retires every stale host, and afterwards reads the region as empty: the floor never recovers until a Start or Add Host. | Fixed: a host automatic retirement removed keeps the region present while its cloud cleanup is unconfirmed or for an hour after it (`retired_recently` in `supply`, counted by `floor_short`); such rows join the snapshot. Operator removals carry other drain reasons and still end the floor. Test: `the_floor_recovers_after_a_stale_fleet_retires_against_the_resource_bound`. |

Consequence of round 2: a retired stale fleet keeps the region present, so a Start arriving after cleanup gets its own claim and a floor claim; `a_spent_claim_is_released_while_the_quota_hides_its_start` and `a_spent_stale_claim_and_a_stale_fleet_free_the_resource_bound` pin the floor to 0 like the other Start-growth tests.

## Independent review round 3 (same reviewer; round-2 repair listed for verification)

Verdict: changes requested.

| Finding | Disposition |
|---|---|
| P1 · Presence from a recent automatic retirement survived an operator's later removal of the region's last host, so the floor re-created a host the administrator had just deleted. | Fixed: presence is an explicit marker, `worker_hosts.capacity_floor_presence`, set on the row automatic retirement removes while the floor is set and cleared for the whole region by `remove_host` with a real actor (the sweep passes the nil actor). Test: `an_operator_removal_ends_the_floor_after_an_automatic_retirement`. |
| P2 · The design and the ontology footer said a sweep never retires and claims in the same region; an empty stale host does both. | Fixed: both qualify the rule to current hosts and state the stale exception. |
| The `zip` 9 bump: no defect found. | — |

## Independent review round 4 (same reviewer; round-3 repairs listed for verification)

Verdict: changes requested.

| Finding | Disposition |
|---|---|
| P1 · An exhausted floor claim past its window stayed alive while any Start waited (`drive_claim` found it needed by demand but could not retry it), holding a resource against the `quota + 1` bound the Start needed. | Fixed: a floor claim past its window with no cloud instance is terminated regardless of demand; a Start's exhausted claim remains that Start's evidence. Test: `an_expired_floor_claim_frees_the_bound_for_a_waiting_start`. |
| P2 · The region-wide presence clear in `remove_host` locked rows in a different order from the sweep's `supply` (host id order under the admission lock), so a concurrent removal and sweep could deadlock. | Fixed: `remove_host` takes the organization admission lock before its first host row (the sweep already holds it; the route now serializes with sweeps, placement and rotation). Test: `an_operator_removal_waits_for_an_in_flight_capacity_decision`. |

## Independent review round 5 (same reviewer; round-4 repairs listed for verification)

Verdict: changes requested, one finding.

| Finding | Disposition |
|---|---|
| P1 · A floor claim awaiting launch or retry survived an operator's removal of the last established host, because recovery exempted floor claims from the presence rule (the round-1 shape, made redundant by the round-3 marker). | Fixed: recovery requires presence for every claim; the marker set by automatic retirement keeps the stale-retirement case working. Test: `a_pending_floor_claim_is_abandoned_after_the_operator_removes_the_last_host`. |

## Independent review round 6 (same reviewer; round-5 repair listed for verification)

Verdict: **APPROVE**. One P2: the ontology footer still said presence gates only creation; it now states that presence (an established host or the automatic-retirement marker) gates creation and recovery, and that an operator's removal clears the marker. No defect in the migration, settings or `zip` change.

## CI

The first two CI pairs on this PR failed outside the change: the shared `build-ci-image` step (`network mode "cloudbuild" not supported by buildkit`, fixed by PR #597) and then `rust-deny` on a yanked crate: every `zip 0.6.x` release was yanked on 2026-10-16. This branch bumped `zip` to 9 to get past it; main replaced the dependency the same day (`a78e6805f`, merged with PR #597), so the merge of main takes main's version of those six files and this branch carries no `zip` change of its own. `cargo deny check advisories` passes locally.
