/// Native replay of B1 model cases (cases.json). One call per case; field names follow l2_predicted keys.
/// Slots: top, dep1, dep2. Values: 1/0 for booleans, OK/ERR/ZERO for statuses, <none> when absent.
Class B2Native.Replay Extends B2Native.Util
{

/// setup: shell commands separated by " ;; ". nest: top | callertx.
/// endmode: return | return-rollback | terminate | sigkill. waitTag: package whose Wait() stalls the load.
/// extra: comma list; "no-registry" disables the remote 'registry' repository for this case only.
ClassMethod Case(
	case As %String,
	top As %String,
	dep1 As %String,
	dep2 As %String,
	setup As %String,
	cmd As %String,
	nest As %String,
	endmode As %String,
	waitTag As %String = "",
	extra As %String = "")
{
    do ..Begin(case)
    try {
        if extra [ "no-registry" {
            do ..Run(case, "setup.registry.off", "repo -n registry -disable")
        }
        for i=1:1:$length(setup, " ;; ") {
            set s = $piece(setup, " ;; ", i)
            continue:s=""
            do ..Run(case, "setup"_i, s)
        }
        do ..Slots(case, "before.", top, dep1, dep2)
        // LastUpdated has one-second resolution: separate the command from the setup writes.
        hang 1.1
        set base = ..MaxHistId(), c0 = +$get(^IPM.General.HistoryD), tbase = ..MaxTempId()
        do ..Obs(case, "binding.command", cmd)
        if (endmode = "return") || (endmode = "return-rollback") {
            if nest = "callertx" {
                tstart
            }
            write !,"CMD ",case," ",cmd,!
            set sc = $$$OK
            try {
                set sc = ##class(%IPM.Main).Shell(cmd)
            } catch ex {
                set sc = ex.AsStatus()
            }
            do ..Obs(case, "status", $select($$$ISOK(sc):"OK", 1:"ERR"))
            if $$$ISERR(sc) {
                do ..Obs(case, "err", $extract($system.Status.GetErrorText(sc), 1, 300))
            }
            do ..Obs(case, "binding.tlevel_after_command", $tlevel)
            if endmode = "return-rollback" {
                trollback
                do ..Obs(case, "open_transaction_levels", $tlevel)
                do ..Obs(case, "end", "returned; caller rolled back")
            } else {
                do ..Obs(case, "open_transaction_levels", $tlevel)
                do ..Obs(case, "end", "returned")
            }
            do ..Observe(case, top, dep1, dep2, base, c0, tbase)
            // A caller transaction left open for observation (B1-C41) is rolled back afterwards.
            while $tlevel > 0 { trollback 1 }
        } else {
            kill ^IRIS.Temp.B2Signal(waitTag)
            write !,"CMD ",case," job ",cmd,!
            if extra [ "zb-c3-setcommitted" {
                // Approximation: stall on the C3 block's `do pLog.SetCommitted()`, after its tcommit and before the mark.
                set off = ..FindOffsetAfter("%IPM.Utils.Module.1", "LoadNewModule", "ExecutePhases(tModuleName", "do pLog.SetCommitted()")
                set loc = "LoadNewModule+"_off_"^%IPM.Utils.Module.1"
                do ..Obs(case, "binding.zbreak.location", loc)
                do ..Obs(case, "binding.zbreak.line", ..LineAt("%IPM.Utils.Module.1", "LoadNewModule", off))
                do ..Obs(case, "binding.zbreak.prev.line", ..LineAt("%IPM.Utils.Module.1", "LoadNewModule", off - 2))
                job ##class(B2Native.Replay).JobLoadZb(cmd, waitTag, nest, loc)
            } elseif extra [ "zb-after-commit" {
                // Approximation: a ZBREAK in the JOB stalls LoadNewModule on the line after its final commit.
                set off = ..FindOffset("%IPM.Utils.Module.1", "LoadNewModule", "do tModule.WriteAfterInstallMessage()")
                set loc = "LoadNewModule+"_off_"^%IPM.Utils.Module.1"
                do ..Obs(case, "binding.zbreak.location", loc)
                do ..Obs(case, "binding.zbreak.line", ..LineAt("%IPM.Utils.Module.1", "LoadNewModule", off))
                job ##class(B2Native.Replay).JobLoadZb(cmd, waitTag, nest, loc)
            } else {
                job ##class(B2Native.Replay).JobLoad(cmd, waitTag, nest)
            }
            set pid = $zchild
            do ..Obs(case, "harness.signal", ..WaitSignal(waitTag, 300))
            do ..Obs(case, "binding.invoke.tlevel", $get(^IRIS.Temp.B2Signal(waitTag, "tlevel")))
            do ..Slots(case, "inflight.", top, dep1, dep2)
            do ..TempRows(case, "inflight.temp.", tbase)
            set t0 = $zhorolog
            if endmode = "terminate" {
                do ..Obs(case, "harness.end.rc", $system.Process.Terminate(pid))
                set gone = ..WaitGone(pid, 120)
            } else {
                do ..Obs(case, "harness.end.rc", $zf(-100, "", "/usr/bin/kill", "-9", pid))
                // IRIS removes a SIGKILLed process slot, and rolls back its transaction, only after a delay.
                set gone = ..WaitGone(pid, 1500)
            }
            do ..Obs(case, "harness.gone", gone)
            do ..Obs(case, "binding.end.seconds", $justify($zhorolog - t0, 0, 0))
            hang 2
            do ..Obs(case, "status", "<none>")
            do ..Obs(case, "end", $select(endmode="terminate":"process terminated", 1:"process killed"))
            // The ended process holds no transaction once it is gone.
            do ..Obs(case, "open_transaction_levels", $select(gone:0, 1:"<process still listed>"))
            do ..Obs(case, "harness.job.sc.recorded", $data(^IRIS.Temp.B2Signal(waitTag, "sc")))
            do ..Observe(case, top, dep1, dep2, base, c0, tbase)
            // Orphaned IRISTEMP rows are instance-wide; remove them after recording.
            set id = tbase
            for {
                set id = $order(^IRIS.Temp.HistoryTempD(id))
                quit:id=""
                do ##class(%IPM.General.HistoryTemp).%DeleteId(id)
            }
            kill ^IRIS.Temp.B2Signal(waitTag)
        }
    } catch ex {
        do ..Obs(case, "case.exception", ex.DisplayString())
    }
    while $tlevel > 0 { trollback 1 }
    if extra [ "no-registry" {
        do ..Run(case, "teardown.registry.on", "repo -n registry -enable")
    }
    do ..End(case)
}

/// Offset of the first line containing text after the label line of a routine, or -1.
ClassMethod FindOffset(
	rtn As %String,
	label As %String,
	text As %String) As %Integer
{
    set r = ##class(%Routine).%OpenId(rtn_".INT")
    quit:'$isobject(r) -1
    set n = 0, base = "", off = -1
    while 'r.AtEnd {
        set line = r.ReadLine(), n = n + 1
        if base = "" {
            set:$piece($piece(line, "(", 1), " ", 1)=label base = n
            continue
        }
        if line [ text {
            set off = n - base
            quit
        }
    }
    quit off
}

/// Offset of the first line containing text that follows the first line containing marker, after the label.
ClassMethod FindOffsetAfter(
	rtn As %String,
	label As %String,
	marker As %String,
	text As %String) As %Integer
{
    set r = ##class(%Routine).%OpenId(rtn_".INT")
    quit:'$isobject(r) -1
    set n = 0, base = "", seen = 0, off = -1
    while 'r.AtEnd {
        set line = r.ReadLine(), n = n + 1
        if base = "" {
            set:$piece($piece(line, "(", 1), " ", 1)=label base = n
            continue
        }
        if 'seen {
            set:line[marker seen = 1
            continue
        }
        if line [ text {
            set off = n - base
            quit
        }
    }
    quit off
}

ClassMethod LineAt(
	rtn As %String,
	label As %String,
	off As %Integer) As %String
{
    set r = ##class(%Routine).%OpenId(rtn_".INT")
    quit:'$isobject(r) ""
    set n = 0, base = ""
    while 'r.AtEnd {
        set line = r.ReadLine(), n = n + 1
        if base = "" {
            set:$piece($piece(line, "(", 1), " ", 1)=label base = n
            continue
        }
        quit:(n - base)=off
    }
    quit $zstrip(line, "<>W")
}

ClassMethod JobLoadZb(
	cmd As %String,
	tag As %String,
	nest As %String,
	loc As %String)
{
    set arg = loc_":""N""::""do ##class(B2Native.Replay).ZbStall("""""_tag_""""")"""
    set ^IRIS.Temp.B2Signal(tag, "zbarg") = arg
    zbreak @arg
    if nest = "callertx" {
        tstart
    }
    set sc = ##class(%IPM.Main).Shell(cmd)
    set ^IRIS.Temp.B2Signal(tag, "sc") = sc
}

ClassMethod ZbStall(tag As %String)
{
    set ^IRIS.Temp.B2Signal(tag, "tlevel") = $tlevel
    set ^IRIS.Temp.B2Signal(tag, "pid") = $job
    for i=1:1:1500 {
        quit:$data(^IRIS.Temp.B2Signal(tag, "release"))
        hang 0.2
    }
}

ClassMethod JobLoad(
	cmd As %String,
	tag As %String,
	nest As %String)
{
    if nest = "callertx" {
        tstart
    }
    set sc = ##class(%IPM.Main).Shell(cmd)
    set ^IRIS.Temp.B2Signal(tag, "sc") = sc
}

ClassMethod Slots(
	case,
	prefix,
	top,
	dep1,
	dep2)
{
    for pair = $listbuild("top", top), $listbuild("dep1", dep1), $listbuild("dep2", dep2) {
        set slot = $listget(pair, 1), name = $listget(pair, 2)
        continue:name=""
        do ..ModSlot(case, prefix_"modules."_slot_".", name)
    }
}

ClassMethod ModSlot(
	case,
	prefix,
	name)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT VersionString, DeveloperMode, LastUpdated FROM %IPM_Storage.ModuleItem WHERE Name = ?", name)
    if rs.%Next() {
        do ..Obs(case, prefix_"manifest_present", 1)
        do ..Obs(case, prefix_"version", rs.%Get("VersionString"))
        do ..Obs(case, prefix_"developer_mode", +rs.%Get("DeveloperMode"))
        do ..Obs(case, prefix_"x.last_updated", rs.%Get("LastUpdated"))
    } else {
        do ..Obs(case, prefix_"manifest_present", 0)
    }
    do ..Obs(case, prefix_"resources_loaded", ##class(%Dictionary.CompiledClass).%ExistsId(..PkgOf(name)_".Main"))
    do ..Obs(case, prefix_"zpm_document_version", ..DocVersion(name))
}

/// Version declared in the stored .ZPM document (%IPM.StudioDocument.ModuleStream), or <none>.
ClassMethod DocVersion(name As %String) As %String
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT ID FROM %IPM_StudioDocument.ModuleStream WHERE Name = ?", name)
    quit:'rs.%Next() "<none>"
    set doc = ##class(%IPM.StudioDocument.ModuleStream).%OpenId(rs.%Get("ID"), 0)
    quit:'$isobject(doc) "<none>"
    set text = doc.Contents.Read(32000)
    set m = ##class(%Regex.Matcher).%New("<Version>([^<]*)</Version>", text)
    quit $select(m.Locate():m.Group(1), 1:"<no version element>")
}

ClassMethod PkgOf(name) As %String
{
    set out = ""
    for i=1:1:$length(name, "-") {
        set p = $piece(name, "-", i)
        set out = out_$zconvert($extract(p), "U")_$extract(p, 2, *)
    }
    quit out
}

ClassMethod Observe(
	case,
	top,
	dep1,
	dep2,
	base,
	c0,
	tbase)
{
    do ..Slots(case, "", top, dep1, dep2)
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT * FROM %IPM_General.History WHERE ID > ? ORDER BY ID", base)
    set n = 0
    kill seen
    while rs.%Next() {
        set n = n + 1
        set pkg = rs.%Get("Package")
        set slot = ..SlotOf(pkg, dep1, dep2)
        set seen(slot) = $get(seen(slot)) + 1
        continue:seen(slot)>1
        set p = "history."_slot_"."
        do ..Obs(case, p_"row", 1)
        do ..Obs(case, p_"id_order", rs.%Get("ID") - c0)
        do ..Obs(case, p_"package_set", pkg '= "")
        do ..Obs(case, p_"version", ..VersionFrom(rs))
        do ..Obs(case, p_"success", ..StatusKind(rs.%Get("Success")))
        do ..Obs(case, p_"committed", rs.%Get("Committed"))
        do ..Obs(case, p_"time_end_set", rs.%Get("TimeEnd") '= "")
        do ..Obs(case, p_"finalized", +rs.%Get("Finalized"))
        do ..PhaseFields(case, p, rs.%Get("Phases"))
        do ..Obs(case, p_"x.package", pkg)
        do ..Obs(case, p_"x.success_text", ..StatusText(rs.%Get("Success")))
    }
    for slot = "top", "dep1", "dep2" {
        continue:(slot="dep1")&&(dep1="")
        continue:(slot="dep2")&&(dep2="")
        if '$data(seen(slot)) {
            do ..Obs(case, "history."_slot_".row", 0)
        } elseif seen(slot) > 1 {
            do ..Obs(case, "history."_slot_".x.rows", seen(slot))
        }
    }
    do ..Obs(case, "history_rows", n)
    do ..Obs(case, "x.ids_allocated", +$get(^IPM.General.HistoryD) - c0)
    kill seen
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT * FROM %IPM_General.HistoryTemp WHERE ID > ? ORDER BY ID", tbase)
    while rs.%Next() {
        set pkg = rs.%Get("Package")
        set slot = ..SlotOf(pkg, dep1, dep2)
        set seen(slot) = $get(seen(slot)) + 1
        continue:seen(slot)>1
        set p = "history_temp."_slot_"."
        do ..Obs(case, p_"row", 1)
        do ..Obs(case, p_"package_set", pkg '= "")
        do ..Obs(case, p_"success", ..StatusKind(rs.%Get("Success")))
        do ..Obs(case, p_"committed", rs.%Get("Committed"))
        do ..Obs(case, p_"time_end_set", rs.%Get("TimeEnd") '= "")
        do ..PhaseFields(case, p, rs.%Get("Phases"))
    }
    for slot = "top", "dep1", "dep2" {
        continue:(slot="dep1")&&(dep1="")
        continue:(slot="dep2")&&(dep2="")
        if '$data(seen(slot)) {
            do ..Obs(case, "history_temp."_slot_".row", 0)
        }
    }
}

/// Dependency rows carry the dependency name from Init; any other row of the action is the top module's.
ClassMethod SlotOf(
	pkg,
	dep1,
	dep2) As %String
{
    if (dep1 '= "") && (pkg = dep1) { quit "dep1" }
    if (dep2 '= "") && (pkg = dep2) { quit "dep2" }
    quit "top"
}

ClassMethod PhaseFields(
	case,
	prefix,
	phases)
{
    set n = $listlength(phases)
    do ..Obs(case, prefix_"phases_started", n)
    if n = 0 {
        do ..Obs(case, prefix_"last_phase_status", "<none>")
        quit
    }
    set ph = $listget($listget(phases, n), 1)
    if $listget(ph, 3) = "" {
        do ..Obs(case, prefix_"last_phase_status", "OPEN")
    } else {
        do ..Obs(case, prefix_"last_phase_status", ..StatusKind($listget(ph, 4)))
    }
    do ..Obs(case, prefix_"x.last_phase_name", $listget(ph, 1))
}

}
