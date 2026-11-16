/// IPM load/history scenarios for the uplift. Each prints one line: OBS|<name>|<key>=<value>;...
/// Fixtures live in /tmp/uplift/fixtures (copied from validation/iris/fixtures).
Class Uplift.Probe.Scenario
{

Parameter FIX = "/tmp/uplift/fixtures/";

Parameter SIMPLEST = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/simplest-module/";

ClassMethod Reset(withRepo As %Boolean = 1)
{
    if $tlevel { trollback }
    do ##class(%IPM.Main).Shell("uninstall -all")
    // uninstall -all does not remove every scenario artefact when a load failed half way
    for m = "up-dep", "up-parent-fail", "up-parent-ok", "up-halt", "up-hang", "up-res-fail", "up-dev-dep-fail", "simplest-module", "up-dep-halt", "up-dev-parent-crash" {
        if ##class(%IPM.Storage.Module).NameExists(m) { do ##class(%IPM.Main).Shell("uninstall "_m_" -force") }
    }
    for c = "UpParent.Hooks", "UpDep.Hello", "UpDepHalt.Hooks" {
        if $$$defClassDefined(c) { do $system.OBJ.Delete(c, "-d") }
    }
    if ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("uplift") {
        do ##class(%IPM.Main).Shell("repo -n uplift -delete")
    }
    if withRepo {
        do ##class(%IPM.Main).Shell("repo -n uplift -fs -path "_..#FIX_"repo/")
    }
    do ##class(%IPM.Main).Shell("history delete -confirm")
    kill ^IRIS.Temp.HistoryTempD, ^UpliftSignal, ^IRIS.Temp.UpliftSignal
}

ClassMethod Shell(cmd As %String) As %Status
{
    set sc = ##class(%IPM.Main).Shell(cmd)
    quit sc
}

/// Read back everything the history model talks about.
ClassMethod Observe(name As %String, sc As %Status, extra As %String = "")
{
    set kv("cmdStatus") = $select(sc="":"n/a", $system.Status.IsOK(sc):"ok", 1:"err")
    set:(sc'="")&&'$system.Status.IsOK(sc) kv("cmdError") = $translate($extract($system.Status.GetErrorText(sc), 1, 140), ";|=", ",/:")
    set kv("tlevel") = $tlevel
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT ID, Action, Package, Version_Major, Version_Minor, Version_Patch, Success, Committed, TimeEnd, Finalized, Phases FROM %IPM_General.History ORDER BY ID")
    set n = 0
    while rs.%Next() {
        set n = n + 1
        set succ = rs.%Get("Success")
        set phases = rs.%Get("Phases"), plist = "", ptr = 0
        while $listnext(phases, ptr, ph) {
            set p = $listget(ph, 1)
            set plist = plist_$listget(p, 1)_":"_$select($system.Status.IsOK($listget(p, 4)):"ok", 1:"err")_$select($listget(p, 3)="":"(open)", 1:"")_" "
        }
        set kv("row"_n) = rs.%Get("Action")_"/"_rs.%Get("Package")_"/"_rs.%Get("Version_Major")_"."_rs.%Get("Version_Minor")_"."_rs.%Get("Version_Patch")_"/success:"_$select($system.Status.IsOK(succ):"ok", 1:"err")_"/committed:"_rs.%Get("Committed")_"/ended:"_(rs.%Get("TimeEnd")'="")_"/finalized:"_rs.%Get("Finalized")_"/phases:"_$zstrip(plist, ">W")
    }
    set kv("historyRows") = n
    set kv("historyTempLeft") = ..CountGlobal("^IRIS.Temp.HistoryTempD")
    for m = "up-dep", "up-parent-fail", "up-parent-ok", "up-halt", "up-hang", "up-res-fail", "up-dev-dep-fail", "simplest-module", "up-dep-halt", "up-dev-parent-crash" {
        if ##class(%IPM.Storage.Module).NameExists(m) {
            set mod = ##class(%IPM.Storage.Module).NameOpen(m)
            set kv("mod:"_m) = mod.VersionString_$select(mod.DeveloperMode:"/dev", 1:"")
        }
    }
    for c = "UpParent.Hooks", "UpDep.Hello", "UpDepHalt.Hooks" {
        set kv("cls:"_c) = $select($$$comClassDefined(c):"compiled", $$$defClassDefined(c):"defined", 1:"absent")
    }
    if extra '= "" { set kv("note") = extra }
    do ##class(Uplift.Probe.Env).Emit(name, .kv)
}

ClassMethod CountGlobal(gref As %String) As %Integer
{
    set n = 0, k = ""
    for { set k = $order(@gref@(k)) quit:k=""  set n = n + 1 }
    quit n
}

ClassMethod LoadOk()
{
    do ..Reset()
    set sc = ..Shell("load "_..#SIMPLEST_"1.0.0/")
    do ..Observe("ipm-load-ok", sc)
}

ClassMethod DevDowngradeRefused()
{
    do ..Reset()
    set sc = ..Shell("load "_..#SIMPLEST_"2.0.0/")
    set sc = ..Shell("load -dev "_..#SIMPLEST_"1.0.0/")
    do ..Observe("ipm-dev-downgrade-refused", sc)
}

ClassMethod DevActivateFails()
{
    do ..Reset()
    set sc = ..Shell("load -dev "_..#FIX_"up-res-fail/")
    do ..Observe("ipm-dev-activate-fails", sc)
}

ClassMethod ActivateFails()
{
    do ..Reset()
    set sc = ..Shell("load "_..#FIX_"up-res-fail/")
    do ..Observe("ipm-activate-fails", sc)
}

ClassMethod NoTxnActivateFails()
{
    do ..Reset()
    set sc = ..Shell("load "_..#FIX_"up-res-fail/ -DNoTransaction=1")
    do ..Observe("ipm-notxn-activate-fails", sc)
}

ClassMethod DepOk()
{
    do ..Reset()
    set sc = ..Shell("load "_..#FIX_"up-parent-ok/")
    do ..Observe("ipm-dep-ok", sc)
}

ClassMethod DepParentFails()
{
    do ..Reset()
    set sc = ..Shell("load "_..#FIX_"up-parent-fail/")
    do ..Observe("ipm-dep-parent-fails", sc)
}

ClassMethod DevDepParentFails()
{
    do ..Reset()
    set sc = ..Shell("load -dev "_..#FIX_"up-dev-dep-fail/")
    do ..Observe("ipm-dev-dep-parent-fails", sc)
}

ClassMethod DevMissingDependency()
{
    do ..Reset(0)
    set sc = ..Shell("load -dev "_..#FIX_"up-parent-ok/")
    do ..Observe("ipm-dev-missing-dependency", sc)
}

ClassMethod MissingDependency()
{
    do ..Reset(0)
    set sc = ..Shell("load "_..#FIX_"up-parent-ok/")
    do ..Observe("ipm-missing-dependency", sc)
}

ClassMethod InstallFromRepo()
{
    do ..Reset()
    set sc = ..Shell("install up-dep")
    do ..Observe("ipm-install-from-fs-repo", sc)
}

ClassMethod UpdateWithPath()
{
    do ..Reset()
    set sc = ..Shell("load "_..#SIMPLEST_"1.0.0/")
    set sc = ..Shell("update simplest-module -path "_..#SIMPLEST_"2.0.0/")
    do ..Observe("ipm-update-with-path", sc)
}

ClassMethod InsideCallerTransaction()
{
    do ..Reset()
    tstart
    set sc = ..Shell("load "_..#SIMPLEST_"1.0.0/")
    set kv = $tlevel
    trollback
    do ..Observe("ipm-load-inside-caller-transaction-rolled-back", sc, "tlevelDuringLoadAfter:"_kv)
}

ClassMethod InsideCallerTransactionCommitted()
{
    do ..Reset()
    tstart
    set sc = ..Shell("load "_..#FIX_"up-res-fail/")
    tcommit
    do ..Observe("ipm-failed-load-inside-caller-transaction-committed", sc)
}

ClassMethod LoadInvalidPath()
{
    do ..Reset()
    set sc = ..Shell("load relative/path/")
    do ..Observe("ipm-load-relative-path-refused", sc)
}

/// Halting scenarios: the first call runs the load in this process (it halts);
/// a second session calls ObserveAfterHalt.
ClassMethod HaltSetup(mode As %String)
{
    do ..Reset()
    set flags = $case(mode, "txn":"", "notxn":" -DNoTransaction=1", "dev":" -dev")
    set ^UpliftHaltMode = mode
    set sc = ..Shell("load "_..#FIX_"up-halt/"_flags)
    // only reached if the load did not halt
    do ..Observe("ipm-halt-"_mode_"-did-not-halt", sc)
}

/// Crash inside a dependency load while the parent's second (developer-mode) transaction is open.
ClassMethod DepHaltSetup(dev As %Boolean = 1)
{
    do ..Reset()
    set ^UpliftHaltMode = $select(dev:"dev-crash-in-dependency", 1:"txn-crash-in-dependency")
    set sc = ..Shell("load "_$select(dev:"-dev ", 1:"")_..#FIX_"up-dev-parent-crash/")
    do ..Observe("ipm-dep-halt-did-not-halt", sc)
}

ClassMethod ObserveAfterHalt()
{
    set mode = $get(^UpliftHaltMode)
    set k = $order(^UpliftSignal("halt", "")), tl = $select(k="":"<rolled-back>", 1:$get(^UpliftSignal("halt", k)))
    if $data(^IRIS.Temp.UpliftSignal("dephalt"), dh) { set tl = tl_" depHaltTlevel:"_$listget(dh, 2) }
    do ..Observe("ipm-halt-"_mode, "", "tlevelAtHalt:"_tl)
    kill ^UpliftHaltMode
}

/// Terminate a job that is inside Activate, from another process.
ClassMethod TerminateDuringActivate(mode As %String = "txn")
{
    do ..Reset()
    set flags = $case(mode, "txn":"", "notxn":" -DNoTransaction=1", "dev":" -dev")
    job ##class(%IPM.Main).Shell("load "_..#FIX_"up-hang/"_flags)
    set child = $zchild
    for i=1:1:300 { quit:$data(^IRIS.Temp.UpliftSignal("hang"))  hang 0.2 }
    set sig = $get(^IRIS.Temp.UpliftSignal("hang"))
    set tsc = $system.Process.Terminate(child)
    for i=1:1:100 { quit:'$data(^$JOB(child))  hang 0.2 }
    hang 1
    do ..Observe("ipm-terminate-"_mode, "", "tlevelInActivate:"_$listget(sig, 2)_" terminateRC:"_tsc_" childGone:"_'$data(^$JOB(child)))
}

ClassMethod RunAll()
{
    for m = "LoadOk", "DevDowngradeRefused", "DevActivateFails", "ActivateFails", "NoTxnActivateFails", "DepOk", "DepParentFails", "DevDepParentFails", "DevMissingDependency", "MissingDependency", "InstallFromRepo", "UpdateWithPath", "InsideCallerTransaction", "InsideCallerTransactionCommitted", "LoadInvalidPath" {
        try {
            do $classmethod("Uplift.Probe.Scenario", m)
        } catch e {
            write "OBS|error-"_m_"|error="_$translate(e.DisplayString(), ";|=", ",/:")_";", !
        }
        if $tlevel { trollback }
    }
    for mode = "txn", "notxn", "dev" {
        try {
            do ..TerminateDuringActivate(mode)
        } catch e {
            write "OBS|error-terminate-"_mode_"|error="_$translate(e.DisplayString(), ";|=", ",/:")_";", !
        }
    }
}

}
