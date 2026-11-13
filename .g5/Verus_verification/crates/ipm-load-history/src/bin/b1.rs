//! Driver for the explorer runs, the domain survey and the native replay cases.
//!
//!   b1 explore <run-name>     run one named explorer configuration, print a JSON summary line
//!   b1 explore-list           list the named runs
//!   b1 survey <out.json>      evaluate L1 on every end state of every configuration in the domain
//!   b1 cases <out.json>       write the curated native replay cases
use ipm_load_history::contract::*;
use ipm_load_history::explore::*;
use ipm_load_history::model::*;
use explorer::{explore, Limits};
use std::collections::BTreeMap;
use std::io::Write;

struct Run {
    name: &'static str,
    kind: &'static str,
    variant: &'static str,
    expect: &'static str,
    concern: &'static str,
    txms: Vec<u8>,
    callers: Vec<bool>,
    var: u8,
    checks: Vec<Check>,
    edits: bool,
    q: ExecQ,
    /// Restrict to configurations where the as-built conforms to L1 (for L1-END controls).
    conforming: bool,
}

fn runs() -> Vec<Run> {
    let p = qprov_x();
    let inv_ok = vec![Check::R1RowKept, Check::R2FinalUnchanged, Check::R3Top, Check::CommitSoundTop, Check::TLevel, Check::PersistTarget, Check::IsOkRedundant];
    let mut v = vec![];
    for (t, n) in [(0u8, "default"), (1, "notx"), (2, "nojournal")] {
        v.push(Run { name: Box::leak(format!("asbuilt-invariants-{n}").into_boxed_str()), kind: "explore", variant: "as-built", expect: "pass",
            concern: "R1/R2/R3-top/COMMIT-SOUND/TLEVEL/PERSIST-TARGET hold as-built",
            txms: vec![t], callers: vec![false, true], var: 0, checks: inv_ok.clone(), edits: false, q: p.clone(), conforming: false });
    }
    v.push(Run { name: "asbuilt-r2-with-foreign-edits", kind: "explore", variant: "as-built", expect: "pass",
        concern: "R2 finalized rows immutable under foreign edits (trigger)", txms: vec![0, 1], callers: vec![false], var: 0,
        checks: vec![Check::R1RowKept, Check::R2FinalUnchanged], edits: true, q: p.clone(), conforming: false });
    v.push(Run { name: "asbuilt-r3-dep", kind: "explore", variant: "as-built", expect: "violation",
        concern: "R3-dep: a started dependency load has a durable History row (Q-06 provisional)", txms: vec![0], callers: vec![false], var: 0,
        checks: vec![Check::R3Dep], edits: false, q: p.clone(), conforming: false });
    v.push(Run { name: "asbuilt-l1-end", kind: "explore", variant: "as-built", expect: "violation",
        concern: "L1 end-state contract (provisional Q)", txms: vec![0, 1, 2], callers: vec![false], var: 0,
        checks: vec![Check::L1End], edits: false, q: p.clone(), conforming: false });
    v.push(Run { name: "asbuilt-l1-normal-conforming-domain", kind: "explore", variant: "as-built", expect: "pass",
        concern: "baseline for L1 controls: the as-built passes L1-END-NORMAL on its conforming configurations", txms: vec![0, 1, 2], callers: vec![false, true], var: 0,
        checks: vec![Check::L1EndNormal], edits: false, q: p.clone(), conforming: true });
    v.push(Run { name: "asbuilt-l1-ns-all-conforming-domain", kind: "explore", variant: "as-built", expect: "pass",
        concern: "baseline for the process-end control: the as-built passes L1-NS-ALL on its conforming configurations", txms: vec![0, 1, 2], callers: vec![false, true], var: 0,
        checks: vec![Check::L1NsAll], edits: false, q: p.clone(), conforming: true });
    v.push(Run { name: "repair-103-dep-clauses", kind: "explore", variant: "repair-103", expect: "pass",
        concern: "D1 repair (dependency rows outside the parent's transaction, Committed when the dependency's writes become durable): dependency record clauses, transactional modes (default and -dev), with and without caller transaction", txms: vec![0], callers: vec![false, true], var: 103,
        checks: vec![Check::DepRepair], edits: false, q: p.clone(), conforming: false });
    v.push(Run { name: "asbuilt-dep-clauses", kind: "explore", variant: "as-built", expect: "violation",
        concern: "as-built baseline for the D1 repair check (transactional modes)", txms: vec![0], callers: vec![false, true], var: 0,
        checks: vec![Check::DepRepair], edits: false, q: p.clone(), conforming: false });
    v.push(Run { name: "repair-103-invariants", kind: "explore", variant: "repair-103", expect: "pass",
        concern: "D1 repair keeps R1, R2, R3-top, COMMIT-SOUND, TLEVEL, PERSIST-TARGET", txms: vec![0, 1, 2], callers: vec![false, true], var: 103,
        checks: vec![Check::R1RowKept, Check::R2FinalUnchanged, Check::R3Top, Check::CommitSoundTop, Check::TLevel, Check::PersistTarget], edits: false, q: p.clone(), conforming: false });
    v.push(Run { name: "asbuilt-commit-exact", kind: "explore", variant: "as-built", expect: "violation",
        concern: "COMMIT-EXACT: Committed=1 implies something kept (top-level)", txms: vec![0, 1, 2], callers: vec![false], var: 0,
        checks: vec![Check::CommitExactTop], edits: false, q: p.clone(), conforming: false });
    // controls: one guard or effect removed; the named property must detect it
    let ctl = |name: &'static str, var: u8, concern: &'static str, txms: Vec<u8>, checks: Vec<Check>, edits: bool| Run {
        name, kind: "control", variant: name, expect: "violation", concern, txms, callers: vec![false], var, checks: checks.clone(), edits, q: qprov_x(),
        conforming: checks.iter().any(|c| matches!(c, Check::L1EndNormal | Check::L1NsAll | Check::RecFieldsNormal | Check::ComNormal)) };
    v.push(ctl("control-01-no-setcommitted-c1", 1, "SetCommitted after C1 removed; COMMIT-SOUND must fail", vec![0], vec![Check::CommitSoundTop], false));
    v.push(ctl("control-02-no-setcommitted-c3", 2, "SetCommitted after C3 removed; COMMIT-SOUND must fail", vec![0], vec![Check::CommitSoundTop], false));
    v.push(ctl("control-03-no-setcommitted-notx", 3, "SetCommitted before writes (no transaction) removed; COMMIT-SOUND must fail", vec![1, 2], vec![Check::CommitSoundTop], false));
    v.push(ctl("control-04-no-rollback-loop", 4, "trollback loop removed; TLEVEL must fail", vec![0], vec![Check::TLevel], false));
    v.push(ctl("control-04b-no-rollback-loop-l1", 4, "trollback loop removed; L1-NS-ALL must fail", vec![0], vec![Check::L1NsAll], false));
    v.push(ctl("control-05-finalize-no-isok", 5, "Finalize's `|| ISOK(status)` removed; COMMIT-SOUND checked (expected to survive: equivalent in load scope)", vec![0, 1, 2], vec![Check::CommitSoundTop], false));
    v.push(Run { name: "control-05c-finalize-no-isok-all-callers", kind: "control", variant: "control-05-finalize-no-isok", expect: "pass",
        concern: "investigation of the surviving control 05: L1-END with caller transactions too, on conforming configurations", txms: vec![0, 1, 2], callers: vec![false, true], var: 5,
        checks: vec![Check::L1EndNormal], edits: false, q: p.clone(), conforming: true });
    v.push(ctl("control-05b-finalize-no-isok-l1", 5, "Finalize's `|| ISOK(status)` removed; L1-END on success cases", vec![0, 1, 2], vec![Check::ComNormal], false));
    v.push(ctl("control-06-iristemp-rolled-back", 6, "IRISTEMP premise negated (temp rows rolled back); H-RECORD-FIELDS (per-clause domain) must fail", vec![0], vec![Check::RecFieldsNormal], false));
    v.push(ctl("control-07-no-finalized-flag", 7, "PersistToTwin does not set Finalized; H-RECORD-FIELDS (per-clause domain) must fail", vec![0], vec![Check::RecFieldsNormal], false));
    v.push(ctl("control-08-no-downgrade-guard", 8, "downgrade guard removed; L1-NS-ALL (refusal writes no module data) must fail", vec![0], vec![Check::L1NsAll], false));
    v.push(ctl("control-09-no-update-guard", 9, "UpdatePackage guard removed; L1-NS-ALL (refusal writes no module data) must fail", vec![0], vec![Check::L1NsAll], false));
    v.push(ctl("control-10-no-trigger", 10, "Finalized UPDATE trigger removed; R2 must fail", vec![0], vec![Check::R2FinalUnchanged], true));
    v.push(ctl("control-12-dev-phases-in-transaction", 12, "reviewer variant 212: -dev phases inside an inner transaction rolled back on failure; L1-NS-ALL (dev failure keeps phase work) must fail", vec![0], vec![Check::L1NsAll], false));
    v.push(ctl("control-13-notransaction-ignored", 13, "reviewer variant 15: -DNoTransaction=1 ignored (the load runs in a transaction); L1-NS-ALL (a NoTransaction failure keeps what it wrote) must fail", vec![1], vec![Check::L1NsAll], false));
    v.push(ctl("control-11-death-keeps-transaction", 11, "process death keeps the open transaction; L1-NS-ALL must fail", vec![0], vec![Check::L1NsAll], false));
    v.push(Run { name: "diagnostic-11b-death-keeps-transaction-r3top", kind: "explore", variant: "control-11", expect: "pass",
        concern: "diagnostic, not a control: R3-top cannot fail under variant 11 (the top row is inserted at $TLEVEL 0)", txms: vec![0], callers: vec![false], var: 11,
        checks: vec![Check::R3Top], edits: false, q: qprov_x(), conforming: false });
    v
}

