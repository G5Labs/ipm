/// B2 native cases for load and history hypotheses a-g. Each case drives the real entry point
/// (##class(%IPM.Main).Shell) and reads back the persisted state by SQL. Expected values live in
/// native/cases/expected-*.json and are never computed here.
Class B2Native.Cases Extends B2Native.Util
{

/// Run case methods by name, e.g. do ##class(B2Native.Cases).Go("A0,A1")
ClassMethod Go(list As %String)
{
    for i=1:1:$length(list, ",") {
        set case = $piece(list, ",", i)
        continue:case=""
        do ..Begin(case)
        try {
            do $classmethod($classname(), "Case"_$translate(case, "-", ""), case)
        } catch ex {
            do ..Obs(case, "case.exception", ex.DisplayString())
        }
        while $tlevel > 0 { trollback 1 }
        do ..End(case)
    }
}

ClassMethod HistCounter() As %Integer
{
    quit +$get(^IPM.General.HistoryD)
}

/// Common read-back after a load of a parent with one dependency.
ClassMethod ParentDepState(
	case,
	parent,
	dep,
	base,
	tbase,
	counter0)
{
    do ..ModState(case, parent, "par.")
    do ..ModState(case, dep, "dep.")
    do ..ClassState(case, ..PkgOf(parent)_".Main", "par.cls.")
    do ..ClassState(case, ..PkgOf(dep)_".Main", "dep.cls.")
    do ..HistFor(case, parent, base, "par.hist.")
    do ..HistFor(case, dep, base, "dep.hist.")
    do ..HistSince(case, base, "all.")
    do ..Obs(case, "ids.allocated", ..HistCounter() - counter0)
    do ..TempRows(case, "temp.", tbase)
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

/// Uninstall every b2-* fixture module and update-simple so the cases can be replayed from a clean module set.
ClassMethod CaseReset(case)
{
    for pass=1:1:4 {
        set names = ""
        set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %IPM_Storage.ModuleItem WHERE Name %STARTSWITH 'b2-' OR Name = 'update-simple'")
        while rs.%Next() { set names = names_$listbuild(rs.%Get("Name")) }
        quit:names=""
        set ptr = 0
        while $listnext(names, ptr, name) {
            do ..Run(case, "uninstall."_name_".pass"_pass, "uninstall "_name)
        }
    }
    do ..Obs(case, "remaining", ..SqlCount("SELECT COUNT(*) FROM %IPM_Storage.ModuleItem WHERE Name %STARTSWITH 'b2-' OR Name = 'update-simple'"))
    do ..Obs(case, "temp.rows", ..SqlCount("SELECT COUNT(*) FROM %IPM_General.HistoryTemp"))
    kill ^IRIS.Temp.B2Signal
}

// ---------- Q-14 and reader-side checks ----------

ClassMethod CaseH1(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId()
    do ..Run(case, "load", "load "_..Fixture("b2-invoke-activate"))
    do ..ModState(case, "b2-invoke-activate", "mod.")
    do ..HistSince(case, base, "hist.")
    do ..TempRows(case, "temp.", tbase)
}

/// Latest history row whose CommandString equals cmd.
ClassMethod RowFor(cmd As %String) As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT TOP 1 ID FROM %IPM_General.History WHERE CommandString = ? ORDER BY ID DESC", cmd)
    quit $select(rs.%Next():rs.%Get("ID"), 1:"")
}

/// Run `history details <id> -phases` and record each "Column: value" line and each phase line.
ClassMethod Details(
	case,
	cmd)
{
    set id = ..RowFor(cmd)
    do ..Obs(case, "row.id", id)
    quit:id=""
    set sc = ..RunCapture(case, "details", "history details "_id_" -phases", .lines)
    set inPhases = 0
    for i=1:1:lines {
        set line = lines(i)
        if line = "Phases: " || (line = "Phases:") { set inPhases = 1 continue }
        if inPhases {
            set name = $piece(line, " ", 1)
            continue:(name="")||(name="Name")
            set ended = (line [ " sec)")
            do ..Obs(case, "det.phase."_name, $select(ended:"ended", 1:"open")_"|"_$piece(line, " ", $select(ended:6, 1:4), *))
            continue
        }
        if line [ ": " {
            set col = $piece(line, ": ", 1)
            continue:col'?1A.AN
            set val = $piece(line, ": ", 2, *)
            if col = "Success" { set val = $select(val="0":"0", val="1":"1", 1:"<raw %Status, "_$length(val)_" chars>") }
            do ..Obs(case, "det."_col, val)
        }
    }
    set time = ..SqlCount("SELECT COUNT(*) FROM %IPM_General.History WHERE ID = ? AND TimeEnd IS NULL", id)
    do ..Obs(case, "sql.timeend.null", time)
}

ClassMethod CaseR1(case)
{
    do ..Details(case, "load "_..Fixture("b2-hang-nd"))
}

ClassMethod CaseR2(case)
{
    do ..Details(case, "load -dev "_..Fixture("b2-hang-dev"))
}

ClassMethod CaseR3(case)
{
    do ..Details(case, "load "_..UpdDir(2))
}

/// Which load attempts on update-simple does a package filter find?
ClassMethod CaseR4(case)
{
    do ..Obs(case, "truth.package", ..SqlCount("SELECT COUNT(*) FROM %IPM_General.History WHERE Package = 'update-simple'"))
    do ..Obs(case, "truth.attempts", ..SqlCount("SELECT COUNT(*) FROM %IPM_General.History WHERE CommandString [ 'update-simple'"))
    set sc = ..RunCapture(case, "find", "history find -DPackage=update-simple -limit 0", .lines)
    do ..Obs(case, "shown", ..CountFindRows(.lines))
}

// ---------- (a) dependency loads inside a parent load (Q-06, INT-HIST-02/08) ----------

ClassMethod CaseA0(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId(), c0 = ..HistCounter()
    do ..Run(case, "load", "load "_..Fixture("b2-par-ok"))
    do ..ParentDepState(case, "b2-par-ok", "b2-dep-a0", base, tbase, c0)
}

ClassMethod CaseA1(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId(), c0 = ..HistCounter()
    do ..Run(case, "load", "load "_..Fixture("b2-par-actfail"))
    do ..ParentDepState(case, "b2-par-actfail", "b2-dep-a1", base, tbase, c0)
}

ClassMethod CaseA3(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId(), c0 = ..HistCounter()
    do ..Run(case, "load", "load "_..Fixture("b2-par-depfail"))
    do ..ParentDepState(case, "b2-par-depfail", "b2-dep-fail", base, tbase, c0)
}

ClassMethod CaseA2(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId(), c0 = ..HistCounter()
    do ..Run(case, "load", "load -dev "_..Fixture("b2-par-depfail"))
    do ..ParentDepState(case, "b2-par-depfail", "b2-dep-fail", base, tbase, c0)
}

ClassMethod CaseA4(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId(), c0 = ..HistCounter()
    do ..Run(case, "load", "load -dev "_..Fixture("b2-par-dev-ok"))
    do ..ParentDepState(case, "b2-par-dev-ok", "b2-dep-a2", base, tbase, c0)
}

ClassMethod CaseA5(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId(), c0 = ..HistCounter()
    do ..Run(case, "load", "load -dev "_..Fixture("b2-par-actfail"))
    do ..ParentDepState(case, "b2-par-actfail", "b2-dep-a1", base, tbase, c0)
}

// ---------- (b) system requirement refusals (Q-17, INT-LOAD-11) ----------

ClassMethod SysReq(
	case,
	name,
	cmd)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId()
    do ..Run(case, "load", cmd)
    do ..ModState(case, name, "mod.")
    do ..ClassState(case, ..PkgOf(name)_".Main", "cls.")
    do ..HistSince(case, base, "hist.")
    do ..TempRows(case, "temp.", tbase)
}

ClassMethod CaseB0(case)
{
    do ..SysReq(case, "b2-sysreq-ok", "load -dev "_..Fixture("b2-sysreq-ok"))
}

ClassMethod CaseB1(case)
{
    do ..SysReq(case, "b2-sysreq-nd", "load "_..Fixture("b2-sysreq-nd"))
}

ClassMethod CaseB2(case)
{
    do ..SysReq(case, "b2-sysreq-dev", "load -dev "_..Fixture("b2-sysreq-dev"))
    // Can the manifest left behind be removed with the ordinary command?
    do ..Run(case, "uninstall", "uninstall b2-sysreq-dev")
    do ..ModState(case, "b2-sysreq-dev", "after.uninstall.mod.")
}

ClassMethod CaseB3(case)
{
    do ..SysReq(case, "b2-sysreq-notx", "load "_..Fixture("b2-sysreq-notx")_" -DNoTransaction=1")
}

ClassMethod CaseB4(case)
{
    do ..SysReq(case, "b2-sysreq-nojrn", "load "_..Fixture("b2-sysreq-nojrn")_" -DNoJournal=1")
}

// ---------- (c) process end mid-load (Q-08, INT-HIST-03) ----------

/// JOB entry: run one shell command and record its status in IRISTEMP.
ClassMethod JobShell(
	cmd As %String,
	tag As %String)
{
    set ^IRIS.Temp.B2Signal(tag, "jobstart") = $job
    set sc = ##class(%IPM.Main).Shell(cmd)
    set ^IRIS.Temp.B2Signal(tag, "sc") = sc
}

/// how: "release" lets the load finish, "terminate" uses $SYSTEM.Process.Terminate, "sigkill" uses kill -9.
ClassMethod Stall(
	case,
	name,
	flags,
	how)
{
    set tag = ..PkgOf(name)
    kill ^IRIS.Temp.B2Signal(tag)
    if how = "release" {
        set ^IRIS.Temp.B2Signal(tag, "release") = 1
    }
    set base = ..MaxHistId(), tbase = ..MaxTempId()
    set cmd = "load "_flags_..Fixture(name)
    write !,"CMD ",case," job ",cmd,!
    job ##class(B2Native.Cases).JobShell(cmd, tag)
    set pid = $zchild
    do ..Obs(case, "job.started", pid > 0)
    do ..Obs(case, "signal", ..WaitSignal(tag, 300))
    do ..Obs(case, "invoke.tlevel", $get(^IRIS.Temp.B2Signal(tag, "tlevel")))
    if how '= "release" {
        // In-flight state as another process sees it (no isolation for uncommitted data).
        do ..TempRows(case, "inflight.temp.", tbase)
        do ..HistSince(case, base, "inflight.hist.")
        do ..ModState(case, name, "inflight.mod.")
        set t0 = $zhorolog
        if how = "terminate" {
            do ..Obs(case, "end.rc", $system.Process.Terminate(pid))
        } else {
            do ..Obs(case, "end.rc", $zf(-100, "", "/usr/bin/kill", "-9", pid))
        }
        do ..Obs(case, "gone", ..WaitGone(pid, 400))
        do ..Obs(case, "end.seconds", $justify($zhorolog - t0, 0, 0))
        hang 2
    } else {
        do ..Obs(case, "gone", ..WaitGone(pid, 300))
        set sc = $get(^IRIS.Temp.B2Signal(tag, "sc"))
        do ..Obs(case, "job.sc.ok", $select(sc="":"<none>", 1:$$$ISOK(sc)))
    }
    do ..Obs(case, "job.sc.recorded", $data(^IRIS.Temp.B2Signal(tag, "sc")))
    do ..ModState(case, name, "mod.")
    do ..ClassState(case, tag_".Main", "cls.")
    do ..HistSince(case, base, "hist.")
    do ..TempRows(case, "temp.", tbase)
    // Orphaned IRISTEMP rows are instance-wide; remove them after recording so later suites start clean.
    set id = tbase, n = 0
    for {
        set id = $order(^IRIS.Temp.HistoryTempD(id))
        quit:id=""
        do ##class(%IPM.General.HistoryTemp).%DeleteId(id)
        set n = n + 1
    }
    do ..Obs(case, "cleanup.temp.deleted", n)
    kill ^IRIS.Temp.B2Signal(tag)
}

ClassMethod CaseC0(case)
{
    do ..Stall(case, "b2-hang-ok", "", "release")
}

ClassMethod CaseC1(case)
{
    do ..Stall(case, "b2-hang-nd", "", "terminate")
}

ClassMethod CaseC2(case)
{
    do ..Stall(case, "b2-hang-dev", "-dev ", "terminate")
}

ClassMethod CaseC3(case)
{
    do ..Stall(case, "b2-hang-kill-nd", "", "sigkill")
}

ClassMethod CaseC4(case)
{
    do ..Stall(case, "b2-hang-kill-dev", "-dev ", "sigkill")
}

/// Re-observe C4 once IRIS has removed the SIGKILLed process slot.
ClassMethod CaseC4R(case)
{
    set t0 = $zhorolog, left = 1
    for i=1:1:300 {
        set left = ..LiveB2Jobs()
        quit:left=0
        hang 2
    }
    do ..Obs(case, "gone", left = 0)
    do ..Obs(case, "wait.seconds", $justify($zhorolog - t0, 0, 0))
    do ..ModState(case, "b2-hang-kill-dev", "mod.")
    do ..ClassState(case, "B2HangKillDev.Main", "cls.")
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT TOP 1 * FROM %IPM_General.History WHERE CommandString %STARTSWITH 'load -dev /home/irisowner/zpm/.native/fixtures/b2-hang-kill-dev' ORDER BY ID DESC")
    set n = 0
    while rs.%Next() {
        set n = n + 1
        do ..ObsRow(case, "hist.row1.", rs)
    }
    do ..Obs(case, "hist.rows", n)
    do ..Obs(case, "temp.orphans", ..SqlCount("SELECT COUNT(*) FROM %IPM_General.HistoryTemp"))
}

/// Number of processes (live or awaiting cleanup) whose routine is a B2 fixture or B2Native class.
ClassMethod LiveB2Jobs() As %Integer
{
    set n = 0
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Pid, Routine FROM %SYS.ProcessQuery")
    while rs.%Next() {
        if (rs.%Get("Pid") '= $job) && ((rs.%Get("Routine") [ "B2Hang") || (rs.%Get("Routine") [ "B2Native")) {
            set n = n + 1
            write !,"NOTE live ",rs.%Get("Pid")," ",rs.%Get("Routine"),!
        }
    }
    quit n
}

// ---------- (d) load inside a caller-held transaction (Q-05) ----------

ClassMethod CaseD0(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId()
    tstart
    do ..Run(case, "load", "load "_..Fixture("b2-ctx-commit"))
    tcommit
    do ..Obs(case, "after.tlevel", $tlevel)
    do ..ModState(case, "b2-ctx-commit", "mod.")
    do ..ClassState(case, "B2CtxCommit.Main", "cls.")
    do ..HistSince(case, base, "hist.")
    do ..TempRows(case, "temp.", tbase)
}

ClassMethod CaseD1(case)
{
    set base = ..MaxHistId(), tbase = ..MaxTempId(), c0 = ..HistCounter()
    tstart
    do ..Run(case, "load", "load "_..Fixture("b2-ctx-rb"))
    do ..HistSince(case, base, "inflight.hist.")
    trollback
    do ..Obs(case, "after.tlevel", $tlevel)
    do ..ModState(case, "b2-ctx-rb", "mod.")
    do ..ClassState(case, "B2CtxRb.Main", "cls.")
    do ..HistSince(case, base, "hist.")
    do ..Obs(case, "ids.allocated", ..HistCounter() - c0)
    do ..TempRows(case, "temp.", tbase)
}

// ---------- (e) history filters (Q-10) ----------

ClassMethod Total() As %Integer
{
    quit ..SqlCount("SELECT COUNT(*) FROM %IPM_General.History")
}

ClassMethod Find(
	case,
	cmd,
	truthSql,
	args...)
{
    do ..Obs(case, "total", ..Total())
    if $get(truthSql) '= "" {
        do ..Obs(case, "truth", ..SqlCount(truthSql, args...))
    }
    set sc = ..RunCapture(case, "find", cmd, .lines)
    do ..Obs(case, "shown", ..CountFindRows(.lines))
}

