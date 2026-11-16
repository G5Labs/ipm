/// Helpers for the n2 native gap checks (P1..P9). Observation lines: OBS|<name>|k=v;...
/// Fixtures: /tmp/uplift/n2/fixtures (copied from validation/iris/n2/fixtures by scripts/iris_sync.sh).
/// Observe follows Uplift.Probe.Scenario:Observe, generalized to any module and any N2* class.
Class Uplift.N2.Base
{

Parameter FIX = "/tmp/uplift/n2/fixtures/";

Parameter STEPFILE = "/tmp/n2-upd-steps.txt";

ClassMethod Clean(v As %String, len As %Integer = 400) As %String
{
    quit $zstrip($translate($extract(v, 1, len), ";|="_$char(9, 10, 13, 27), ",/:    "), "<=>W")
}

ClassMethod Emit(name As %String, ByRef kv)
{
    set out = "OBS|"_name_"|", k = ""
    for {
        set k = $order(kv(k), 1, v)
        quit:k=""
        set out = out_k_"="_..Clean(v, 1200)_";"
    }
    write !, out, !
}

/// Compile the Invoke target class N2H.Hooks (not a module resource).
ClassMethod Setup()
{
    set sc = $system.OBJ.Load(..#FIX_"hooks/N2H/Hooks.cls", "ck-d")
    write !, "SETUP hooks ", $system.Status.IsOK(sc), !
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

ClassMethod Reset(withRepo As %Boolean = 1)
{
    while $tlevel { trollback }
    kill ^N2Fail, ^IRIS.Temp.N2Signal, ^IRIS.Temp.N2Release, ^N2HaltStep, ^N2Upd, ^N2UpdX, ^N2UpdY, ^UpliftCrashAt, ^UpliftForceDeployErr
    do ..Sh("uninstall -all")
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %IPM_Storage.ModuleItem")
    while rs.%Next() {
        set n = rs.%Get("Name")
        continue:(n = "zpm")
        do ..Sh("uninstall "_n_" -force")
    }
    for p = "N2Wait", "N2PhA", "N2PhB", "N2PhC", "N2Arc", "N2Git", "N2GitF", "N2Dpl", "N2CB", "N2CC", "N2PD", "N2D1", "N2D2", "N2CA", "N2PA", "N2TP", "N2Solo", "N2Upd" {
        do $system.OBJ.DeletePackage(p, "-d")
    }
    do ##class(%IPM.Storage.UpdateStep.AnyMember).%KillExtent()
    do ##class(%IPM.Storage.UpdateStep.PrimaryOnly).%KillExtent()
    do ..Sh("repo -delete-all")
    if withRepo {
        do ..Sh("repo -n n2 -fs -path "_..#FIX_"p9/repo/")
    }
    do ..Sh("history delete -confirm")
    kill ^IRIS.Temp.HistoryTempD
    do ##class(%File).Delete(..#STEPFILE)
}

ClassMethod CountGlobal(gref As %String) As %Integer
{
    set n = 0, k = ""
    for { set k = $order(@gref@(k)) quit:k=""  set n = n + 1 }
    quit n
}

/// History rows, IRISTEMP leftovers, module records, N2* classes, Invoke runs.
ClassMethod Observe(name As %String, sc As %Status = "", extra As %String = "")
{
    merge kv = %n2kv
    kill %n2kv
    set kv("cmdStatus") = $select(sc="":"n/a", $system.Status.IsOK(sc):"ok", 1:"err")
    set:(sc'="")&&'$system.Status.IsOK(sc) kv("cmdError") = $extract($system.Status.GetErrorText(sc), 1, 160)
    set kv("tlevel") = $tlevel
    do ..HistoryRows(.kv)
    set kv("historyTempLeft") = ..CountGlobal("^IRIS.Temp.HistoryTempD")
    do ..Modules(.kv)
    do ..Classes(.kv)
    set ran = "", i = ""
    for { set i = $order(^IRIS.Temp.N2Signal("ran", i), 1, r) quit:i=""  set ran = ran_r_" " }
    set kv("invokesRan") = $select(ran="":"none", 1:$zstrip(ran, ">W"))
    if extra '= "" { set kv("note") = extra }
    do ..Emit(name, .kv)
}

ClassMethod HistoryRows(ByRef kv)
{
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
        set kv("row"_n) = "id"_rs.%Get("ID")_"/"_rs.%Get("Action")_"/"_rs.%Get("Package")_"/"_rs.%Get("Version_Major")_"."_rs.%Get("Version_Minor")_"."_rs.%Get("Version_Patch")_"/success:"_$select($system.Status.IsOK(succ):"ok", 1:"err")_"/committed:"_rs.%Get("Committed")_"/ended:"_(rs.%Get("TimeEnd")'="")_"/finalized:"_+rs.%Get("Finalized")_"/phases:"_$zstrip(plist, ">W")
    }
    set kv("historyRows") = n
}

ClassMethod Modules(ByRef kv)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %IPM_Storage.ModuleItem ORDER BY Name")
    while rs.%Next() {
        set m = rs.%Get("Name")
        continue:m="zpm"
        set mod = ##class(%IPM.Storage.Module).NameOpen(m, 0)
        set kv("mod:"_m) = mod.VersionString_$select(mod.DeveloperMode:"/dev", 1:"")
        kill mod
    }
}

/// Every class whose name starts with N2 (except the hooks): compiled / defined, and V() of compiled markers.
ClassMethod Classes(ByRef kv)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %Dictionary.ClassDefinition WHERE Name %STARTSWITH 'N2' AND Name <> 'N2H.Hooks' ORDER BY Name")
    set any = 0
    while rs.%Next() {
        set c = rs.%Get("Name"), any = 1
        set kv("cls:"_c) = $select($$$comClassDefined(c):"compiled", 1:"defined")
        if $$$comClassDefined(c) && ($piece(c, ".", *) = "Marker") {
            try { set kv("cls:"_c) = "compiled/V:"_$classmethod(c, "V") } catch { set kv("cls:"_c) = "compiled/V:<error>" }
        }
    }
    // compiled but no definition (orphan runtime)
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %Dictionary.CompiledClass WHERE Name %STARTSWITH 'N2' AND Name <> 'N2H.Hooks'")
    while rs.%Next() {
        set c = rs.%Get("Name")
        set:'$data(kv("cls:"_c)) kv("cls:"_c) = "compiled-only"
    }
}

/// Wait (at most secs) for ^IRIS.Temp.N2Signal(sub, name). Returns 1 if seen.
ClassMethod WaitSignal(sub As %String, name As %String, secs As %Integer = 120) As %Boolean
{
    for i = 1:1:(secs * 5) {
        quit:$data(^IRIS.Temp.N2Signal(sub, name))
        hang 0.2
    }
    quit ''$data(^IRIS.Temp.N2Signal(sub, name))
}

/// Wait (at most secs) for process pid to end. Returns 1 if gone.
ClassMethod WaitGone(pid As %Integer, secs As %Integer = 120) As %Boolean
{
    for i = 1:1:(secs * 5) {
        quit:'$data(^$JOB(pid))
        hang 0.2
    }
    quit '$data(^$JOB(pid))
}

/// Background job entry: one IPM shell command (output discarded).
ClassMethod JobShell(cmd As %String)
{
    set ^IRIS.Temp.N2Signal("job", $job) = cmd
    set sc = ##class(%IPM.Main).Shell(cmd)
    set ^IRIS.Temp.N2Signal("jobdone", $job) = $select($system.Status.IsOK(sc):"ok", 1:$system.Status.GetErrorText(sc))
}

/// Run a shell command line with $ZF(-100); output lines returned in out (space separated).
ClassMethod Os(cmd As %String, Output out As %String) As %Integer
{
    set f = "/tmp/n2-os-out.txt"
    set rc = $zf(-100, "/STDOUT="""_f_""" /STDERR="""_f_"""", "/bin/sh", "-c", cmd)
    set out = "", s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(f)
    while 's.AtEnd { set l = s.ReadLine() set:l'="" out = out_l_" " }
    kill s
    do ##class(%File).Delete(f)
    write !, "OS> ", cmd, " rc=", rc, !, "OS< ", out, !
    quit rc
}

ClassMethod FileText(path As %String) As %String
{
    if '##class(%File).Exists(path) { quit "<absent>" }
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(path)
    set t = $zstrip(s.Read(10000), "<>WC")
    quit $select(t="":"<empty>", 1:t)
}

ClassMethod FileLines(path As %String) As %Integer
{
    if '##class(%File).Exists(path) { quit 0 }
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(path)
    set n = 0
    while 's.AtEnd { set l = s.ReadLine() set:l'="" n = n + 1 }
    quit n
}

/// Step record: none, or start:<0/1>/end:<0/1>/ok:<0/1> (as Uplift.Inst.Checks:Step).
ClassMethod Step(cls As %String, meth As %String) As %String
{
    set step = ##class(%IPM.Storage.UpdateStep.AnyMember).GetUpdateStep(cls, meth)
    if '$isobject(step) || (step.%Id() = "") { quit "none" }
    quit "start:"_(step.TimeStampStart '= "")_"/end:"_(step.TimeStampEnd '= "")_"/ok:"_$system.Status.IsOK(step.Status)
}

ClassMethod MaxHistoryId() As %Integer
{
    &sql(SELECT MAX(ID) INTO :id FROM %IPM_General.History)
    quit $select(SQLCODE=0:+id, 1:0)
}

}