fn json_str(s: &str) -> String { format!("\"{}\"", esc(s)) }

fn cmd_explore(name: &str) {
    let r = runs().into_iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no run {name}"));
    let cfgs = if r.conforming { conforming_domain(&r.txms, &r.callers, r.var, r.checks[0]) } else { domain(&r.txms, &r.callers, r.var) };
    let n = cfgs.len();
    let m = LoadModel { cfgs, q: r.q.clone(), checks: r.checks.clone(), edits: r.edits };
    let limits = Limits { max_states: 6_000_000, max_depth: 400, check_deadlock: false };
    let rep = explore(&m, limits);
    println!("{}", rep.summary());
    let observed = if n == 0 { "incomplete (empty domain: no configurations)" } else if rep.passed() { if rep.truncated { "incomplete" } else { "pass" } } else { "violation" };
    let mut trace_txt = String::new();
    if let Some(ce) = &rep.violation {
        let c = &m.cfgs[ce.init.ci as usize];
        trace_txt.push_str(&format!("Counterexample for {}\n config: {:?}\n", ce.property, c));
        trace_txt.push_str(&format!("  init: top_pc={} tx={}\n", ce.init.s.top_pc, ce.init.s.tx.len()));
        for (i, (a, st)) in ce.steps.iter().enumerate() {
            let s = &st.s;
            trace_txt.push_str(&format!("  {:>3}. {:?} -> top_pc={} lnm0.pc={} lmr_pc={} lnm1.pc={} tlevel={} jrn={} alive={}\n",
                i + 1, a, s.top_pc, s.lnm0.pc, s.lmr_pc, s.lnm1.pc, s.tx.len(), s.jrn, s.alive));
        }
        let last = &ce.steps.last().map(|x| x.1.s.clone()).unwrap_or(ce.init.s.clone());
        trace_txt.push_str(&format!(" final observation: {}\n", obs_json(c, last)));
        trace_txt.push_str(&format!(" L1 expectation: {}\n", expect_json(c, &exec_expect_for(c, &m.q, last))));
        trace_txt.push_str(&format!(" violated clauses: {:?}\n", violated_clauses(c, &m.q, last)));
        print!("{trace_txt}");
    }
    println!("RESULT {{\"run\": {}, \"kind\": {}, \"variant\": {}, \"concern\": {}, \"expect\": {}, \"observed\": {}, \"configs\": {}, \"states\": {}, \"transitions\": {}, \"depth\": {}, \"truncated\": {}, \"seconds\": {:.2}, \"checks\": [{}], \"txms\": {:?}, \"caller_tx\": {:?}, \"foreign_edits\": {}, \"conforming_domain_only\": {}, \"limits\": {{\"max_states\": {}, \"max_depth\": {}}}}}",
        json_str(r.name), json_str(r.kind), json_str(r.variant), json_str(r.concern), json_str(r.expect), json_str(observed), n,
        rep.states, rep.transitions, rep.depth, rep.truncated, rep.seconds,
        r.checks.iter().map(|c| json_str(check_name(*c))).collect::<Vec<_>>().join(", "), r.txms, r.callers, r.edits, r.conforming,
        limits.max_states, limits.max_depth);
}

