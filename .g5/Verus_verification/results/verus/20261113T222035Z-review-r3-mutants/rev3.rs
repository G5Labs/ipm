//! Round 3 reviewer harness (scratch copy only). Compares L1 verdicts of mutants with their base.
use ipm_load_history::contract::*;
use ipm_load_history::explore::*;
use ipm_load_history::model::*;
use std::collections::BTreeMap;

fn ends(c: &ExecCfg) -> Vec<ExecState> { end_states(c) }

fn main() {
    let q = qprov_x();
    let args: Vec<String> = std::env::args().collect();
    let base: u8 = args[1].parse().unwrap();
    let mutv: u8 = args[2].parse().unwrap();
    let txms: Vec<u8> = args[3].split(',').map(|x| x.parse().unwrap()).collect();
    let base_cfgs = domain(&txms, &[false, true], base);
    let mut_cfgs = domain(&txms, &[false, true], mutv);
    let mut behave_diff = 0usize;
    let mut verdict_diff_cfgs = 0usize;
    let mut new_clauses: BTreeMap<String, usize> = BTreeMap::new();
    let mut gone_clauses: BTreeMap<String, usize> = BTreeMap::new();
    let mut dep_repair_base = 0usize;
    let mut dep_repair_mut = 0usize;
    let mut dep_repair_examples: Vec<String> = vec![];
    let mut ns_detect = 0usize;
    for (cb, cm) in base_cfgs.iter().zip(mut_cfgs.iter()) {
        let eb = ends(cb);
        let em = ends(cm);
        let differs = eb.len() != em.len() || eb.iter().zip(em.iter()).any(|(a, b)| a.ns != b.ns || a.top_err != b.top_err || a.tx.len() != b.tx.len());
        if differs { behave_diff += 1; }
        let mut vd = false;
        // Non-death ends are the last 1 (no caller tx) or 3 (caller tx) entries and align by kind.
        let nb = if cb.caller_tx { 3 } else { 1 };
        let tb: Vec<&ExecState> = eb.iter().rev().take(nb).collect();
        let tm: Vec<&ExecState> = em.iter().rev().take(nb).collect();
        for (a, b) in tb.iter().zip(tm.iter()) {
            let va = violated_clauses(cb, &q, a);
            let vb = violated_clauses(cm, &q, b);
            if va != vb { vd = true; }
            for x in vb.iter().filter(|x| !va.contains(x)) {
                *new_clauses.entry(format!("{} | {} | end {}", x, verdict_str(exec_l1_verdict(cm, &q)), exec_end_kind(b))).or_insert(0) += 1;
                if x == "L1-NAMESPACE" { ns_detect += 1; }
            }
            for x in va.iter().filter(|x| !vb.contains(x)) {
                *gone_clauses.entry(format!("{} | {} | end {}", x, verdict_str(exec_l1_verdict(cb, &q)), exec_end_kind(a))).or_insert(0) += 1;
            }
        }
        // Death ends: compare the set of clauses violated at any death of this configuration.
        let mut db: Vec<String> = vec![];
        let mut dm: Vec<String> = vec![];
        for a in eb.iter().filter(|e| exec_end_kind(e) == 2) { for x in violated_clauses(cb, &q, a) { if !db.contains(&x) { db.push(x); } } }
        for b in em.iter().filter(|e| exec_end_kind(e) == 2) { for x in violated_clauses(cm, &q, b) { if !dm.contains(&x) { dm.push(x); } } }
        for x in dm.iter().filter(|x| !db.contains(x)) {
            vd = true;
            *new_clauses.entry(format!("{} | {} | some death (configs)", x, verdict_str(exec_l1_verdict(cm, &q)))).or_insert(0) += 1;
            if x == "L1-NAMESPACE" { ns_detect += 1; }
        }
        for x in db.iter().filter(|x| !dm.contains(x)) {
            vd = true;
            *gone_clauses.entry(format!("{} | {} | some death (configs)", x, verdict_str(exec_l1_verdict(cb, &q)))).or_insert(0) += 1;
        }
        for (a, b) in eb.iter().zip(em.iter()) {
            if !check_state(cb, &q, Check::DepRepair, a) { dep_repair_base += 1; }
        }
        for b in em.iter() {
            if !check_state(cm, &q, Check::DepRepair, b) {
                dep_repair_mut += 1;
                if dep_repair_examples.len() < 1 && exec_end_kind(b) == 2 && b.how == 1 {
                    dep_repair_examples.push(format!("cfg txm={} devf={} caller={} deps={} top.act={} dep0.act={} how={}\n   mutant obs: {}\n   L1: {}",
                        cm.txm, cm.devf, cm.caller_tx, cm.deps.len(), cm.top.act, cm.deps.get(0).map(|d| d.act).unwrap_or(9), b.how,
                        obs_json(cm, b), expect_json(cm, &exec_expect_for(cm, &q, b))));
                }
            }
        }
        if vd { verdict_diff_cfgs += 1; }
    }
    println!("base var {} vs mutant var {} over txms {:?}: {} configs; {} configs behave differently; {} configs with any L1 clause verdict change; L1-NAMESPACE newly violated at {} end states",
        base, mutv, txms, base_cfgs.len(), behave_diff, verdict_diff_cfgs, ns_detect);
    println!("clauses newly violated by the mutant:");
    for (k, v) in &new_clauses { println!("  {:6} {}", v, k); }
    println!("clauses no longer violated under the mutant:");
    for (k, v) in &gone_clauses { println!("  {:6} {}", v, k); }
    println!("DEP-REPAIR violations (end states): base {} mutant {}", dep_repair_base, dep_repair_mut);
    for e in &dep_repair_examples { println!(" example: {}", e); }
    // valid completion: default mode, one dependency, no fault
    let mut c = base_cfg(); c.deps = vec![pkg(1)]; c.var = mutv;
    let (s, _) = run_to_end(&c);
    println!("valid completion (default mode, one dependency, no fault) under var {}: violated clauses {:?}; obs {}", mutv, violated_clauses(&c, &q, &s), obs_json(&c, &s));
}
