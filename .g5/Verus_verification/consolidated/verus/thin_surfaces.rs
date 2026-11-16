//! Consequential boundary effects recovered at IPM 571e04a1.
//! Product-body correspondence is finite native testing, not a Verus proof.
//! Filesystem keys are normalized, disjoint paths; path normalization and %File outcomes
//! are environment inputs. Erroring copies may have partial effects (copy_step below).
use vstd::prelude::*;

verus! {

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Phase { Activate, Clean, Other }

pub fn copies_before(phase: Phase, defer: bool, target_set: bool) -> (r: bool)
    ensures r == (phase == Phase::Activate && !defer && target_set)
{ match phase { Phase::Activate => !defer && target_set, _ => false } }

pub fn copies_after(phase: Phase, defer: bool, target_set: bool) -> (r: bool)
    ensures r == (phase == Phase::Activate && defer && target_set)
{ match phase { Phase::Activate => defer && target_set, _ => false } }

// FileCopy.DoCopy: protected targets force overlay even when Overlay=false.
pub fn clears_target(overlay: bool, protected: bool) -> (r: bool)
    ensures r == (!overlay && !protected)
{ !overlay && !protected }

// FileCopy.OnPhase. Single files do not receive the directory protection exemption.
pub fn clean_removes(phase: Phase, keep: bool, as_file: bool, protected: bool) -> (r: bool)
    ensures r == (phase == Phase::Clean && !keep && (as_file || !protected))
{ match phase { Phase::Clean => !keep && (as_file || !protected), _ => false } }

// The file branch of DoCopy. The OS result and resulting target existence are
// environment values only after the wrapper's mkdir and source-presence checks.
pub fn file_copy_observation(target_before: bool, mkdir_ok: bool, source_exists: bool,
    os_ok: bool, os_target_after: bool) -> (r: (bool,bool))
    ensures r.0 == (mkdir_ok && source_exists && os_ok),
            r.1 == if mkdir_ok && source_exists { os_target_after } else { target_before }
{
    if !mkdir_ok || !source_exists { (false,target_before) }
    else { (os_ok,os_target_after) }
}

pub struct World {
    pub database: Map<int, int>,
    pub files: Map<int, int>,
    pub directories: Set<int>,
    pub configuration: int,
}

pub open spec fn rollback(w: World, snapshot: Map<int,int>) -> World {
    World { database: snapshot, ..w }
}

// Caller passes the actual %File/CopyDir result, including partial effects on error.
// This is a relation, rather than the unsound premise "error means no file changed".
pub open spec fn copy_step(before: World, after: World, result_ok: bool,
    source: Map<int,int>, target_paths: Set<int>, overlay: bool, protected: bool,
    target_dir: int, directory_created: bool, mkdir_ok: bool, source_exists: bool) -> bool {
    &&& after.database == before.database
    &&& after.configuration == before.configuration
    &&& after.directories == if directory_created { before.directories.insert(target_dir) } else { before.directories }
    &&& (directory_created ==> mkdir_ok)
    &&& (result_ok ==> mkdir_ok && source_exists)
    &&& (result_ok ==> (forall |p: int| #[trigger] source.dom().contains(p) ==> after.files.dom().contains(p) && after.files[p] == source[p]))
    &&& (result_ok ==> (forall |p: int| !source.dom().contains(p) && (!target_paths.contains(p) || overlay || protected)
        ==> (after.files.dom().contains(p) == before.files.dom().contains(p)) && (before.files.dom().contains(p) ==> after.files[p] == before.files[p])))
    &&& (result_ok && !overlay && !protected ==> (forall |p: int| target_paths.contains(p) && !source.dom().contains(p) ==> !after.files.dom().contains(p)))
    // Failure can leave any subset/contents at target paths, preserving foreign paths.
    &&& (!result_ok ==> (forall |p: int| !target_paths.contains(p) ==> (after.files.dom().contains(p) == before.files.dom().contains(p)) && (before.files.dom().contains(p) ==> after.files[p] == before.files[p])))
}

pub open spec fn write_file(w: World, path: int, bytes: int) -> World {
    World { files: w.files.insert(path,bytes), ..w }
}

pub proof fn rollback_preserves_filesystem(w: World, snapshot: Map<int,int>)
    ensures rollback(w,snapshot).files == w.files,
            rollback(w,snapshot).directories == w.directories,
            rollback(w,snapshot).configuration == w.configuration,
            rollback(w,snapshot).database == snapshot,
{}

pub proof fn copied_file_survives_rollback(w: World, snapshot: Map<int,int>, path: int, bytes: int)
    ensures rollback(write_file(w,path,bytes),snapshot).files.dom().contains(path),
            rollback(write_file(w,path,bytes),snapshot).files[path] == bytes,
            rollback(write_file(w,path,bytes),snapshot).database == snapshot,
{}

// Source owner LoadDependencies: a lockfile creation exception is caught, printed and
// does not propagate. Dependencies failing before this block do propagate.
pub fn dependency_status(dependencies_ok: bool, requested: bool, top_level: bool, lock_write_ok: bool) -> (ok: bool)
    ensures ok == dependencies_ok
{ dependencies_ok }

pub fn lock_written(dependencies_ok: bool, requested: bool, top_level: bool, lock_write_ok: bool) -> (written: bool)
    ensures written == (dependencies_ok && requested && top_level && lock_write_ok)
{ dependencies_ok && requested && top_level && lock_write_ok }

// A module.Root of "" is concatenated with module-lock.json. The resulting relative
// path is resolved by %Stream.FileCharacter against process cwd, not reconstructed here.
pub fn lock_in_module_root(root_nonempty: bool) -> (in_root: bool)
    ensures in_root == root_nonempty
{ root_nonempty }

// The IRIS external iris merge child may change instance configuration independently
// of journaled writes; exit status is not a database transaction result.
pub open spec fn cpf_effect(w: World, resulting_configuration: int) -> World {
    World { configuration: resulting_configuration, ..w }
}
pub fn cpf_status(child_exit: i32) -> (ok: bool)
    ensures ok == (child_exit == 0)
{ child_exit == 0 }

pub proof fn cpf_database_rollback_boundary(w: World, snapshot: Map<int,int>, external: int)
    ensures rollback(cpf_effect(w,external),snapshot).configuration == external,
            rollback(cpf_effect(w,external),snapshot).database == snapshot,
{}

// Repo.Oras.PackageService.IsAvailable. HTTP response, not auth/pull success.
pub fn oras_available(transport_ok: bool, status: u16, www_authenticate: bool) -> (ok: bool)
    ensures ok == (transport_ok && ((200 <= status && status < 300) || (status == 401 && www_authenticate)))
{ transport_ok && ((200 <= status && status < 300) || (status == 401 && www_authenticate)) }

// ModuleSummary.Show tries stored Name first, then stored ExternalName, and never
// mutates the selected stored record. Name normalization is the IRIS datatype premise.
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum Selection { Name, ExternalName, Missing }
pub fn inspection_choice(name_found: bool, external_found: bool) -> (r: Selection)
    ensures r == if name_found { Selection::Name } else if external_found { Selection::ExternalName } else { Selection::Missing }
{
    if name_found { Selection::Name } else if external_found { Selection::ExternalName } else { Selection::Missing }
}
pub fn inspection_ok(name_found: bool, external_found: bool) -> (ok: bool)
    ensures ok == (name_found || external_found)
{ name_found || external_found }
pub open spec fn inspection_state(w: World) -> World { w }
pub proof fn inspection_preserves_stores(w: World)
    ensures inspection_state(w).database == w.database, inspection_state(w).files == w.files,
            inspection_state(w).configuration == w.configuration,
{}

pub struct Report { pub file_written: bool, pub status_ok: bool }

// Extension parsing and format-class lookup are separate input decisions. File export
// precedes summary format validation. Export failure may itself leave partial bytes;
// file_written records completed export only, not file absence.
pub open spec fn report_result(file_requested: bool, extension_supported: bool,
    export_ok: bool, summary_class_exists: bool, summary_ok: bool,
    fatal: bool, tests_ok: bool) -> Report {
    let exported = file_requested && extension_supported && export_ok;
    let file_stage_ok = !file_requested || (extension_supported && export_ok);
    Report { file_written: exported,
        status_ok: file_stage_ok && summary_class_exists && summary_ok && (!fatal || tests_ok) }
}

pub fn report_file_written(requested: bool, extension_supported: bool, export_ok: bool) -> (written: bool)
    ensures written == (requested && extension_supported && export_ok)
{ requested && extension_supported && export_ok }

pub fn report_status(requested: bool, extension_supported: bool, export_ok: bool,
    summary_class_exists: bool, summary_ok: bool, fatal: bool, tests_ok: bool) -> (ok: bool)
    ensures ok == ((!requested || (extension_supported && export_ok)) && summary_class_exists && summary_ok && (!fatal || tests_ok))
{ (!requested || (extension_supported && export_ok)) && summary_class_exists && summary_ok && (!fatal || tests_ok) }

pub proof fn report_error_can_keep_output()
    ensures report_result(true,true,true,false,true,true,true).file_written,
            !report_result(true,true,true,false,true,true,true).status_ok,
            report_result(true,true,true,true,true,true,true).status_ok,
{}

pub proof fn held_out_distinctions()
    ensures (Phase::Activate == Phase::Activate && !false && true),
            !(Phase::Activate == Phase::Activate && !true && true),
            (true && ((200 <= 401 && 401 < 300) || (401 == 401 && true))),
            !(true && ((200 <= 401 && 401 < 300) || (401 == 401 && false))),
{}

}

// Native-comparison harness. Decision functions above are verified; printing is test code.
fn main() {
    for (id,transport,status,header) in [
        ("ORAS-18171",true,200,true),("ORAS-18172",true,401,true),
        ("ORAS-18173",true,401,false),("ORAS-18174",true,403,true)] {
        println!("FORMAL {} available={}",id,oras_available(transport,status,header) as u8);
    }
    println!("FORMAL INSPECT-g5-independent-report status={}",inspection_ok(true,false) as u8);
    println!("FORMAL INSPECT-g5-independent-report-alias status={}",inspection_ok(false,true) as u8);
    println!("FORMAL INSPECT-G5-INDEPENDENT-REPORT-ALIAS status={}",inspection_ok(false,true) as u8);
    println!("FORMAL INSPECT-g5-independent-report-missing status={}",inspection_ok(false,false) as u8);
    let (ok,target)=file_copy_observation(false,true,true,true,true);
    println!("FORMAL F-YES status={} target={}",ok as u8,target as u8);
    let (ok,target)=file_copy_observation(false,true,false,true,true);
    println!("FORMAL F-MISSING status={} target={}",ok as u8,target as u8);
    println!("FORMAL LOCKFAIL status={} file={}",dependency_status(true,true,true,false) as u8,lock_written(true,true,true,false) as u8);
    println!("FORMAL LOCKOK status={} file={}",dependency_status(true,true,true,true) as u8,lock_written(true,true,true,true) as u8);
    println!("FORMAL C-KEEP target={}",(!clean_removes(Phase::Clean,true,true,false)) as u8);
    println!("FORMAL C-DELETE target={}",(!clean_removes(Phase::Clean,false,true,false)) as u8);
    println!("FORMAL IMMEDIATE copied={}",copies_before(Phase::Activate,false,true) as u8);
    println!("FORMAL DEFERRED copied={}",copies_before(Phase::Activate,true,true) as u8);
    println!("FORMAL REPORT-COMPOSE status={} file={}",report_status(true,true,true,false,true,true,true) as u8,report_file_written(true,true,true) as u8);
    println!("FORMAL REPORT-NEIGHBOR status={} file={}",report_status(true,true,true,true,true,true,true) as u8,report_file_written(true,true,true) as u8);
}
