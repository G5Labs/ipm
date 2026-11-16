/// Round-4 native check U21: full cleanup of IPM in USER while NCMAP borrows IPM from USER (enable -map).
/// Reads state from globals and the class dictionary only, because IPM's classes are gone after the cleanup.
Class Uplift.R4.Full Extends Uplift.Repo.Util
{

Parameter MAPNS = "NCMAP";

Parameter R4FIX = "/tmp/uplift/r4repo/fixtures/";

/// IPM state of one namespace, from globals, the class dictionary and the namespace mapping.
ClassMethod NsState(ns As %String, ByRef kv, prefix As %String)
{
    new $namespace
    set $namespace = ns
    set kv(prefix_"ipmMain") = ..ClassState("%IPM.Main")
    set kv(prefix_"pkgDest") = ##class(%SYS.Namespace).GetPackageDest(ns, "%IPM")
    set kv(prefix_"routineDest") = ##class(%SYS.Namespace).GetRoutineDest(ns, "%IPM.Main.1")
    set names = "", id = ""
    for {
        set id = $order(^IPM.Storage.ModuleD(id), 1, data)
        quit:id=""
        set names = names_$listget(data, 2)_" "
    }
    set kv(prefix_"moduleRecords") = $zstrip(names, ">W")
    set names = "", id = ""
    for {
        set id = $order(^IPM.Repo.DefinitionD(id), 1, data)
        quit:id=""
        set names = names_id_" "
    }
    set kv(prefix_"repoIds") = $zstrip(names, ">W")
    set kv(prefix_"fsCacheDIS") = $data(^IPM.Repo.Filesystem.CacheD)_"/"_$data(^IPM.Repo.Filesystem.CacheI)_"/"_$data(^IPM.Repo.Filesystem.CacheS)
    set kv(prefix_"ncScLibClass") = ..ClassState("NCScLib.Hello")
    set kv(prefix_"historyD") = $data(^IPM.General.HistoryD)
}

/// Run an IPM shell command in another namespace, catching a missing %IPM.Main.
ClassMethod ShellIn(ns As %String, cmd As %String, Output err As %String) As %Boolean
{
    new $namespace
    set $namespace = ns, err = ""
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
        set ok = $system.Status.IsOK(sc)
        set:'ok err = $system.Status.GetErrorText(sc)
    } catch e {
        set ok = 0, err = e.DisplayString()
    }
    quit ok
}

/// Step A: prepare, then full cleanup in USER with -force (no prompt).
ClassMethod RunA()
{
    // Safety copy of USER's IPM globals before the cleanup (removed in step B).
    kill ^UpliftR4Bak
    set g = "^IPM"
    for {
        set g = $order(^$GLOBAL(g))
        quit:(g="")||($extract(g, 1, 4)'="^IPM")
        continue:$extract(g, 1, 5)'="^IPM."
        merge ^UpliftR4Bak(g) = @g
    }
    set kv("loadInNcmap") = ..ShellIn(..#MAPNS, "load "_..#R4FIX_"nc-sc-lib/", .err)
    set kv("loadInNcmapErr") = err
    do ..NsState("USER", .kv, "pre.user.")
    do ..NsState(..#MAPNS, .kv, "pre.map.")
    do ##class(%IPM.Main).GetMapInfo(.from, .to)
    set kv("pre.mappedTo") = $select($data(to("USER", ..#MAPNS)):1, 1:0)
    do ..Emit("r4-repo-u21-prepare", .kv)
    kill kv
    set sc = ..Run("uninstall zpm -force -DClean.FullCleanup=1", .out)
    do ..Status(sc, .kv)
    set kv("printedAffects") = ..Has(out, "Will affect the following namespaces:USER, "_..#MAPNS)
    set kv("printedFullCleanup") = ..Has(out, "Performing full cleanup")
    set kv("printedPrompt") = ..Has(out, "Do you want to fully remove")
    set kv("printedMappedWarning") = (out [ "mapped") || (out [ "NCMAP: ")
    set kv("selfUninstallFlagLeft") = +$get(^||%IPM.SelfUninstallFlag)
    kill ^||%IPM.SelfUninstallFlag
    do ..NsState("USER", .kv, "post.user.")
    do ..NsState(..#MAPNS, .kv, "post.map.")
    set kv("post.map.shellOK") = ..ShellIn(..#MAPNS, "list", .err)
    set kv("post.map.shellErr") = $extract(err, 1, 120)
    do ..Emit("r4-repo-u21-full-cleanup-with-mapped-ns", .kv)
}

/// Step B (after IPM.Installer.setup in USER): read state back, check NCMAP, repair and tidy.
ClassMethod RunB(setupOK As %Boolean)
{
    set kv("setupOK") = setupOK
    do ..NsState("USER", .kv, "user.")
    do ..NsState(..#MAPNS, .kv, "map.")
    set kv("map.shellListOK") = ..ShellIn(..#MAPNS, "list", .err)
    set kv("map.shellListErr") = $extract(err, 1, 120)
    do ##class(%IPM.Main).GetMapInfo(.from, .to)
    set kv("mappedTo") = $select($data(to("USER", ..#MAPNS)):1, 1:0)
    set kv("userIpmVersion") = ##class(%IPM.Storage.Module).NameOpen("zpm").VersionString
    set kv("map.ncScLibListed") = 0
    do {
        new $namespace
        set $namespace = ..#MAPNS
        do ##class(%IPM.Main).GetListModules(,, .list)
        for i = 1:1:$get(list) { set:$listget(list(i))="nc-sc-lib" kv("map.ncScLibListed") = 1 }
    } while 0
    do ..Emit("r4-repo-u21-after-reinstall", .kv)

    // Tidy: leftover folder-cache index (F-UNIN-10) would block folder registries in USER.
    kill kv
    set kv("fsCacheBeforeRepair") = $data(^IPM.Repo.Filesystem.CacheD)_"/"_$data(^IPM.Repo.Filesystem.CacheI)_"/"_$data(^IPM.Repo.Filesystem.CacheS)
    if '$data(^IPM.Repo.Filesystem.CacheD) { kill ^IPM.Repo.Filesystem.CacheI, ^IPM.Repo.Filesystem.CacheS }
    set kv("fsRepoAddOK") = $system.Status.IsOK(..Run("repo -fs -n nc-r4-fs -path "_..#R4FIX, .o))
    set kv("fsRepoDeleteOK") = $system.Status.IsOK(..Run("repo -n nc-r4-fs -delete", .o))
    set kv("ncmapUninstallOK") = ..ShellIn(..#MAPNS, "uninstall nc-sc-lib", .err)
    do ..NsState("USER", .kv, "final.user.")
    do ..NsState(..#MAPNS, .kv, "final.map.")
    set kv("final.map.shellListOK") = ..ShellIn(..#MAPNS, "list", .err)
    set g = "", missing = ""
    for {
        set g = $order(^UpliftR4Bak(g))
        quit:g=""
        set:'$data(@g) missing = missing_g_" "
    }
    set kv("globalsMissingVsBackup") = $zstrip(missing, ">W")
    do ..Emit("r4-repo-u21-restored", .kv)
}

}
