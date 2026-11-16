use vstd::prelude::*;
verus! {
pub enum Phase {Initialize,Clean,Other}
pub open spec fn wheel_path(root:Seq<int>,directory:Seq<int>,name:int)->Seq<int> {root+directory+seq![name]}
pub open spec fn requirements_called(command:int,download:bool,bypass:bool,exists:bool)->bool {(download || command==1 || command==2 || command==3) && !bypass && exists}
pub struct Wheel {pub pip_called:bool,pub handled:bool,pub ok:bool,pub installed:Option<bool>}
// Module 1 is the pinned IPM name zpm. Other modules propagate failures.
pub open spec fn wheel(module:int,phase:Phase,bypass:bool,exists:bool,pip_ok:bool,installed_before:bool)->Wheel {
 match phase {
 Phase::Clean=>Wheel{pip_called:false,handled:true,ok:true,installed:Some(installed_before)},
 Phase::Other=>Wheel{pip_called:false,handled:false,ok:true,installed:Some(installed_before)},
 Phase::Initialize=>if exists && pip_ok {Wheel{pip_called:true,handled:true,ok:true,installed:Some(true)}}
 else {Wheel{pip_called:exists,handled:false,ok:true,installed:if exists {None}else{Some(installed_before)}}}
 }
}
pub open spec fn without_empty(args:Seq<Seq<int>>)->Seq<Seq<int>>
 decreases args.len()
{if args.len()==0{seq![]}else{let rest=without_empty(args.skip(1));if args[0].len()==0 {rest}else{seq![args[0]]+rest}}}
pub proof fn wheel_neighbors() {
 assert(wheel(2,Phase::Initialize,true,true,true,false).pip_called);
 assert(wheel(2,Phase::Initialize,true,true,true,false).installed==Some(true));
 assert(!requirements_called(1,false,true,true));
 assert(!wheel(2,Phase::Initialize,false,false,false,false).ok);
 assert(wheel(1,Phase::Initialize,false,false,false,false).ok);
 assert(!wheel(1,Phase::Initialize,false,false,false,false).handled);
 assert(wheel(2,Phase::Clean,false,true,true,true).installed==Some(true));
 assert(wheel(2,Phase::Clean,false,true,true,true).handled);
 assert(requirements_called(1,false,false,true));
 assert(!wheel(2,Phase::Other,false,true,true,false).pip_called);
 assert(wheel(2,Phase::Initialize,false,true,false,false).installed==None);
 assert(wheel(2,Phase::Initialize,false,true,false,true).installed==None);
 reveal_with_fuel(without_empty,6);
 assert(without_empty(seq![seq![1int],seq![],seq![2int],seq![]]).len()==2);
 assert(wheel_path(seq![1int],seq![2int],3) =~= seq![1int,2,3]);
}
}
fn main() {}
