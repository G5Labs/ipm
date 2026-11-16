/// N1 (NSP-B1..B6): state after the IPM installer runs in USER of a fresh IRIS.
Class Uplift.Nsp.Fresh
{

ClassMethod N1()
{
    new $namespace
    set $namespace = "%SYS"
    // first run (log 89-nsp-fresh-install) listed no namespaces from the SQL query; use %SYS.Namespace.ListAll
    do ##class(%SYS.Namespace).ListAll(.all)
    set list = "", mapped = "", rtnMapped = "", ns = ""
    for {
        set ns = $order(all(ns))
        quit:ns=""
        continue:$zconvert(ns, "U")'=ns
        set list = list_$select(list="":"", 1:" ")_ns
        if ##class(Config.MapPackages).Exists(ns, "%IPM") {
            do ##class(Config.MapPackages).Get(ns, "%IPM", .p)
            set mapped = mapped_$select(mapped="":"", 1:" ")_ns_":"_$get(p("Database"))
        }
        if ##class(Config.MapRoutines).Exists(ns, "%IPM.*") {
            do ##class(Config.MapRoutines).Get(ns, "%IPM.*", .r)
            set rtnMapped = rtnMapped_$select(rtnMapped="":"", 1:" ")_ns_":"_$get(r("Database"))
        }
    }
    set kv("namespaces") = list
    set kv("pkgMappings") = $select(mapped="":"<none>", 1:mapped)
    set kv("rtnMappings") = $select(rtnMapped="":"<none>", 1:rtnMapped)
    set kv("userPkgDest") = ##class(%SYS.Namespace).GetPackageDest("USER", "%IPM")
    set kv("sysPkgDest") = ##class(%SYS.Namespace).GetPackageDest("%SYS", "%IPM")
    set kv("defaultRegistry") = $get(^IPM.settings("default_registry"))
    set kv("settingsGlobalDest") = ##class(%SYS.Namespace).GetGlobalDest("%SYS", "IPM.settings")
    set kv("legacyZpm") = ($order(^oddCOM("%ZPM.PackageManager.")) [ "%ZPM.PackageManager.")
    set $namespace = "USER"
    set kv("userZpm") = ##class(Uplift.Nsp.Util).Mod("zpm")
    set kv("userRepos") = ##class(Uplift.Nsp.Util).RepoNames()
    set kv("userRepoRows") = $data(^IPM.Repo.DefinitionD)
    set sc = ##class(Uplift.Nsp.Util).Run("repo -list", .out)
    set kv("repoListStatus") = ##class(Uplift.Nsp.Util).St(sc)
    set kv("repoListLines") = +$get(out)
    set kv("samplesHasIPM") = $select(##class(Uplift.Nsp.Util).NsExists("SAMPLES"):##class(Uplift.Nsp.Util).HasIPM("SAMPLES"), 1:"n/a")
    set kv("sysHasIPM") = ##class(Uplift.Nsp.Util).HasIPM("%SYS")
    do ##class(Uplift.Nsp.Util).Emit("nc-nsp-N1-fresh-install-all-ns", .kv)
}

}
