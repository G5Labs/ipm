/// Probes for IRIS facts and code paths that the Verus models of modules 05-06 depend on
/// (models/verus/uninstall.rs, models/verus/settings.rs). Observation names: nc-repo-model-*.
/// Each probe saves and restores the instance-wide state it touches (^IPM.settings, ^%IPM.DefaultModifiers).
Class Uplift.Repo.Model Extends Uplift.Repo.Util
{

ClassMethod Load(dir As %String) As %Status
{
    quit ..Run("load "_..#FIX_dir_"/", .o)
}

ClassMethod StreamExists(name As %String) As %Boolean
{
    quit ##class(%IPM.StudioDocument.ModuleStream).NameExists(name)
}

/// Remove the modules, classes and stored manifests of these probes, then the shared Reset.
ClassMethod Clean()
{
    for m = "nc-rec-top", "nc-rec-other", "nc-rec-ipmdep" {
        if ..Installed(m) { do ..Run("uninstall "_m_" -force", .o) }
    }
    do ..Reset()
    for c = "NCRecTop.Hello", "NCRecOther.Hello", "NCRecIpm.Hello" {
        if ##class(%Dictionary.ClassDefinition).%ExistsId(c) { do $system.OBJ.Delete(c, "-d") }
    }
    for m = "nc-web-dep", "nc-rec-top", "nc-rec-other", "nc-rec-ipmdep" {
        if ..StreamExists(m) && '..Installed(m) { do ##class(%IPM.StudioDocument.ModuleStream).NameDelete(m) }
    }
}

/// uninstall.rs dep_step: a dependency that another module still needs is skipped inside -r, after its
/// OnBeforePhase deleted its web app; its history row is a success.
ClassMethod RecSkip()
{
    do ..Clean()
    set kv("loadDep") = $system.Status.IsOK(..Load("nc-web-dep"))
    set kv("loadTop") = $system.Status.IsOK(..Load("nc-rec-top"))
    set kv("loadOther") = $system.Status.IsOK(..Load("nc-rec-other"))
    set kv("webBefore") = ..WebAppExists("/nc-dep")
    set h0 = ..HistoryCount()
    set sc = ..Run("uninstall nc-rec-top -r", .out)
    do ..Status(sc, .kv)
    set kv("topInstalled") = ..Installed("nc-rec-top")
    set kv("topClass") = ..ClassState("NCRecTop.Hello")
    set kv("depInstalled") = ..Installed("nc-web-dep")
    set kv("depClass") = ..ClassState("NCWebDep.Hello")
    set kv("otherInstalled") = ..Installed("nc-rec-other")
    set kv("webAfter") = ..WebAppExists("/nc-dep")
    set kv("historyRowsAdded") = ..HistoryCount() - h0
    set kv("lastHistoryDep") = ..LastHistory("nc-web-dep")
    set kv("lastHistoryTop") = ..LastHistory("nc-rec-top")
    set kv("printedSkipped") = ..Has(out, "Clean SKIPPED - required by 1 other module")
    do ..Emit("nc-repo-model-unin-recurse-skipped-dep", .kv)
}

/// uninstall.rs dep_step: a dependency removed inside -r loses its record by NameDelete only;
/// the top module goes through %IPM.StudioDocument.Module:Delete (record and stored manifest).
ClassMethod RecRemove()
{
    do ..Clean()
    set kv("loadDep") = $system.Status.IsOK(..Load("nc-web-dep"))
    set kv("loadTop") = $system.Status.IsOK(..Load("nc-rec-top"))
    set kv("depStreamBefore") = ..StreamExists("nc-web-dep")
    set kv("topStreamBefore") = ..StreamExists("nc-rec-top")
    set kv("webBefore") = ..WebAppExists("/nc-dep")
    set h0 = ..HistoryCount()
    set sc = ..Run("uninstall nc-rec-top -r", .out)
    do ..Status(sc, .kv)
    set kv("topInstalled") = ..Installed("nc-rec-top")
    set kv("depInstalled") = ..Installed("nc-web-dep")
    set kv("depClass") = ..ClassState("NCWebDep.Hello")
    set kv("webAfter") = ..WebAppExists("/nc-dep")
    set kv("depStreamAfter") = ..StreamExists("nc-web-dep")
    set kv("topStreamAfter") = ..StreamExists("nc-rec-top")
    set kv("historyRowsAdded") = ..HistoryCount() - h0
    set kv("lastHistoryDep") = ..LastHistory("nc-web-dep")
    set kv("listInstalledHasDep") = 0
    do ##class(%IPM.Main).GetListModules(,, .list)
    for i = 1:1:$get(list) { set:$listget(list(i))="nc-web-dep" kv("listInstalledHasDep") = 1 }
    do ..Emit("nc-repo-model-unin-recurse-removed-dep", .kv)
}

/// Rows of one kind's PublishingServer query, in the order IRIS returns them.
ClassMethod Rows(cls As %String) As %String
{
    set rs = $classmethod(cls, "PublishingServerFunc"), s = ""
    while rs.%Next() { set s = s_rs.Name_":"_rs.PublishOrder_" " }
    quit $zstrip(s, ">W")
}

ClassMethod Pick() As %String
{
    set rs = ##class(%IPM.Repo.Http.Definition).PublishingServerFunc()
    quit $select(rs.%Next():rs.Name, 1:"<none>")
}

ClassMethod Flag(name As %String) As %String
{
    set d = ##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen(name)
    quit $select($isobject(d):"["_d.DeploymentEnabled_"]", 1:"<none>")
}

/// settings.rs first_of / publishing_server: which registry publish uses with no -repo.
ClassMethod PublishOrder()
{
    do ..Clean()
    set kv("load") = $system.Status.IsOK(..Load("nc-repo-pub"))
    // 1. A (no flag, no credentials) made before B (publish flag), both remote.
    set kv("c1.a") = $system.Status.IsOK(..Run("repo -r -n nc-pm-a -url http://127.0.0.1:91/", .o))
    set kv("c1.b") = $system.Status.IsOK(..Run("repo -r -n nc-pm-b -url http://127.0.0.1:92/ -publish 1", .o))
    set kv("c1.aFlag") = ..Flag("nc-pm-a"), kv("c1.bFlag") = ..Flag("nc-pm-b")
    set kv("c1.remoteRows") = ..Rows("%IPM.Repo.Remote.Definition")
    set kv("c1.pick") = ..Pick()
    set sc = ..Run("publish nc-repo-pub -v -only", .out)
    set kv("c1.publishTo91") = ..Has(out_$system.Status.GetErrorText(sc), "127.0.0.1:91")
    set kv("c1.publishTo92") = ..Has(out_$system.Status.GetErrorText(sc), "127.0.0.1:92")
    do ##class(%IPM.Repo.Definition).%DeleteExtent()
    // 2. A (token) made before B (publish flag), both remote.
    set kv("c2.a") = $system.Status.IsOK(..Run("repo -r -n nc-pm-a -url http://127.0.0.1:91/ -token t", .o))
    set kv("c2.b") = $system.Status.IsOK(..Run("repo -r -n nc-pm-b -url http://127.0.0.1:92/ -publish 1", .o))
    set kv("c2.remoteRows") = ..Rows("%IPM.Repo.Remote.Definition")
    set kv("c2.pick") = ..Pick()
    do ##class(%IPM.Repo.Definition).%DeleteExtent()
    // 3. Remote A (no flag) and ORAS O (publish flag): kinds are compared by order, then name.
    set kv("c3.a") = $system.Status.IsOK(..Run("repo -r -n nc-pm-a -url http://127.0.0.1:91/", .o))
    set kv("c3.o") = $system.Status.IsOK(..Run("repo -o -n nc-pm-o -url http://127.0.0.1:93 -publish 1", .o))
    set kv("c3.orasRows") = ..Rows("%IPM.Repo.Oras.Definition")
    set kv("c3.pick") = ..Pick()
    do ##class(%IPM.Repo.Definition).%DeleteExtent()
    // 4. The flag passed along three remote registries (the old holder is set to 0).
    set kv("c4.c1") = $system.Status.IsOK(..Run("repo -r -n nc-pm-c1 -url http://127.0.0.1:94/ -publish 1", .o))
    set kv("c4.c2") = $system.Status.IsOK(..Run("repo -r -n nc-pm-c2 -url http://127.0.0.1:95/ -publish 1", .o))
    set sc = ..Run("repo -r -n nc-pm-c3 -url http://127.0.0.1:96/ -publish 1", .o)
    do ..Status(sc, .kv, "c4.c3.")
    set kv("c4.flags") = ..Flag("nc-pm-c1")_..Flag("nc-pm-c2")_..Flag("nc-pm-c3")
    set kv("c4.remoteRows") = ..Rows("%IPM.Repo.Remote.Definition")
    set kv("c4.pick") = ..Pick()
    do ..Clean()
    do ..Emit("nc-repo-model-publish-row-order", .kv)
}

/// The two nodes of one setting: value and default ("<undef>" when absent).
ClassMethod Node(key As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    quit $select($data(^IPM.settings(key))#2:^IPM.settings(key), 1:"<undef>")_"/"_$select($data(^IPM.settings(key,"default"))#2:^IPM.settings(key,"default"), 1:"<undef>")
}

ClassMethod SaveSetting(key As %String, ByRef save)
{
    new $namespace
    set $namespace = "%SYS"
    kill save
    merge save = ^IPM.settings(key)
}

ClassMethod RestoreSetting(key As %String, ByRef save)
{
    new $namespace
    set $namespace = "%SYS"
    kill ^IPM.settings(key)
    merge ^IPM.settings(key) = save
}

ClassMethod KillSetting(key As %String)
{
    new $namespace
    set $namespace = "%SYS"
    kill ^IPM.settings(key)
}

/// settings.rs set_value / reset: config delete on a key with no value and no default; and on a seeded key.
ClassMethod ConfigDelete()
{
    do ..SaveSetting("publish_timeout", .save)
    do ..KillSetting("publish_timeout")
    set kv("u.fresh") = ..Node("publish_timeout")
    set kv("u.delete1") = $system.Status.IsOK(..Run("config delete PublishTimeout", .o))
    set kv("u.afterDelete1") = ..Node("publish_timeout")
    set kv("u.set7") = $system.Status.IsOK(..Run("config set PublishTimeout 7", .o))
    set kv("u.afterSet7") = ..Node("publish_timeout")
    set kv("u.delete2") = $system.Status.IsOK(..Run("config delete PublishTimeout", .o))
    set kv("u.afterDelete2") = ..Node("publish_timeout")
    do ..RestoreSetting("publish_timeout", .save)
    set kv("u.restored") = ..Node("publish_timeout")
    do ..SaveSetting("ColorScheme", .save2)
    set kv("s.fresh") = ..Node("ColorScheme")
    set kv("s.set") = $system.Status.IsOK(..Run("config set ColorScheme none", .o))
    set kv("s.afterSet") = ..Node("ColorScheme")
    set kv("s.delete") = $system.Status.IsOK(..Run("config delete ColorScheme", .o))
    set kv("s.afterDelete") = ..Node("ColorScheme")
    do ..RestoreSetting("ColorScheme", .save2)
    do ..Emit("nc-repo-model-config-delete", .kv)
}

ClassMethod DmList() As %String
{
    set s = "", k = ""
    for { set k = $order(^%IPM.DefaultModifiers(k), 1, v) quit:k=""  set s = s_k_"="_v_" " }
    quit $zstrip(s, ">W")
}

/// settings.rs dm_set / dm_delete / effective: -set merges, -delete clears.
ClassMethod DefaultModifiers()
{
    merge save = ^%IPM.DefaultModifiers
    kill ^%IPM.DefaultModifiers
    set kv("set1") = $system.Status.IsOK(..Run("default-modifiers -set -DNcA=1", .o))
    set kv("set2") = $system.Status.IsOK(..Run("default-modifiers -set -DNcB=2", .o))
    set kv("afterTwoSets") = ..DmList()
    set sc = ..Run("default-modifiers -get", .out)
    set kv("getListsA") = ..Has(out, "-DNcA=1"), kv("getListsB") = ..Has(out, "-DNcB=2")
    set kv("set3") = $system.Status.IsOK(..Run("default-modifiers -set -DNcA=3", .o))
    set kv("afterOverwrite") = ..DmList()
    set kv("delete") = $system.Status.IsOK(..Run("default-modifiers -delete", .o))
    set kv("afterDelete") = $data(^%IPM.DefaultModifiers)
    kill ^%IPM.DefaultModifiers
    merge ^%IPM.DefaultModifiers = save
    do ..Emit("nc-repo-model-default-modifiers-merge", .kv)
}

ClassMethod RunAll()
{
    for m = "RecSkip", "RecRemove", "PublishOrder", "ConfigDelete", "DefaultModifiers" {
        try {
            do $classmethod("Uplift.Repo.Model", m)
        } catch e {
            write !, "OBS|error-model-"_m_"|error="_..Safe(e.DisplayString())_";", !
        }
        if $tlevel { trollback }
        new $namespace
        set $namespace = "USER"
    }
    do ..Clean()
}

/// uninstall.rs witness unin_witness_recurse_removes_ipm: a module that declares a need on zpm, uninstalled with -r,
/// while no other module needs zpm. Run in its own session; IPM is reinstalled by the next script.
ClassMethod IpmDepRun()
{
    do ..Clean()
    set kv("load") = $system.Status.IsOK(..Load("nc-rec-ipmdep"))
    set kv("pre.installed") = ..Installed("nc-rec-ipmdep")
    set kv("pre.ipmMain") = ..ClassState("%IPM.Main")
    set sc = ..Run("uninstall nc-rec-ipmdep -r", .out)
    do ..Status(sc, .kv)
    set kv("printedPrompt") = ..Has(out, "Do you want to fully remove")
    set kv("printedListHeader") = ..Has(out, "The following modules are currently installed")
    set kv("printedOrphanWarning") = ..Has(out, "module(s) orphaned")
    set kv("printedCleanZpm") = ..Has(out, "|zpm]")
    set kv("selfUninstallFlagLeft") = +$get(^||%IPM.SelfUninstallFlag)
    do ##class(Uplift.Repo.Self).State(.kv)
    set kv("ncRecIpmClass") = ..ClassState("NCRecIpm.Hello")
    set kv("ncRecIpmRecord") = 0, id = ""
    for { set id = $order(^IPM.Storage.ModuleD(id), 1, data) quit:id=""  set:$listget(data, 2)="nc-rec-ipmdep" kv("ncRecIpmRecord") = 1 }
    do ..Emit("nc-repo-model-unin-recurse-ipm-dependency", .kv)
}

ClassMethod IpmDepAfterReinstall()
{
    do ##class(Uplift.Repo.Self).State(.kv)
    set kv("ipmVersion") = $select(##class(%Dictionary.CompiledClass).%ExistsId("%IPM.Storage.Module"):##class(%IPM.Storage.Module).NameOpen("zpm").VersionString, 1:"n/a")
    set kv("depInstalled") = ..Installed("nc-rec-ipmdep")
    set sc = ..Run("list-installed", .out)
    set kv("listStatus") = $system.Status.IsOK(sc)
    do ..Clean()
    set kv("cleanedDepInstalled") = ..Installed("nc-rec-ipmdep")
    do ..Emit("nc-repo-model-unin-recurse-ipm-reinstall", .kv)
}

}
