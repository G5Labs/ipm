/// IRIS environment-contract probes for the IPM history and load models.
/// Each probe prints one line: OBS|<name>|<key>=<value>;...
Class Uplift.Probe.Env
{

ClassMethod Emit(name As %String, ByRef kv)
{
    // ^UpliftObsSuffix (e.g. "@2024.1") keeps observations from another IRIS version apart.
    set out = "OBS|"_name_$get(^UpliftObsSuffix)_"|", k = ""
    for {
        set k = $order(kv(k), 1, v)
        quit:k=""
        set out = out_k_"="_v_";"
    }
    write out, !
}

ClassMethod Reset()
{
    kill ^UpProbe, ^IRIS.Temp.UpProbe, ^IRIS.Temp.UpProbeTempD, ^IRIS.Temp.UpProbeTempI
    do ##class(Uplift.Probe.Rec).%KillExtent()
}

/// ENV-NEST-1: a write committed at an inner level is undone by an outer TROLLBACK 1.
ClassMethod NestedCommitThenOuterRollback()
{
    do ..Reset()
    tstart
    set ^UpProbe("outer") = 1
    tstart
    set ^UpProbe("inner") = 1
    tcommit
    set kv("tlevelAfterInnerCommit") = $tlevel
    trollback 1
    set kv("tlevelAfterRollback") = $tlevel
    set kv("outerKept") = $data(^UpProbe("outer"))
    set kv("innerKept") = $data(^UpProbe("inner"))
    do ..Emit("env-nested-commit-outer-rollback", .kv)
}

/// ENV-NEST-2: TROLLBACK 1 at level 2 undoes only the inner level.
ClassMethod RollbackOneLevel()
{
    do ..Reset()
    tstart
    set ^UpProbe("outer") = 1
    tstart
    set ^UpProbe("inner") = 1
    trollback 1
    set kv("tlevelAfterFirst") = $tlevel
    set kv("outerKeptAfterFirst") = $data(^UpProbe("outer"))
    set kv("innerKeptAfterFirst") = $data(^UpProbe("inner"))
    trollback 1
    set kv("tlevelAfterSecond") = $tlevel
    set kv("outerKeptAfterSecond") = $data(^UpProbe("outer"))
    do ..Emit("env-rollback-one-level", .kv)
}

/// ENV-NEST-3: plain TROLLBACK (no argument) undoes every level.
ClassMethod RollbackAll()
{
    do ..Reset()
    tstart
    set ^UpProbe("outer") = 1
    tstart
    set ^UpProbe("inner") = 1
    trollback
    set kv("tlevel") = $tlevel
    set kv("outerKept") = $data(^UpProbe("outer"))
    set kv("innerKept") = $data(^UpProbe("inner"))
    do ..Emit("env-rollback-all", .kv)
}

/// ENV-TEMP-1: ^IRIS.Temp.* writes survive TROLLBACK; record where the global maps.
ClassMethod TempSurvivesRollback()
{
    do ..Reset()
    tstart
    set ^IRIS.Temp.UpProbe("x") = 1
    set ^UpProbe("x") = 1
    trollback
    set kv("tempKept") = $data(^IRIS.Temp.UpProbe("x"))
    set kv("ordinaryKept") = $data(^UpProbe("x"))
    set kv("tempDest") = ##class(%SYS.Namespace).GetGlobalDest($namespace, "IRIS.Temp.UpProbe")
    set kv("ordinaryDest") = ##class(%SYS.Namespace).GetGlobalDest($namespace, "UpProbe")
    set kv("historyTwinDest") = ##class(%SYS.Namespace).GetGlobalDest($namespace, "IPM.General.HistoryD")
    set kv("historyTempDest") = ##class(%SYS.Namespace).GetGlobalDest($namespace, "IRIS.Temp.HistoryTempD")
    do ..Emit("env-iristemp-survives-rollback", .kv)
}

/// Helper: creates a TempRec in a local variable and returns; %OnClose should fire on return.
ClassMethod MakeAndDrop(name As %String, setCommitted As %Boolean = 0)
{
    set log = ##class(Uplift.Probe.TempRec).Init(name)
    if setCommitted {
        $$$ThrowOnError(log.%Reload())
        set log.Committed = 1
        $$$ThrowOnError(log.%Save())
    }
    set ^UpProbe("beforeReturn", name) = $get(^UpProbe("onclose"), 0)
}

/// ENV-CLOSE-1: %OnClose runs when the last reference goes out of scope at method return,
/// at the caller's $tlevel; its writes then belong to the caller's open transaction.
ClassMethod OnCloseInsideCallerTransaction()
{
    do ..Reset()
    tstart
    do ..MakeAndDrop("dep", 1)
    set kv("closedBeforeCallerContinues") = $get(^UpProbe("onclose"), 0)
    set kv("tlevelSeenInOnClose") = $listget($get(^UpProbe("onclose", 1)), 2)
    set kv("twinRowsBeforeRollback") = ..CountRec()
    trollback
    set kv("twinRowsAfterRollback") = ..CountRec()
    set kv("tempRowsAfterRollback") = ..CountTemp()
    do ..Emit("env-onclose-inside-caller-transaction", .kv)
}

/// ENV-CLOSE-2: the same object outside any transaction keeps its twin.
ClassMethod OnCloseOutsideTransaction()
{
    do ..Reset()
    do ..MakeAndDrop("top", 1)
    set kv("closed") = $get(^UpProbe("onclose"), 0)
    set kv("twinRows") = ..CountRec()
    set kv("twinCommitted") = ..FirstRecProp("Committed")
    set kv("twinFinalized") = ..FirstRecProp("Finalized")
    set kv("tempRows") = ..CountTemp()
    do ..Emit("env-onclose-outside-transaction", .kv)
}

/// ENV-RESAVE-1: an object whose row was inserted then rolled back is saved again: is the row recreated?
ClassMethod ResaveAfterRollback()
{
    do ..Reset()
    tstart
    set rec = ##class(Uplift.Probe.Rec).%New()
    set rec.Name = "ghost"
    $$$ThrowOnError(rec.%Save())
    set id = rec.%Id()
    trollback
    set kv("rowsAfterRollback") = ..CountRec()
    set kv("idStillOnOref") = (rec.%Id() = id)
    set rec.Name = "ghost2"
    set sc = rec.%Save()
    set kv("resaveOK") = $system.Status.IsOK(sc)
    set:'kv("resaveOK") kv("resaveErr") = $translate($system.Status.GetErrorText(sc), ";|", ",/")
    set kv("rowsAfterResave") = ..CountRec()
    set kv("existsById") = ##class(Uplift.Probe.Rec).%ExistsId(id)
    do ..Emit("env-resave-after-rollback", .kv)
}

/// ENV-TRIG-1: object save of a finalized row: fails when a value changes; what if no value changes?
ClassMethod FinalizedTrigger()
{
    do ..Reset()
    set rec = ##class(Uplift.Probe.Rec).%New()
    set rec.Name = "r"
    set rec.Finalized = 1
    $$$ThrowOnError(rec.%Save())
    set id = rec.%Id()
    kill rec
    set rec = ##class(Uplift.Probe.Rec).%OpenId(id)
    set rec.Name = "r"
    set kv("modifiedAfterSameValue") = rec.%IsModified()
    set sc = rec.%Save()
    set kv("saveSameValueOK") = $system.Status.IsOK(sc)
    set rec.Name = "changed"
    set sc = rec.%Save()
    set kv("saveChangedOK") = $system.Status.IsOK(sc)
    set:'kv("saveChangedOK") kv("saveChangedErr") = $translate($extract($system.Status.GetErrorText(sc), 1, 120), ";|", ",/")
    set kv("nameAfter") = ..FirstRecProp("Name")
    &sql(DELETE FROM Uplift_Probe.Rec WHERE ID = :id)
    set kv("sqlDeleteSQLCODE") = SQLCODE
    do ..Emit("env-finalized-trigger", .kv)
}

/// ENV-HALT-1 (run in a child job): open transaction + local TempRec, then HALT.
ClassMethod ChildHaltWithOpenTransaction()
{
    set log = ##class(Uplift.Probe.TempRec).Init("child")
    $$$ThrowOnError(log.%Reload())
    set log.Committed = 1
    $$$ThrowOnError(log.%Save())
    tstart
    set ^UpProbe("childTxnWrite") = 1
    set ^UpProbe("childReached") = 1
    halt
}

ClassMethod HaltProbe()
{
    do ..Reset()
    job ..ChildHaltWithOpenTransaction()
    set child = $zchild
    for i=1:1:50 { quit:'$data(^$JOB(child))  hang 0.2 }
    hang 0.5
    set kv("childGone") = '$data(^$JOB(child))
    set kv("childTxnWriteKept") = $data(^UpProbe("childTxnWrite"))
    set kv("onCloseRanInChild") = $get(^UpProbe("onclose"), 0)
    set kv("twinRows") = ..CountRec()
    set kv("twinCommitted") = ..FirstRecProp("Committed")
    set kv("twinFinalized") = ..FirstRecProp("Finalized")
    set kv("tempRowsLeft") = ..CountTemp()
    set kv("tempCommitted") = $listget($get(^IRIS.Temp.UpProbeTempD(1)), 3)
    set kv("onCloseTlevel") = $listget($get(^UpProbe("onclose", 1)), 2)
    set kv("onCloseSaveStatus") = $translate($extract($get(^UpProbe("onclose", 1, "sc"), "<unset>"), 1, 150), ";|", ",/")
    do ..Emit("env-halt-open-transaction", .kv)
}

ClassMethod CountRec() As %Integer
{
    &sql(SELECT COUNT(*) INTO :n FROM Uplift_Probe.Rec)
    quit +$get(n)
}

ClassMethod CountTemp() As %Integer
{
    set n = 0, k = ""
    for { set k = $order(^IRIS.Temp.UpProbeTempD(k)) quit:k=""  set n = n + 1 }
    quit n
}

ClassMethod FirstRecProp(prop As %String) As %String
{
    set id = ""
    &sql(SELECT MIN(ID) INTO :id FROM Uplift_Probe.Rec)
    if id = "" { quit "<none>" }
    set rec = ##class(Uplift.Probe.Rec).%OpenId(id)
    quit $property(rec, prop)
}

ClassMethod RunAll()
{
    for m = "NestedCommitThenOuterRollback", "RollbackOneLevel", "RollbackAll", "TempSurvivesRollback", "OnCloseInsideCallerTransaction", "OnCloseOutsideTransaction", "ResaveAfterRollback", "FinalizedTrigger", "HaltProbe" {
        try {
            do $classmethod("Uplift.Probe.Env", m)
        } catch e {
            write "OBS|error-"_m_"|error="_$translate(e.DisplayString(), ";|", ",/")_";", !
        }
        if $tlevel { trollback }
    }
    do ..Reset()
}

}