/// Every end state of every configuration: the normal end, caller commit/rollback, and death
/// after each step.
fn cmd_survey(out: &str) {
    let q = qprov_x();
    let cfgs = domain(&[0, 1, 2], &[false, true], 0);
    let mut by_clause: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_class: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let mut ends = 0usize;
    for c in &cfgs {
        let mut s = exec_init(c);
        let mut states = vec![s.clone()];
        while exec_running(&s) { s = exec_step(c, &s); states.push(s.clone()); }
        let mut endstates = vec![("normal".to_string(), s.clone())];
        if c.caller_tx {
            endstates.push(("caller-commit".into(), exec_caller_end(c, &s, true)));
            endstates.push(("caller-rollback".into(), exec_caller_end(c, &s, false)));
        }
        for (k, st) in states.iter().enumerate() {
            if exec_running(st) {
                endstates.push((format!("kill@{k}"), exec_kill(c, st)));
                endstates.push((format!("terminate@{k}"), exec_terminate(c, st)));
            }
        }
        let mode = format!("txm={} devf={} caller_tx={}", c.txm, c.devf, c.caller_tx);
        for (label, e) in &endstates {
            ends += 1;
            let v = violated_clauses(c, &q, e);
            let endk = if label.starts_with("kill") { "kill" } else if label.starts_with("terminate") { "terminate" } else { label.as_str() };
            let verdict = verdict_str(exec_l1_verdict(c, &q));
            let key = format!("{verdict} | {endk} | {mode}");
            let ent = by_class.entry(key).or_insert((0, 0));
            ent.0 += 1;
            if !v.is_empty() {
                ent.1 += 1;
                for cl in &v {
                    let k2 = format!("{cl} | {verdict} | {endk}");
                    *by_clause.entry(k2.clone()).or_insert(0) += 1;
                    examples.entry(k2).or_insert_with(|| format!("{:?} {} => {}", c, label, obs_json(c, e)));
                }
            }
        }
    }
    let mut f = std::fs::File::create(out).unwrap();
    writeln!(f, "{{\"domain_configs\": {}, \"end_states\": {}, \"q\": \"provisional\",", cfgs.len(), ends).unwrap();
    writeln!(f, " \"violations_by_clause\": {{").unwrap();
    let n = by_clause.len();
    for (i, (k, v)) in by_clause.iter().enumerate() {
        writeln!(f, "  {}: {{\"count\": {}, \"example\": {}}}{}", json_str(k), v, json_str(&examples[k]), if i + 1 < n { "," } else { "" }).unwrap();
    }
    writeln!(f, " }},\n \"by_class\": {{").unwrap();
    let n = by_class.len();
    for (i, (k, (t, bad))) in by_class.iter().enumerate() {
        writeln!(f, "  {}: {{\"end_states\": {}, \"violating\": {}}}{}", json_str(k), t, bad, if i + 1 < n { "," } else { "" }).unwrap();
    }
    writeln!(f, " }}\n}}").unwrap();
    println!("survey: {} configs, {} end states, {} violation classes", cfgs.len(), ends, by_clause.len());
}

// ------------------------------------------------------------------ native cases

struct CaseDef {
    id: String,
    title: &'static str,
    cfg: ExecCfg,
    end: End,
    nesting: &'static str,
    fault: &'static str,
    fixture: &'static str,
    lemma: &'static str,
}

fn with<F: FnOnce(&mut ExecCfg)>(f: F) -> ExecCfg { let mut c = base_cfg(); f(&mut c); c }

