/// P7: IPM removing itself with full cleanup or -force (UNIN-B51..B62, UNIN-D12). After the self-uninstall the
/// %IPM classes may be gone, so State reads globals and the class dictionary only.
Class Uplift.N2.Self Extends Uplift.N2.Base
{

ClassMethod Prepare(name As %String)
{
    do ..Setup()
    do ..Reset(1)
    set kv("loadSolo") = $system.Status.IsOK(..Sh("load "_..#FIX_"p9/top/n2-solo/"))
    set kv("loadChain") = $system.Status.IsOK(..Sh("load "_..#FIX_"p9/top/n2-ca/"))
    set kv("loadUpd") = $system.Status.IsOK(..Sh("load "_..#FIX_"p6/n2-upd-1.0.0/"))
    set ^N2UserData = "kept?"
    do ..State(.kv, "pre.")
    do ..Emit(name, .kv)
}

ClassMethod ClassState(c As %String) As %String
{
    quit $select($$$comClassDefined(c):"compiled", $$$defClassDefined(c):"defined", 1:"absent")
}

/// Read IPM state from globals and the class dictionary only.
ClassMethod State(ByRef kv, p As %String = "")
{
    for c = "%IPM.Main", "%IPM.Storage.Module", "%IPM.General.History", "%IPM.General.HistoryTemp", "N2Solo.Marker", "N2CA.Marker", "N2CB.Marker", "N2CC.Marker", "N2Upd.Upd.V1" {
        set kv(p_"cls:"_c) = ..ClassState(c)
    }
    set n = 0, c = "%IPM."
    for { set c = $order(^oddDEF(c)) quit:($extract(c, 1, 5) '= "%IPM.")  set n = n + 1 }
    set kv(p_"ipmClassDefs") = n
    set names = "", n = 0, id = ""
    for { set id = $order(^IPM.Storage.ModuleD(id), 1, data) quit:id=""  set n = n + 1, names = names_id_":"_$listget(data, 2)_" " }
    set kv(p_"moduleRecords") = n, kv(p_"moduleRecordNames") = $select(names="":"<none>", 1:$zstrip(names, ">W"))
    // every ^IPM.* global in this namespace that holds data (the global directory can list killed globals)
    set g = "^IPM", list = ""
    for { set g = $order(^$GLOBAL(g)) quit:(g = "")||(g ]] "^IPMZ")  continue:($extract(g, 1, 5) '= "^IPM.")  continue:'$data(@g)  set list = list_$extract(g, 2, *)_" " }
    set kv(p_"ipmGlobals") = $select(list="":"<none>", 1:$zstrip(list, ">W"))
    set kv(p_"iristempHistoryTemp") = $data(^IRIS.Temp.HistoryTempD)
    set kv(p_"iristempLoadedResource") = $data(^IRIS.Temp.IPM.LoadedResourceD)
    set kv(p_"userData") = $data(^N2UserData)
    set n = 0, id = "", last = ""
    for { set id = $order(^IPM.General.HistoryD(id), 1, data) quit:id=""  set n = n + 1, last = id }
    set kv(p_"historyRows") = n
    if last '= "" {
        set d = ^IPM.General.HistoryD(last)
        set kv(p_"historyLast") = "id"_last_"/"_$listget(d, 2)_"/"_$listget(d, 3)_"/committed:"_$listget(d, 12)_"/ended:"_($listget(d, 9)'="")_"/finalized:"_+$listget(d, 15)
    }
    set ns = $namespace
    new $namespace
    set $namespace = "%SYS"
    set kv(p_"sysIpmSettings") = $data(^IPM.settings)
    set kv(p_"sysClassMappingIPM") = ##class(Config.MapPackages).Exists(ns, "%IPM")
    set kv(p_"sysRoutineMappingIPM") = ##class(Config.MapRoutines).Exists(ns, "%IPM.*")
}

ClassMethod After(name As %String, sc As %Status)
{
    set kv("status") = $select($system.Status.IsOK(sc):"ok", 1:$system.Status.GetErrorText(sc))
    set kv("selfUninstallFlagLeft") = +$get(^||%IPM.SelfUninstallFlag)
    do ..State(.kv)
    do ..Emit(name, .kv)
}

/// After reinstalling IPM: which modules does IPM list again?
/// After a reinstall that follows a full cleanup: can a filesystem registry be added, and a module with update steps loaded?
ClassMethod PostCleanupProbe(name As %String)
{
    do ..Setup()
    set kv("cacheI") = $data(^IPM.Repo.Filesystem.CacheI), kv("cacheD") = $data(^IPM.Repo.Filesystem.CacheD)
    set kv("updateStepAnyMemberI") = $data(^IPM.UpdateStep.AnyMemberI), kv("updateStepAnyMemberD") = $data(^IPM.UpdateStep.AnyMemberD)
    set sc = ..Sh("repo -n n2 -fs -path "_..#FIX_"p9/repo/")
    set kv("repoAddFs") = $select($system.Status.IsOK(sc):"ok", 1:$system.Status.GetErrorText(sc))
    set sc = ..Sh("install n2-cb")
    set kv("installFromFs") = $select($system.Status.IsOK(sc):"ok", 1:$system.Status.GetErrorText(sc))
    set sc = ..Sh("load "_..#FIX_"p6/n2-upd-1.0.0/")
    set kv("loadWithUpdateSteps") = $select($system.Status.IsOK(sc):"ok", 1:$system.Status.GetErrorText(sc))
    set kv("stepV1.S1") = ..Step("N2Upd.Upd.V1", "S1")
    do ..Emit(name, .kv)
}

/// Cause check for a filesystem registry that finds nothing after a full cleanup: stale cache index globals.
ClassMethod FsCacheCause()
{
    do ..Setup()
    for step = "stale", "repaired" {
        kill kv
        if step = "repaired" {
            // manual repair: remove the cache index and stream globals the full cleanup left (Lifecycle/Module.cls#L41-L43 kills only CacheD)
            kill ^IPM.Repo.Filesystem.CacheI, ^IPM.Repo.Filesystem.CacheS
        }
        do ..Sh("repo -delete-all")
        do ..Sh("repo -n n2 -fs -path "_..#FIX_"p9/repo/")
        set kv("cacheD") = $data(^IPM.Repo.Filesystem.CacheD), kv("cacheI") = $data(^IPM.Repo.Filesystem.CacheI)
        set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT COUNT(*) AS N FROM %IPM_Repo_Filesystem.Cache"), kv("cacheRows") = $select(rs.%Next():rs.%Get("N"), 1:"?")
        set sc = ..Sh("install n2-cb")
        set kv("installN2cb") = $select($system.Status.IsOK(sc):"ok", 1:$system.Status.GetErrorText(sc))
        do ..Sh("uninstall -all")
        do ..Emit("nc-gap-P7-fscache-"_step, .kv)
    }
}

ClassMethod Readopt(name As %String)
{
    do ..State(.kv)
    set kv("ipmVersion") = $select(##class(%Dictionary.CompiledClass).%ExistsId("%IPM.Storage.Module"):##class(%IPM.Storage.Module).NameOpen("zpm").VersionString, 1:"n/a")
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %IPM_Storage.ModuleItem ORDER BY Name"), s = ""
    while rs.%Next() { set s = s_rs.%Get("Name")_" " }
    set kv("listInstalled") = $zstrip(s, ">W")
    set kv("reposDefined") = $data(^IPM.Repo.DefinitionD)
    do ..Emit(name, .kv)
}

}