ClassMethod CaseE0(case)
{
    do ..Find(case, "history find -DPackage=b2-dep-a0 -limit 0", "SELECT COUNT(*) FROM %IPM_General.History WHERE Package = ?", "b2-dep-a0")
}

ClassMethod CaseE1(case)
{
    do ..Find(case, "history find -DVersion_Major=1 -limit 0", "SELECT COUNT(*) FROM %IPM_General.History WHERE Version_Major = ?", 1)
}

ClassMethod CaseE2(case)
{
    do ..Find(case, "history find -DAction_x=1 -DPackage=no-such-package -limit 0", "SELECT COUNT(*) FROM %IPM_General.History WHERE Package = ?", "no-such-package")
}

ClassMethod CaseE3(case)
{
    do ..Find(case, "history find -DNoSuchColumn=1 -limit 0", "")
}

ClassMethod CaseE8(case)
{
    do ..Find(case, "history find -DCommitted=1 -DVersion_Major=0 -limit 0", "SELECT COUNT(*) FROM %IPM_General.History WHERE Committed = 1 AND Version_Major = 0")
    do ..Obs(case, "truth.committed.only", ..SqlCount("SELECT COUNT(*) FROM %IPM_General.History WHERE Committed = 1"))
}

ClassMethod Backup()
{
    kill ^B2Backup
    merge ^B2Backup("D") = ^IPM.General.HistoryD
    merge ^B2Backup("I") = ^IPM.General.HistoryI
}