fn cases() -> Vec<CaseDef> {
    use End::*;
    let simple = "module m 2.0.0 with one class resource, no dependencies (e.g. a copy of _data/simplest-module/2.0.0)";
    let withdep = "module m 2.0.0 depending on d 1.0.0; d placed under <m root>/.modules/d/ (temporary filesystem repository); both with one class resource";
    let mut v = vec![
        CaseDef { id: "B1-C01".to_string(), title: "Clean load succeeds", cfg: base_cfg(), end: Normal, nesting: "top-level", fault: "none", fixture: simple, lemma: "case_c01_conforms" },
        CaseDef { id: "B1-C02".to_string(), title: "Clean -dev load succeeds", cfg: with(|c| c.devf = 1), end: Normal, nesting: "top-level", fault: "none", fixture: simple, lemma: "" },
        CaseDef { id: "B1-C03".to_string(), title: "Clean NoTransaction load succeeds", cfg: with(|c| c.txm = 1), end: Normal, nesting: "top-level", fault: "none", fixture: simple, lemma: "" },
        CaseDef { id: "B1-C04".to_string(), title: "Clean NoJournal load succeeds", cfg: with(|c| c.txm = 2), end: Normal, nesting: "top-level", fault: "none", fixture: simple, lemma: "" },
        CaseDef { id: "B1-C05".to_string(), title: "Relative path refused", cfg: with(|c| c.path_ok = false), end: Normal, nesting: "top-level", fault: "path refusal (relative, nonexistent path)", fixture: "load some/relative/path", lemma: "" },
        CaseDef { id: "B1-C06".to_string(), title: "Unparseable manifest refused (default)", cfg: with(|c| c.top.mf = 1), end: Normal, nesting: "top-level", fault: "invalid manifest: XML not parseable", fixture: "_data/malformed-module", lemma: "" },
        CaseDef { id: "B1-C07".to_string(), title: "Unparseable manifest refused (NoTransaction)", cfg: with(|c| { c.top.mf = 1; c.txm = 1; }), end: Normal, nesting: "top-level", fault: "invalid manifest: XML not parseable", fixture: "_data/malformed-module", lemma: "" },
        CaseDef { id: "B1-C08".to_string(), title: "Manifest without a module document, NoTransaction: Committed=1 with nothing kept", cfg: with(|c| { c.top.mf = 2; c.txm = 1; }), end: Normal, nesting: "top-level", fault: "invalid manifest: parseable, no <Document name=\"*.ZPM\"> (e.g. an <Export> with no documents)", fixture: "a parseable module.xml holding no <Module> and no .ZPM document, e.g. <Export generator=\"IRIS\" version=\"26\"></Export>; load it with -DNoTransaction=1", lemma: "case_c08_committed_without_change" },
        CaseDef { id: "B1-C09".to_string(), title: "Manifest refused by ImportStream validation, NoTransaction: Committed=1 with nothing kept", cfg: with(|c| { c.top.mf = 3; c.txm = 1; }), end: Normal, nesting: "top-level", fault: "invalid manifest: module document fails %OnValidateObject (e.g. declares a resource owned by another installed module)", fixture: "install module o owning class X.Y first (outside the case), then load m declaring resource X.Y.CLS", lemma: "case_c09_committed_without_change" },
        CaseDef { id: "B1-C10".to_string(), title: "Manifest refused by ImportStream validation, NoJournal: document kept, Committed=1", cfg: with(|c| { c.top.mf = 3; c.txm = 2; }), end: Normal, nesting: "top-level", fault: "invalid manifest: module document fails validation", fixture: "as B1-C09 with -DNoJournal=1 (premise P-03: an unjournaled write is not rolled back)", lemma: "" },
        CaseDef { id: "B1-C11".to_string(), title: "Downgrade refused (default)", cfg: with(|c| { c.init[0] = installed(3, false); }), end: Normal, nesting: "top-level", fault: "downgrade", fixture: "m 3.0.0 installed, load m 2.0.0", lemma: "case_c11_downgrade_refused" },
        CaseDef { id: "B1-C12".to_string(), title: "Downgrade refused (-dev)", cfg: with(|c| { c.init[0] = installed(3, false); c.devf = 1; }), end: Normal, nesting: "top-level", fault: "downgrade", fixture: "m 3.0.0 installed, load -dev m 2.0.0", lemma: "" },
        CaseDef { id: "B1-C13".to_string(), title: "Downgrade refused (NoTransaction)", cfg: with(|c| { c.init[0] = installed(3, false); c.txm = 1; }), end: Normal, nesting: "top-level", fault: "downgrade", fixture: "m 3.0.0 installed, load m 2.0.0 -DNoTransaction=1", lemma: "" },
        CaseDef { id: "B1-C14".to_string(), title: "Downgrade refused even with -force", cfg: with(|c| { c.init[0] = installed(3, false); c.force = true; }), end: Normal, nesting: "top-level", fault: "downgrade", fixture: "m 3.0.0 installed, load -force m 2.0.0", lemma: "" },
        CaseDef { id: "B1-C15".to_string(), title: "Upgrade with UpdatePackage refused", cfg: with(|c| { c.init[0] = installed(1, false); c.top.upd = true; }), end: Normal, nesting: "top-level", fault: "update required", fixture: "m 1.0.0 installed; m 2.0.0 declares <UpdatePackage>; load m 2.0.0", lemma: "case_c15_update_refused" },
        CaseDef { id: "B1-C16".to_string(), title: "Upgrade with UpdatePackage allowed with -force", cfg: with(|c| { c.init[0] = installed(1, false); c.top.upd = true; c.force = true; }), end: Normal, nesting: "top-level", fault: "none (guard bypassed by -force)", fixture: "as B1-C15 with -force", lemma: "" },
        CaseDef { id: "B1-C17".to_string(), title: "Upgrade with UpdatePackage allowed with -dev (Q-02)", cfg: with(|c| { c.init[0] = installed(1, false); c.top.upd = true; c.devf = 1; }), end: Normal, nesting: "top-level", fault: "none (guard bypassed by -dev)", fixture: "as B1-C15 with -dev", lemma: "" },
        CaseDef { id: "B1-C18".to_string(), title: "Upgrade with UpdatePackage refused (NoTransaction)", cfg: with(|c| { c.init[0] = installed(1, false); c.top.upd = true; c.txm = 1; }), end: Normal, nesting: "top-level", fault: "update required", fixture: "as B1-C15 with -DNoTransaction=1", lemma: "" },
        CaseDef { id: "B1-C19".to_string(), title: "Same version reload allowed", cfg: with(|c| { c.init[0] = installed(2, false); c.top.upd = true; }), end: Normal, nesting: "top-level", fault: "none", fixture: "m 2.0.0 installed (declares UpdatePackage), load m 2.0.0 again", lemma: "" },
        CaseDef { id: "B1-C20".to_string(), title: "Plain load of a module stored in developer mode keeps it (Q-01)", cfg: with(|c| { c.init[0] = installed(2, true); }), end: Normal, nesting: "top-level", fault: "none", fixture: "load -dev m 2.0.0, then load m 2.0.0 (no flag)", lemma: "" },
        CaseDef { id: "B1-C21".to_string(), title: "-nodev load of a developer-mode module turns it off", cfg: with(|c| { c.init[0] = installed(2, true); c.devf = 2; }), end: Normal, nesting: "top-level", fault: "none", fixture: "load -dev m 2.0.0, then load -nodev m 2.0.0", lemma: "" },
        CaseDef { id: "B1-C22".to_string(), title: "System requirements refused (default)", cfg: with(|c| c.top.sysreq = false), end: Normal, nesting: "top-level", fault: "system requirements", fixture: "m 2.0.0 with <SystemRequirements SYSNamespace=\"true\"/> loaded outside %SYS (or an unmet IRIS version)", lemma: "" },
        CaseDef { id: "B1-C23".to_string(), title: "System requirements refused (-dev): manifest kept (Q-17)", cfg: with(|c| { c.top.sysreq = false; c.devf = 1; }), end: Normal, nesting: "top-level", fault: "system requirements", fixture: "as B1-C22 with -dev", lemma: "case_c23_sysreq_dev_keeps_manifest" },
        CaseDef { id: "B1-C24".to_string(), title: "System requirements refused (NoTransaction): manifest kept (Q-17)", cfg: with(|c| { c.top.sysreq = false; c.txm = 1; }), end: Normal, nesting: "top-level", fault: "system requirements", fixture: "as B1-C22 with -DNoTransaction=1", lemma: "" },
        CaseDef { id: "B1-C25".to_string(), title: "Activate fails (default): rolled back", cfg: with(|c| c.top.act = 1), end: Normal, nesting: "top-level", fault: "phase (Activate) failure, thrown", fixture: "_data/invoke/failure/ReturnErrorCheckStatus (Configure-phase Invoke returns an error; %Activate fails)", lemma: "" },
        CaseDef { id: "B1-C26".to_string(), title: "Activate fails (-dev): manifest and resources kept", cfg: with(|c| { c.top.act = 1; c.devf = 1; }), end: Normal, nesting: "top-level", fault: "phase (Activate) failure, thrown", fixture: "as B1-C25 with -dev", lemma: "" },
        CaseDef { id: "B1-C27".to_string(), title: "Activate fails (NoTransaction): partial kept", cfg: with(|c| { c.top.act = 1; c.txm = 1; }), end: Normal, nesting: "top-level", fault: "phase (Activate) failure, thrown", fixture: "as B1-C25 with -DNoTransaction=1", lemma: "" },
        CaseDef { id: "B1-C28".to_string(), title: "Activate-phase Invoke returns an error: phase left open (Q-14)", cfg: with(|c| c.top.act = 2), end: Normal, nesting: "top-level", fault: "phase (Activate) failure, returned by an Invoke with Phase=Activate", fixture: "module with <Invoke Class=\"Invoke.Test\" Method=\"ReturnError\" Phase=\"Activate\" When=\"After\" CheckStatus=\"1\"/>", lemma: "case_c28_phase_left_open" },
        CaseDef { id: "B1-C29".to_string(), title: "Error after the load committed (CleanUp): installed but reported failed", cfg: with(|c| c.cleanup_fails = true), end: Normal, nesting: "top-level", fault: "error after the last commit (tmpRepoMgr.CleanUp, OBS-L12)", fixture: "m with a <root>/.modules/x/module.xml that cannot be cached (malformed), no dependencies", lemma: "case_c29_error_after_commit" },
        CaseDef { id: "B1-C30".to_string(), title: "Load with a dependency succeeds", cfg: with(|c| c.deps = vec![pkg(1)]), end: Normal, nesting: "top-level with one dependency", fault: "none", fixture: withdep, lemma: "" },
        CaseDef { id: "B1-C31".to_string(), title: "Parent Activate fails after the dependency loaded (default): dependency record lost", cfg: with(|c| { c.deps = vec![pkg(1)]; c.top.act = 1; }), end: Normal, nesting: "dependency of a load", fault: "phase (Activate) failure of the parent", fixture: "as B1-C30; m's Configure-phase Invoke returns an error", lemma: "case_c31_dep_record_lost" },
        CaseDef { id: "B1-C32".to_string(), title: "Dependency Activate fails (default): its failure record lost", cfg: with(|c| { let mut d = pkg(1); d.act = 1; c.deps = vec![d]; }), end: Normal, nesting: "dependency of a load", fault: "dependency failure (dependency's Activate)", fixture: "as B1-C30; d's Configure-phase Invoke returns an error", lemma: "" },
        CaseDef { id: "B1-C33".to_string(), title: "Dependency Activate fails (-dev parent): parent manifest kept, dependency record lost", cfg: with(|c| { let mut d = pkg(1); d.act = 1; c.deps = vec![d]; c.devf = 1; }), end: Normal, nesting: "dependency of a load", fault: "dependency failure", fixture: "as B1-C32 with load -dev", lemma: "case_c33_dev_dep_record_lost" },
        CaseDef { id: "B1-C34".to_string(), title: "Dependency Activate fails (NoTransaction): dependency record kept", cfg: with(|c| { let mut d = pkg(1); d.act = 1; c.deps = vec![d]; c.txm = 1; }), end: Normal, nesting: "dependency of a load", fault: "dependency failure", fixture: "as B1-C32 with -DNoTransaction=1", lemma: "" },
        CaseDef { id: "B1-C35".to_string(), title: "Dependency not found: no dependency record, parent fails", cfg: with(|c| { let mut d = pkg(1); d.found = false; c.deps = vec![d]; }), end: Normal, nesting: "dependency of a load", fault: "dependency failure (not found in any repository)", fixture: "m 2.0.0 requires d 1.0.0 which is in no configured repository and no .modules folder", lemma: "" },
        CaseDef { id: "B1-C36".to_string(), title: "Second dependency fails (default): both dependency records lost", cfg: with(|c| { let mut d2 = pkg(1); d2.act = 1; c.deps = vec![pkg(1), d2]; }), end: Normal, nesting: "dependencies of a load", fault: "dependency failure (second)", fixture: "m depends on d1 1.0.0 then d2 1.0.0 (both under .modules); d2's Configure-phase Invoke returns an error", lemma: "" },
        CaseDef { id: "B1-C37".to_string(), title: "Second dependency fails (-dev): both dependency records lost, parent manifest kept", cfg: with(|c| { let mut d2 = pkg(1); d2.act = 1; c.deps = vec![pkg(1), d2]; c.devf = 1; }), end: Normal, nesting: "dependencies of a load", fault: "dependency failure (second)", fixture: "as B1-C36 with -dev", lemma: "" },
        CaseDef { id: "B1-C38".to_string(), title: "Parent Activate fails (-dev) after dependencies committed: dependency record kept", cfg: with(|c| { c.deps = vec![pkg(1)]; c.top.act = 1; c.devf = 1; }), end: Normal, nesting: "dependency of a load", fault: "phase (Activate) failure of the parent", fixture: "as B1-C31 with -dev", lemma: "" },
        CaseDef { id: "B1-C39".to_string(), title: "Snapshot dependency loaded in developer mode under a -dev parent", cfg: with(|c| { let mut d = pkg(1); d.snap = true; c.deps = vec![d]; c.devf = 1; }), end: Normal, nesting: "dependency of a load", fault: "none", fixture: "as B1-C30 with d at a snapshot version (e.g. 1.0.0-SNAPSHOT) and load -dev", lemma: "" },
        CaseDef { id: "B1-C40".to_string(), title: "Dependency already installed: skipped, no record", cfg: with(|c| { c.deps = vec![pkg(1)]; c.init[1] = installed(1, false); }), end: Normal, nesting: "dependency of a load", fault: "none", fixture: "as B1-C30 with d 1.0.0 installed first", lemma: "" },
        CaseDef { id: "B1-C41".to_string(), title: "Caller holds a transaction, load succeeds: Committed=1 while nothing is durable", cfg: with(|c| c.caller_tx = true), end: Normal, nesting: "caller already in a transaction", fault: "none", fixture: "tstart; Shell(\"load <m>\"); observe in the same process before the caller decides", lemma: "" },
        CaseDef { id: "B1-C42".to_string(), title: "Caller holds a transaction, then rolls back: module and history row gone", cfg: with(|c| c.caller_tx = true), end: CallerRollback, nesting: "caller already in a transaction", fault: "none", fixture: "as B1-C41, then trollback", lemma: "" },
        CaseDef { id: "B1-C43".to_string(), title: "Caller transaction, -dev load fails at Activate: Committed=1, then caller rolls back", cfg: with(|c| { c.caller_tx = true; c.devf = 1; c.top.act = 1; }), end: CallerRollback, nesting: "caller already in a transaction", fault: "phase (Activate) failure", fixture: "tstart; Shell(\"load -dev <ReturnErrorCheckStatus>\"); trollback", lemma: "" },
        CaseDef { id: "B1-C44".to_string(), title: "Process killed inside Activate (default)", cfg: base_cfg(), end: Kill(KillPoint::TopActivate), nesting: "top-level", fault: "process end inside m's Activate phase", fixture: "m with <Invoke Class=\"K.Halt\" Method=\"Run\" Phase=\"Activate\" When=\"Before\"/>; T variant: the method HALTs or another process calls $SYSTEM.Process.Terminate on the JOBbed load; K variant: SIGKILL of the JOBbed process", lemma: "" },
        CaseDef { id: "B1-C45".to_string(), title: "Process killed inside parent Activate after a dependency loaded (default): dependency record lost", cfg: with(|c| c.deps = vec![pkg(1)]), end: Kill(KillPoint::TopActivate), nesting: "dependency of a load", fault: "process kill inside m's Activate phase", fixture: "as B1-C30 plus the halting Activate Invoke on m; JOB'd process", lemma: "case_c45_kill_dep_record_lost" },
        CaseDef { id: "B1-C46".to_string(), title: "Process killed inside Activate (-dev, with dependency): changes kept, row reads Committed=0", cfg: with(|c| { c.deps = vec![pkg(1)]; c.devf = 1; }), end: Kill(KillPoint::TopActivate), nesting: "dependency of a load", fault: "process kill inside m's Activate phase", fixture: "as B1-C45 with -dev", lemma: "" },
        CaseDef { id: "B1-C47".to_string(), title: "Process killed inside the dependency's Activate (default)", cfg: with(|c| c.deps = vec![pkg(1)]), end: Kill(KillPoint::DepActivate(0)), nesting: "dependency of a load", fault: "process kill inside d's Activate phase", fixture: "as B1-C30 with the halting Activate Invoke on d; JOB'd process", lemma: "" },
        CaseDef { id: "B1-C48".to_string(), title: "Process killed inside Activate (NoTransaction): partial kept, row reads Committed=0", cfg: with(|c| c.txm = 1), end: Kill(KillPoint::TopActivate), nesting: "top-level", fault: "process kill inside m's Activate phase", fixture: "as B1-C44 with -DNoTransaction=1", lemma: "" },
        CaseDef { id: "B1-C49".to_string(), title: "Process killed after the final commit, before Finalize (default)", cfg: base_cfg(), end: Kill(KillPoint::AfterC3), nesting: "top-level", fault: "process kill after C3 (not natively injectable without a debugger; model-only)", fixture: "model-only: no product hook between the C3 tcommit and SetCommitted", lemma: "" },
        CaseDef { id: "B1-C50".to_string(), title: "Process killed inside Activate with a caller transaction: everything rolled back, row lost", cfg: with(|c| c.caller_tx = true), end: Kill(KillPoint::TopActivate), nesting: "caller already in a transaction", fault: "process kill inside m's Activate phase", fixture: "JOB'd process: tstart; Shell(\"load <m with halting Activate Invoke>\")", lemma: "" },
        CaseDef { id: "B1-C51".to_string(), title: "Manifest with two documents refused after import (default): rolled back", cfg: with(|c| c.top.mf = 4), end: Normal, nesting: "top-level", fault: "invalid manifest: written, then refused (module.xml contains multiple documents)", fixture: "module.xml with the module's .ZPM <Document> and a second <Document> (e.g. a class) in the same <Export>", lemma: "" },
        CaseDef { id: "B1-C52".to_string(), title: "Manifest with two documents refused after import (NoTransaction): module record kept", cfg: with(|c| { c.top.mf = 4; c.txm = 1; }), end: Normal, nesting: "top-level", fault: "invalid manifest: written, then refused (module.xml contains multiple documents)", fixture: "as B1-C51 with -DNoTransaction=1", lemma: "case_c52_multiple_documents_kept" },
    ];
    for c in v.iter_mut() { c.cfg.var = 0; }
    // Every process-end case in two variants: SIGKILL-like (no %OnClose, N-P18) and
    // Terminate/HALT-like (%OnClose runs before the rollback, N-P10, N-P11).
    let mut out = vec![];
    for c in v {
        if let End::Kill(kp) = c.end {
            out.push(CaseDef { id: format!("{}T", c.id), title: c.title, cfg: c.cfg.clone(), end: End::Terminate(kp), nesting: c.nesting, fault: c.fault,
                fixture: c.fixture, lemma: if c.lemma == "case_c45_kill_dep_record_lost" { "case_c45_terminate_dep_record_lost" } else if c.id == "B1-C46" { "case_c46_dev_end_inside_activate" } else { "" } });
            out.push(CaseDef { id: format!("{}K", c.id), title: c.title, cfg: c.cfg, end: c.end, nesting: c.nesting, fault: c.fault, fixture: c.fixture,
                lemma: if c.id == "B1-C46" { "case_c46_dev_end_inside_activate" } else { c.lemma } });
        } else {
            out.push(c);
        }
    }
    out
}

