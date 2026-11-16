use vstd::prelude::*;
verus! {
// Paths are normalized segment sequences. Filesystem normalization and inode ancestry are provider assumptions.
pub open spec fn below(a: Seq<int>, b: Seq<int>) -> bool { a.len() <= b.len() && (forall|i: int| 0 <= i < a.len() ==> a[i] == b[i]) }
pub open spec fn overlap(a: Seq<int>, b: Seq<int>, same_inode_ancestor: bool) -> bool {
    below(a,b) || below(b,a) || same_inode_ancestor
}
pub open spec fn copy_ok(a: Seq<int>, b: Seq<int>, alias: bool, provider_ok: bool) -> bool {
    !overlap(a,b,alias) && provider_ok
}
pub struct Item { pub path: Seq<int>, pub loaded: bool, pub supported_ext: bool }
pub open spec fn application_item(i: Item, test_dir: Seq<int>) -> bool {
    i.loaded && i.supported_ext 
}
pub open spec fn test_item(i: Item, test_dir: Seq<int>, test_dir_exists: bool) -> bool {
    test_dir.len()>0 && test_dir_exists && below(test_dir,i.path) && i.loaded && i.supported_ext
}
pub open spec fn template_status(source_exists: bool) -> int { if source_exists { 1 } else { 0 } }
// The default global directory is gbl; SourcesRoot prefixes both exporter and importer.
pub open spec fn global_path(sources: Seq<int>, directory: Option<Seq<int>>, name: int) -> Seq<int> {
    sources + (match directory { Some(d) => d, None => seq![10int] }) + seq![name]
}
pub open spec fn package_global(sources: Seq<int>, directory: Option<Seq<int>>, name: int, value: int) -> Map<Seq<int>,int> {
 Map::empty().insert(global_path(sources,directory,name),value)
}
pub open spec fn reload_global(files: Map<Seq<int>, int>, sources: Seq<int>, directory: Option<Seq<int>>, name: int, old: int) -> int {
    let path=global_path(sources,directory,name);
    if files.contains_key(path) { files[path] } else { old }
}
pub open spec fn selected_module(internal: Map<int,int>, external: Map<int,int>, arg: int) -> Option<int> {
    if internal.contains_key(arg) { Some(internal[arg]) } else if external.contains_key(arg) { Some(external[arg]) } else { None }
}
pub open spec fn custom_hook(selected_phases: Seq<int>, custom_phase: int) -> bool { selected_phases.contains(custom_phase) }
pub struct Entry { pub name: int, pub size: nat, pub regular: bool }
pub open spec fn end_header(e: Entry) -> bool { e.name == 0 }
pub open spec fn creates_file(e: Entry) -> bool { !end_header(e) && e.regular }
pub open spec fn next_header(offset: nat, size: nat) -> nat { offset+512+((size+511)/512)*512 }
pub open spec fn b(v: bool) -> int { if v {1} else {0} }
pub proof fn neighbors() {
 let src=seq![1int,2];let child=seq![1int,2,3];let sibling=seq![1int,4];
 assert(!copy_ok(src,child,false,true));assert(!copy_ok(child,src,false,true));assert(!copy_ok(src,src,false,true));assert(copy_ok(src,sibling,false,true));
 assert(!copy_ok(src,sibling,true,true));assert(!copy_ok(src,sibling,false,false));
 let t=seq![1int,9];let app=Item{path:seq![1int,2,3],loaded:true,supported_ext:true};
 let test=Item{path:seq![1int,9,8],loaded:true,supported_ext:true};
 assert(application_item(app,t));assert(!application_item(test,t));assert(test_item(test,t,true));assert(!test_item(test,t,false));
 assert(template_status(false)==0 && template_status(true)==1);
 let path=global_path(seq![5int],None,7);assert(path==seq![5int,10,7]);
 let files=package_global(seq![5int],None,7,42);assert(reload_global(files,seq![5int],None,7,0)==42);
 assert(reload_global(files,seq![],None,7,0)==0);
 assert(selected_module(map![1int=>99int],map![2int=>99int],1)==Some(99));
 assert(selected_module(map![1int=>99int],map![2int=>99int],2)==Some(99));
 assert(selected_module(map![1int=>99int],map![2int=>99int],3)==None);
 assert(!custom_hook(seq![1int,2,3],9));assert(custom_hook(seq![9int],9)) by { assert(seq![9int][0]==9); }
 let e=Entry{name:7,size:0,regular:true};assert(creates_file(e));assert(!end_header(e));
 assert(next_header(0,0)==512);assert(next_header(512,1)==1536);
 assert(end_header(Entry{name:0,size:0,regular:false}));
}
}
fn main() {}
