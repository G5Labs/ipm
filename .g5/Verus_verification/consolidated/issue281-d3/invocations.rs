// #281 D3+: verifier state across concurrent checks of one repository on one trust server.
//
// Source roles (bindings in .g5/uplift/consolidated/issue281-d3/bindings.json):
//   Repo/Oras/ContentTrust.cls::LookUpSignedTargets   takes the instance-wide lock for (trust server, repository),
//                                                      runs notary in a new private trust directory and releases
//   Repo/Oras/ContentTrust.cls::SeedTrustDirectory     copies the pinned metadata except timestamp.json into it
//   Repo/Oras/ContentTrust.cls::KeepTrustData          after a successful run, replaces the pinned metadata with the
//                                                      verified metadata except timestamp.json
//   Repo/Oras/ContentTrust.cls::TrustDirectory         the pin store under <manager directory>/ipm/content-trust/
//
// Premises on notary (the reference client, not verified here; notary client/tufclient.go downloadTimestamp):
//   - it reads the timestamp cached in its own trust directory, then asks the server for a current one;
//   - a successful download writes the new timestamp into that directory;
//   - when the server cannot be reached it falls back to the cached timestamp if there is one;
//   - it refuses trust data whose root version is older than the root it started from.
// The #281 iteration70 model supplied `cached_timestamp = false` as a constant, which a concurrent writer of the
// shared directory can falsify. Here the cached timestamp is read from the state at the moment notary samples it.
use vstd::prelude::*;

