/// Probes for the namespace-mapping and sync models (models/verus/ns_mapping.rs, sync_baseline.rs).
/// Env: IRIS mapping facts the model's environment contract needs that were never observed directly.
/// Predict: scenarios the model predicts, run before comparing. Scratch namespaces NCM1-NCM6 (created here).
Class Uplift.Nsp.Model
{

ClassMethod U() As %String [ CodeMode = expression ]
{
"Uplift.Nsp.Util"
}

/// ns exists and has no %IPM package, %IPM.* routine or IPM.Repo.Definition* mapping (all activated).
ClassMethod Clean(ns As %String) As %Status
{
    set sc = ##class(Uplift.Nsp.Util).MkNs(ns)
    quit:'$system.Status.IsOK(sc) sc
    set sc = ##class(%IPM.Utils.Module).RemovePackageMapping(ns, "%IPM", 0)
    set sc = $$$ADDSC(sc, ##class(%IPM.Utils.Module).RemoveRoutineMapping(ns, "%IPM.*", "ALL", 0))
    for s = "D", "S", "I" { set sc = $$$ADDSC(sc, ##class(%IPM.Utils.Module).RemoveGlobalMapping(ns, "IPM.Repo.Definition"_s, 0)) }
    quit sc
}

ClassMethod RtnDest(ns As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(%SYS.Namespace).GetRoutineDest(ns, "%IPM.Main.1")
}

ClassMethod Look(ns As %String, prefix As %String, ByRef kv)
{
    set u = ..U()
    set kv(prefix_"Maps") = $classmethod(u, "Maps", ns)
    set kv(prefix_"PkgDest") = $classmethod(u, "PkgDest", ns)
    set kv(prefix_"RtnDest") = ..RtnDest(ns)
    set kv(prefix_"HasIPM") = $classmethod(u, "HasIPM", ns)
}

/// E2: a mapping added with skip-activation is not seen by GetPackageDest until it is activated.
/// E7: RemoveRoutineMapping(ns, "%IPM.*", 1, .changed), as UnmapIPM calls it, removes nothing.
/// E1: precedence of the %IPM.* routine mapping over the %IPM package mapping.
/// E3': MapGlobalEquivalently(..., 1) is active at once (its 1 lands in AddGlobalMapping's collation slot).
ClassMethod Env()
{
    set u = ..U()
    new $namespace
    set $namespace = "USER"
    // E2
    set kv("clean") = $classmethod(u, "St", ..Clean("NCM1"))
    do ..Look("NCM1", "before", .kv)
    set kv("addSkip") = $classmethod(u, "St", ##class(%IPM.Utils.Module).AddPackageMapping("NCM1", "%IPM", "USER", 1))
    do ..Look("NCM1", "afterAdd", .kv)
    set kv("activate") = $classmethod(u, "St", ##class(%IPM.Utils.Module).ActivateMappings("NCM1"))
    do ..Look("NCM1", "afterActivate", .kv)
    do $classmethod(u, "Emit", "nc-nsp-model-env-activation", .kv)
    kill kv
    // E1 combinations, each activated; NCC is a second local install
    set kv("nccZpm") = $classmethod(u, "Mod", "zpm", "NCC")
    set kv("nccPkgDest") = $classmethod(u, "PkgDest", "NCC")
    set kv("clean") = $classmethod(u, "St", ..Clean("NCM2"))
    do ..Look("NCM2", "none", .kv)
    do ##class(%IPM.Utils.Module).AddPackageMapping("NCM2", "%IPM", "USER", 0)
    do ..Look("NCM2", "pkgUser", .kv)
    do ##class(%IPM.Utils.Module).AddRoutineMapping("NCM2", "%IPM.*", "ALL", "NCC", 0)
    do ..Look("NCM2", "pkgUserRtnNcc", .kv)
    do ##class(%IPM.Utils.Module).RemovePackageMapping("NCM2", "%IPM", 0)
    do ..Look("NCM2", "rtnNcc", .kv)
    do ##class(%IPM.Utils.Module).AddPackageMapping("NCM2", "%IPM", "NCC", 0)
    do ##class(%IPM.Utils.Module).RemoveRoutineMapping("NCM2", "%IPM.*", "ALL", 0)
    do ##class(%IPM.Utils.Module).AddRoutineMapping("NCM2", "%IPM.*", "ALL", "USER", 0)
    do ..Look("NCM2", "pkgNccRtnUser", .kv)
    do $classmethod(u, "Emit", "nc-nsp-model-env-precedence", .kv)
    kill kv
    // E7
    set kv("clean") = $classmethod(u, "St", ..Clean("NCM3"))
    do ##class(%IPM.Utils.Module).AddRoutineMapping("NCM3", "%IPM.*", "ALL", "USER", 0)
    set kv("mapsBefore") = $classmethod(u, "Maps", "NCM3")
    set changed = 0
    set kv("slotCall") = $classmethod(u, "St", ##class(%IPM.Utils.Module).RemoveRoutineMapping("NCM3", "%IPM.*", 1, .changed))
    set kv("slotCallChanged") = changed
    set kv("mapsAfterSlotCall") = $classmethod(u, "Maps", "NCM3")
    set kv("allCall") = $classmethod(u, "St", ##class(%IPM.Utils.Module).RemoveRoutineMapping("NCM3", "%IPM.*", "ALL", 0))
    set kv("mapsAfterAllCall") = $classmethod(u, "Maps", "NCM3")
    do $classmethod(u, "Emit", "nc-nsp-model-env-rtn-slot", .kv)
    kill kv
    // E3'
    set kv("clean") = $classmethod(u, "St", ..Clean("NCM4"))
    set kv("repoBefore") = $classmethod(u, "GlobalDest", "NCM4", "IPM.Repo.DefinitionD")
    set kv("mapGlobal") = $classmethod(u, "St", ##class(%IPM.Utils.Build).MapGlobalEquivalently("IPM.Repo.DefinitionD", "USER", "NCM4", 1))
    set kv("repoAfterMapGlobal") = $classmethod(u, "GlobalDest", "NCM4", "IPM.Repo.DefinitionD")
    set kv("mapPackage") = $classmethod(u, "St", ##class(%IPM.Utils.Build).MapPackageEquivalently("%IPM", "USER", "NCM4", 1))
    set kv("pkgDestAfterMapPackage") = $classmethod(u, "PkgDest", "NCM4")
    set kv("maps") = $classmethod(u, "Maps", "NCM4")
    set kv("userRepoDest") = $classmethod(u, "GlobalDest", "USER", "IPM.Repo.DefinitionD")
    do $classmethod(u, "Emit", "nc-nsp-model-env-repo-activation", .kv)
    do ..Clean("NCM4")
}

/// Model predictions for enable -map -repos and unmap on a namespace that has only a %IPM package mapping
/// (the state %IPM.Test.Utils.CreateNamespace leaves).
ClassMethod Predict()
{
    set u = ..U()
    new $namespace
    set $namespace = "USER"
    set kv("userRepos") = $classmethod(u, "RepoNames", "USER")
    set kv("clean") = $classmethod(u, "St", ..Clean("NCM5"))
    do ##class(%IPM.Utils.Module).AddPackageMapping("NCM5", "%IPM", "USER", 0)
    set kv("mapsBefore") = $classmethod(u, "Maps", "NCM5")
    set kv("pkgDestBefore") = $classmethod(u, "PkgDest", "NCM5")
    set sc = $classmethod(u, "Run", "enable -map -repos -ns NCM5", .out)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("skipLine") = $classmethod(u, "Line", .out, "repository mapping of NCM5")
    set kv("mapsAfter") = $classmethod(u, "Maps", "NCM5")
    set kv("repoDestAfter") = $classmethod(u, "GlobalDest", "NCM5", "IPM.Repo.DefinitionD")
    set kv("reposAfter") = $classmethod(u, "RepoNames", "NCM5")
    do $classmethod(u, "Emit", "nc-nsp-model-repos-premapped", .kv)
    kill kv
    set kv("clean") = $classmethod(u, "St", ..Clean("NCM6"))
    do ##class(%IPM.Utils.Module).AddPackageMapping("NCM6", "%IPM", "USER", 0)
    set kv("mapsBefore") = $classmethod(u, "Maps", "NCM6")
    set kv("hasIPMBefore") = $classmethod(u, "HasIPM", "NCM6")
    set sc = $classmethod(u, "Run", "unmap -ns NCM6", .out)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("noMapping") = $classmethod(u, "Has", .out, "No mapping found for NCM6")
    set kv("mapsAfter") = $classmethod(u, "Maps", "NCM6")
    set kv("pkgDestAfter") = $classmethod(u, "PkgDest", "NCM6")
    set kv("hasIPMAfter") = $classmethod(u, "HasIPM", "NCM6")
    do $classmethod(u, "Emit", "nc-nsp-model-unmap-package-only", .kv)
}

ClassMethod Source() As %String
{
    set m = ##class(%Dictionary.MethodDefinition).%OpenId("SyncTest.SuperClass||GetValue")
    quit:'$isobject(m) "<no-method>"
    set t = m.Implementation.Read(1000)
    quit $select(t [ """edited""":"edited", t [ """original""":"original", 1:"other")
}

/// F-SYNC-1, reload case (sync_witness_reload_then_revert): load -dev, edit, reload, exact revert, sync.
ClassMethod SyncReload()
{
    set u = ..U()
    set s = "Uplift.Nsp.Sync"
    do $classmethod(s, "Fresh")
    set f = "/tmp/nc-sync/src/cls/SyncTest/SuperClass.cls"
    set sc = $classmethod(u, "Run", "load /tmp/nc-sync/ -dev", .o1, 0)
    set kv("loadDev") = $classmethod(u, "St", sc)
    set sha0 = $classmethod(u, "FileSha", f)
    do $classmethod(u, "Replace", f, """original""", """edited""")
    set sc = $classmethod(u, "Run", "reload sync-test", .o2)
    set kv("reload") = $classmethod(u, "St", sc)
    set kv("sourceAfterReload") = ..Source()
    set kv("rowAfterReload") = $classmethod(s, "Row")
    do ##class(%File).CopyFile("/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/sync-test/src/cls/SyncTest/SuperClass.cls", f, 1)
    set kv("restoredExactBytes") = ($classmethod(u, "FileSha", f) = sha0)
    set sc = $classmethod(u, "Run", "sync sync-test -verbose", .o3)
    set kv("sync") = $classmethod(u, "St", sc)
    set kv("nothingToSync") = $classmethod(u, "Has", .o3, "Nothing to sync")
    set kv("sourceAfterSync") = ..Source()
    set kv("rowAfterSync") = $classmethod(s, "Row")
    do $classmethod(u, "Emit", "nc-nsp-model-sync-reload-revert", .kv)
}

/// OUT premise, -skip-sync case: load -dev, edit, load -dev -skip-sync (compiles, no stamp), exact revert, sync.
ClassMethod SyncSkipSync()
{
    set u = ..U()
    set s = "Uplift.Nsp.Sync"
    do $classmethod(s, "Fresh")
    set f = "/tmp/nc-sync/src/cls/SyncTest/SuperClass.cls"
    set sc = $classmethod(u, "Run", "load /tmp/nc-sync/ -dev", .o1, 0)
    set kv("loadDev") = $classmethod(u, "St", sc)
    set sha0 = $classmethod(u, "FileSha", f)
    do $classmethod(u, "Replace", f, """original""", """edited""")
    set sc = $classmethod(u, "Run", "load /tmp/nc-sync/ -dev -skip-sync", .o2, 0)
    set kv("loadSkipSync") = $classmethod(u, "St", sc)
    set kv("valueAfterLoad") = $classmethod(s, "Value")
    set kv("rowAfterLoad") = $classmethod(s, "Row")
    do ##class(%File).CopyFile("/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/sync-test/src/cls/SyncTest/SuperClass.cls", f, 1)
    set kv("restoredExactBytes") = ($classmethod(u, "FileSha", f) = sha0)
    set sc = $classmethod(u, "Run", "sync sync-test -verbose", .o3)
    set kv("sync") = $classmethod(u, "St", sc)
    set kv("nothingToSync") = $classmethod(u, "Has", .o3, "Nothing to sync")
    set kv("valueAfterSync") = $classmethod(s, "Value")
    set kv("sourceAfterSync") = ..Source()
    set kv("rowAfterSync") = $classmethod(s, "Row")
    do $classmethod(u, "Emit", "nc-nsp-model-sync-skipsync-revert", .kv)
    do $classmethod(s, "Fresh")
}


/// GetMapInfo as IPM computes it, with every namespace's zpm record and %IPM destination (for the GetMapInfo
/// tie rule: the last source in name order that shares a database wins).
ClassMethod MapInfo()
{
    set u = ..U()
    new $namespace
    set $namespace = "USER"
    do ##class(%IPM.Main).GetMapInfo(.from, .to)
    set ns = "", out = ""
    for { set ns = $order(from(ns), 1, src) quit:ns=""  set out = out_ns_">"_src_" " }
    set kv("isMappedFrom") = out
    do ##class(%IPM.Main).GetListNamespace(.all)
    set ns = "", srcs = "", dests = ""
    for {
        set ns = $order(all(ns)) quit:ns=""
        set z = $classmethod(u, "Mod", "zpm", ns)
        set d = $piece($classmethod(u, "PkgDest", ns), "/", *-1)
        set dests = dests_ns_":"_d_" "
        if (z '= "-") && (z '= "<no-ipm>") { set srcs = srcs_ns_" " }
    }
    set kv("sources") = srcs
    set kv("dests") = dests
    do $classmethod(u, "Emit", "nc-nsp-model-mapinfo", .kv)
}

}
