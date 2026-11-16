use vstd::prelude::*;
// Mechanically projected unchanged declarations; provenance checked before use.
verus! {
pub struct Node { pub val: Option<nat>, pub dflt: Option<nat> }

pub struct Cfg {
    pub nodes: Map<nat, Node>,   // ^IPM.settings in %SYS
    pub conf: Set<nat>,          // CONFIGURABLE (static)
    pub seed: Map<nat, nat>,     // defaults the installer seeds (preload/cls/IPM/Installer.cls#L114-L122) (static)
}

pub open spec fn empty_v() -> nat { 0 }

pub open spec fn k_trf() -> nat { 1 }

pub open spec fn k_mroot() -> nat { 2 }

pub open spec fn mroot_default() -> nat { 3 }

pub open spec fn valid_trf(v: nat) -> bool { v == empty_v() || v == 10 || v == 11 || v == 12 }

pub enum St { Ok, Err }

pub open spec fn get_value(c: Cfg, k: nat) -> nat {
    if c.nodes.contains_key(k) && c.nodes[k].val is Some { c.nodes[k].val->0 } else { empty_v() }
}

pub open spec fn set_value(c: Cfg, k: nat, v: nat) -> Cfg {
    let old = if c.nodes.contains_key(k) { c.nodes[k] } else { Node { val: None, dflt: None } };
    let d = if old.dflt is None { Some(v) } else { old.dflt };
    Cfg { nodes: c.nodes.insert(k, Node { val: Some(v), dflt: d }), ..c }
}

pub open spec fn update_one(c: Cfg, k: nat, v: nat) -> (Cfg, St) {
    if !c.conf.contains(k) { (c, St::Ok) }                                           // "Config key ... not found"
    else if k == k_trf() { if valid_trf(v) { (set_value(c, k, v), St::Ok) } else { (c, St::Err) } }
    else if k == k_mroot() && v == empty_v() { (c, St::Err) }
    else { (set_value(c, k, v), St::Ok) }
}

pub open spec fn config_set(c: Cfg, ns: nat, k: nat, v: nat) -> (Cfg, St) {
    if v == empty_v() { (c, St::Ok) } else { (update_one(c, k, v).0, St::Ok) }
}
}
