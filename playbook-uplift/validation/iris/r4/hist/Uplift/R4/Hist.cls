/// Round-4 history probes (decisions/research/hist.md). Observation lines: OBS|r4-hist-<name>|k=v;...
/// Fixtures: /tmp/uplift/r4/hist/fixtures (copied from validation/iris/r4/hist). Only r4h-* modules are touched.
Class Uplift.R4.Hist
{

Parameter FIX = "/tmp/uplift/r4/hist/fixtures/";

ClassMethod Clean(v As %String, len As %Integer = 400) As %String
{
    quit $zstrip($translate($extract(v, 1, len), ";|="_$char(9, 10, 13, 27), ",/:    "), "<=>W")
}

ClassMethod Emit(name As %String, ByRef kv)
{
    set out = "OBS|r4-hist-"_name_"|", k = ""
    for {
        set k = $order(kv(k), 1, v)
        quit:k=""
        set out = out_k_"="_..Clean(v, 1200)_";"
    }
    write !, out, !
}

ClassMethod Flags(mode As %String) As %String
{
    quit $case(mode, "txn":"", "dev":" -dev", "notxn":" -DNoTransaction=1", :"")
}

ClassMethod Sh(cmd As %String) As %Status
{
    write !, "CMD> ", cmd, !
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
    } catch e {
        set sc = e.AsStatus()
    }
    write !, "CMD< ", $select($system.Status.IsOK(sc): "OK", 1: ..Clean($system.Status.GetErrorText(sc), 300)), !
    quit sc
}

/// Compile the hooks class and define the filesystem repo that holds r4h-dep-wait.
ClassMethod Setup()
{
    set sc = $system.OBJ.Load("/tmp/uplift/r4/hist/hooks/Hooks.cls", "ck-d")
    write !, "SETUP hooks ", $system.Status.IsOK(sc), !
    if '##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("r4hist") {
        do ..Sh("repo -n r4hist -fs -path "_..#FIX_"repo/")
    }
}

/// Remove r4h-* modules, their classes, the repo, and the history rows these probes wrote.
ClassMethod Cleanup()
{
    while $tlevel { trollback }
    for m = "r4h-msg", "r4h-msg2", "r4h-wait-compile", "r4h-parent", "r4h-dep-wait" {
        if ##class(%IPM.Storage.Module).NameExists(m) { do ..Sh("uninstall "_m_" -force") }
    }
    for c = "R4HMsg.Marker", "R4HWc.Marker", "R4HPar.Marker", "R4HDep.Marker" {
        if $$$defClassDefined(c) { do $system.OBJ.Delete(c, "-d") }
    }
    if ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("r4hist") { do ..Sh("repo -n r4hist -delete") }
    &sql(DELETE FROM %IPM_General.History WHERE CommandString LIKE '%r4h%' OR Package %STARTSWITH 'r4h' OR CommandString LIKE '%r4hist%')
    write !, "CLEANUP history SQLCODE=", SQLCODE, " rows=", $get(%ROWCOUNT), !
    kill ^IRIS.Temp.R4H
}

ClassMethod Rows(ByRef kv, minId As %Integer = 0)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT ID, Action, Package, Success, Committed, TimeStart, TimeEnd, Finalized, Phases, CommandString FROM %IPM_General.History WHERE ID > ? ORDER BY ID", minId)
    set n = 0
    while rs.%Next() {
        set n = n + 1
        set phases = rs.%Get("Phases"), plist = "", ptr = 0
        while $listnext(phases, ptr, ph) {
            set p = $listget(ph, 1)
            set plist = plist_$listget(p, 1)_":"_$select($system.Status.IsOK($listget(p, 4)):"ok", 1:"err")_$select($listget(p, 3)="":"(open)", 1:"")_" "
        }
        set kv("row"_n) = "id"_rs.%Get("ID")_"/"_rs.%Get("Action")_"/"_rs.%Get("Package")_"/success:"_$select($system.Status.IsOK(rs.%Get("Success")):"ok", 1:"err")_"/committed:"_rs.%Get("Committed")_"/ended:"_(rs.%Get("TimeEnd")'="")_"/finalized:"_rs.%Get("Finalized")_"/phases:"_$zstrip(plist, ">W")_"/start:"_$piece(rs.%Get("TimeStart")," ",2)_"/end:"_$piece(rs.%Get("TimeEnd")," ",2)_"/cmd:"_$extract(rs.%Get("CommandString"),1,60)
    }
    set kv("historyRows") = n
    set kv("historyTempData") = $data(^IRIS.Temp.HistoryTempD)
}