ClassMethod Restore()
{
    kill ^IPM.General.HistoryD, ^IPM.General.HistoryI
    merge ^IPM.General.HistoryD = ^B2Backup("D")
    merge ^IPM.General.HistoryI = ^B2Backup("I")
}

ClassMethod Delete(
	case,
	cmd,
	truthSql,
	args...)
{
    set before = ..Total()
    do ..Obs(case, "total.before", before)
    if $get(truthSql) '= "" {
        set truth = ..SqlCount(truthSql, args...)
        do ..Obs(case, "truth", truth)
        do ..Obs(case, "expected.after", before - truth)
    }
    do ..Backup()
    set sc = ..RunCapture(case, "delete", cmd, .lines)
    do ..Obs(case, "total.after", ..Total())
    do ..Restore()
    do ..Obs(case, "total.restored", ..Total())
}

ClassMethod CaseE4(case)
{
    do ..Delete(case, "history delete -DVersion_Major=99", "SELECT COUNT(*) FROM %IPM_General.History WHERE Version_Major = ?", 99)
}

ClassMethod CaseE5(case)
{
    do ..Delete(case, "history delete", "")
}

ClassMethod CaseE6(case)
{
    do ..Delete(case, "history delete -DPackage=b2-probe-p14", "SELECT COUNT(*) FROM %IPM_General.History WHERE Package = ?", "b2-probe-p14")
}

