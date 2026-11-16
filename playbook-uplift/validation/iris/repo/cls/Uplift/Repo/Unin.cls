/// Native checks NC-UNIN-1..10 (module 05) and environment facts for module 05.
Class Uplift.Repo.Unin Extends Uplift.Repo.Util
{

ClassMethod Load(dir As %String) As %Status
{
    quit ..Run("load "_..#FIX_dir_"/", .o)
}

/// NC-UNIN-1: uninstall of a module that a dependent needs, with a web app.
ClassMethod Unin1()
{
    do ..Reset()
    set kv("loadDep") = $system.Status.IsOK(..Load("nc-web-dep"))
    set kv("loadTop") = $system.Status.IsOK(..Load("nc-web-top"))
    set kv("webBefore") = ..WebAppExists("/nc-dep")
    set h0 = ..HistoryCount()
    set sc = ..Run("uninstall nc-web-dep", .out)
    do ..Status(sc, .kv)
    set kv("printedSkipped") = ..Has(out, "Clean SKIPPED - required by 1 other module")
    set kv("printedNamesTop") = ..Has(out, "nc-web-top")
    set kv("printedCleanSuccess") = ..Has(out, "Clean SUCCESS")
    set kv("depInstalled") = ..Installed("nc-web-dep")
    set kv("depClass") = ..ClassState("NCWebDep.Hello")
    set kv("webAfter") = ..WebAppExists("/nc-dep")
    set kv("historyRowsAdded") = ..HistoryCount() - h0
    set kv("lastHistoryDep") = ..LastHistory("nc-web-dep")
    do ..Emit("nc-unin-1-blocked-uninstall", .kv)
}

/// NC-UNIN-2: <module> clean on a module with a web app and an installer class, no dependencies.
ClassMethod Unin2()
{
    do ..Reset()
    set kv("load") = $system.Status.IsOK(..Load("nc-web-w"))
    set kv("webBefore") = ..WebAppExists("/nc-w")
    kill ^NCProbe("w")
    set sc = ..Run("nc-web-w clean", .out)
    do ..Status(sc, .kv)
    set kv("printedCleanSuccess") = ..Has(out, "Clean SUCCESS")
    set kv("installed") = ..Installed("nc-web-w")
    set kv("class") = ..ClassState("NCW.Hello")
    set kv("webAfter") = ..WebAppExists("/nc-w")
    set kv("unconfigureCalls") = +$get(^NCProbe("w", "unconfigure"))
    set kv("uninstallHookCalls") = +$get(^NCProbe("w", "uninstall"))
    do ..Emit("nc-unin-2-clean-own-module", .kv)
}

ClassMethod PurgeRun(name As %String, cmd As %String)
{
    do ..Reset()
    set kv("load") = $system.Status.IsOK(..Load("nc-purge"))
    set item = $classmethod("NCPurge.Item", "%New")
    set item.Name = "row1"
    set kv("insert") = $system.Status.IsOK(item.%Save())
    kill item
    set dataGlobal = ##class(%Dictionary.CompiledStorage).%OpenId("NCPurge.Item||Default").DataLocation
    set kv("dataGlobal") = dataGlobal
    set kv("rowsBefore") = $data(@dataGlobal)
    set sc = ..Run(cmd, .out)
    do ..Status(sc, .kv)
    set kv("installed") = ..Installed("nc-purge")
    set kv("class") = ..ClassState("NCPurge.Item")
    set n = $get(^NCProbe("purge"))
    set kv("hookCalls") = +n
    set kv("hookPurgeArg") = $select(n:$get(^NCProbe("purge", n)), 1:"<not called>")
    set kv("hookParamsPurge") = $select(n:$get(^NCProbe("purge", n, "paramPurge")), 1:"<not called>")
    set kv("dataAfter") = $data(@dataGlobal)
    set kv("dataNodeAfter") = $listget($get(@dataGlobal@(1)), 2)
    do ..Emit(name, .kv)
    kill @dataGlobal
}

/// NC-UNIN-3: uninstall with and without -purge; read the purge flag the installer saw and the table data.
ClassMethod Unin3()
{
    do ..PurgeRun("nc-unin-3-uninstall-plain", "uninstall nc-purge")
    do ..PurgeRun("nc-unin-3-uninstall-purge", "uninstall nc-purge -purge")
}

/// NC-UNIN-5: a dependent with an unresolvable need does not block uninstall.
ClassMethod Unin5()
{
    do ..Reset()
    set kv("loadA") = $system.Status.IsOK(..Load("nc-ghost-a"))
    set kv("loadB") = $system.Status.IsOK(..Load("nc-ghost-b"))
    // control: b needs a, so a is kept
    set sc = ..Run("uninstall nc-ghost-a", .out)
    do ..Status(sc, .kv, "control.")
    set kv("control.skipped") = ..Has(out, "Clean SKIPPED")
    set kv("control.aInstalled") = ..Installed("nc-ghost-a")
    // give b a need that no registry has (no registries are configured)
    set b = ##class(%IPM.Storage.Module).NameOpen("nc-ghost-b")
    set ref = ##class(%IPM.Storage.ModuleReference).%New()
    set ref.Name = "nc-ghost-missing"
    set ref.VersionString = "1.0.0"
    do b.Dependencies.Insert(ref)
    set kv("editB") = $system.Status.IsOK(b.%Save())
    kill b, ref
    set sc = ##class(%IPM.Utils.Module).GetDependentsList(.list, .errs, "nc-ghost-a")
    set kv("dependentsListed") = list.Count()
    set kv("graphErrors") = +$get(errs)
    set sc = ..Run("uninstall nc-ghost-a", .out)
    do ..Status(sc, .kv)
    set kv("skipped") = ..Has(out, "Clean SKIPPED")
    set kv("aInstalled") = ..Installed("nc-ghost-a")
    set kv("aClass") = ..ClassState("NCGhostA.Hello")
    set kv("bInstalled") = ..Installed("nc-ghost-b")
    do ..Emit("nc-unin-5-unresolvable-dependent", .kv)
}

/// NC-UNIN-6: .GBL resources with and without the Global attribute.
ClassMethod Unin6()
{
    do ..Reset()
    set kv("load1") = $system.Status.IsOK(..Load("nc-gbl1"))
    set kv("load2") = $system.Status.IsOK(..Load("nc-gbl2"))
    set kv("dataBefore") = $data(^NC.Data)
    set kv("data2Before") = $data(^NC.Data2)
    set sc1 = ..Run("uninstall nc-gbl1", .o1)
    set sc2 = ..Run("uninstall nc-gbl2", .o2)
    do ..Status(sc1, .kv, "u1.")
    do ..Status(sc2, .kv, "u2.")
    set kv("installed1") = ..Installed("nc-gbl1")
    set kv("installed2") = ..Installed("nc-gbl2")
    set kv("dataAfter") = $data(^NC.Data)
    set kv("data2After") = $data(^NC.Data2)
    do ..Emit("nc-unin-6-global-resources", .kv)
}

ClassMethod FilesIn(dir As %String, ByRef kv, prefix As %String)
{
    for f = "shared.txt", "shared-file.txt", "mine.txt", "other.txt" {
        set kv(prefix_f) = ##class(%File).Exists(dir_f)
    }
    set kv(prefix_"dir") = ##class(%File).DirectoryExists(dir)
}

/// NC-UNIN-7: a module without the keep flag copies into the folder of a module with it, then is removed.
ClassMethod Unin7()
{
    do ..Reset()
    set dir = ##class(%File).NormalizeDirectory("../fc-keep-on-uninstall/", $system.Util.ManagerDirectory())
    do ##class(%File).RemoveDirectoryTree(dir)
    // control first: the keep module alone (as in IPM's own test)
    set kv("loadKeep") = $system.Status.IsOK(..Run("load "_..#IPMDATA_"file-copy-test/fc-keep-on-uninstall/", .o))
    do ..FilesIn(dir, .kv, "before.")
    set sc = ..Run("uninstall fc-keep-on-uninstall", .out)
    do ..Status(sc, .kv)
    do ..FilesIn(dir, .kv, "after.")
    do ..Emit("nc-unin-7-control-keep-module", .kv)
    kill kv
    do ##class(%File).RemoveDirectoryTree(dir)
    set kv("loadKeep") = $system.Status.IsOK(..Run("load "_..#IPMDATA_"file-copy-test/fc-keep-on-uninstall/", .o))
    set kv("loadOther") = $system.Status.IsOK(..Load("nc-fc-other"))
    do ..FilesIn(dir, .kv, "before.")
    set sc = ..Run("uninstall nc-fc-other", .out)
    do ..Status(sc, .kv)
    do ..FilesIn(dir, .kv, "after.")
    set kv("keepInstalled") = ..Installed("fc-keep-on-uninstall")
    do ..Emit("nc-unin-7-shared-target-folder", .kv)
    do ..Run("uninstall fc-keep-on-uninstall", .o)
    do ##class(%File).RemoveDirectoryTree(dir)
}

/// NC-UNIN-8: uninstall -r where the dependency's uninstall hook fails.
ClassMethod Unin8()
{
    do ..Reset()
    set kv("loadQ") = $system.Status.IsOK(..Load("nc-q"))
    set kv("loadP") = $system.Status.IsOK(..Load("nc-p"))
    set ^NCProbe("q", "fail") = 1
    set sc = ..Run("uninstall nc-p -r", .out)
    do ..Status(sc, .kv)
    set kv("qHookCalls") = +$get(^NCProbe("q", "uninstall"))
    set kv("pInstalled") = ..Installed("nc-p")
    set kv("pClass") = ..ClassState("NCP.Hello")
    set kv("qInstalled") = ..Installed("nc-q")
    set kv("qClass") = ..ClassState("NCQ.Hello")
    set kv("historyP") = ..LastHistory("nc-p")
    set kv("historyQ") = ..LastHistory("nc-q")
    do ..Emit("nc-unin-8-dependency-hook-fails", .kv)
    // leave nothing behind
    kill ^NCProbe("q")
}

/// NC-UNIN-10: orphans with and without -type.
ClassMethod Unin10()
{
    do ..Reset()
    set kv("loadOwned") = $system.Status.IsOK(..Load("nc-ghost-a"))
    set def = ##class(%Dictionary.ClassDefinition).%New()
    set def.Name = "NC.Orphan"
    set kv("createOrphan") = $system.Status.IsOK(def.%Save())
    set kv("compileOrphan") = $system.Status.IsOK($system.OBJ.Compile("NC.Orphan", "-d"))
    set before = ..ModuleGlobalSnapshot()
    for t = "CLS", "" {
        set p = $select(t="":"all.", 1:"cls.")
        set sc = ..Run("orphans"_$select(t="":"", 1:" -type "_t), .out)
        do ..Status(sc, .kv, p)
        set lines = 0, pct = 0, owned = 0, orphan = 0, types = ""
        for i = 1:1:$length(out, $char(10)) {
            set l = $zstrip($piece(out, $char(10), i), "<>W")
            continue:l=""
            set lines = lines + 1
            set:$extract(l)="%" pct = pct + 1
            set:$zconvert(l, "U")="NC.ORPHAN.CLS" orphan = 1
            set:$zconvert(l, "U")="NCGHOSTA.HELLO.CLS" owned = 1
            set ext = $zconvert($piece(l, ".", *), "U")
            set:ext'="" types(ext) = $get(types(ext)) + 1
        }
        set kv(p_"lines") = lines
        set kv(p_"percentItems") = pct
        set kv(p_"listsOrphan") = orphan
        set kv(p_"listsOwnedClass") = owned
        set e = "", s = ""
        for { set e = $order(types(e), 1, n) quit:e=""  set s = s_e_":"_n_" " }
        set kv(p_"byType") = s
        kill types
    }
    set kv("moduleGlobalsUnchanged") = (before = ..ModuleGlobalSnapshot())
    do ..Emit("nc-unin-10-orphans", .kv)
    do $system.OBJ.Delete("NC.Orphan", "-d")
}

ClassMethod ModuleGlobalSnapshot() As %String
{
    set s = "", g = "^IPM.Storage.ModuleD"
    for { set g = $query(@g) quit:g=""  quit:$qsubscript(g, 0)'="^IPM.Storage.ModuleD"  set s = s_g_"="_$zcrc(@g, 7)_"," }
    quit $zcrc(s, 7)
}

/// NC-UNIN-9: zpm clean with two other modules installed.
ClassMethod Unin9()
{
    do ..Reset()
    set kv("loadA") = $system.Status.IsOK(..Load("nc-ghost-a"))
    set kv("loadW") = $system.Status.IsOK(..Load("nc-web-w"))
    set sc = ..Run("zpm clean", .out)
    do ..Status(sc, .kv)
    set others = ##class(%SQL.Statement).%ExecDirect(, "SELECT COUNT(*) AS N FROM %IPM_Storage.ModuleItem WHERE Name <> 'zpm'")
    do others.%Next()
    set kv("otherModules") = others.%Get("N")
    set kv("printedOrphaned") = ..Has(out, "module(s) orphaned")
    set kv("printedOrphanCountMatches") = ..Has(out, "WARNING: "_kv("otherModules")_" module(s) orphaned")
    set kv("zpmInstalled") = ..Installed("zpm")
    set kv("aInstalled") = ..Installed("nc-ghost-a")
    set kv("wInstalled") = ..Installed("nc-web-w")
    set kv("ipmMainCompiled") = ..ClassState("%IPM.Main")
    set kv("selfUninstallFlag") = +$get(^||%IPM.SelfUninstallFlag)
    kill ^||%IPM.SelfUninstallFlag
    do ..Emit("nc-unin-9-clean-on-ipm", .kv)
}

/// Facts: name and external-name lookups are case-blind; collation of the indexes.
ClassMethod EnvCase()
{
    for n = "zpm", "ZPM", "Zpm", "ipm", "IPM" {
        set kv("NameExists."_n) = ##class(%IPM.Storage.Module).NameExists(n)
        set kv("ExternalNameExists."_n) = ##class(%IPM.Storage.Module).ExternalNameExists(n)
    }
    set m = ##class(%IPM.Storage.Module).NameOpen("zpm")
    set kv("storedName") = m.Name
    set kv("storedExternalName") = m.ExternalName
    // SQLUPPER index subscripts are upper case with a leading space
    set kv("nameIndexFirstSub") = $order(^IPM.Storage.ModuleI("Name", ""))
    set kv("nameIndexHasUpperZPM") = $data(^IPM.Storage.ModuleI("Name", " ZPM"))
    set kv("extIndexHasUpperIPM") = $data(^IPM.Storage.ModuleI("ExternalName", " IPM"))
    do ..Emit("nc-unin-env-name-case", .kv)
}

/// Fact: $system.OBJ.Delete keeps the data of a persistent class unless asked.
ClassMethod EnvObjDelete()
{
    for flags = "-d", "-de" {
        kill kv
        do ..MakeEnvItem()
        set item = $classmethod("NCEnv.Item", "%New")
        set item.Name = "x"
        do item.%Save()
        kill item
        set kv("dataBefore") = $data(^NCEnv.ItemD)
        set sc = $system.OBJ.Delete("NCEnv.Item", flags)
        do ..Status(sc, .kv)
        set kv("classAfter") = ..ClassState("NCEnv.Item")
        set kv("dataAfter") = $data(^NCEnv.ItemD)
        set kv("flags") = flags
        do ..Emit("nc-unin-env-obj-delete"_$select(flags="-d":"-default", 1:"-e-flag"), .kv)
        kill ^NCEnv.ItemD, ^NCEnv.ItemI
    }
}

ClassMethod MakeEnvItem()
{
    if ##class(%Dictionary.ClassDefinition).%ExistsId("NCEnv.Item") { do $system.OBJ.Delete("NCEnv.Item", "-d") }
    set def = ##class(%Dictionary.ClassDefinition).%New()
    set def.Name = "NCEnv.Item"
    set def.Super = "%Persistent"
    set p = ##class(%Dictionary.PropertyDefinition).%New()
    set p.Name = "Name"
    set p.Type = "%String"
    do def.Properties.Insert(p)
    $$$ThrowOnError(def.%Save())
    $$$ThrowOnError($system.OBJ.Compile("NCEnv.Item", "-d"))
}

/// NC-UNIN-10 detail: where do the items listed by orphans without -type come from?
ClassMethod Unin10Detail()
{
    set sc = ..Run("orphans", .out)
    set mapped = 0, notMapped = 0, mappedList = "", localList = ""
    for i = 1:1:$length(out, $char(10)) {
        set l = $zstrip($piece(out, $char(10), i), "<>W")
        continue:l=""
        continue:$zconvert($piece(l, ".", *), "U")="CLS"
        if ##class(%Library.RoutineMgr).IsMapped(l) {
            set mapped = mapped + 1, mappedList = mappedList_l_" "
        } else {
            set notMapped = notMapped + 1, localList = localList_l_" "
        }
    }
    set kv("nonClsMapped") = mapped
    set kv("nonClsLocal") = notMapped
    set kv("mappedItems") = $extract(mappedList, 1, 280)
    set kv("localItems") = $extract(localList, 1, 280)
    set kv("ensLibPrintingDispatcherDest") = ##class(%SYS.Namespace).GetPackageDest($namespace, "EnsLib.Printing")
    set kv("userRoutineDb") = ##class(%SYS.Namespace).GetRoutineDest($namespace)
    set kv("ensLookupTableDest") = ##class(%SYS.Namespace).GetGlobalDest($namespace, "Ens.LookupTable")
    do ..Emit("nc-unin-10-orphans-detail", .kv)
}

ClassMethod RunAll()
{
    for m = "EnvCase", "EnvObjDelete", "Unin1", "Unin2", "Unin3", "Unin5", "Unin6", "Unin7", "Unin8", "Unin10" {
        try {
            do $classmethod("Uplift.Repo.Unin", m)
        } catch e {
            write !, "OBS|error-unin-"_m_"|error="_..Safe(e.DisplayString())_";", !
        }
        if $tlevel { trollback }
    }
}

}
