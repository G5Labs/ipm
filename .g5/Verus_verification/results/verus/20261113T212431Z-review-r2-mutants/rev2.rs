//! Stage C round 2 reviewer harness (scratch copy only, not part of the reviewed crate).
//! For each reviewer variant, evaluates every end state of every configuration in the full
//! single-fault domain (3 transaction modes, with and without a caller transaction) and compares
//! the L1 verdict and the named properties with the as-built (variant 0) on the same
//! configuration. End states come from `end_states` (normal end, kill-like and terminate-like
//! ends after every step, caller commit and rollback), which is the full reachable end-state set
//! of the deterministic machine without foreign edits.
use ipm_load_history::contract::*;
use ipm_load_history::explore::*;
use ipm_load_history::model::*;
use std::collections::BTreeMap;

fn tlevel_ok(s: &ExecState) -> bool {
    let a = !(s.alive && s.top_pc == 3 && s.lnm0.pc == 32) || s.tx.len() == s.lnm0.itl as usize;
    let b = !(s.alive && s.top_pc == 3 && s.lnm0.pc == 11 && s.lmr_pc == 5 && s.lnm1.pc == 32) || s.tx.len() == s.lnm1.itl as usize;
    a && b
}

/// All states visited on the normal path (for the TLEVEL check, which is not an end-state check).
fn path_states(c: &ExecCfg) -> Vec<ExecState> {
    let mut s = exec_init(c);
    let mut out = vec![s.clone()];
    let mut n = 0;
    while exec_running(&s) && n < 1000 {
        s = exec_step(c, &s);
        out.push(s.clone());
        n += 1;
    }
    out
}

fn props(c: &ExecCfg, q: &ExecQ) -> BTreeMap<String, bool> {
    let mut m = BTreeMap::new();
    let ends = end_states(c);
    let mut clauses: BTreeMap<String, bool> = BTreeMap::new();
    for e in &ends {
        for v in violated_clauses(c, q, e) {
            let k = if exec_end_kind(e) == 2 { format!("{v}@process-end") } else if exec_end_kind(e) == 4 { format!("{v}@caller-rollback") } else { v };
            clauses.insert(k, true);
        }
    }
    for (k, _) in clauses { m.insert(format!("L1:{k}"), false); }
    let cs = ends.iter().all(|e| check_state(c, q, Check::CommitSoundTop, e));
    let ce = ends.iter().all(|e| check_state(c, q, Check::CommitExactTop, e));
    let tl = path_states(c).iter().all(|s| tlevel_ok(s));
    if !cs { m.insert("COMMIT-SOUND".into(), false); }
    if !ce { m.insert("COMMIT-EXACT".into(), false); }
    if !tl { m.insert("TLEVEL".into(), false); }
    m
}

fn label(c: &ExecCfg) -> String {
    format!("txm={} devf={} caller_tx={} init0={{present:{},ver:{},dev:{}}} deps={} top{{ver:{},upd:{},sysreq:{},act:{},mf:{}}} dep0{{act:{},mf:{},found:{},sysreq:{},snap:{}}} path_ok={} cleanup_fails={} force={}",
        c.txm, c.devf, c.caller_tx, c.init[0].present, c.init[0].ver, c.init[0].dev, c.deps.len(),
        c.top.ver, c.top.upd, c.top.sysreq, c.top.act, c.top.mf,
        c.deps.get(0).map(|d| d.act).unwrap_or(0), c.deps.get(0).map(|d| d.mf).unwrap_or(0), c.deps.get(0).map(|d| d.found).unwrap_or(true),
        c.deps.get(0).map(|d| d.sysreq).unwrap_or(true), c.deps.get(0).map(|d| d.snap).unwrap_or(false), c.path_ok, c.cleanup_fails, c.force)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let vars: Vec<u8> = args[1..].iter().map(|a| a.parse().unwrap()).collect();
    let q = qprov_x();
    let base = domain(&[0, 1, 2], &[false, true], 0);
    println!("domain configurations: {}", base.len());
    // valid completion: default mode, one dependency, no fault, nothing installed
    let mut valid = base_cfg();
    valid.deps = vec![pkg(1)];
    for &var in std::iter::once(&0u8).chain(vars.iter()) {
        let mut c = valid.clone();
        c.var = var;
        let (s, n) = run_to_end(&c);
        println!("valid completion var={var}: steps={n} l1_ok={} violated={:?} obs={}", exec_l1_ok(&c, &q, &s), violated_clauses(&c, &q, &s), obs_json(&c, &s));
    }
    let base_props: Vec<BTreeMap<String, bool>> = base.iter().map(|c| props(c, &q)).collect();
    for &var in &vars {
        let mut newly: BTreeMap<String, usize> = BTreeMap::new();
        let mut masked: BTreeMap<String, usize> = BTreeMap::new();
        let mut changed_cfgs = 0usize;
        let mut example: BTreeMap<String, String> = BTreeMap::new();
        let mut behaviour_changed = 0usize;
        for (i, c0) in base.iter().enumerate() {
            let mut c = c0.clone();
            c.var = var;
            let (s0, _) = run_to_end(c0);
            let (s1, _) = run_to_end(&c);
            let ends0 = end_states(c0);
            let ends1 = end_states(&c);
            if ends0 != ends1 || s0 != s1 { behaviour_changed += 1; }
            let p1 = props(&c, &q);
            let p0 = &base_props[i];
            let mut diff = false;
            for k in p1.keys() {
                if !p0.contains_key(k) {
                    *newly.entry(k.clone()).or_default() += 1;
                    example.entry(format!("new {k}")).or_insert_with(|| label(c0));
                    diff = true;
                }
            }
            for k in p0.keys() {
                if !p1.contains_key(k) {
                    *masked.entry(k.clone()).or_default() += 1;
                    example.entry(format!("masked {k}")).or_insert_with(|| label(c0));
                    diff = true;
                }
            }
            if diff { changed_cfgs += 1; }
        }
        println!("== variant {var}: configurations whose behaviour (any end state) differs from as-built: {behaviour_changed}; configurations whose verdict set differs: {changed_cfgs}");
        println!("   newly violated (property: configurations): {:?}", newly);
        println!("   masked (violated as-built, holds under the variant): {:?}", masked);
        for (k, v) in &example { println!("   example {k}: {v}"); }
    }
}