fn mode_str(c: &ExecCfg) -> &'static str { match c.txm { 0 => "default", 1 => "NoTransaction", _ => "NoJournal" } }
fn devf_str(c: &ExecCfg) -> &'static str { match c.devf { 0 => "none", 1 => "-dev", _ => "-nodev" } }

fn inst_str(m: &ExecMod) -> String { if m.present { format!("{} {}", ver_str(m.ver), if m.dev { "(developer mode)" } else { "" }) } else { "absent".to_string() } }

fn cmd_cases(out: &str) {
    let q = qprov_x();
    let mut f = std::fs::File::create(out).unwrap();
    writeln!(f, "{{").unwrap();
    writeln!(f, "  \"generator\": \"ipm-load-history b1 cases (executable twin of the L2 spec, exec_spec_verified)\",").unwrap();
    writeln!(f, "  \"product\": \"571e04a19aba9f9b4bc0ea23fbe09182038f4e55\",").unwrap();
    writeln!(f, "  \"q_provisional\": {{\"Q-01\": \"keep stored dev mode\", \"Q-02\": \"-dev bypasses UpdatePackage guard\", \"Q-05\": \"under a caller-held transaction Committed is not constrained (with no caller transaction it must be exact for every action)\", \"Q-06b\": \"records survive a caller-held transaction's rollback\", \"Q-08\": \"unended record Committed reflects kept changes\", \"Q-14\": \"started phases must end with status\", \"Q-17\": \"system requirements checked before any write\", \"Q-FIN\": \"an unended record is not Finalized\"}}, \"l1_revision\": \"stage C round 1: R1-ONT-01..03 (dependency record survival required; Q-06b; Q-08 default flipped; Q-FIN; Package after the manifest was read) and R1-MOD-01..03 (dependency Committed exact without a caller transaction, Q-05 limited to caller-held transactions; manifest refusals write nothing in any mode and name the package once read; unended rows list their started phases and package)\",").unwrap();
    writeln!(f, "  \"versions\": \"version ordinals 1,2,3 stand for 1.0.0, 2.0.0, 3.0.0; module m is slot 'top', dependencies d/d1 are 'dep1', d2 is 'dep2'\",").unwrap();
    writeln!(f, "  \"observation_notes\": \"history.* are %IPM_General.History rows for the action (load of m, load of each dependency); history_temp.* are ^IRIS.Temp.HistoryTempD rows left over; modules.*.manifest_present is %IPM.Storage.Module:NameExists; resources_loaded is a class resource of the module present; zpm_document_version is the .ZPM ModuleStream; writes counts module record writes (not natively comparable, LastUpdated changes); id_order is the History ID allocation order (gaps reveal rolled-back inserts)\",").unwrap();
    writeln!(f, "  \"cases\": [").unwrap();
    let cs = cases();
    let n = cs.len();
    for (k, d) in cs.iter().enumerate() {
        let c = &d.cfg;
        let (s, trace) = run_case(c, d.end).unwrap_or_else(|| panic!("case {} does not reach its end", d.id));
        let e = exec_expect_for(c, &q, &s);
        let viol = violated_clauses(c, &q, &s);
        let qs = q_sensitivity(c, &s);
        let deps: Vec<String> = c.deps.iter().enumerate().map(|(i, p)| format!("{{\"slot\": \"dep{}\", \"version\": \"{}\", \"installed_before\": {}, \"activate\": \"{}\", \"manifest\": {}, \"snapshot\": {}, \"found\": {}, \"system_requirements_met\": {}}}",
            i + 1, ver_str(p.ver), json_str(&inst_str(&c.init[i + 1])), match p.act { 0 => "ok", 1 => "throws", _ => "returns error" }, p.mf, p.snap, p.found, p.sysreq)).collect();
        let end = match d.end { End::Normal => "command returns".to_string(), End::CallerCommit => "command returns, caller commits".to_string(), End::CallerRollback => "command returns, caller rolls back".to_string(), End::Kill(kp) => format!("process killed without %OnClose (SIGKILL-like) at {:?}", kp), End::Terminate(kp) => format!("process terminated with %OnClose ($SYSTEM.Process.Terminate or HALT) at {:?}", kp) };
        writeln!(f, "    {{").unwrap();
        writeln!(f, "      \"id\": \"{}\", \"title\": {},", d.id, json_str(d.title)).unwrap();
        writeln!(f, "      \"inputs\": {{\"command\": \"load\", \"mode\": \"{}\", \"dev_flag\": \"{}\", \"force\": {}, \"nesting\": {}, \"installed_before\": {{\"top\": {}}}, \"top_package\": {{\"version\": \"{}\", \"update_package\": {}, \"system_requirements_met\": {}, \"activate\": \"{}\", \"manifest\": {}}}, \"dependencies\": [{}], \"fault_point\": {}, \"end\": {}, \"fixture_hint\": {}}},",
            mode_str(c), devf_str(c), c.force, json_str(d.nesting), json_str(&inst_str(&c.init[0])), ver_str(c.top.ver), c.top.upd, c.top.sysreq,
            match c.top.act { 0 => "ok", 1 => "throws", _ => "invoke returns error (phase left open)" },
            json_str(match c.top.mf { 0 => "valid", 1 => "not parseable", 2 => "no module document", 3 => "module document refused by validation", _ => "written, then refused (multiple documents)" }),
            deps.join(", "), json_str(d.fault), json_str(&end), json_str(d.fixture)).unwrap();
        writeln!(f, "      \"l1_permitted\": {},", expect_json(c, &e)).unwrap();
        writeln!(f, "      \"l2_predicted\": {},", obs_json(c, &s)).unwrap();
        writeln!(f, "      \"l1_vs_l2\": {{\"conforms\": {}, \"violated_clauses\": [{}], \"q_alternatives_that_change_the_verdict\": [{}]}},",
            viol.is_empty(), viol.iter().map(|x| json_str(x)).collect::<Vec<_>>().join(", "), qs.iter().map(|x| json_str(x)).collect::<Vec<_>>().join(", ")).unwrap();
        writeln!(f, "      \"model\": {{\"steps\": {}, \"verus_lemma\": {}}}", trace.len() - 1, if d.lemma.is_empty() { "null".to_string() } else { json_str(d.lemma) }).unwrap();
        writeln!(f, "    }}{}", if k + 1 < n { "," } else { "" }).unwrap();
    }
    writeln!(f, "  ]\n}}").unwrap();
    println!("cases: {} written to {}", n, out);
}