ClassMethod Observe(name As %String, sc As %Status = "", minId As %Integer = 0, ByRef kv)
{
    set:sc'="" kv("cmdStatus") = $select($system.Status.IsOK(sc):"ok", 1:"err")
    set:(sc'="")&&'$system.Status.IsOK(sc) kv("cmdError") = $system.Status.GetErrorText(sc)
    set kv("tlevel") = $tlevel
    do ..Rows(.kv, minId)
    for m = "r4h-msg", "r4h-msg2", "r4h-wait-compile", "r4h-parent", "r4h-dep-wait" {
        if ##class(%IPM.Storage.Module).NameExists(m) {
            set mod = ##class(%IPM.Storage.Module).NameOpen(m, 0)
            set kv("mod:"_m) = $select($isobject(mod): mod.VersionString_$select(mod.DeveloperMode:"/dev", 1:""), 1:"<cannot open>")
        }
    }
    for c = "R4HMsg.Marker", "R4HWc.Marker", "R4HPar.Marker", "R4HDep.Marker" {
        set v = $select($$$comClassDefined(c):"compiled", $$$defClassDefined(c):"defined", 1:"")
        set:v'="" kv("cls:"_c) = v
    }
    do ..Emit(name, .kv)
}

ClassMethod MaxId() As %Integer
{
    &sql(SELECT MAX(ID) INTO :id FROM %IPM_General.History)
    quit +$get(id)
}

ClassMethod WaitSignal(sub As %String, name As %String, secs As %Integer = 120) As %Boolean
{
    for i = 1:1:(secs * 5) {
        quit:$data(^IRIS.Temp.R4H(sub, name))
        hang 0.2
    }
    quit ''$data(^IRIS.Temp.R4H(sub, name))
}

ClassMethod WaitGone(pid As %Integer, secs As %Integer = 120) As %Boolean
{
    for i = 1:1:(secs * 5) {
        quit:'$data(^$JOB(pid))
        hang 0.2
    }
    quit '$data(^$JOB(pid))
}

/// Background job entry: one IPM shell command.
ClassMethod JobShell(cmd As %String)
{
    set ^IRIS.Temp.R4H("job", $job) = cmd
    set sc = ##class(%IPM.Main).Shell(cmd)
    set ^IRIS.Temp.R4H("jobdone", $job) = $select($system.Status.IsOK(sc):"ok", 1:$system.Status.GetErrorText(sc))
}

// ===== U1: can anything fail after the final commit? =====

/// Does %EvaluateMacro (used by WriteAfterInstallMessage through %Evaluate) throw for these texts?
ClassMethod MacroProbe()
{
    for text = "Installed $$$ISERR", "Installed $$$NoSuchMacroR4h", "Installed #{1/0}", "Installed {$root} $$$ISERR" {
        kill kv
        set kv("text") = text
        try {
            set v = ##class(%IPM.Utils.Module).%EvaluateMacro(text)
            set kv("evalMacro") = "ok:"_v
        } catch e {
            set kv("evalMacro") = "throws:"_e.DisplayString()
        }
        set mod = ##class(%IPM.Storage.Module).%New()
        set mod.Name = "r4h-probe"
        set mod.VersionString = "1.0.0"
        try {
            set v = mod.%Evaluate(text)
            set kv("moduleEvaluate") = "ok:"_v
        } catch e {
            set kv("moduleEvaluate") = "throws:"_e.DisplayString()
        }
        set kv("tlevel") = $tlevel
        do ..Emit("macro-probe-"_$case($piece(text, " ", 2, *), "$$$ISERR":"iserr", "$$$NoSuchMacroR4h":"undefined", "{$root} $$$ISERR":"brace-iserr", :"expr"), .kv)
    }
}

