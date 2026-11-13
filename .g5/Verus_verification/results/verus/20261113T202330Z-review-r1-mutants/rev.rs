//! Reviewer harness (stage C round 1): differential evaluation of variants against the as-built.
use ipm_load_history::contract::*;
use ipm_load_history::explore::*;
use ipm_load_history::model::*;
use std::collections::{BTreeMap, BTreeSet};

fn all_states(c: &ExecCfg) -> Vec<(String, ExecState)> {
    let mut s = exec_init(c);
    let mut out = vec![("init".to_string(), s.clone())];
    let mut n = 0;
    while exec_running(&s) && n < 1000 {
        out.push((format!("kill@{n}"), exec_kill(c, &s)));
        out.push((format!("terminate@{n}"), exec_terminate(c, &s)));
        s = exec_step(c, &s);
        n += 1;
        out.push((format!("step@{n}"), s.clone()));
    }
    if c.caller_tx {
        out.push(("caller-commit".into(), exec_caller_end(c, &s, true)));
        out.push(("caller-rollback".into(), exec_caller_end(c, &s, false)));
    }
    out
}

fn endk(label: &str) -> &str {
    if label.starts_with("kill") { "kill" } else if label.starts_with("terminate") { "terminate" } else if label.starts_with("caller") { label } else { "normal" }
}

fn labels(c: &ExecCfg, q: &ExecQ) -> BTreeSet<String> {
    let mut v = BTreeSet::new();
    for (l, s) in all_states(c) {
        let k = endk(&l);
        for cl in violated_clauses(c, q, &s) { v.insert(format!("{cl}@{k}")); }
        for (ch, name) in [(Check::CommitSoundTop, "COMMIT-SOUND"), (Check::CommitExactTop, "COMMIT-EXACT"), (Check::R3Top, "R3-TOP"),
                           (Check::R3Dep, "R3-DEP"), (Check::TLevel, "TLEVEL"), (Check::PersistTarget, "PERSIST-TARGET"), (Check::IsOkRedundant, "ISOK-REDUNDANT")] {
            if !check_state(c, q, ch, &s) { v.insert(name.to_string()); }
        }
    }
    v
}

fn main() {
    let q = exec_qprov();
    let mut q05 = q.clone(); q05.q05_all = true;
    let base = domain(&[0, 1, 2], &[false, true], 0);
    let base_l: Vec<BTreeSet<String>> = base.iter().map(|c| labels(c, &q)).collect();
    let base_l05: Vec<BTreeSet<String>> = base.iter().map(|c| labels(c, &q05)).collect();
    // The authors' L1 control domain: configurations whose every end state passes L1-END-NORMAL (resp. L1-NS-ALL) as-built.
    let conf = |ch: Check| -> Vec<bool> { base.iter().map(|c| end_states(c).iter().all(|e| check_state(c, &q, ch, e))).collect() };
    let conf_normal = conf(Check::L1EndNormal);
    let conf_ns = conf(Check::L1NsAll);
    println!("domain {} configs; as-built conforming: L1-END-NORMAL {}, L1-NS-ALL {}", base.len(),
        conf_normal.iter().filter(|b| **b).count(), conf_ns.iter().filter(|b| **b).count());
    for var in [201u8, 202, 203, 204, 205, 206] {
        let cfgs = domain(&[0, 1, 2], &[false, true], var);
        let mut new_by: BTreeMap<String, usize> = BTreeMap::new();
        let mut lost_by: BTreeMap<String, usize> = BTreeMap::new();
        let mut new05: BTreeMap<String, usize> = BTreeMap::new();
        let mut ex: BTreeMap<String, String> = BTreeMap::new();
        let mut differs = 0usize;
        let (mut det_normal, mut det_ns) = (0usize, 0usize);
        for (k, c) in cfgs.iter().enumerate() {
            let l = labels(c, &q);
            let l05 = labels(c, &q05);
            let mut d = false;
            for x in l.difference(&base_l[k]) { *new_by.entry(x.clone()).or_insert(0) += 1; ex.entry(x.clone()).or_insert(format!("{:?}", c)); d = true; }
            for x in base_l[k].difference(&l) { *lost_by.entry(x.clone()).or_insert(0) += 1; d = true; }
            for x in l05.difference(&base_l05[k]) { *new05.entry(x.clone()).or_insert(0) += 1; }
            if d { differs += 1; }
            if conf_normal[k] && !end_states(c).iter().all(|e| check_state(c, &q, Check::L1EndNormal, e)) { det_normal += 1; }
            if conf_ns[k] && !end_states(c).iter().all(|e| check_state(c, &q, Check::L1NsAll, e)) { det_ns += 1; }
            // any observable difference in end states at all?
        }
        let mut obs_diff = 0usize;
        for (k, c) in cfgs.iter().enumerate() {
            let a: Vec<String> = all_states(&base[k]).iter().filter(|(_, s)| exec_end_kind(s) != 0).map(|(_, s)| obs_json(&base[k], s)).collect();
            let b: Vec<String> = all_states(c).iter().filter(|(_, s)| exec_end_kind(s) != 0).map(|(_, s)| obs_json(c, s)).collect();
            if a != b { obs_diff += 1; }
        }
        println!("\n=== variant {var}: configs with any end-observation difference {obs_diff}; configs whose violated-clause set differs {differs}");
        println!("  authors' control method: detected by L1-END-NORMAL on its conforming domain in {det_normal} configs; by L1-NS-ALL on its conforming domain in {det_ns} configs");
        println!("  new violations vs as-built (provisional Q): {:?}", new_by);
        println!("  violations removed vs as-built: {:?}", lost_by);
        println!("  new violations vs as-built with Q-05 alternative (Committed exact for every action): {:?}", new05);
        for (k, v) in ex.iter().take(4) { println!("  example {k}: {v}"); }
    }
    // Variant 206 (false premise): predicted observation of B1-C42 (caller tstart, load, caller trollback).
    for var in [0u8, 206] {
        let mut c = base_cfg(); c.caller_tx = true; c.var = var;
        let (s, _) = run_case(&c, End::CallerRollback).unwrap();
        println!("\nB1-C42 under variant {var}: {}", obs_json(&c, &s));
    }
    // Variant 203 (dependency rows outside the transaction): B1-C31 (parent Activate fails after its dependency loaded).
    for var in [0u8, 203] {
        let mut c = base_cfg(); c.deps = vec![pkg(1)]; c.top.act = 1; c.var = var;
        let (s, _) = run_case(&c, End::Normal).unwrap();
        println!("\nB1-C31 under variant {var}: {}\n  violated (provisional Q): {:?}\n  violated (Q-05 alternative): {:?}", obs_json(&c, &s),
            violated_clauses(&c, &q, &s), violated_clauses(&c, &q05, &s));
    }
}
