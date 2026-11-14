/// Further native checks for modules 01 and 02.
Class Uplift.Probe.Extra
{

/// A plain load (no -dev, no -nodev) over a module installed in developer mode.
ClassMethod DevThenPlainLoad()
{
    do ##class(Uplift.Probe.Scenario).Reset()
    set dir = ##class(Uplift.Probe.Scenario).#SIMPLEST
    set sc1 = ##class(%IPM.Main).Shell("load -dev "_dir_"1.0.0/")
    set sc2 = ##class(%IPM.Main).Shell("load "_dir_"1.0.0/")
    set kv("devAfterPlainLoad") = ##class(%IPM.Storage.Module).NameOpen("simplest-module").DeveloperMode
    set sc3 = ##class(%IPM.Main).Shell("load -nodev "_dir_"1.0.0/")
    set kv("devAfterNodevLoad") = ##class(%IPM.Storage.Module).NameOpen("simplest-module").DeveloperMode
    set kv("statuses") = $system.Status.IsOK(sc1)_","_$system.Status.IsOK(sc2)_","_$system.Status.IsOK(sc3)
    do ##class(Uplift.Probe.Env).Emit("ipm-dev-then-plain-load", .kv)
}

/// IPM's own trigger: a finalized history row cannot be edited, through SQL or objects; it can be deleted.
ClassMethod FinalizedHistoryRow()
{
    do ##class(Uplift.Probe.Scenario).Reset()
    do ##class(%IPM.Main).Shell("load "_##class(Uplift.Probe.Scenario).#SIMPLEST_"1.0.0/")
    &sql(SELECT MAX(ID) INTO :id FROM %IPM_General.History)
    set kv("finalized") = ##class(%IPM.General.History).%OpenId(id).Finalized
    &sql(UPDATE %IPM_General.History SET Committed = 0 WHERE ID = :id)
    set kv("sqlUpdateSQLCODE") = SQLCODE
    set obj = ##class(%IPM.General.History).%OpenId(id)
    set obj.CommandString = "tampered"
    set sc = obj.%Save()
    set kv("objectSaveOK") = $system.Status.IsOK(sc)
    kill obj
    &sql(SELECT Committed, CommandString INTO :c, :cs FROM %IPM_General.History WHERE ID = :id)
    set kv("committedAfter") = c, kv("commandAfter") = $translate(cs, ";|=", ",/:")
    &sql(DELETE FROM %IPM_General.History WHERE ID = :id)
    set kv("sqlDeleteSQLCODE") = SQLCODE
    do ##class(Uplift.Probe.Env).Emit("ipm-finalized-history-row", .kv)
}

/// Auto-purge: HistoryRetain days; the first shell command of a day deletes older records.
ClassMethod AutoPurge()
{
    do ##class(Uplift.Probe.Scenario).Reset()
    do ##class(%IPM.Main).Shell("load "_##class(Uplift.Probe.Scenario).#SIMPLEST_"1.0.0/")
    &sql(SELECT MAX(ID) INTO :recent FROM %IPM_General.History)
    // An old record (TimeStart in 2000) inserted directly, not finalized.
    set old = ##class(%IPM.General.History).%New()
    set old.Action = "load", old.Package = "old-module", old.CommandString = "load /old/"
    set old.TimeStart = "2000-01-01 00:00:00"
    do old.SetVersion("1.0.0")
    set oldId = old.%Id()
    kill old
    set kv("retainBefore") = ##class(%IPM.Repo.UniversalSettings).GetHistoryRetain()
    do ##class(%IPM.Main).Shell("config set HistoryRetain 30")
    set index = ##class(%IPM.Repo.UniversalSettings).#HistoryRetain
    new $namespace
    set ns = $namespace, $namespace = "%SYS"
    kill ^IPM.settings(index, "lastPurge")
    set $namespace = ns
    set sc = ##class(%IPM.General.History).AutoPurgeOldHistory()
    set kv("purgeStatus") = $select($system.Status.IsOK(sc): "ok", 1: $translate($system.Status.GetErrorText(sc), ";|=", ",/:"))
    set kv("oldRecordExists") = ##class(%IPM.General.History).%ExistsId(oldId)
    set kv("recentRecordExists") = ##class(%IPM.General.History).%ExistsId(recent)
    // The filter value IPM builds, for the record
    set kv("filterValue") = "<='"_$system.SQL.Functions.DATEADD("d", -30, +$horolog)_"'"
    do ##class(%IPM.Main).Shell("config set HistoryRetain "_kv("retainBefore"))
    do ##class(Uplift.Probe.Env).Emit("ipm-history-autopurge", .kv)
}

ClassMethod NoJournalActivateFails()
{
    do ##class(Uplift.Probe.Scenario).Reset()
    set sc = ##class(%IPM.Main).Shell("load "_##class(Uplift.Probe.Scenario).#FIX_"up-res-fail/ -DNoJournal=1")
    do ##class(Uplift.Probe.Scenario).Observe("ipm-nojournal-activate-fails", sc)
}

ClassMethod RunAll()
{
    for m = "DevThenPlainLoad", "FinalizedHistoryRow", "AutoPurge", "NoJournalActivateFails" {
        try {
            do $classmethod("Uplift.Probe.Extra", m)
        } catch e {
            write "OBS|error-"_m_"|error="_$translate(e.DisplayString(), ";|=", ",/:")_";", !
        }
        if $tlevel { trollback }
    }
}

}
