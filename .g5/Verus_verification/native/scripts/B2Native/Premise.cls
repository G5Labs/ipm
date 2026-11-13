/// Provider premise probes (N-Pnn) for IRIS transaction, IRISTEMP, object-lifetime and trigger behavior
/// that the IPM load/history design relies on. Each probe prints OBS lines only.
Class B2Native.Premise
{

ClassMethod All()
{
    for p = "P01","P02","P03","P04","P05","P06","P07","P08","P09","P10","P11","P12","P13","P14","P15","P16","P17" {
        set case = "N-"_p
        do ##class(B2Native.Util).Begin(case)
        try {
            do $classmethod($classname(), p, case)
        } catch ex {
            do ##class(B2Native.Util).Obs(case, "probe.exception", ex.DisplayString())
        }
        // A probe must never leave a transaction open for the next one.
        while $tlevel > 0 { trollback 1 }
        do ##class(B2Native.Util).End(case)
    }
}

/// Full TROLLBACK reverts a journaled global set in the namespace database.
ClassMethod P01(case)
{
    kill ^B2P("p01")
    tstart
    set ^B2P("p01") = 1
    do ##class(B2Native.Util).Obs(case, "inside.data", $data(^B2P("p01")))
    trollback
    do ##class(B2Native.Util).Obs(case, "after.data", $data(^B2P("p01")))
    do ##class(B2Native.Util).Obs(case, "after.tlevel", $tlevel)
}

/// TROLLBACK 1 at $TLEVEL=2 undoes only the innermost level.
ClassMethod P02(case)
{
    kill ^B2P("p02")
    tstart
    set ^B2P("p02","L1") = 1
    tstart
    set ^B2P("p02","L2") = 1
    do ##class(B2Native.Util).Obs(case, "before.tlevel", $tlevel)
    trollback 1
    do ##class(B2Native.Util).Obs(case, "after1.tlevel", $tlevel)
    do ##class(B2Native.Util).Obs(case, "after1.L1.data", $data(^B2P("p02","L1")))
    do ##class(B2Native.Util).Obs(case, "after1.L2.data", $data(^B2P("p02","L2")))
    tcommit
    do ##class(B2Native.Util).Obs(case, "final.L1.data", $data(^B2P("p02","L1")))
    kill ^B2P("p02")
}

/// An inner TCOMMIT at $TLEVEL=2 is not durable when the outer level rolls back.
ClassMethod P03(case)
{
    kill ^B2P("p03")
    tstart
    set ^B2P("p03","L1") = 1
    tstart
    set ^B2P("p03","L2") = 1
    tcommit
    do ##class(B2Native.Util).Obs(case, "after.inner.tcommit.tlevel", $tlevel)
    trollback
    do ##class(B2Native.Util).Obs(case, "after.outer.rollback.L1.data", $data(^B2P("p03","L1")))
    do ##class(B2Native.Util).Obs(case, "after.outer.rollback.L2.data", $data(^B2P("p03","L2")))
}

/// ^IRIS.Temp* (IRISTEMP) sets survive both TROLLBACK and TROLLBACK 1.
ClassMethod P04(case)
{
    kill ^IRIS.Temp.B2P
    tstart
    set ^IRIS.Temp.B2P("full") = 1
    trollback
    do ##class(B2Native.Util).Obs(case, "full.after.data", $data(^IRIS.Temp.B2P("full")))
    tstart
    tstart
    set ^IRIS.Temp.B2P("partial") = 1
    trollback 1
    do ##class(B2Native.Util).Obs(case, "partial.after.data", $data(^IRIS.Temp.B2P("partial")))
    trollback
    do ##class(B2Native.Util).Obs(case, "partial.after.outer.data", $data(^IRIS.Temp.B2P("partial")))
    kill ^IRIS.Temp.B2P
}

/// $INCREMENT is not undone by rollback, so a History row inserted in a rolled-back transaction leaves an ID gap.
ClassMethod P05(case)
{
    set before = +$get(^B2P("p05ctr"))
    tstart
    set x = $increment(^B2P("p05ctr"))
    trollback
    do ##class(B2Native.Util).Obs(case, "counter.delta", +$get(^B2P("p05ctr")) - before)
    tstart
    set h = ..NewHistory("b2-probe-p05")
    set sc = h.%Save()
    set id1 = h.%Id()
    kill h
    trollback
    do ##class(B2Native.Util).Obs(case, "rolledback.row.exists", ##class(%IPM.General.History).%ExistsId(id1))
    set h = ..NewHistory("b2-probe-p05")
    set sc = h.%Save()
    set id2 = h.%Id()
    kill h
    do ##class(B2Native.Util).Obs(case, "next.id.gap", id2 - id1)
    do ##class(%IPM.General.History).%DeleteId(id2)
}

/// ^IRIS.Temp* is shared across namespaces; a namespace-database global is not.
ClassMethod P06(case)
{
    set orig = $namespace
    set ^IRIS.Temp.B2P("ns") = $namespace
    set ^B2P("p06") = $namespace
    set $namespace = "%SYS"
    set sysTemp = $get(^IRIS.Temp.B2P("ns"))
    set sysUser = $data(^B2P("p06"))
    set uvTemp = "<namespace missing>"
    try {
        set $namespace = "USER-VERIFY"
        set uvTemp = $get(^IRIS.Temp.B2P("ns"))
    } catch {}
    set $namespace = orig
    do ##class(B2Native.Util).Obs(case, "sys.iristemp.value", sysTemp)
    do ##class(B2Native.Util).Obs(case, "sys.userglobal.data", sysUser)
    do ##class(B2Native.Util).Obs(case, "userverify.iristemp.value", uvTemp)
    kill ^IRIS.Temp.B2P
}

/// With process journaling disabled through %IPM.Utils.JournalManager (the NoJournal path), what TROLLBACK does.
ClassMethod P07(case)
{
    kill ^B2P("p07")
    set jm = ##class(%IPM.Utils.JournalManager).%New(0)
    do ##class(B2Native.Util).Obs(case, "journal.enabled.during", jm.Enabled())
    tstart
    set ^B2P("p07") = 1
    trollback
    do ##class(B2Native.Util).Obs(case, "after.rollback.data", $data(^B2P("p07")))
    kill jm
    set jm2 = ##class(%IPM.Utils.JournalManager).%New(1)
    do ##class(B2Native.Util).Obs(case, "journal.enabled.after.scope", jm2.Enabled())
    kill jm2
    kill ^B2P("p07")
}

/// %OnClose runs when the last reference is dropped, not before.
ClassMethod P08(case)
{
    kill ^IRIS.Temp.B2Close("p08")
    set o = ##class(B2Native.CloseProbe).%New("p08")
    set o2 = o
    kill o
    do ##class(B2Native.Util).Obs(case, "after.first.kill", $data(^IRIS.Temp.B2Close("p08")))
    kill o2
    do ##class(B2Native.Util).Obs(case, "after.last.kill", $data(^IRIS.Temp.B2Close("p08")))
}

/// %OnClose runs while an exception unwinds the frame that held the only reference,
/// before the caller's CATCH block runs. P09b repeats it through a catch-and-rethrow frame (Main:Load pattern).
ClassMethod P09(case)
{
    kill ^IRIS.Temp.B2Close("p09"), ^IRIS.Temp.B2Close("p09b")
    try {
        do ..Thrower("p09")
    } catch ex {
        do ##class(B2Native.Util).Obs(case, "closed.in.catch", $data(^IRIS.Temp.B2Close("p09")))
    }
    try {
        do ..Rethrower("p09b")
    } catch ex {
        do ##class(B2Native.Util).Obs(case, "rethrow.closed.in.catch", $data(^IRIS.Temp.B2Close("p09b")))
    }
}

ClassMethod Thrower(tag)
{
    set o = ##class(B2Native.CloseProbe).%New(tag)
    $$$ThrowStatus($$$ERROR($$$GeneralError, "probe"))
}

ClassMethod Rethrower(tag)
{
    set o = ##class(B2Native.CloseProbe).%New(tag)
    try {
        $$$ThrowStatus($$$ERROR($$$GeneralError, "probe"))
    } catch inner {
        throw inner
    }
}

/// %OnClose does not run when a JOBbed process holding the object is terminated.
ClassMethod P10(case)
{
    kill ^IRIS.Temp.B2Close("p10"), ^IRIS.Temp.B2Signal("p10")
    job ##class(B2Native.Premise).JobHold("p10")
    set pid = $zchild
    do ##class(B2Native.Util).Obs(case, "job.started", pid>0)
    do ##class(B2Native.Util).Obs(case, "signal", ##class(B2Native.Util).WaitSignal("p10", 60))
    set rc = $system.Process.Terminate(pid)
    do ##class(B2Native.Util).Obs(case, "terminate.rc", rc)
    do ##class(B2Native.Util).Obs(case, "gone", ##class(B2Native.Util).WaitGone(pid, 60))
    do ##class(B2Native.Util).Obs(case, "closed", $data(^IRIS.Temp.B2Close("p10")))
}

ClassMethod JobHold(tag)
{
    set o = ##class(B2Native.CloseProbe).%New(tag)
    set ^IRIS.Temp.B2Signal(tag, "pid") = $job
    for i=1:1:600 {
        quit:$data(^IRIS.Temp.B2Signal(tag, "release"))
        hang 0.2
    }
    kill o
}

/// Whether %OnClose runs for a live local object when a JOBbed process HALTs normally.
ClassMethod P11(case)
{
    kill ^IRIS.Temp.B2Close("p11"), ^IRIS.Temp.B2Signal("p11")
    job ##class(B2Native.Premise).JobHalt("p11")
    set pid = $zchild
    do ##class(B2Native.Util).Obs(case, "signal", ##class(B2Native.Util).WaitSignal("p11", 60))
    do ##class(B2Native.Util).Obs(case, "gone", ##class(B2Native.Util).WaitGone(pid, 60))
    do ##class(B2Native.Util).Obs(case, "closed", $data(^IRIS.Temp.B2Close("p11")))
}

ClassMethod JobHalt(tag)
{
    set o = ##class(B2Native.CloseProbe).%New(tag)
    set ^IRIS.Temp.B2Signal(tag, "pid") = $job
    halt
}

/// IRIS rolls back the open transaction of a process terminated with $SYSTEM.Process.Terminate.
ClassMethod P12(case)
{
    kill ^B2P("p12"), ^IRIS.Temp.B2Signal("p12")
    job ##class(B2Native.Premise).JobTx("p12")
    set pid = $zchild
    do ##class(B2Native.Util).Obs(case, "signal", ##class(B2Native.Util).WaitSignal("p12", 60))
    do ##class(B2Native.Util).Obs(case, "dirty.read.data", $data(^B2P("p12")))
    do ##class(B2Native.Util).Obs(case, "job.tlevel", $get(^IRIS.Temp.B2Signal("p12", "tlevel")))
    set rc = $system.Process.Terminate(pid)
    do ##class(B2Native.Util).Obs(case, "terminate.rc", rc)
    do ##class(B2Native.Util).Obs(case, "gone", ##class(B2Native.Util).WaitGone(pid, 60))
    hang 1
    do ##class(B2Native.Util).Obs(case, "after.data", $data(^B2P("p12")))
    do ##class(B2Native.Util).Obs(case, "after.iristemp.data", $data(^IRIS.Temp.B2Signal("p12", "temp")))
}

ClassMethod JobTx(tag)
{
    tstart
    set ^B2P(tag) = 1
    set ^IRIS.Temp.B2Signal(tag, "temp") = 1
    set ^IRIS.Temp.B2Signal(tag, "tlevel") = $tlevel
    set ^IRIS.Temp.B2Signal(tag, "pid") = $job
    for i=1:1:600 {
        quit:$data(^IRIS.Temp.B2Signal(tag, "release"))
        hang 0.2
    }
    tcommit
}

/// The Finalized trigger on %IPM.General.History, for object save, SQL UPDATE and DELETE.
ClassMethod P13(case)
{
    set h = ..NewHistory("b2-probe-p13")
    set sc = h.%Save()
    set id = h.%Id()
    do ##class(B2Native.Util).Obs(case, "insert.ok", $$$ISOK(sc))
    set h.CommandString = "object-edit-unfinalized"
    set sc = h.%Save()
    do ##class(B2Native.Util).Obs(case, "unfinalized.object.save.ok", $$$ISOK(sc))
    set rs = ##class(%SQL.Statement).%ExecDirect(, "UPDATE %IPM_General.History SET CommandString = 'sql-edit-unfinalized' WHERE ID = ?", id)
    do ##class(B2Native.Util).Obs(case, "unfinalized.sql.update.sqlcode", rs.%SQLCODE)
    do h.%Reload()
    set h.Finalized = 1
    set sc = h.%Save()
    do ##class(B2Native.Util).Obs(case, "finalize.save.ok", $$$ISOK(sc))
    set h.CommandString = "object-edit-finalized"
    set sc = h.%Save()
    do ##class(B2Native.Util).Obs(case, "finalized.object.save.ok", $$$ISOK(sc))
    if $$$ISERR(sc) {
        do ##class(B2Native.Util).Obs(case, "finalized.object.save.err", $extract($system.Status.GetErrorText(sc), 1, 200))
    }
    kill h
    set rs = ##class(%SQL.Statement).%ExecDirect(, "UPDATE %IPM_General.History SET CommandString = 'sql-edit-finalized' WHERE ID = ?", id)
    do ##class(B2Native.Util).Obs(case, "finalized.sql.update.sqlcode", rs.%SQLCODE)
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT CommandString FROM %IPM_General.History WHERE ID = ?", id)
    do rs.%Next()
    do ##class(B2Native.Util).Obs(case, "stored.CommandString", rs.%Get("CommandString"))
    set rs = ##class(%SQL.Statement).%ExecDirect(, "DELETE FROM %IPM_General.History WHERE ID = ?", id)
    do ##class(B2Native.Util).Obs(case, "finalized.sql.delete.sqlcode", rs.%SQLCODE)
    do ##class(B2Native.Util).Obs(case, "after.delete.exists", ##class(%IPM.General.History).%ExistsId(id))
}

/// HistoryTemp lifecycle with the product classes: the durable twin lags until %OnClose,
/// then a deep %Save of the swizzled PersistedTwin finalizes it and the temp row is deleted.
ClassMethod P14(case)
{
    set log = ##class(%IPM.General.HistoryTemp).LoadInit("b2-probe-p14")
    set tid = log.%Id()
    set hid = log.PersistedTwin.%Id()
    do log.SetVersion("1.2.3")
    do log.SetCommitted()
    do log.Finalize($$$OK)
    do ..ObsHistory(case, "before.close.twin.", hid)
    do ##class(B2Native.Util).Obs(case, "before.close.temp.exists", ##class(%IPM.General.HistoryTemp).%ExistsId(tid))
    kill log
    do ..ObsHistory(case, "after.close.twin.", hid)
    do ##class(B2Native.Util).Obs(case, "after.close.temp.exists", ##class(%IPM.General.HistoryTemp).%ExistsId(tid))
}

/// Deleting the twin of an in-progress log (e.g. a concurrent `history delete`) before %OnClose.
ClassMethod P15(case)
{
    set log = ##class(%IPM.General.HistoryTemp).LoadInit("b2-probe-p15")
    set tid = log.%Id()
    set hid = log.PersistedTwin.%Id()
    set rs = ##class(%SQL.Statement).%ExecDirect(, "DELETE FROM %IPM_General.History WHERE ID = ?", hid)
    do ##class(B2Native.Util).Obs(case, "delete.sqlcode", rs.%SQLCODE)
    do log.Finalize($$$OK)
    set thrown = 0
    try {
        kill log
    } catch ex {
        set thrown = 1
        do ##class(B2Native.Util).Obs(case, "kill.exception", ex.DisplayString())
    }
    do ##class(B2Native.Util).Obs(case, "kill.thrown", thrown)
    do ##class(B2Native.Util).Obs(case, "twin.exists.after", ##class(%IPM.General.History).%ExistsId(hid))
    if ##class(%IPM.General.History).%ExistsId(hid) {
        do ..ObsHistory(case, "twin.after.", hid)
    }
    do ##class(B2Native.Util).Obs(case, "temp.exists.after", ##class(%IPM.General.HistoryTemp).%ExistsId(tid))
    do ##class(%IPM.General.HistoryTemp).%DeleteId(tid)
}

