/// Native checks on IPM itself that keep IPM installed: NC-UNIN-9 and NC-UNIN-4 (mapped namespace).
Class Uplift.Repo.UninIpm Extends Uplift.Repo.Unin
{

Parameter MAPNS = "NCMAP";

ClassMethod EnsureMappedNamespace(ByRef kv)
{
    set kv("createNs") = $system.Status.IsOK(##class(%IPM.Test.Utils).CreateNamespace(..#MAPNS))
    set sc = ..Run("enable -map -ns "_..#MAPNS_" -q", .out)
    do ..Status(sc, .kv, "enable.")
}

ClassMethod MapState(ns As %String, ByRef kv, prefix As %String)
{
    new $namespace
    set $namespace = ns
    set kv(prefix_"ipmMain") = ..ClassState("%IPM.Main")
    set kv(prefix_"ipmStorageModule") = ..ClassState("%IPM.Storage.Module")
    set kv(prefix_"zpmRecord") = $select(##class(%Dictionary.CompiledClass).%ExistsId("%IPM.Storage.Module"):##class(%IPM.Storage.Module).NameExists("zpm"), 1:"n/a")
    set kv(prefix_"pkgDest") = ##class(%SYS.Namespace).GetPackageDest(ns, "%IPM")
}

/// NC-UNIN-4: in a namespace that borrows IPM from USER, try every name for IPM.
ClassMethod Unin4()
{
    do ..EnsureMappedNamespace(.kv)
    do ..MapState(..#MAPNS, .kv, "pre.map.")
    do ..MapState("USER", .kv, "pre.user.")
    do ..Emit("nc-unin-4-mapped-setup", .kv)
    for name = "zpm", "ZPM", "ipm", "IPM" {
        kill kv
        new $namespace
        set $namespace = ..#MAPNS
        do ##class(%IPM.Main).GetMapInfo(.from, .to)
        set kv("mappedFrom") = $get(from(..#MAPNS))
        set sc = ..Run("uninstall "_name, .out)
        set $namespace = "USER"
        do ..Status(sc, .kv)
        set kv("refusedMapped") = ..Has($get(kv("error")), "mapped from namespace")
        set kv("printedPrompt") = ..Has(out, "Do you want to fully remove")
        set kv("printedListHeader") = ..Has(out, "The following modules are currently installed")
        set kv("printedCleanStart") = ..Has(out, "Clean START")
        do ..MapState(..#MAPNS, .kv, "post.map.")
        do ..MapState("USER", .kv, "post.user.")
        do ..Emit("nc-unin-4-mapped-uninstall-"_name, .kv)
    }
}

ClassMethod RunAll()
{
    for m = "Unin9", "Unin4" {
        try {
            do $classmethod("Uplift.Repo.UninIpm", m)
        } catch e {
            write !, "OBS|error-uninipm-"_m_"|error="_..Safe(e.DisplayString())_";", !
        }
        if $tlevel { trollback }
    }
    do ..Reset()
}

}
