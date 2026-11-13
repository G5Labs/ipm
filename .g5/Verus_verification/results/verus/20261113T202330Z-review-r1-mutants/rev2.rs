use ipm_load_history::contract::*;
use ipm_load_history::explore::*;
use std::collections::BTreeMap;
fn main() {
    let q = exec_qprov();
    let base = domain(&[0, 1, 2], &[false, true], 0);
    let mutd = domain(&[0, 1, 2], &[false, true], 201);
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for (k, c) in base.iter().enumerate() {
        let conf = end_states(c).iter().all(|e| check_state(c, &q, Check::L1EndNormal, e));
        let m = &mutd[k];
        let det = !end_states(m).iter().all(|e| check_state(m, &q, Check::L1EndNormal, e));
        if conf && det {
            let key = format!("txm={} mf={} upd={} init0={}v{} caller={}", c.txm, c.top.mf, c.top.upd, if c.init[0].present {"present "} else {"absent "}, c.init[0].ver, c.caller_tx);
            *kinds.entry(key).or_insert(0) += 1;
        }
    }
    for (k, v) in kinds { println!("{v:4}  {k}"); }
}