/// Explicit PersistToTwin followed by %OnClose (a second PersistToTwin on a finalized twin).
ClassMethod P16(case)
{
    set log = ##class(%IPM.General.HistoryTemp).LoadInit("b2-probe-p16")
    set tid = log.%Id()
    set hid = log.PersistedTwin.%Id()
    do log.Finalize($$$OK)
    set sc = log.PersistToTwin()
    do ##class(B2Native.Util).Obs(case, "first.persist.ok", $$$ISOK(sc))
    do log.SetName("b2-probe-p16-renamed")
    set sc = log.PersistToTwin()
    do ##class(B2Native.Util).Obs(case, "second.persist.ok", $$$ISOK(sc))
    if $$$ISERR(sc) {
        do ##class(B2Native.Util).Obs(case, "second.persist.err", $extract($system.Status.GetErrorText(sc), 1, 200))
    }
    set thrown = 0
    try {
        kill log
    } catch ex {
        set thrown = 1
    }
    do ##class(B2Native.Util).Obs(case, "kill.thrown", thrown)
    do ..ObsHistory(case, "twin.", hid)
    do ##class(B2Native.Util).Obs(case, "temp.exists.after.close", ##class(%IPM.General.HistoryTemp).%ExistsId(tid))
    do ##class(%IPM.General.HistoryTemp).%DeleteId(tid)
}

