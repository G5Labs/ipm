use vstd::prelude::*;
verus! {
// 571e04a1 FileCopy directory protection, scoped hand translation. Path segments
// are already normalized and lowercased provider outputs. The root list is the
// system/IPM + configured database list supplied by actual callers; completeness
// and raw normalization are not proved. All file operations here succeed.
pub type Path=Seq<int>;
pub enum SourceKind {Directory,File}
pub struct Files {
 pub dir:bool,pub exact_marker:bool,pub nested_marker:bool,pub sentinel:bool,pub payload:bool
}
pub open spec fn prefix(a:Path,b:Path)->bool {
 a.len()<=b.len() && forall|i:int|0<=i<a.len() ==> a[i]==b[i]
}
pub open spec fn ancestor_of_root(target:Path,roots:Seq<Path>)->bool
 decreases roots.len()
{ roots.len()>0 && (target==roots[0] || ancestor_of_root(target,roots.skip(1))) }
// IsProtectedDirectory checks only target/IRIS.DAT before consulting its root
// list. A marker in an unconfigured descendant is not a recursive search input.
pub open spec fn protected(target:Path,roots:Seq<Path>,files:Files)->bool {
 files.exact_marker || ancestor_of_root(target,roots)
}
pub open spec fn delete_first(kind:SourceKind,overlay:bool,target:Path,roots:Seq<Path>,files:Files)->bool {
 kind==SourceKind::Directory && !overlay && !protected(target,roots,files)
}
// FileCopy derives this flag, then the Unix wrapper always passes overlay=1
// as the third File.CopyDir argument. The fifth flag deletes a matching file
// before overwrite; it does not delete unrelated destination entries.
pub struct UnixArgs {pub overlay_existing:bool,pub delete_matching:bool}
pub open spec fn unix_args(delete_matching:bool)->UnixArgs {
 UnixArgs{overlay_existing:true,delete_matching}
}
// Pinned Unix successful-copy port, qualified by direct flag0/flag1 native
// observations. Files represents distinct payload/marker/sentinel paths only;
// overwrite permissions, inodes, failures and arbitrary file layouts are open.
pub open spec fn unix_copy(files:Files,args:UnixArgs)->Files {
 if args.overlay_existing || !files.dir {
  let clear=false;
  Files{dir:true,payload:true,exact_marker:files.exact_marker && !clear,nested_marker:files.nested_marker && !clear,sentinel:files.sentinel && !clear}
 }else{files}
}
// NormalizeNames chooses the directory/file branch. This is a provider input,
// not a proof of path parsing. The selected file differs from other tracked files.
pub open spec fn copy(files:Files,kind:SourceKind,overlay:bool,target:Path,roots:Seq<Path>)->Files {
 if kind==SourceKind::File {Files{dir:true,payload:true,..files}}
 else {unix_copy(files,unix_args(delete_first(kind,overlay,target,roots,files)))}
}
pub open spec fn marker_write(files:Files,present:bool)->Files {
 if files.dir {Files{exact_marker:present,..files}}else{files}
}
pub open spec fn absent()->Files {Files{dir:false,exact_marker:false,nested_marker:false,sentinel:false,payload:false}}
pub open spec fn clean(files:Files,kind:SourceKind,keep:bool,target:Path,roots:Seq<Path>)->Files {
 if keep {files}
 else if kind==SourceKind::File {Files{payload:false,..files}}
 else if protected(target,roots,files) {files}
 else {absent()}
}
pub open spec fn roots()->Seq<Path> {seq![seq![1int,2],seq![3int,4]]}
pub open spec fn scratch()->Path {seq![8int,1]}
pub open spec fn before(marker:bool,nested:bool)->Files {Files{dir:true,exact_marker:marker,nested_marker:nested,sentinel:true,payload:false}}
pub open spec fn ordinary_loaded()->Files {copy(before(false,false),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn ordinary_after()->Files {clean(ordinary_loaded(),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn marked_loaded()->Files {copy(before(true,false),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn marked_after()->Files {clean(marked_loaded(),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn added_ready()->Files {marker_write(ordinary_loaded(),true)}
pub open spec fn added_after()->Files {clean(added_ready(),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn removed_ready()->Files {marker_write(marked_loaded(),false)}
pub open spec fn removed_after()->Files {clean(removed_ready(),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn nested_loaded()->Files {copy(before(false,true),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn nested_after()->Files {clean(nested_loaded(),SourceKind::Directory,false,scratch(),roots())}
pub open spec fn file_loaded()->Files {copy(before(true,false),SourceKind::File,false,scratch(),roots())}
pub open spec fn file_after()->Files {clean(file_loaded(),SourceKind::File,false,scratch(),roots())}
pub open spec fn b(x:bool)->int {if x{1}else{0}}
pub proof fn whole_segment_neighbors()
 ensures protected(seq![1int,2],roots(),before(false,false)),protected(seq![1int],roots(),before(false,false)),
 !protected(seq![1int,2,5],roots(),before(false,false)),!protected(seq![1int,20],roots(),before(false,false)),
 !protected(scratch(),roots(),before(false,true)),protected(scratch(),roots(),before(true,false))
{
 reveal_with_fuel(ancestor_of_root,4);
 assert(protected(seq![1int,2],roots(),before(false,false)));
 assert(protected(seq![1int],roots(),before(false,false)));
 assert(!protected(seq![1int,2,5],roots(),before(false,false)));
 assert(!protected(seq![1int,20],roots(),before(false,false)));
 assert(!protected(scratch(),roots(),before(false,true)));
 assert(protected(scratch(),roots(),before(true,false)));
}
pub proof fn protected_directory_copy_preserves(files:Files,target:Path,rs:Seq<Path>)
 requires protected(target,rs,files)
 ensures copy(files,SourceKind::Directory,false,target,rs).sentinel==files.sentinel,
 copy(files,SourceKind::Directory,false,target,rs).exact_marker==files.exact_marker,
 copy(files,SourceKind::Directory,false,target,rs).nested_marker==files.nested_marker,
 copy(files,SourceKind::Directory,false,target,rs).payload
{}
pub proof fn unix_copy_preserves_unrelated(files:Files,delete_matching:bool)
 ensures unix_copy(files,unix_args(delete_matching)).sentinel==files.sentinel,
 unix_copy(files,unix_args(delete_matching)).nested_marker==files.nested_marker,
 unix_copy(files,unix_args(delete_matching)).exact_marker==files.exact_marker,
 unix_copy(files,unix_args(delete_matching)).payload
{}
pub proof fn request_is_distinct_from_clearing()
 ensures delete_first(SourceKind::Directory,false,scratch(),roots(),before(false,true)),
 !delete_first(SourceKind::Directory,false,scratch(),roots(),before(true,false)),
 !delete_first(SourceKind::Directory,true,scratch(),roots(),before(false,false)),
 unix_args(true).overlay_existing,unix_args(true).delete_matching,
 !unix_args(false).delete_matching,
 unix_copy(before(false,true),unix_args(true)).sentinel,
 unix_copy(before(false,true),unix_args(false)).sentinel,
 unix_copy(before(false,true),unix_args(true)).nested_marker
{
 whole_segment_neighbors();reveal_with_fuel(ancestor_of_root,4);
 assert(delete_first(SourceKind::Directory,false,scratch(),roots(),before(false,true)));
 assert(!delete_first(SourceKind::Directory,false,scratch(),roots(),before(true,false)));
 assert(!delete_first(SourceKind::Directory,true,scratch(),roots(),before(false,false)));
}
pub proof fn keep_and_overlay_are_preserved(files:Files,kind:SourceKind,target:Path,rs:Seq<Path>)
 ensures clean(files,kind,true,target,rs)==files,
 copy(files,SourceKind::Directory,true,target,rs).sentinel==files.sentinel,
 copy(files,SourceKind::Directory,true,target,rs).nested_marker==files.nested_marker
{}
pub proof fn lifecycle_neighbors()
 ensures ordinary_loaded().dir,ordinary_loaded().payload,ordinary_loaded().sentinel,
 !ordinary_after().dir,!ordinary_after().payload,
 marked_loaded().sentinel,marked_loaded().payload,marked_loaded().exact_marker,
 marked_after().dir,marked_after().payload,marked_after().sentinel,marked_after().exact_marker,
 added_ready().exact_marker,added_after().dir,added_after().payload,added_after().sentinel,
 !removed_ready().exact_marker,!removed_after().dir,!removed_after().payload,
 nested_loaded().nested_marker,nested_loaded().sentinel,nested_loaded().payload,!nested_after().dir,
 file_loaded().payload,file_after().dir,!file_after().payload,file_after().exact_marker,file_after().sentinel
{
 whole_segment_neighbors();reveal_with_fuel(ancestor_of_root,4);
 assert(ordinary_loaded().dir);assert(ordinary_loaded().payload);assert(ordinary_loaded().sentinel);
 assert(!ordinary_after().dir);assert(!ordinary_after().payload);
 assert(marked_loaded().sentinel);assert(marked_loaded().payload);assert(marked_loaded().exact_marker);
 assert(marked_after().dir);assert(marked_after().payload);assert(marked_after().sentinel);assert(marked_after().exact_marker);
 assert(added_ready().exact_marker);assert(added_after().dir);assert(added_after().payload);assert(added_after().sentinel);
 assert(!removed_ready().exact_marker);assert(!removed_after().dir);assert(!removed_after().payload);
 assert(nested_loaded().nested_marker);assert(nested_loaded().sentinel);assert(nested_loaded().payload);assert(!nested_after().dir);
 assert(file_loaded().payload);assert(file_after().dir);assert(!file_after().payload);assert(file_after().exact_marker);assert(file_after().sentinel);
}
}
fn main(){}
