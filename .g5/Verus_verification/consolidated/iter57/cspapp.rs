#![allow(non_snake_case)]
// Iteration57: the deprecated CSPApplication resource processor across load, reload and uninstall.
//
// Source roles at pin 571e04a1 (see .g5/uplift/consolidated/iter57/bindings.json):
//   ResourceProcessor/CSPApplication.cls::OnBeforePhase   Directory = DeployPath, or the module's source folder
//   ::OnPhaseInternal / OnAfterPhase                       Before Activate: create or modify the application; after
//                                                          Reload: copy the source folder into Directory when they differ;
//                                                          before Clean: delete the application, purging Directory's files
//   ::CreateOrUpdateCSPApp                                 Modify with every ATTRIBUTES property, so properties the
//                                                          manifest does not declare take processor defaults
//   ::DeleteCSPApp                                         %Library.EnsembleMgr.deletePortal(Url, purgeFiles)
//   ::%OnNew                                               deprecation warning once per module and process
//   Utils/File.cls::MakeDirectoryWriteable, CopyDir, WarnIfInInstallDirectory, IsProtectedDirectory
// Premises (native-qualified): Security.Applications Create/Modify/Get, deletePortal removing the
// directory's files, chmod/copy providers.
use vstd::prelude::*;

verus! {

pub struct App { pub path_is_deploy: bool, pub admin_desc: bool, pub admin_timeout: bool, pub unauth: bool }
pub struct W { pub app: Option<App>, pub deploy_files: int, pub source_files: int }   // file version, 0 = absent

pub struct Res { pub has_deploy: bool }

// Load (and reload): create or modify; every attribute is written, so admin-only values reset.
pub open spec fn load(w: W, r: Res) -> W {
    let app = App { path_is_deploy: r.has_deploy, admin_desc: false, admin_timeout: false, unauth: true };
    W { app: Some(app), deploy_files: if r.has_deploy { w.source_files } else { w.deploy_files }, ..w }
}

// Uninstall: delete the application and purge the files of its directory.
pub open spec fn uninstall(w: W, r: Res) -> W {
    if r.has_deploy { W { app: None, deploy_files: 0, ..w } } else { W { app: None, source_files: 0, ..w } }
}

pub proof fn no_deploy_path_uninstall_deletes_source(w: W)
    requires w.source_files != 0
    ensures uninstall(load(w, Res { has_deploy: false }), Res { has_deploy: false }).source_files == 0
{}
pub proof fn deploy_path_uninstall_keeps_source(w: W)
    ensures uninstall(load(w, Res { has_deploy: true }), Res { has_deploy: true }).source_files == w.source_files
{}
pub proof fn deploy_path_uninstall_purges_deploy(w: W)
    ensures uninstall(load(w, Res { has_deploy: true }), Res { has_deploy: true }).deploy_files == 0,
            uninstall(load(w, Res { has_deploy: true }), Res { has_deploy: true }).app.is_none()
{}
pub proof fn takeover_resets_undeclared(w: W, a: App)
    requires w.app == Some(a), a.admin_desc, a.admin_timeout
    ensures !load(w, Res { has_deploy: true }).app.unwrap().admin_desc, !load(w, Res { has_deploy: true }).app.unwrap().admin_timeout
{}
pub proof fn reload_recopies(w: W)
    ensures load(W { source_files: 2, ..load(w, Res { has_deploy: true }) }, Res { has_deploy: true }).deploy_files == 2
{}

pub open spec fn b(x: bool) -> int { if x { 1 } else { 0 } }
pub open spec fn start() -> W { W { app: None, deploy_files: 0, source_files: 1 } }
pub open spec fn admin() -> W { W { app: Some(App { path_is_deploy: false, admin_desc: true, admin_timeout: true, unauth: false }), deploy_files: 0, source_files: 1 } }

} // verus!

fn main() {}
