/// n2 native gap checks P1..P6, P8, P9 (see validation/native-gaps-n2.md).
Class Uplift.N2.Gaps Extends Uplift.N2.Base
{

// ===== P1: IRIS restart while a load hangs in Activate =====

/// Start a load of n2-wait in a job and return once it waits inside Activate. The job stays (waits up to 300 s).
ClassMethod P1Start(kind As %String, mode As %String)
{
    do ..Reset(0)
    job ##class(Uplift.N2.Gaps).JobShell("load "_..#FIX_"p1/n2-wait/"_..Flags(mode))
    set child = $zchild
    set seen = ..WaitSignal("wait", "n2-wait", 120)
    set sig = $get(^IRIS.Temp.N2Signal("wait", "n2-wait"))
    set ^N2P1 = $listbuild(kind, mode, child, ..MaxHistoryId())
    set %n2kv("jobPid") = child, %n2kv("waiting") = seen, %n2kv("tlevelInActivate") = $listget(sig, 2)
    set %n2kv("historyTempData") = $data(^IRIS.Temp.HistoryTempD)
    do ..Observe("nc-gap-P1-"_kind_"-"_mode_"-during")
}

/// After the restart: read history, IRISTEMP, module record and classes.
ClassMethod P1After()
{
    set info = $get(^N2P1), kind = $listget(info, 1), mode = $listget(info, 2), id = $listget(info, 4)
    set %n2kv("oldJobAlive") = ''$data(^$JOB(+$listget(info, 3)))
    set %n2kv("iristempSignalLeft") = $data(^IRIS.Temp.N2Signal)
    set %n2kv("historyTempData") = $data(^IRIS.Temp.HistoryTempD)
    set %n2kv("rowId") = id
    if id {
        set raw = $get(^IPM.General.HistoryD(id))
        set %n2kv("rawCommitted") = $listget(raw, 12), %n2kv("rawFinalized") = $listget(raw, 15)
        set %n2kv("rawPackage") = $select($listget(raw, 3)="":"<empty>", 1:$listget(raw, 3))
        // can the unfinalized row still be edited through SQL (trigger allows non-finalized rows)?
        &sql(UPDATE %IPM_General.History SET CommandString = CommandString WHERE ID = :id)
        set %n2kv("sqlUpdateSQLCODE") = SQLCODE
    }
    do ..Observe("nc-gap-P1-"_kind_"-"_mode_"-after")
    kill ^N2P1
}

// ===== P2: history delete of a running record (DEC-HIST-6) =====

ClassMethod P2(variant As %String)
{
    do ..Reset(0)
    set mode = $select(variant [ "dev": "dev", 1: "txn")
    job ##class(Uplift.N2.Gaps).JobShell("load "_..#FIX_"p1/n2-wait/"_..Flags(mode))
    set child = $zchild
    set seen = ..WaitSignal("wait", "n2-wait", 120)
    set id = ..MaxHistoryId()
    set %n2kv("waiting") = seen, %n2kv("rowId") = id
    do ..Observe("nc-gap-P2-"_variant_"-during")
    if variant [ "by-package" {
        set sc1 = ..Sh("history delete -DPackage=n2-wait")
        set %n2kv("deleteByPackage") = $select($system.Status.IsOK(sc1):"ok", 1:"err")
        set %n2kv("rowExistsAfterPackageDelete") = ##class(%IPM.General.History).%ExistsId(id)
        set sc2 = ..Sh("history delete -DID="_id)
        set %n2kv("deleteById") = $select($system.Status.IsOK(sc2):"ok", 1:"err")
    } else {
        set sc2 = ..Sh("history delete -confirm")
        set %n2kv("deleteConfirm") = $select($system.Status.IsOK(sc2):"ok", 1:"err")
    }
    set %n2kv("rowExistsAfterDelete") = ##class(%IPM.General.History).%ExistsId(id)
    set %n2kv("historyTempData") = $data(^IRIS.Temp.HistoryTempD)
    set ^IRIS.Temp.N2Release("n2-wait") = 1
    set %n2kv("jobGone") = ..WaitGone(child, 120)
    hang 1
    set %n2kv("jobResult") = $get(^IRIS.Temp.N2Signal("jobdone", child), "<none>")
    set %n2kv("rowExistsAfterRelease") = ##class(%IPM.General.History).%ExistsId(id)
    do ..Observe("nc-gap-P2-"_variant_"-after-release")
}

// ===== P3: phase record for a phase-level Invoke that fails by returned status =====

ClassMethod P3()
{
    for fx = "n2-ph-after", "n2-ph-before", "n2-ph-compile" {
        for mode = "txn", "dev", "notxn" {
            continue:(fx '= "n2-ph-after")&&(mode '= "txn")
            do ..Reset(0)
            set sc = ..Sh("load "_..#FIX_"p3/"_fx_"/"_..Flags(mode))
            set id = ..MaxHistoryId()
            do ..Sh("history details "_id_" -phases")
            do ..Observe("nc-gap-P3-"_fx_"-"_mode, sc)
        }
    }
}

// ===== P4: archive loads =====

ClassMethod P4()
{
    for mode = "txn", "dev" {
        do ..Reset(0)
        set arc = "/tmp/n2arc/"
        do ..Os("rm -rf "_arc_" && mkdir -p "_arc_"good "_arc_"bad && tar czf "_arc_"good/n2-arc-1.0.0.tgz -C "_..#FIX_"p4/good n2-arc && tar czf "_arc_"bad/n2-arc-1.0.0.tgz -C "_..#FIX_"p4/bad n2-arc && tar tzf "_arc_"bad/n2-arc-1.0.0.tgz")
        set root = ##class(%IPM.Repo.UniversalSettings).GetModuleRoot()
        set final = ##class(%File).NormalizeDirectory(root_"n2-arc/1.0.0")
        do ##class(%File).RemoveDirectoryTree(root_"n2-arc/")
        set sc = ..Sh("load "_arc_"good/n2-arc-1.0.0.tgz")
        set %n2kv("moduleRoot") = root
        set %n2kv("payloadAfterGood") = ..FileText(final_"payload.txt")
        set %n2kv("recordRoot") = $select(##class(%IPM.Storage.Module).NameExists("n2-arc"): ##class(%IPM.Storage.Module).NameOpen("n2-arc").Root, 1: "<none>")
        do ..Observe("nc-gap-P4-"_mode_"-good-archive", sc)
        // a local file in the module root folder: is the folder replaced or merged?
        set f = ##class(%Stream.FileCharacter).%New(), f.Filename = final_"local-edit.txt"
        do f.WriteLine("local"), f.%Save()
        kill f
        set %n2kv("localEditBefore") = ##class(%File).Exists(final_"local-edit.txt")
        set sc = ..Sh("load "_arc_"bad/n2-arc-1.0.0.tgz"_..Flags(mode))
        set %n2kv("payloadAfterBad") = ..FileText(final_"payload.txt")
        set %n2kv("localEditAfterBad") = ##class(%File).Exists(final_"local-edit.txt")
        set %n2kv("markerSourceInFolder") = $select(..FileText(final_"src/N2Arc/Marker.cls") [ "1.0.0-bad": "bad", ..FileText(final_"src/N2Arc/Marker.cls") [ "1.0.0": "good", 1: "?")
        set %n2kv("recordRoot") = $select(##class(%IPM.Storage.Module).NameExists("n2-arc"): ##class(%IPM.Storage.Module).NameOpen("n2-arc").Root, 1: "<none>")
        do ..Os("ls "_##class(%SYS.System).TempDirectory())
        do ..Observe("nc-gap-P4-"_mode_"-bad-archive-same-version", sc)
    }
}

// ===== P5: URL (git) loads; the repositories are served at http://localhost:8765/ =====

ClassMethod P5()
{
    set base = "http://localhost:8765/"
    set tmp = ##class(%File).NormalizeDirectory(##class(%File).GetDirectory($zutil(86))_"mgr/Temp/")
    for case = "ok-txn", "ok-dev", "fail-txn", "fail-notxn", "missing" {
        do ..Reset(0)
        set before = ..TempDirs(tmp)
        set url = base_$case(case, "ok-txn":"n2-git.git", "ok-dev":"n2-git.git", "fail-txn":"n2-gitfail.git", "fail-notxn":"n2-gitfail.git", :"n2-nope.git")
        set flags = $case(case, "ok-dev":" -dev", "fail-notxn":" -DNoTransaction=1", :"")
        set sc = ..Sh("load "_url_flags)
        set after = ..TempDirs(tmp)
        set %n2kv("newTempDirs") = after - before
        set mname = $case(case, "ok-txn":"n2-git", "ok-dev":"n2-git", "missing":"n2-nope", :"n2-gitfail")
        if ##class(%IPM.Storage.Module).NameExists(mname) {
            set r = ##class(%IPM.Storage.Module).NameOpen(mname).Root
            set %n2kv("recordRoot") = r, %n2kv("recordRootExists") = ##class(%File).DirectoryExists(r)
        }
        &sql(SELECT MAX(ID) INTO :id FROM %IPM_General.History)
        &sql(SELECT CommandString, SourceName INTO :cs, :sn FROM %IPM_General.History WHERE ID = :id)
        set %n2kv("command") = $get(cs), %n2kv("sourceName") = $select($get(sn)="":"<empty>", 1:sn)
        do ..Observe("nc-gap-P5-url-"_case, sc)
    }
}

ClassMethod TempDirs(dir As %String) As %Integer
{
    set n = 0, rs = ##class(%File).FileSetFunc(dir)
    while rs.%Next() { set:rs.Type="D" n = n + 1 }
    quit n
}

// ===== P6: update step halts between its effect and its record save =====

ClassMethod P6()
{
    for mode = "txn", "dev", "notxn" {
        do ..Reset(0)
        set sc = ..Sh("load "_..#FIX_"p6/n2-upd-1.0.0/"_..Flags(mode))
        set ^N2HaltStep = "A"
        set cmd = "update"_..Flags(mode)_" n2-upd -path "_..#FIX_"p6/n2-upd-2.0.0/"
        job ##class(Uplift.N2.Gaps).JobShell(cmd)
        set child = $zchild
        set %n2kv("load100") = $select($system.Status.IsOK(sc):"ok", 1:"err")
        set %n2kv("jobGone") = ..WaitGone(child, 180)
        set %n2kv("jobDone") = $get(^IRIS.Temp.N2Signal("jobdone", child), "<none: halted>")
        set %n2kv("stepARuns") = ..CountGlobal("^IRIS.Temp.N2Signal(""stepA"")")
        do ..UpdState()
        do ..Observe("nc-gap-P6-"_mode_"-halt-in-step")
        kill ^N2HaltStep
        set sc = ..Sh(cmd)
        set %n2kv("stepARuns") = ..CountGlobal("^IRIS.Temp.N2Signal(""stepA"")")
        do ..UpdState()
        do ..Observe("nc-gap-P6-"_mode_"-retry", sc)
    }
}

ClassMethod UpdState()
{
    set %n2kv("ordinaryX") = $get(^N2UpdX, "undef")
    set %n2kv("incrementY") = $get(^N2UpdY, "undef")
    set %n2kv("fileLines") = ..FileLines(..#STEPFILE)
    set %n2kv("B.runs") = $get(^N2Upd("V2.B"), 0)
    set %n2kv("step:V1.S1") = ..Step("N2Upd.Upd.V1", "S1")
    set %n2kv("step:V2.A") = ..Step("N2Upd.Upd.V2", "A")
    set %n2kv("step:V2.B") = ..Step("N2Upd.Upd.V2", "B")
}

// ===== P8 (patched IPM only) =====

/// (a) crash between the outer TCOMMIT and SetCommitted (probe patch reads ^UpliftCrashAt).
ClassMethod P8a()
{
    for case = "control", "after-commit" {
        do ..Reset(0)
        set:case="after-commit" ^UpliftCrashAt = "after-commit"
        job ##class(Uplift.N2.Gaps).JobShell("load "_..#FIX_"p9/top/n2-solo/")
        set child = $zchild
        set %n2kv("jobGone") = ..WaitGone(child, 180)
        set %n2kv("jobDone") = $get(^IRIS.Temp.N2Signal("jobdone", child), "<none: halted>")
        kill ^UpliftCrashAt
        do ..Observe("nc-gap-P8-commit-window-"_case)
    }
}

/// (b) F-RES-1: deployed item export fails (probe patch reads ^UpliftForceDeployErr); does the package ship source?
ClassMethod P8b()
{
    for case = "control", "forced-error" {
        do ..Reset(0)
        set sc = ..Sh("load "_..#FIX_"p8/n2-dpl/")
        set:case="forced-error" ^UpliftForceDeployErr = 1
        set dir = "/tmp/n2pkg/"_case_"/"
        do ..Os("rm -rf /tmp/n2pkg/"_case_"*")
        set sc = ..Sh("package n2-dpl -v -path "_dir)
        kill ^UpliftForceDeployErr
        set tgz = "/tmp/n2pkg/"_case_".tgz"
        set %n2kv("tgzExists") = ##class(%File).Exists(tgz)
        do ..Os("tar tzf "_tgz, .list)
        set %n2kv("tgzEntries") = list
        set %n2kv("shipsMarkerSourceFile") = (list [ "Marker.cls")
        set %n2kv("shipsDeployedProject") = ($zconvert(list, "L") [ "deployed.xml")
        do ..Os("rm -rf /tmp/n2pkg/x-"_case_" && mkdir -p /tmp/n2pkg/x-"_case_" && tar xzf "_tgz_" -C /tmp/n2pkg/x-"_case_" && grep -rl 'ClassMethod V' /tmp/n2pkg/x-"_case_" ; grep -c 'N2Dpl.Marker' /tmp/n2pkg/x-"_case_"/studio-project/Deployed.xml", .found)
        set %n2kv("filesWithMethodSource") = $select(found="":"<none>", 1:found)
        do ..Observe("nc-gap-P8-deployed-export-"_case, sc)
    }
}

// ===== P9: model bounds =====

ClassMethod P9()
{
    for mode = "txn", "dev", "notxn" {
        for case = "chain-ok", "chain-c-fails", "upgrade-ok", "upgrade-parent-fails", "upgrade-dep-fails", "two-deps-parent-fails" {
            do ..Reset(1)
            set top = $select(case [ "chain": "n2-ca", case [ "upgrade": "n2-pa", 1: "n2-tp")
            if case [ "upgrade" {
                set sc0 = ..Sh("load "_..#FIX_"p9/repo/n2-pd-1.0.0/")
                set %n2kv("preinstall") = $select($system.Status.IsOK(sc0):"ok", 1:"err")
                kill ^IRIS.Temp.N2Signal
            }
            set:case="chain-c-fails" ^N2Fail("n2-cc") = 1
            set:case="upgrade-parent-fails" ^N2Fail("n2-pa") = 1
            set:case="upgrade-dep-fails" ^N2Fail("n2-pd") = 1
            set:case="two-deps-parent-fails" ^N2Fail("n2-tp") = 1
            set sc = ..Sh("load "_..#FIX_"p9/top/"_top_"/"_..Flags(mode))
            do ..Observe("nc-gap-P9-"_case_"-"_mode, sc)
        }
    }
}

ClassMethod Run(list As %String)
{
    do ..Setup()
    for i = 1:1:$length(list, ",") {
        set m = $piece(list, ",", i)
        try {
            if m [ ":" { do $classmethod($classname(), $piece(m, ":"), $piece(m, ":", 2)) } else { do $classmethod($classname(), m) }
        } catch e {
            write !, "OBS|error-n2-"_$translate(m, ":", "-")_"|error="_..Clean(e.DisplayString())_";", !
        }
        while $tlevel { trollback }
    }
}

}