/// D1 repair (variant 103) against the as-built: every end state of every configuration, the
/// DEP-REPAIR check, with each violation classified.
fn cmd_repair_survey(out: &str) {
    let q = qprov_x();
    let mut f = std::fs::File::create(out).unwrap();
    let mut rows: BTreeMap<String, (usize, usize, String)> = BTreeMap::new();
    for var in [0u8, 103u8] {
        for c in domain(&[0, 1, 2], &[false, true], var) {
            let mut s = exec_init(&c);
            let mut ends: Vec<(String, ExecState, Option<ExecState>)> = vec![];
            while exec_running(&s) {
                ends.push(("kill".into(), exec_kill(&c, &s), Some(s.clone())));
                ends.push(("terminate".into(), exec_terminate(&c, &s), Some(s.clone())));
                s = exec_step(&c, &s);
            }
            if c.caller_tx {
                ends.push(("caller-commit".into(), exec_caller_end(&c, &s, true), None));
                ends.push(("caller-rollback".into(), exec_caller_end(&c, &s, false), None));
            }
            ends.push(("normal".into(), s.clone(), None));
            for (k, e, pre) in ends {
                let ok = check_state(&c, &q, Check::DepRepair, &e);
                // classification: a dependency's no-transaction marker (Utils/Module.cls:1226-1228) set
                // Committed while none of its writes had happened (V4 window)
                let v4 = !ok && c.txm != 0 && pre.as_ref().map_or(false, |p| exec_active(p) == 3 && p.lnm1.pc >= 4 && p.lnm1.pc <= 4)
                    || (!ok && c.txm != 0 && (1..3).any(|i| { let t = exec_gtwin(&e.ns.twins, i); t.ex && t.com && !exec_kept(&c.init, &e.ns.mods, i) && c.deps.get(i - 1).map_or(false, |d| d.mf == 2 || d.mf == 3) }));
                let key = format!("var={} txm={} end={}", var, c.txm, k);
                let ent = rows.entry(key).or_insert((0, 0, String::new()));
                ent.0 += 1;
                if !ok {
                    ent.1 += 1;
                    let lag = (1..3).any(|i| { let t = exec_gtwin(&e.ns.twins, i); t.ex && !t.com && !t.tend && exec_kept(&c.init, &e.ns.mods, i) });
                    let cls = if var == 103 && v4 { "V4-window (dependency marked Committed before its first write)" }
                        else if var == 103 && k != "normal" && lag { "V9-lag (unended dependency row reads Committed=0 although its writes were kept; SetCommitted writes IRISTEMP only)" }
                        else { "other" };
                    if !ent.2.contains(cls) { ent.2.push_str(cls); ent.2.push_str("; "); }
                }
            }
        }
    }
    writeln!(f, "{{").unwrap();
    let n = rows.len();
    for (i, (k, (t, bad, cls))) in rows.iter().enumerate() {
        writeln!(f, "  {}: {{\"end_states\": {}, \"dep_repair_violations\": {}, \"classes\": {}}}{}", json_str(k), t, bad, json_str(cls), if i + 1 < n { "," } else { "" }).unwrap();
    }
    writeln!(f, "}}").unwrap();
    println!("repair-survey written to {}", out);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("explore") => cmd_explore(&args[2]),
        Some("explore-list") => for r in runs() { println!("{}\t{}\t{}\t{}", r.name, r.kind, r.variant, r.expect) },
        Some("survey") => cmd_survey(&args[2]),
        Some("cases") => cmd_cases(&args[2]),
        Some("repair-survey") => cmd_repair_survey(&args[2]),
        _ => eprintln!("usage: b1 explore <run>|explore-list|survey <out>|cases <out>"),
    }
}