ClassMethod CaseE7(case)
{
    do ..Delete(case, "history delete -DNoSuchColumn=1", "")
}

// ---------- (f) retention purge (Q-13, INT-HIST-13) ----------

ClassMethod NewRow(
	package,
	daysAgo,
	finalized) As %Integer
{
    set h = ##class(%IPM.General.History).%New()
    set h.Action = "load"
    set h.Package = package
    set h.CommandString = "b2 retention probe"
    set h.Version = ##class(%IPM.General.SemanticVersion).FromString("1.0.0")
    set h.TimeStart = $zdatetime(($horolog - daysAgo)_","_$piece($horolog, ",", 2), 3)
    set h.TimeEnd = h.TimeStart
    set h.Success = 1
    set h.Committed = 1
    set h.Finalized = finalized
    $$$ThrowOnError(h.%Save())
    quit h.%Id()
}

ClassMethod LastPurge() As %String
{
    quit ##class(%IPM.General.History).GetHistoryLastPurge()
}

ClassMethod KillLastPurge()
{
    new $namespace
    set index = ##class(%IPM.Repo.UniversalSettings).#HistoryRetain
    set $namespace = "%SYS"
    kill ^IPM.settings(index, "lastPurge")
}

/// `config delete HistoryRetain` on a key whose first ever value was non-zero (A2 OBS-H4).
/// The settings subtree is saved and restored so the instance is left as found.
ClassMethod CaseF0(case)
{
    set orig = $namespace
    set index = ##class(%IPM.Repo.UniversalSettings).#HistoryRetain
    set $namespace = "%SYS"
    kill ^IRIS.Temp.B2Settings
    merge ^IRIS.Temp.B2Settings = ^IPM.settings(index)
    kill ^IPM.settings(index)
    set $namespace = orig
    do ..Obs(case, "fresh.value", ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain())
    do ..Run(case, "set7", "config set HistoryRetain 7")
    do ..Run(case, "set0", "config set HistoryRetain 0")
    do ..Obs(case, "after.set0", ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain())
    do ..Run(case, "delete", "config delete HistoryRetain")
    do ..Obs(case, "after.delete", ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain())
    set $namespace = "%SYS"
    kill ^IPM.settings(index)
    merge ^IPM.settings(index) = ^IRIS.Temp.B2Settings
    kill ^IRIS.Temp.B2Settings
    set $namespace = orig
    do ..Obs(case, "restored.value", ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain())
}

