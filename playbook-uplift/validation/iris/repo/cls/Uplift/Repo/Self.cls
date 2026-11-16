/// IPM removing itself (UNIN-B51..B62, F-UNIN-3). After the self-uninstall the %IPM classes may be gone,
/// so the After* methods read globals and the class dictionary only.
Class Uplift.Repo.Self Extends Uplift.Repo.Util
{

ClassMethod Prepare()
{
    do ..Reset()
    set kv("loadA") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-ghost-a/", .o))
    set kv("loadW") = $system.Status.IsOK(..Run("load "_..#FIX_"nc-web-w/", .o))
    set kv("repoFs") = $system.Status.IsOK(..Run("repo -fs -n nc-repo-self -path "_..#FIX_"fsrepo-extra/", .o))
    do ..State(.kv, "pre.")
    do ..Emit("nc-unin-self-prepare", .kv)
}

/// Read IPM state from globals and the class dictionary only.
ClassMethod State(ByRef kv, prefix As %String = "")
{
    set kv(prefix_"ipmMain") = ..ClassState("%IPM.Main")
    set kv(prefix_"ipmStorageModule") = ..ClassState("%IPM.Storage.Module")
    set kv(prefix_"ipmStudioDocModule") = ..ClassState("%IPM.StudioDocument.Module")
    set names = "", n = 0, zpm = 0, id = ""
    for {
        set id = $order(^IPM.Storage.ModuleD(id), 1, data)
        quit:id=""
        set n = n + 1, nm = $listget(data, 2)
        set names = names_id_":"_nm_" "
        set:$zconvert(nm, "L")="zpm" zpm = zpm + 1
    }
    set kv(prefix_"moduleRecords") = n
    set kv(prefix_"moduleRecordNames") = $zstrip(names, ">W")
    set kv(prefix_"zpmRecords") = zpm
    set kv(prefix_"moduleIndex") = $data(^IPM.Storage.ModuleI)
    set kv(prefix_"repoDefinitions") = $data(^IPM.Repo.DefinitionD)
    set kv(prefix_"manifestStreams") = $data(^IPM.StudioDoc.ModuleStreamD)
    set c = 0, k = ""
    for { set k = $order(^IPM.General.HistoryD(k)) quit:k=""  set c = c + 1 }
    set kv(prefix_"historyRows") = c
    set kv(prefix_"ncGhostAClass") = ..ClassState("NCGhostA.Hello")
    set kv(prefix_"webAppNcW") = ..WebAppExists("/nc-w")
}

ClassMethod After(name As %String, sc As %Status)
{
    do ..Status(sc, .kv)
    set kv("selfUninstallFlagLeft") = +$get(^||%IPM.SelfUninstallFlag)
    do ..State(.kv)
    do ..Emit(name, .kv)
}

/// F-UNIN-3: uninstall IPM by its external name, output captured (no prompt is expected by the code reading).
ClassMethod UninstallByExternalName(name As %String)
{
    set sc = ..Run("uninstall "_name, .out)
    do ..Status(sc, .kv)
    set kv("printedPrompt") = ..Has(out, "Do you want to fully remove")
    set kv("printedListHeader") = ..Has(out, "The following modules are currently installed")
    set kv("printedOrphanWarning") = ..Has(out, "module(s) orphaned")
    set kv("printedExitMessage") = ..Has(out, "Exiting shell")
    set kv("printedCleanSuccess") = ..Has(out, "Clean SUCCESS")
    set kv("selfUninstallFlagLeft") = +$get(^||%IPM.SelfUninstallFlag)
    do ..State(.kv)
    do ..Emit("nc-unin-self-external-name-"_name, .kv)
}

/// After reinstalling IPM: are the left modules adopted again, and how many IPM records are there?
ClassMethod Readopt(name As %String)
{
    do ..State(.kv)
    set kv("ipmVersion") = $select(##class(%Dictionary.CompiledClass).%ExistsId("%IPM.Storage.Module"):##class(%IPM.Storage.Module).NameOpen("zpm").VersionString, 1:"n/a")
    set kv("aInstalled") = ..Installed("nc-ghost-a")
    set kv("wInstalled") = ..Installed("nc-web-w")
    do ##class(%IPM.Main).GetListModules(,, .list)
    set kv("listInstalledCount") = $get(list)
    set s = ""
    for i = 1:1:$get(list) { set s = s_$listget(list(i))_" " }
    set kv("listInstalled") = $zstrip(s, ">W")
    do ..Emit(name, .kv)
}

}
