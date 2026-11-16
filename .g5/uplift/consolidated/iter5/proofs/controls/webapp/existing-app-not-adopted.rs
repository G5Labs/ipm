//! Web application lifecycle: create or adopt on install, keep or lose on failure, delete on
//! uninstall (including a blocked one), orphan on upgrade. Owners: module.webapp.* in
//! concepts/modules.g5.md and uninstall.dependents.as-built in features/uninstall.g5.md.
//! Sources: ResourceProcessor/WebApplication.cls::{OnBeforePhase,CreateOrUpdateWebApp,DeleteWebApp,OnPhase};
//! Storage/Module.cls::ExecutePhases (processor OnBeforePhase before the lifecycle method);
//! Lifecycle/Base.cls::{%Validate,%Clean,CheckBeforeClean}.
use vstd::prelude::*;
verus! {

/// Properties are abstract key -> value maps; apps are name -> properties in %SYS.
pub type Props = Map<int, int>;
pub type Apps = Map<int, Props>;

/// Declared keys win; keys the manifest does not declare keep their old values.
pub open spec fn merged(old: Props, declared: Props) -> Props {
    Map::new(old.dom().union(declared.dom()),
             |k: int| if declared.contains_key(k) { declared[k] } else { old[k] })
}

/// Configure/Activate: create, or adopt any existing app of that name. No owner is recorded.
pub open spec fn install_app(apps: Apps, name: int, declared: Props) -> Apps {
    if apps.contains_key(name) { apps }
    else { apps.insert(name, declared) }
}

/// Modify is called only when a declared key differs from the stored value.
pub open spec fn modify_called(old: Props, declared: Props) -> bool {
    exists|k: int| declared.contains_key(k) && (!old.contains_key(k) || old[k] != declared[k])
}

/// The lifecycle's Validate never dispatches OnPhase("Validate") to resource processors, so the
/// processor's unknown-attribute check does not run.
pub open spec fn validate_dispatches() -> bool { false }
pub open spec fn refused_at_validate(has_unknown_key: bool) -> bool {
    validate_dispatches() && has_unknown_key
}

/// Another installed module already lists a resource with this name: the manifest is refused at import.
pub open spec fn refused_at_import(name_owned_by_other_module: bool) -> bool { name_owned_by_other_module }

/// A failed install inside a transaction rolls the %SYS write back; without one it stays,
/// together with the module record.
pub struct FailedInstall { pub apps: Apps, pub module_recorded: bool }
pub open spec fn failed_after_activate(before: Apps, after_activate: Apps, transaction: bool) -> FailedInstall {
    if transaction { FailedInstall { apps: before, module_recorded: false } }
    else { FailedInstall { apps: after_activate, module_recorded: true } }
}

pub open spec fn blocked(dependents: nat, force: bool, level: nat) -> bool {
    !force && dependents > 0 && level > 0
}

pub struct CleanResult { pub apps: Apps, pub module_kept: bool, pub ok: bool }

/// As built: processors delete the module's declared app before the dependent check, and a blocked
/// uninstall still reports success.
pub open spec fn uninstall(apps: Apps, declared_name: Option<int>, dependents: nat, force: bool, level: nat) -> CleanResult {
    let after = match declared_name { Some(n) => apps.remove(n), None => apps };
    CleanResult { apps: after, module_kept: blocked(dependents, force, level), ok: true }
}

/// Proposed (P DESIGN-SIMPLIFICATIONS RC-6, C:S17): check first; a blocked uninstall changes nothing and fails.
pub open spec fn repaired_uninstall(apps: Apps, declared_name: Option<int>, dependents: nat, force: bool, level: nat) -> CleanResult {
    if blocked(dependents, force, level) { CleanResult { apps, module_kept: true, ok: false } }
    else { uninstall(apps, declared_name, dependents, force, level) }
}

/// Upgrading to a manifest without the app leaves the app; uninstall then only knows the new manifest.
pub open spec fn upgrade_drops_declaration(apps: Apps) -> Apps { apps }

pub proof fn lemma_blocked_uninstall_deletes(apps: Apps, n: int)
    requires apps.contains_key(n)
    ensures !uninstall(apps, Some(n), 1, false, 1).apps.contains_key(n),
            uninstall(apps, Some(n), 1, false, 1).module_kept,
            uninstall(apps, Some(n), 1, false, 1).ok,
            repaired_uninstall(apps, Some(n), 1, false, 1).apps == apps,
            !repaired_uninstall(apps, Some(n), 1, false, 1).ok,
            repaired_uninstall(apps, Some(n), 0, false, 1) == uninstall(apps, Some(n), 0, false, 1),
{
}

pub proof fn lemma_adoption_keeps_undeclared(apps: Apps, n: int, declared: Props, k: int)
    requires apps.contains_key(n), apps[n].contains_key(k), !declared.contains_key(k)
    ensures install_app(apps, n, declared)[n][k] == apps[n][k]
{
}

pub proof fn webapp_neighbors() {
    let desc = 1int; let timeout = 2int;
    let admin = map![desc => 10int, timeout => 777int];
    let declared_b = map![desc => 20int];
    let apps = Map::<int, Props>::empty().insert(5, admin);
    // Adoption: declared Description replaced, undeclared Timeout kept, Modify called.
    let adopted = install_app(apps, 5, declared_b);
    assert(adopted[5][desc] == 20 && adopted[5][timeout] == 777);
    assert(modify_called(admin, declared_b)) by { assert(declared_b.contains_key(desc) && admin[desc] != declared_b[desc]); }
    assert(!modify_called(merged(admin, declared_b), declared_b));
    // Uninstall of the adopting module deletes the administrator's app.
    assert(!uninstall(adopted, Some(5), 0, false, 1).apps.contains_key(5));
    // A fresh install creates exactly the declared properties.
    let fresh = install_app(Map::empty(), 6, declared_b);
    assert(fresh.contains_key(6) && fresh[6] == declared_b);
    // Unknown attributes are not refused; a name another module lists is.
    assert(!refused_at_validate(true));
    assert(refused_at_import(true) && !refused_at_import(false));
    // Failed install: rolled back with a transaction, kept without one.
    let after = install_app(Map::empty(), 7, declared_b);
    assert(!failed_after_activate(Map::empty(), after, true).apps.contains_key(7));
    assert(failed_after_activate(Map::empty(), after, false).apps.contains_key(7));
    assert(failed_after_activate(Map::empty(), after, false).module_recorded);
    // Blocked uninstall deletes, keeps the module and succeeds; force removes the module.
    lemma_blocked_uninstall_deletes(fresh, 6);
    assert(!uninstall(fresh, Some(6), 1, true, 1).module_kept);
    // Upgrade without the app, then uninstall: the app survives as an orphan.
    let up = upgrade_drops_declaration(fresh);
    assert(uninstall(up, None, 0, false, 1).apps.contains_key(6));
}

}
fn main() {}