/// The HistoryTemp SQL table reads the same instance-wide IRISTEMP global from another namespace.
ClassMethod P17(case)
{
    set orig = $namespace
    set log = ##class(%IPM.General.HistoryTemp).LoadInit("b2-probe-p17")
    set tid = log.%Id()
    set n = "<not read>"
    try {
        set $namespace = "USER-VERIFY"
        set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT COUNT(*) AS c FROM %IPM_General.HistoryTemp WHERE ID = ?", tid)
        if rs.%SQLCODE < 0 {
            set n = "SQLERR"_rs.%SQLCODE
        } else {
            do rs.%Next()
            set n = rs.%Get("c")
        }
    } catch ex {
        set n = "<error> "_ex.DisplayString()
    }
    set $namespace = orig
    do ##class(B2Native.Util).Obs(case, "userverify.sees.user.temp.row", n)
    kill log
}

/// Run one probe by name, e.g. do ##class(B2Native.Premise).One("P18")
ClassMethod One(p As %String)
{
    set case = "N-"_p
    do ##class(B2Native.Util).Begin(case)
    try {
        do $classmethod($classname(), p, case)
    } catch ex {
        do ##class(B2Native.Util).Obs(case, "probe.exception", ex.DisplayString())
    }
    while $tlevel > 0 { trollback 1 }
    do ##class(B2Native.Util).End(case)
}

