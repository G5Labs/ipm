/// Probes added by the review fix for modules 05-06 (review/REVIEW.md finding 8). Observation names: nc-repo-fix-*.
Class Uplift.Repo.Fix Extends Uplift.Repo.Util
{

/// UNIN-B46: `uninstall -all` without -force removes a dependent before the module it needs.
/// nc-ghost-b needs nc-ghost-a; name order would take nc-ghost-a first, so the order seen discriminates.
ClassMethod UninAllOrder()
{
    do ..Reset()
    set kv("loadA") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-ghost-a/", .o))
    set kv("loadB") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-ghost-b/", .o))
    set kv("pre.aInstalled") = ..Installed("nc-ghost-a")
    set kv("pre.bInstalled") = ..Installed("nc-ghost-b")
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT MAX(ID) AS M FROM %IPM_General.History")
    do rs.%Next()
    set id0 = +rs.%Get("M")
    set sc = ..Run("uninstall -all", .out)
    do ..Status(sc, .kv)
    set pa = $find(out, "Uninstalling nc-ghost-a"), pb = $find(out, "Uninstalling nc-ghost-b")
    set kv("printedA") = (pa > 0)
    set kv("printedB") = (pb > 0)
    set kv("printedBBeforeA") = ((pb > 0) && (pa > 0) && (pb < pa))
    set kv("printedSkipped") = ..Has(out, "Clean SKIPPED")
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Package, Action, Success FROM %IPM_General.History WHERE ID > ? ORDER BY ID", id0)
    set rows = ""
    while rs.%Next() {
        set rows = rows_$select(rows="":"", 1:",")_rs.%Get("Package")_"/"_rs.%Get("Action")_"/"_$select($system.Status.IsOK(rs.%Get("Success")):"success", 1:"failure")
    }
    set kv("historyRows") = rows
    set kv("aInstalled") = ..Installed("nc-ghost-a")
    set kv("bInstalled") = ..Installed("nc-ghost-b")
    set kv("aClass") = ..ClassState("NCGhostA.Hello")
    set kv("bClass") = ..ClassState("NCGhostB.Hello")
    set kv("ipmInstalled") = ..Installed("zpm")
    do ..Emit("nc-repo-fix-unin-all-order", .kv)
    do ..Reset()
}

}
