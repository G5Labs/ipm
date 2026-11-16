#[path="loadtx.rs"] mod m;
use vstd::prelude::*;
verus!{ proof fn flags_match_native(){
let f=m::effective(m::Flags{no_journal:None,no_transaction:None},m::Flags{no_journal:Some(true),no_transaction:None});
let w=m::Write{owner:1,key:1,value:1,at:0,journal:!m::enabled(f.no_journal),depth:m::entry_depth(1,f),commit_at:None};
let parent=m::Write{owner:0,key:0,value:1,at:0,journal:true,depth:1,commit_at:None};
assert(m::entry_depth(1,f) == 1); // JA child.tlevel
assert(!m::enabled(f.no_journal) == false); // JA child.journal
assert(m::survived(w,2) == true); // JA stored.child
assert(m::survived(w,2) == true); // JA stored.childwrite
assert(m::survived(parent,2) == false); // JA stored.root
assert(m::survived(parent,2) == false); // JA stored.parentwrite
assert(m::settle(m::End::Return,true,true,true).transaction_depth == 0); // JA exit.tlevel
let f=m::effective(m::Flags{no_journal:Some(false),no_transaction:None},m::Flags{no_journal:Some(true),no_transaction:None});
let w=m::Write{owner:1,key:1,value:1,at:0,journal:!m::enabled(f.no_journal),depth:m::entry_depth(1,f),commit_at:None};
let parent=m::Write{owner:0,key:0,value:1,at:0,journal:true,depth:1,commit_at:None};
assert(m::entry_depth(1,f) == 2); // J0 child.tlevel
assert(!m::enabled(f.no_journal) == true); // J0 child.journal
assert(m::survived(w,2) == false); // J0 stored.child
assert(m::survived(w,2) == false); // J0 stored.childwrite
assert(m::survived(parent,2) == false); // J0 stored.root
assert(m::survived(parent,2) == false); // J0 stored.parentwrite
assert(m::settle(m::End::Return,true,true,true).transaction_depth == 0); // J0 exit.tlevel
let f=m::effective(m::Flags{no_journal:None,no_transaction:None},m::Flags{no_journal:None,no_transaction:Some(true)});
let w=m::Write{owner:1,key:1,value:1,at:0,journal:!m::enabled(f.no_journal),depth:m::entry_depth(1,f),commit_at:None};
let parent=m::Write{owner:0,key:0,value:1,at:0,journal:true,depth:1,commit_at:None};
assert(m::entry_depth(1,f) == 1); // TA child.tlevel
assert(!m::enabled(f.no_journal) == true); // TA child.journal
assert(m::survived(w,2) == false); // TA stored.child
assert(m::survived(w,2) == false); // TA stored.childwrite
assert(m::survived(parent,2) == false); // TA stored.root
assert(m::survived(parent,2) == false); // TA stored.parentwrite
assert(m::settle(m::End::Return,true,true,true).transaction_depth == 0); // TA exit.tlevel
let f=m::effective(m::Flags{no_journal:None,no_transaction:Some(false)},m::Flags{no_journal:None,no_transaction:Some(true)});
let w=m::Write{owner:1,key:1,value:1,at:0,journal:!m::enabled(f.no_journal),depth:m::entry_depth(1,f),commit_at:None};
let parent=m::Write{owner:0,key:0,value:1,at:0,journal:true,depth:1,commit_at:None};
assert(m::entry_depth(1,f) == 2); // T0 child.tlevel
assert(!m::enabled(f.no_journal) == true); // T0 child.journal
assert(m::survived(w,2) == false); // T0 stored.child
assert(m::survived(w,2) == false); // T0 stored.childwrite
assert(m::survived(parent,2) == false); // T0 stored.root
assert(m::survived(parent,2) == false); // T0 stored.parentwrite
assert(m::settle(m::End::Return,true,true,true).transaction_depth == 0); // T0 exit.tlevel
let f=m::effective(m::Flags{no_journal:None,no_transaction:None},m::Flags{no_journal:Some(true),no_transaction:None});
let w=m::Write{owner:1,key:1,value:1,at:0,journal:!m::enabled(f.no_journal),depth:m::entry_depth(1,f),commit_at:Some(1nat)};
let parent=m::Write{owner:0,key:0,value:1,at:0,journal:true,depth:1,commit_at:Some(1nat)};
assert(m::entry_depth(1,f) == 1); // JS child.tlevel
assert(!m::enabled(f.no_journal) == false); // JS child.journal
assert(m::survived(w,2) == true); // JS stored.child
assert(m::survived(w,2) == true); // JS stored.childwrite
assert(m::survived(parent,2) == true); // JS stored.root
assert(m::survived(parent,2) == true); // JS stored.parentwrite
assert(m::settle(m::End::Return,true,true,true).transaction_depth == 0); // JS exit.tlevel
let f=m::effective(m::Flags{no_journal:None,no_transaction:None},m::Flags{no_journal:None,no_transaction:Some(true)});
let w=m::Write{owner:1,key:1,value:1,at:0,journal:!m::enabled(f.no_journal),depth:m::entry_depth(1,f),commit_at:Some(1nat)};
let parent=m::Write{owner:0,key:0,value:1,at:0,journal:true,depth:1,commit_at:Some(1nat)};
assert(m::entry_depth(1,f) == 1); // TS child.tlevel
assert(!m::enabled(f.no_journal) == true); // TS child.journal
assert(m::survived(w,2) == true); // TS stored.child
assert(m::survived(w,2) == true); // TS stored.childwrite
assert(m::survived(parent,2) == true); // TS stored.root
assert(m::survived(parent,2) == true); // TS stored.parentwrite
assert(m::settle(m::End::Return,true,true,true).transaction_depth == 0); // TS exit.tlevel
}}
fn main(){}