/// SIGKILL of a JOBbed process that holds an object and an open transaction:
/// no ObjectScript runs in the victim, so %OnClose cannot run; the instance cleans up the transaction.
ClassMethod P18(case)
{
    kill ^B2P("p18"), ^IRIS.Temp.B2Signal("p18"), ^IRIS.Temp.B2Close("p18")
    job ##class(B2Native.Premise).JobTxObj("p18")
    set pid = $zchild
    do ##class(B2Native.Util).Obs(case, "signal", ##class(B2Native.Util).WaitSignal("p18", 60))
    do ##class(B2Native.Util).Obs(case, "dirty.read.data", $data(^B2P("p18")))
    set t0 = $zhorolog
    set rc = $zf(-100, "", "/usr/bin/kill", "-9", pid)
    do ##class(B2Native.Util).Obs(case, "kill.rc", rc)
    // IRIS removes a SIGKILLed process slot only after a delay; wait long enough for its cleanup.
    do ##class(B2Native.Util).Obs(case, "gone", ##class(B2Native.Util).WaitGone(pid, 400))
    do ##class(B2Native.Util).Obs(case, "cleanup.seconds", $justify($zhorolog - t0, 0, 0))
    hang 2
    do ##class(B2Native.Util).Obs(case, "closed", $data(^IRIS.Temp.B2Close("p18")))
    do ##class(B2Native.Util).Obs(case, "after.data", $data(^B2P("p18")))
    // The instance still accepts transactional work afterwards.
    tstart
    set ^B2P("p18health") = 1
    tcommit
    do ##class(B2Native.Util).Obs(case, "health.tx.ok", $data(^B2P("p18health")))
    kill ^B2P("p18health")
}

ClassMethod JobTxObj(tag)
{
    set o = ##class(B2Native.CloseProbe).%New(tag)
    tstart
    set ^B2P(tag) = 1
    set ^IRIS.Temp.B2Signal(tag, "pid") = $job
    for i=1:1:600 {
        quit:$data(^IRIS.Temp.B2Signal(tag, "release"))
        hang 0.2
    }
    tcommit
    kill o
}

ClassMethod NewHistory(package As %String) As %IPM.General.History
{
    set h = ##class(%IPM.General.History).%New()
    set h.Action = "load"
    set h.Package = package
    set h.CommandString = "b2 probe"
    set h.Version = ##class(%IPM.General.SemanticVersion).FromString("1.0.0")
    quit h
}

ClassMethod ObsHistory(
	case,
	prefix,
	id)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT * FROM %IPM_General.History WHERE ID = ?", id)
    if rs.%Next() {
        do ##class(B2Native.Util).ObsRow(case, prefix, rs)
    } else {
        do ##class(B2Native.Util).Obs(case, prefix_"exists", 0)
    }
}

}
