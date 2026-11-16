/// Round-4 native checks for DEC-UNIN-3 (phase-scoped needs and uninstall).
/// Reuses the helpers of Uplift.Repo.Util (already compiled in USER). OBS lines: OBS|r4-repo-<name>|k=v;...
Class Uplift.R4.Repo Extends Uplift.Repo.Util
{

Parameter R4FIX = "/tmp/uplift/r4repo/fixtures/";

ClassMethod LoadFix(name As %String) As %Boolean
{
    quit $system.Status.IsOK(..Run("load "_..#R4FIX_name_"/", .o))
}

ClassMethod R4Reset()
{
    if $tlevel { trollback }
    for m = "nc-sc-user", "nc-sc-vuser", "nc-sc-plain", "nc-sc-lib" {
        if ..Installed(m) { do ..Run("uninstall "_m_" -force", .o) }
    }
    for c = "NCScLib.Hello", "NCScUser.Hello", "NCScVUser.Hello", "NCScPlain.Hello" {
        if ##class(%Dictionary.ClassDefinition).%ExistsId(c) { do $system.OBJ.Delete(c, "-d") }
    }
}

/// Number and names of dependents that the uninstall check sees.
ClassMethod Dependents(name As %String, ByRef kv, prefix As %String)
{
    kill list, errs
    set sc = ##class(%IPM.Utils.Module).GetDependentsList(.list, .errs, name)
    set kv(prefix_"GetDependentsListOK") = $system.Status.IsOK(sc)
    set names = ""
    for i=1:1:list.Count() { set names = names_$listbuild(list.GetAt(i).Name) }
    set kv(prefix_"GetDependentsListCount") = list.Count()
    set kv(prefix_"GetDependentsListNames") = $listtostring(names, ",")
    set kv(prefix_"graphErrors") = +$get(errs)
}

/// Stored scope of each dependency of a module.
ClassMethod StoredScopes(name As %String) As %String
{
    set m = ##class(%IPM.Storage.Module).NameOpen(name)
    quit:'$isobject(m) "<none>"
    set out = ""
    for i=1:1:m.Dependencies.Count() {
        set d = m.Dependencies.GetAt(i)
        set out = out_$listbuild(d.Name_":"_$select(d.Scope="":"<unscoped>", 1:d.Scope))
    }
    quit $listtostring(out, ",")
}

/// A: a module that needs the target only in one phase (test or verify).
ClassMethod CaseScoped(user As %String, scope As %String)
{
    do ..R4Reset()
    kill kv
    set kv("loadLib") = ..LoadFix("nc-sc-lib")
    set kv("loadUser") = ..LoadFix(user)
    set kv("userStoredDeps") = ..StoredScopes(user)
    do ..Dependents("nc-sc-lib", .kv, "before")
    do ..Run("list-dependents nc-sc-lib", .ldOut)
    set kv("listDependentsNamesUser") = ..Has(ldOut, user)
    do ..Run("show nc-sc-lib", .showOut)
    set kv("showNamesUser") = ..Has(showOut, user)
    set h0 = ..HistoryCount()
    set sc = ..Run("uninstall nc-sc-lib", .out)
    do ..Status(sc, .kv)
    set kv("printedSkipped") = ..Has(out, "Clean SKIPPED")
    set kv("printedCleanSuccess") = ..Has(out, "Clean SUCCESS")
    set kv("libInstalledAfter") = ..Installed("nc-sc-lib")
    set kv("libClassAfter") = ..ClassState("NCScLib.Hello")
    set kv("userInstalledAfter") = ..Installed(user)
    set kv("historyRowsAdded") = ..HistoryCount() - h0
    set kv("lastHistoryLib") = ..LastHistory("nc-sc-lib")
    do ..Emit("r4-repo-unin3-"_scope_"-scoped-dependent", .kv)
}

/// B: control, a module that needs the target in every phase.
ClassMethod CaseUnscoped()
{
    do ..R4Reset()
    kill kv
    set kv("loadLib") = ..LoadFix("nc-sc-lib")
    set kv("loadPlain") = ..LoadFix("nc-sc-plain")
    set kv("plainStoredDeps") = ..StoredScopes("nc-sc-plain")
    do ..Dependents("nc-sc-lib", .kv, "before")
    do ..Run("show nc-sc-lib", .showOut)
    set kv("showNamesPlain") = ..Has(showOut, "nc-sc-plain")
    set sc = ..Run("uninstall nc-sc-lib", .out)
    do ..Status(sc, .kv)
    set kv("printedSkipped") = ..Has(out, "Clean SKIPPED")
    set kv("libInstalledAfter") = ..Installed("nc-sc-lib")
    set kv("lastHistoryLib") = ..LastHistory("nc-sc-lib")
    do ..Emit("r4-repo-unin3-control-unscoped-dependent", .kv)
}

/// C: uninstall -r of the module that has the scoped need.
ClassMethod CaseRecurse()
{
    do ..R4Reset()
    kill kv
    set kv("loadLib") = ..LoadFix("nc-sc-lib")
    set kv("loadUser") = ..LoadFix("nc-sc-user")
    set sc = ..Run("uninstall nc-sc-user -r", .out)
    do ..Status(sc, .kv)
    set kv("userInstalledAfter") = ..Installed("nc-sc-user")
    set kv("libInstalledAfter") = ..Installed("nc-sc-lib")
    set kv("libClassAfter") = ..ClassState("NCScLib.Hello")
    set kv("lastHistoryLib") = ..LastHistory("nc-sc-lib")
    do ..Emit("r4-repo-unin3-recurse-removes-scoped-need", .kv)
}

/// D: uninstall -all orders by every stored dependency, scoped or not.
ClassMethod CaseAll()
{
    do ..R4Reset()
    kill kv
    set kv("loadLib") = ..LoadFix("nc-sc-lib")
    set kv("loadUser") = ..LoadFix("nc-sc-user")
    set rs0 = ##class(%SQL.Statement).%ExecDirect(, "SELECT MAX(ID) AS M FROM %IPM_General.History")
    do rs0.%Next()
    set maxId = +rs0.%Get("M")
    set sc = ..Run("uninstall -all", .out)
    do ..Status(sc, .kv)
    set kv("userInstalledAfter") = ..Installed("nc-sc-user")
    set kv("libInstalledAfter") = ..Installed("nc-sc-lib")
    set kv("zpmInstalledAfter") = ..Installed("zpm")
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Package FROM %IPM_General.History WHERE ID > ? ORDER BY ID", maxId)
    set order = ""
    while rs.%Next() { set order = order_$listbuild(rs.%Get("Package")) }
    set kv("historyOrder") = $listtostring(order, ",")
    do ..Emit("r4-repo-unin3-all-order", .kv)
}

ClassMethod RunAll()
{
    do ..CaseScoped("nc-sc-user", "test")
    do ..CaseScoped("nc-sc-vuser", "verify")
    do ..CaseUnscoped()
    do ..CaseRecurse()
    do ..CaseAll()
    do ..R4Reset()
    kill kv
    set kv("leftModules") = ""
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %IPM_Storage.ModuleItem")
    while rs.%Next() { set kv("leftModules") = kv("leftModules")_rs.%Get("Name")_"," }
    do ..Emit("r4-repo-unin3-cleanup", .kv)
}

}