ClassMethod CaseF1(case)
{
    do ..Obs(case, "retain.initial", ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain())
    do ..Obs(case, "lastpurge.initial", ..LastPurge())
    set oldF = ..NewRow("b2-purge-old-final", 40, 1)
    set oldU = ..NewRow("b2-purge-old-unfinal", 40, 0)
    set recent = ..NewRow("b2-purge-recent", 1, 1)
    do ..Run(case, "config", "config set HistoryRetain 30")
    do ..Obs(case, "retain.set", ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain())
    do ..KillLastPurge()
    do ..Obs(case, "lastpurge.before", ..LastPurge())
    do ..Obs(case, "diag.cutoff", $system.SQL.Functions.DATEADD("d", -30, +$horolog))
    // Any shell command runs AutoPurgeOldHistory first.
    do ..RunCapture(case, "trigger", "history find -limit 1", .lines)
    do ..Obs(case, "old.final.exists", ##class(%IPM.General.History).%ExistsId(oldF))
    do ..Obs(case, "old.unfinal.exists", ##class(%IPM.General.History).%ExistsId(oldU))
    do ..Obs(case, "recent.exists", ##class(%IPM.General.History).%ExistsId(recent))
    do ..Obs(case, "lastpurge.after", ..LastPurge())
    do ..Obs(case, "today", +$horolog)
    // Same day again: the purge is skipped because lastPurge is today.
    set sc = ##class(%IPM.General.History).AutoPurgeOldHistory()
    do ..Obs(case, "second.ok", $$$ISOK(sc))
    do ..Obs(case, "second.old.final.exists", ##class(%IPM.General.History).%ExistsId(oldF))
    // Adapter control, not a product observation: the same cutoff without embedded quotes.
    kill filter
    set filter("TimeStart") = "<="_$system.SQL.Functions.DATEADD("d", -30, +$horolog)
    set n = ##class(%IPM.General.History).DeleteHistory(.filter)
    do ..Obs(case, "control.unquoted.deleted", n)
    do ..Obs(case, "control.old.final.exists", ##class(%IPM.General.History).%ExistsId(oldF))
    do ..Obs(case, "control.recent.exists", ##class(%IPM.General.History).%ExistsId(recent))
    do ..Run(case, "restore", "config set HistoryRetain 0")
    do ..Obs(case, "retain.final", ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain())
    for id = oldF, oldU, recent {
        do ##class(%IPM.General.History).%DeleteId(id)
    }
}

// ---------- (g) same, newer and older versions (INT-LOAD-09/10/18, Q-01, Q-02) ----------

ClassMethod UpdDir(major) As %String
{
    quit "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/update-test/update-modules-"_major_".x/update-simple/"
}

ClassMethod Step(
	case,
	label,
	cmd)
{
    set base = ..MaxHistId()
    do ..Run(case, label, cmd)
    do ..ModState(case, "update-simple", label_".mod.")
    do ..HistSince(case, base, label_".hist.")
}

ClassMethod CaseG(case)
{
    if ##class(%IPM.Storage.Module).NameExists("update-simple") {
        do ..Run(case, "pre", "uninstall update-simple")
    }
    do ..Step(case, "g1", "load "_..UpdDir(1))
    do ..Step(case, "g2", "load "_..UpdDir(1))
    do ..Step(case, "g3", "load "_..UpdDir(2))
    do ..Step(case, "g4", "load -force "_..UpdDir(2))
    do ..Step(case, "g5", "load -dev "_..UpdDir(3))
    do ..Step(case, "g6", "load -force "_..UpdDir(1))
    do ..Step(case, "g7", "load -dev "_..UpdDir(1))
    do ..Step(case, "g8", "load "_..UpdDir(3))
    do ..Step(case, "g9", "load -nodev "_..UpdDir(3))
    do ..Step(case, "g10", "uninstall update-simple")
}

}
