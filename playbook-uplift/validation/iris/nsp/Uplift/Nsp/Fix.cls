/// Review fixes for modules 07-09 (REVIEW.md finding 8). Run in USER of ipm-uplift-iris-nsp.
///   B3()  NSP-B3 with several namespaces present: the installer runs in a new namespace NCFRESH3 while USER (local
///         install), the NCA..NCT scratch namespaces and %SYS exist; every namespace's IPM mappings are compared.
///   N20() N20 re-capture: the summary line of `sync` (all dev modules, one broken) is the "Errors:" line.
Class Uplift.Nsp.Fix
{

ClassMethod U() As %String [ CodeMode = expression ]
{
"Uplift.Nsp.Util"
}

/// s(ns) = IPM mapping state of every namespace: Config maps, the [Map.ns] cpf lines naming IPM, and where %IPM,
/// %IPM.Main.1 and ^IPM.Repo.DefinitionD resolve.
ClassMethod Snap(Output s)
{
    kill s
    do ##class(%SYS.Namespace).ListAll(.all)
    set ns = ""
    for {
        set ns = $order(all(ns))
        quit:ns=""
        continue:$zconvert(ns, "U")'=ns
        continue:ns="%ALL"
        set s(ns) = ##class(Uplift.Nsp.Util).Maps(ns)_" cpf:"_##class(Uplift.Nsp.Ns).CpfMap(ns)
            _" pkgDest:"_##class(Uplift.Nsp.Util).PkgDest(ns)
            _" rtnDest:"_##class(Uplift.Nsp.Ns).RtnDest(ns, "%IPM.Main.1")
            _" repoDest:"_##class(Uplift.Nsp.Util).GlobalDest(ns, "IPM.Repo.DefinitionD")
    }
}

ClassMethod B3()
{
    set u = ..U(), t = "NCFRESH3"
    new $namespace
    set kv("mkNs") = $classmethod(u, "St", $classmethod(u, "MkNs", t))
    // CreateNamespace maps package %IPM from the caller; remove it so NCFRESH3 has no IPM (a fresh namespace).
    set kv("dropPkgMap") = $classmethod(u, "St", ##class(Uplift.Nsp.Ns).DropIPMPkgMap(t))
    set kv("mapUplift") = $classmethod(u, "St", ##class(Uplift.Nsp.Ns).MapUplift(t))
    set kv("targetMapsBefore") = $classmethod(u, "Maps", t)
    set kv("targetHasIPMBefore") = $classmethod(u, "HasIPM", t)
    do ..Snap(.before)
    set n = 0, ns = "", others = "", withIPM = ""
    for { set ns = $order(before(ns)) quit:ns=""  set n = n + 1
          if ns '= t { set others = others_$select(others="":"", 1:" ")_ns
                       set:$classmethod(u, "HasIPM", ns) withIPM = withIPM_$select(withIPM="":"", 1:" ")_ns } }
    set kv("namespaceCount") = n
    set kv("otherNamespaces") = others
    set kv("othersRunningIPM") = withIPM
    set $namespace = t
    set sc = ##class(Uplift.Nsp.Ns).InstallHere()
    set kv("installerStatus") = $classmethod(u, "St", sc)
    set $namespace = "USER"
    do ..Snap(.after)
    set changed = "", ns = ""
    for { set ns = $order(before(ns)) quit:ns=""
          continue:ns=t
          set:$get(after(ns))'=before(ns) changed = changed_$select(changed="":"", 1:" ")_ns_"{"_before(ns)_" -> "_$get(after(ns))_"}" }
    set ns = ""
    for { set ns = $order(after(ns)) quit:ns=""  set:'$data(before(ns)) changed = changed_" new:"_ns }
    set kv("otherNamespacesChanged") = $select(changed="":"none", 1:changed)
    set kv("targetMapsAfter") = $classmethod(u, "Maps", t)
    set kv("targetPkgDestAfter") = $classmethod(u, "PkgDest", t)
    set kv("targetCodeDB") = $classmethod(u, "CodeDB", t)
    set kv("targetZpm") = $classmethod(u, "Mod", "zpm", t)
    set kv("userSnapUnchanged") = ($get(after("USER")) = $get(before("USER")))
    set kv("sysSnapUnchanged") = ($get(after("%SYS")) = $get(before("%SYS")))
    do $classmethod(u, "Emit", "nc-nsp-fix-B3-fresh-install-many-ns", .kv)
}

/// Same steps as Uplift.Nsp.Sync:N20, capturing the summary's module and error lines.
ClassMethod N20()
{
    set u = ..U(), s = "Uplift.Nsp.Sync"
    set data = $parameter(s, "DATA"), dir = $parameter(s, "DIR"), super = $parameter(s, "SUPER")
    do $classmethod(s, "Fresh")
    set cdir = "/tmp/nc-sdep-c/"
    do $classmethod(u, "CopyDir", data_"sync-dep/c/", cdir)
    set sc1 = $classmethod(u, "Run", "load "_dir_" -dev", .o1, 0)
    set sc2 = $classmethod(u, "Run", "load "_cdir_" -dev", .o2, 0)
    set kv("loads") = $classmethod(u, "St", sc1)_" / "_$classmethod(u, "St", sc2)
    do $classmethod(s, "Break", dir_super)
    do $classmethod(u, "Replace", cdir_"src/cls/SyncDepC/Base.cls", """c-original""", """c-edited""")
    set sc = $classmethod(u, "Run", "sync", .o3)
    set kv("syncAll") = $classmethod(u, "St", sc)
    set kv("summaryHeader") = $classmethod(u, "Line", .o3, "Sync Summary")
    set kv("summaryChecked") = $classmethod(u, "Line", .o3, "Modules checked:")
    set kv("summaryUpdated") = $classmethod(u, "Line", .o3, "Modules updated:")
    set kv("summaryErrors") = $classmethod(u, "Line", .o3, "Errors:")
    set kv("summaryFailedModuleLine") = $classmethod(u, "Line", .o3, "sync-test   ERROR")
    set kv("depCValue") = $classmethod("SyncDepC.Base", "Version")
    set kv("syncTestRow") = $classmethod(s, "Row")
    do $classmethod(u, "Emit", "nc-nsp-fix-sync-N20-summary", .kv)
    do $classmethod(s, "Fresh")
}

}