verus! {

pub enum Phase { Idle, Started, Sampled, Done, Finished }

// One verification: what notary saw in its directory, whether it reached the server and what it decided.
pub struct Check {
    pub phase: Phase,
    pub cached_seen: bool,
    pub fetched: bool,
    pub accepted: bool,
    pub read_version: nat,
    pub result_version: nat,
}

// dir_ts[d] is whether trust directory d holds a timestamp. Directory 0 is the pinned trust data.
pub struct St { pub checks: Seq<Check>, pub dir_ts: Seq<bool>, pub lock: Option<nat>, pub pin_root: nat }

// D2 ran notary in the pin store itself after deleting its timestamp, without a lock.
// D3+ runs each check in a private directory under a lock.
pub struct Design { pub private_dirs: bool, pub locked: bool }
pub open spec fn d2() -> Design { Design { private_dirs: false, locked: false } }
pub open spec fn d3() -> Design { Design { private_dirs: true, locked: true } }

pub open spec fn dir(d: Design, i: nat) -> nat { if d.private_dirs { i + 1 } else { 0 } }

pub enum Ev { Start(nat), Sample(nat), Fetch(nat, bool, nat), Finish(nat) }

pub open spec fn wf(s: St) -> bool { s.dir_ts.len() == s.checks.len() + 1 }

pub open spec fn may_start(d: Design, s: St, i: nat) -> bool {
    i < s.checks.len() && s.checks[i as int].phase is Idle && (!d.locked || s.lock is None)
}

pub open spec fn step(d: Design, s: St, e: Ev) -> St {
    match e {
        // D2 deletes the timestamp in the pin store. D3+ creates a directory seeded with the pins, whose timestamp
        // the pin store never holds. Either way the check's directory starts without a timestamp.
        Ev::Start(i) => if may_start(d, s, i) {
            St {
                checks: s.checks.update(i as int, Check { phase: Phase::Started, read_version: s.pin_root, ..s.checks[i as int] }),
                dir_ts: s.dir_ts.update(dir(d, i) as int, false),
                lock: if d.locked { Some(i) } else { s.lock },
                ..s
            }
        } else { s },
        Ev::Sample(i) => if i < s.checks.len() && s.checks[i as int].phase is Started {
            St {
                checks: s.checks.update(i as int, Check {
                    phase: Phase::Sampled, cached_seen: s.dir_ts[dir(d, i) as int], ..s.checks[i as int] }),
                ..s
            }
        } else { s },
        Ev::Fetch(i, reachable, version) => if i < s.checks.len() && s.checks[i as int].phase is Sampled {
            let c = s.checks[i as int];
            if reachable {
                St {
                    checks: s.checks.update(i as int, Check {
                        phase: Phase::Done, fetched: true, accepted: version >= c.read_version, result_version: version, ..c }),
                    dir_ts: s.dir_ts.update(dir(d, i) as int, true),
                    ..s
                }
            } else {
                St {
                    checks: s.checks.update(i as int, Check {
                        phase: Phase::Done, fetched: false, accepted: c.cached_seen, result_version: c.read_version, ..c }),
                    ..s
                }
            }
        } else { s },
        // Only an accepted check updates the pins. KeepTrustData leaves out timestamp.json.
        Ev::Finish(i) => if i < s.checks.len() && s.checks[i as int].phase is Done {
            let c = s.checks[i as int];
            St {
                checks: s.checks.update(i as int, Check { phase: Phase::Finished, ..c }),
                pin_root: if c.accepted { c.result_version } else { s.pin_root },
                lock: if d.locked { None } else { s.lock },
                ..s
            }
        } else { s },
    }
}

pub open spec fn run(d: Design, s: St, evs: Seq<Ev>) -> St
    decreases evs.len()
{
    if evs.len() == 0 { s } else { run(d, step(d, s, evs[0]), evs.subrange(1, evs.len() as int)) }
}

pub open spec fn active(p: Phase) -> bool { p is Started || p is Sampled || p is Done }

pub open spec fn initial(s: St) -> bool {
    &&& wf(s)
    &&& s.lock is None
    &&& forall|i: int| 0 <= i < s.checks.len() ==> #[trigger] s.checks[i].phase is Idle
}

// What D3+ maintains: a check's own directory has no timestamp until it fetches one, so a check that accepts
// fetched; the lock holder is the one active check and the pins have not changed since it read them.
pub open spec fn inv(s: St) -> bool {
    &&& wf(s)
    &&& forall|i: int| 0 <= i < s.checks.len() && (#[trigger] s.checks[i]).phase is Started ==> !s.dir_ts[i + 1]
    &&& forall|i: int| 0 <= i < s.checks.len() && !((#[trigger] s.checks[i]).phase is Idle) && !(s.checks[i].phase is Started)
            ==> !s.checks[i].cached_seen
    &&& forall|i: int| 0 <= i < s.checks.len() && ((#[trigger] s.checks[i]).phase is Done || s.checks[i].phase is Finished)
            && s.checks[i].accepted ==> s.checks[i].fetched && s.checks[i].result_version >= s.checks[i].read_version
    &&& forall|i: int| 0 <= i < s.checks.len() && active((#[trigger] s.checks[i]).phase)
            ==> s.lock == Some(i as nat) && s.checks[i].read_version == s.pin_root
    &&& match s.lock { Some(h) => h < s.checks.len() && active(s.checks[h as int].phase), None => true }
}

pub proof fn initial_inv(s: St)
    requires initial(s)
    ensures inv(s)
{}

pub proof fn step_inv(s: St, e: Ev)
    requires inv(s)
    ensures
        inv(step(d3(), s, e)),
        step(d3(), s, e).pin_root >= s.pin_root,
        step(d3(), s, e).checks.len() == s.checks.len(),
{
    let t = step(d3(), s, e);
    match e {
        Ev::Start(i) => {
            if may_start(d3(), s, i) {
                assert forall|j: int| 0 <= j < t.checks.len() && active((#[trigger] t.checks[j]).phase)
                    implies t.lock == Some(j as nat) && t.checks[j].read_version == t.pin_root by {
                    if j != i as int { assert(s.checks[j] == t.checks[j]); }
                }
            }
        },
        Ev::Sample(i) => {},
        Ev::Fetch(i, reachable, version) => {},
        Ev::Finish(i) => {
            if i < s.checks.len() && s.checks[i as int].phase is Done {
                assert(active(s.checks[i as int].phase));
                assert forall|j: int| 0 <= j < t.checks.len() && active((#[trigger] t.checks[j]).phase)
                    implies t.lock == Some(j as nat) && t.checks[j].read_version == t.pin_root by {
                    if j != i as int { assert(s.checks[j] == t.checks[j]); }
                }
            }
        },
    }
}

pub proof fn run_inv(s: St, evs: Seq<Ev>)
    requires inv(s)
    ensures
        inv(run(d3(), s, evs)),
        run(d3(), s, evs).pin_root >= s.pin_root,
        run(d3(), s, evs).checks.len() == s.checks.len(),
    decreases evs.len()
{
    if evs.len() > 0 {
        step_inv(s, evs[0]);
        run_inv(step(d3(), s, evs[0]), evs.subrange(1, evs.len() as int));
    }
}

// Whatever other checks of the same repository do meanwhile, a check that accepts reached the trust server itself,
// and the pinned root never moves back.
pub proof fn accepted_check_fetched_itself(s: St, evs: Seq<Ev>, i: int)
    requires initial(s), 0 <= i < s.checks.len()
    ensures
        ({
            let t = run(d3(), s, evs);
            &&& (t.checks[i].phase is Done || t.checks[i].phase is Finished) && t.checks[i].accepted ==> t.checks[i].fetched
            &&& t.pin_root >= s.pin_root
        }),
{
    initial_inv(s);
    run_inv(s, evs);
}

// ---- The failing designs and their witnesses ----

pub open spec fn idle() -> Check {
    Check { phase: Phase::Idle, cached_seen: false, fetched: false, accepted: false, read_version: 0, result_version: 0 }
}
pub open spec fn s0() -> St { St { checks: seq![idle(), idle()], dir_ts: seq![false, false, false], lock: None, pin_root: 1 } }

// Check 1 removes the timestamp, check 0 then downloads one into the same directory, and check 1's request fails
// with the server down: notary falls back to check 0's timestamp and accepts.
pub open spec fn overlap() -> Seq<Ev> {
    seq![Ev::Start(1), Ev::Start(0), Ev::Sample(0), Ev::Fetch(0, true, 1), Ev::Sample(1), Ev::Fetch(1, false, 0)]
}

pub proof fn shared_directory_accepts_without_reaching_server()
    ensures
        run(d2(), s0(), overlap()).checks[1].accepted,
        !run(d2(), s0(), overlap()).checks[1].fetched,
        !run(d3(), s0(), overlap()).checks[1].accepted,
{
    reveal_with_fuel(run, 7);
}

// Without the lock, a check that read the pins before another check stored a rotated root writes back the older
// root it accepted.
pub open spec fn crossing_writes() -> Seq<Ev> {
    seq![Ev::Start(0), Ev::Start(1), Ev::Sample(0), Ev::Sample(1), Ev::Fetch(0, true, 2), Ev::Finish(0),
         Ev::Fetch(1, true, 1), Ev::Finish(1)]
}

pub proof fn unlocked_pins_move_back()
    ensures
        run(Design { private_dirs: true, locked: false }, s0(), crossing_writes().subrange(0, 6)).pin_root == 2,
        run(Design { private_dirs: true, locked: false }, s0(), crossing_writes()).pin_root == 1,
        run(d3(), s0(), crossing_writes()).pin_root == 2,
{
    reveal_with_fuel(run, 9);
    assert(crossing_writes().subrange(0, 6) =~= seq![Ev::Start(0), Ev::Start(1), Ev::Sample(0), Ev::Sample(1),
        Ev::Fetch(0, true, 2), Ev::Finish(0)]);
}

} // verus!

fn main() {}
