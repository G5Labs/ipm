//! Iteration-75 neighbours for the deprecated CSPApplication processor (module.csp.lifecycle#1 and #3).
//!
//! The deciding computation is the inherited iter57 `load` and `uninstall`:
//! - ResourceProcessor/CSPApplication.cls:335-373 (CreateOrUpdateCSPApp) modifies the application when
//!   Security.Applications.Exists(Url) and creates it otherwise, so a load always leaves an application.
//! - CSPApplication.cls:106-108 defaults an empty Directory (no DeployPath) to the module's resource folder,
//!   and :137-140 turns copying off when Directory is that folder. Only a given DeployPath gets a copy.
//! The inherited `load` also copies into a DeployPath that does not exist yet, although :163 requires the
//! directory to exist at After Reload. Native case a1 of iteration 57 (.g5/uplift/consolidated/iter57/native)
//! observed that copy, so the model follows the observation.
#![allow(non_snake_case)]
#[path = "../../consolidated/iter57/cspapp.rs"]
mod cspapp;

use vstd::prelude::*;
use cspapp::*;

verus! {

/// module.csp.lifecycle#1: a load with no existing application creates one, a load over an existing one
/// keeps one, and uninstall deletes it.
pub proof fn load_creates_and_uninstall_deletes(w: W, r: Res)
    requires w.app.is_none()
    ensures
        load(w, r).app.is_some(),
        load(admin(), r).app.is_some(),
        uninstall(load(w, r), r).app.is_none(),
{}

pub proof fn created_application_instance() {
    assert(start().app.is_none());
    assert(load(start(), Res { has_deploy: false }).app.is_some());
    assert(load(start(), Res { has_deploy: true }).app.is_some());
    assert(uninstall(load(start(), Res { has_deploy: true }), Res { has_deploy: true }).app.is_none());
}

/// module.csp.lifecycle#3: without DeployPath a load copies nothing, so the files of any deploy folder
/// stay as they were, even after the source changes. With DeployPath each load copies the current source.
pub proof fn no_deploy_path_no_copy(w: W)
    ensures
        load(w, Res { has_deploy: false }).deploy_files == w.deploy_files,
        load(W { source_files: w.source_files + 1, ..load(w, Res { has_deploy: false }) },
            Res { has_deploy: false }).deploy_files == w.deploy_files,
        load(w, Res { has_deploy: true }).deploy_files == w.source_files,
{}

pub proof fn copy_instances() {
    let w = W { app: None, deploy_files: 0, source_files: 1 };
    assert(load(w, Res { has_deploy: false }).deploy_files == 0);
    assert(load(w, Res { has_deploy: true }).deploy_files == 1);
    let edited = W { source_files: 2, ..load(w, Res { has_deploy: true }) };
    assert(load(edited, Res { has_deploy: true }).deploy_files == 2);
    let edited_nodeploy = W { source_files: 2, ..load(w, Res { has_deploy: false }) };
    assert(load(edited_nodeploy, Res { has_deploy: false }).deploy_files == 0);
}

} // verus!

fn main() {}