/// Load r4h-msg (AfterInstallMessage that makes %Evaluate throw) and read the record.
ClassMethod Msg(mode As %String, fixture As %String = "r4h-msg")
{
    kill ^IRIS.Temp.R4H
    set base = ..MaxId()
    set sc = ..Sh("load "_..#FIX_fixture_"/"_..Flags(mode))
    do ..Observe("post-commit-throw-"_$select(fixture="r4h-msg":"", 1:fixture_"-")_mode, sc, base)
    do ..Sh("uninstall "_fixture_" -force")
}

// ===== U20: history delete during another phase, or of a dependency's row =====

/// Delete the running row while the load waits in the Compile phase (Invoke Phase=Compile When=Before).
ClassMethod DelPhase(mode As %String)
{
    kill ^IRIS.Temp.R4H
    set base = ..MaxId()
    job ##class(Uplift.R4.Hist).JobShell("load "_..#FIX_"r4h-wait-compile/"_..Flags(mode))
    set child = $zchild
    set kv("waiting") = ..WaitSignal("wait", "r4h-wait-compile", 120)
    set kv("tlevelInPhase") = $listget($get(^IRIS.Temp.R4H("wait", "r4h-wait-compile")), 2)
    set id = ..MaxId()
    set kv("rowId") = id
    do ..Observe("delete-in-compile-"_mode_"-during", "", base, .kv)
    kill kv
    set sc = ..Sh("history delete -DID="_id)
    set kv("deleteById") = $select($system.Status.IsOK(sc):"ok", 1:"err:"_$system.Status.GetErrorText(sc))
    set kv("rowExistsAfterDelete") = ##class(%IPM.General.History).%ExistsId(id)
    set ^IRIS.Temp.R4H("release", "r4h-wait-compile") = 1
    set kv("jobGone") = ..WaitGone(child, 120)
    hang 1
    set kv("jobResult") = $get(^IRIS.Temp.R4H("jobdone", child), "<none>")
    set kv("rowExistsAfterRelease") = ##class(%IPM.General.History).%ExistsId(id)
    do ..Observe("delete-in-compile-"_mode_"-after-release", "", base, .kv)
    do ..Sh("uninstall r4h-wait-compile -force")
}

/// While the dependency r4h-dep-wait waits in its Activate (inside the parent's load), delete the dependency's row,
/// then the parent's row. Then release and read back.
ClassMethod DelDep(mode As %String)
{
    kill ^IRIS.Temp.R4H
    set base = ..MaxId()
    job ##class(Uplift.R4.Hist).JobShell("load "_..#FIX_"r4h-parent/"_..Flags(mode))
    set child = $zchild
    set kv("waiting") = ..WaitSignal("wait", "r4h-dep-wait", 120)
    set kv("tlevelInDepActivate") = $listget($get(^IRIS.Temp.R4H("wait", "r4h-dep-wait")), 2)
    &sql(SELECT MIN(ID), MAX(ID) INTO :pid, :did FROM %IPM_General.History WHERE ID > :base)
    set kv("parentRowId") = pid, kv("depRowId") = did
    do ..Observe("delete-dep-row-"_mode_"-during", "", base, .kv)
    kill kv
    set t0 = $zhorolog
    set sc = ..Sh("history delete -DID="_did)
    set kv("deleteDepSecs") = $justify($zhorolog - t0, 0, 1)
    set kv("deleteDep") = $select($system.Status.IsOK(sc):"ok", 1:"err:"_$system.Status.GetErrorText(sc))
    set kv("depRowExistsAfterDelete") = ##class(%IPM.General.History).%ExistsId(did)
    set sc = ..Sh("history delete -DID="_pid)
    set kv("deleteParent") = $select($system.Status.IsOK(sc):"ok", 1:"err:"_$system.Status.GetErrorText(sc))
    set kv("parentRowExistsAfterDelete") = ##class(%IPM.General.History).%ExistsId(pid)
    set ^IRIS.Temp.R4H("release", "r4h-dep-wait") = 1
    set kv("releaseSetAt") = $piece($zdatetime($horolog, 3), " ", 2)
    set kv("jobGone") = ..WaitGone(child, 120)
    hang 1
    set kv("jobResult") = $get(^IRIS.Temp.R4H("jobdone", child), "<none>")
    set kv("depWaitLoops") = $get(^IRIS.Temp.R4H("released", "r4h-dep-wait"))
    set kv("releasedAt") = $piece($zdatetime($horolog, 3), " ", 2)
    set kv("depRowExistsAfterRelease") = ##class(%IPM.General.History).%ExistsId(did)
    set kv("parentRowExistsAfterRelease") = ##class(%IPM.General.History).%ExistsId(pid)
    do ..Observe("delete-dep-row-"_mode_"-after-release", "", base, .kv)
    do ..Sh("uninstall r4h-parent -force")
    do ..Sh("uninstall r4h-dep-wait -force")
}

// ===== DEC-SEM-2: which IRISSYS code entries change? =====

/// snap(global, firstSubscript) = hash, for code globals in IRISSYS.
ClassMethod SysSnap(Output snap)
{
    kill snap
    set db = "^^/usr/irissys/mgr/"
    for g = "oddDEF", "oddCOM", "rOBJ", "rINDEX", "rINDEXCLASS", "ISC.Src.Jrn", "oddDEP" {
        set base = "^|"""_db_"""|"_g
        set s1 = ""
        for {
            set s1 = $order(@base@(s1))
            quit:s1=""
            set top = $name(@base@(s1)), h = 0, q = top
            if $data(@q) # 2 { set h = $zcrc($get(@q), 7, h) }
            for {
                set q = $query(@q)
                quit:q=""
                quit:$name(@q, 1)'=top
                set h = $zcrc(q_"="_$get(@q), 7, h)
            }
            set snap(g, s1) = h
        }
    }
}

ClassMethod SysDiff(ByRef before, ByRef after) As %String
{
    set out = "", g = ""
    for {
        set g = $order(after(g))
        quit:g=""
        set s = ""
        for {
            set s = $order(after(g, s), 1, h)
            quit:s=""
            set other = $get(before(g, s))
            if other '= h { set out = out_$listbuild(g_"("_s_")"_$select(other="":"+", 1:"~")) }
        }
    }
    set g = ""
    for {
        set g = $order(before(g))
        quit:g=""
        set s = ""
        for {
            set s = $order(before(g, s))
            quit:s=""
            if '$data(after(g, s)) { set out = out_$listbuild(g_"("_s_")-") }
        }
    }
    quit $listtostring(out, " ")
}

ClassMethod SysMeasure(name As %String, code As %String)
{
    do ..SysSnap(.before)
    xecute code
    do ..SysSnap(.after)
    set kv("code") = code
    set kv("irissysCodeChanged") = ..SysDiff(.before, .after)
    set n = 0, g = "" for { set g = $order(after(g)) quit:g=""  set s = "" for { set s = $order(after(g, s)) quit:s=""  set n = n + 1 } }
    set kv("entriesScanned") = n
    set kv("macroEvalClassNow") = $$$comClassDefined("%IPM.Generated.MacroEval")
    do ..Emit("sem2-"_name, .kv)
}

ClassMethod Sem2()
{
    set data = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/"
    do ..SysMeasure("noop", "set x = 1")
    do ..SysMeasure("evaluate-macro-fails", "try { set x = ##class(%IPM.Utils.Module).%EvaluateMacro(""#{$$$ISERR}"") } catch e { write !,""threw"",! }")
    do ..SysMeasure("evaluate-macro", "set x = ##class(%IPM.Utils.Module).%EvaluateMacro(""#{$$$AutheCache}"")")
    do ..SysMeasure("load-rest-app", "do ##class(%IPM.Main).Shell(""load "_data_"rest-app/"")")
    do ..SysMeasure("uninstall-rest-app", "do ##class(%IPM.Main).Shell(""uninstall rest-demo"")")
}


// ===== U19: graceful iris stop while a load waits in Activate =====

ClassMethod StopStart(mode As %String, maxLoops As %Integer = 480)
{
    kill ^IRIS.Temp.R4H, ^R4HU19
    set ^IRIS.Temp.R4H("maxloops", "r4h-dep-wait") = maxLoops
    set base = ..MaxId()
    job ##class(Uplift.R4.Hist).JobShell("load "_..#FIX_"repo/r4h-dep-wait/"_..Flags(mode))
    set child = $zchild
    set kv("waiting") = ..WaitSignal("wait", "r4h-dep-wait", 120)
    set kv("tlevelInActivate") = $listget($get(^IRIS.Temp.R4H("wait", "r4h-dep-wait")), 2)
    set kv("jobPid") = child, kv("at") = $zdatetime($horolog, 3)
    set ^R4HU19 = $listbuild(mode_$select(maxLoops=480:"", 1:"-wait"_(maxLoops\4)_"s"), child, base)
    do ..Observe("graceful-stop-"_$list(^R4HU19, 1)_"-during", "", base, .kv)
}

ClassMethod StopAfter()
{
    set info = $get(^R4HU19), mode = $listget(info, 1), base = $listget(info, 3)
    set kv("oldJobAlive") = ''$data(^$JOB(+$listget(info, 2)))
    set kv("iristempSignalLeft") = $data(^IRIS.Temp.R4H)
    set kv("at") = $zdatetime($horolog, 3)
    do ..Observe("graceful-stop-"_mode_"-after", "", base, .kv)
    kill ^R4HU19
    do ..Sh("uninstall r4h-dep-wait -force")
}

}
