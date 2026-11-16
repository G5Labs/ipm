/// Iteration 4 native observations. Each line OBS|case|field|value is parsed by run_native.py.
Class Iter4.Probe
{

ClassMethod Obs(case As %String, field As %String, value As %String)
{
    write !,"OBS|",case,"|",field,"|",$translate(value,$char(13,10),"  "),!
}

/// Run one IPM shell command in this process and record its status.
ClassMethod Cmd(case As %String, command As %String)
{
    set sc = ##class(%IPM.Main).Shell(command)
    do ..Obs(case, "ok", $system.Status.IsOK(sc))
    do ..Obs(case, "error", $select($system.Status.IsOK(sc):"", 1:$system.Status.GetErrorText(sc)))
    do ..Obs(case, "accumulator_defined", $data(^||%UnitTest.Manager.AllResultsCount)#2)
    do ..Obs(case, "accumulator_count", $get(^||%UnitTest.Manager.AllResultsCount))
}

/// Suite name of the first suite stored for a %UnitTest result index.
ClassMethod SuiteOf(index As %Integer) As %String
{
    quit $order(^UnitTest.Result(index, ""))
}

/// First case name stored for a %UnitTest result index.
ClassMethod CaseOf(index As %Integer) As %String
{
    quit $order(^UnitTest.Result(index, ..SuiteOf(index), ""))
}

ClassMethod TestOutcome()
{
    do ..Cmd("T0-load", "load /tmp/g5-iter4/tmod")
    // Default: failures are fatal and every in-scope test resource runs.
    do ..Cmd("T1-default", "test g5i4-tests -only")
    // Explicitly non-fatal: same failing tests, accumulator is not consumed.
    kill ^||%UnitTest.Manager.AllResults, ^||%UnitTest.Manager.AllResultsCount
    do ..Cmd("T2-not-fatal", "test g5i4-tests -only -DUnitTest.FailuresAreFatal=0")
    set count = $get(^||%UnitTest.Manager.AllResultsCount)
    for i=1:1:count {
        do ..Obs("T2-not-fatal", "slot"_i_"_suite", ..SuiteOf($get(^||%UnitTest.Manager.AllResults(i))))
    }
    kill ^||%UnitTest.Manager.AllResults, ^||%UnitTest.Manager.AllResultsCount
    // Selector narrows to the passing package only.
    do ..Cmd("T3-pass-only", "test g5i4-tests -only -DUnitTest.Suite=G5I4.Pass")
    // Selector names only the package that has no classes.
    do ..Cmd("T4-empty-only", "test g5i4-tests -only -DUnitTest.Suite=G5I4.Empty")
    // Unsupported report file extension after a passing run.
    do ..Cmd("T5-bad-extension", "test g5i4-tests -only -DUnitTest.Suite=G5I4.Pass -output-file /tmp/g5-iter4/out/report.csv")
    do ..Obs("T5-bad-extension", "file_exists", ##class(%File).Exists("/tmp/g5-iter4/out/report.csv"))
    // Supported report file with failing tests: file written and status still failing.
    do ..Cmd("T6-json-file", "test g5i4-tests -only -DUnitTest.Suite=G5I4.Fail -output-file /tmp/g5-iter4/out/report.json")
    do ..Obs("T6-json-file", "file_exists", ##class(%File).Exists("/tmp/g5-iter4/out/report.json"))
}

/// Direct reader observations of the accumulated-result status and failure display.
ClassMethod Accumulator()
{
    kill ^||%UnitTest.Manager.AllResults, ^||%UnitTest.Manager.AllResultsCount
    do ##class(%IPM.Main).Shell("test g5i4-tests -only -DUnitTest.FailuresAreFatal=0")
    set failIndex = "", passIndex = ""
    for i=1:1:$get(^||%UnitTest.Manager.AllResultsCount) {
        set idx = ^||%UnitTest.Manager.AllResults(i)
        if ..CaseOf(idx) [ "G5I4.Fail" { set failIndex = idx } else { set passIndex = idx }
    }
    do ..Obs("A0", "have_indexes", (failIndex'="")&&(passIndex'=""))
    // A1: nested reader starting after the parent's slot sees only the failing child slot.
    kill ^||%UnitTest.Manager.AllResults
    set ^||%UnitTest.Manager.AllResults(1) = passIndex, ^||%UnitTest.Manager.AllResults(2) = failIndex, ^||%UnitTest.Manager.AllResultsCount = 2
    set sc = ##class(%IPM.Test.Manager).GetAllTestsStatus(.n, 1)
    do ..Obs("A1-nested-fail", "ok", $system.Status.IsOK(sc))
    do ..Obs("A1-nested-fail", "count", n)
    do ..Obs("A1-nested-fail", "error", $select($system.Status.IsOK(sc):"", 1:$system.Status.GetErrorText(sc)))
    do ..Obs("A1-nested-fail", "after_count", $get(^||%UnitTest.Manager.AllResultsCount))
    do ..Obs("A1-nested-fail", "parent_slot_kept", $get(^||%UnitTest.Manager.AllResults(1))=passIndex)
    do ..Obs("A1-nested-fail", "child_slot_kept", $data(^||%UnitTest.Manager.AllResults(2)))
    // A2: the parent then reads from zero: only its passing slot remains, and everything is cleared.
    set sc = ##class(%IPM.Test.Manager).GetAllTestsStatus(.n, 0)
    do ..Obs("A2-parent-after", "ok", $system.Status.IsOK(sc))
    do ..Obs("A2-parent-after", "count", n)
    do ..Obs("A2-parent-after", "accumulator_defined", $data(^||%UnitTest.Manager.AllResultsCount))
    // A3: neighbor - a reader starting at zero sees both slots and fails.
    set ^||%UnitTest.Manager.AllResults(1) = passIndex, ^||%UnitTest.Manager.AllResults(2) = failIndex, ^||%UnitTest.Manager.AllResultsCount = 2
    set sc = ##class(%IPM.Test.Manager).GetAllTestsStatus(.n, 0)
    do ..Obs("A3-top-both", "ok", $system.Status.IsOK(sc))
    do ..Obs("A3-top-both", "count", n)
    // A4: reversed slots - nested reader after the failing parent slot sees only the passing child.
    set ^||%UnitTest.Manager.AllResults(1) = failIndex, ^||%UnitTest.Manager.AllResults(2) = passIndex, ^||%UnitTest.Manager.AllResultsCount = 2
    set sc = ##class(%IPM.Test.Manager).GetAllTestsStatus(.n, 1)
    do ..Obs("A4-nested-pass", "ok", $system.Status.IsOK(sc))
    do ..Obs("A4-nested-pass", "count", n)
    do ..Obs("A4-nested-pass", "after_count", $get(^||%UnitTest.Manager.AllResultsCount))
    kill ^||%UnitTest.Manager.AllResults, ^||%UnitTest.Manager.AllResultsCount
    // D1/D2: failure display from a start slot, with and without an empty middle slot.
    set ^||%UnitTest.Manager.AllResults(1) = failIndex, ^||%UnitTest.Manager.AllResults(2) = failIndex, ^||%UnitTest.Manager.AllResults(3) = failIndex, ^||%UnitTest.Manager.AllResultsCount = 3
    do ..Obs("D1-display-from-1", "lines", ..CountFailedLines(1))
    kill ^||%UnitTest.Manager.AllResults(2)
    do ..Obs("D2-display-gap", "lines", ..CountFailedLines(1))
    do ..Obs("D3-display-one-slot", "lines", ..CountFailedLines(2))
    kill ^||%UnitTest.Manager.AllResults, ^||%UnitTest.Manager.AllResultsCount
}

/// Count FAILED lines written by OutputFailures(start).
ClassMethod CountFailedLines(start As %Integer) As %Integer
{
    set file = "/tmp/g5-iter4/out/display-"_start_"-"_$increment(^||G5I4Display)_".txt"
    set io = $io
    open file:("WNS"):5
    use file
    do ##class(%IPM.Test.Manager).OutputFailures(start)
    close file
    use io
    set stream = ##class(%Stream.FileCharacter).%New()
    set stream.Filename = file
    set n = 0
    while 'stream.AtEnd {
        set line = stream.ReadLine()
        if line [ "FAILED " { set n = n + 1 }
    }
    quit n
}

/// The deploy default each lifecycle class supplies, and the phase admission rule.
ClassMethod Producers()
{
    for cls = "%IPM.Lifecycle.Base", "%IPM.Lifecycle.Module", "%IPM.Lifecycle.DeployedModule" {
        do ..Obs("P-"_$piece(cls, ".", *), "deploy_by_default", $parameter(cls, "DEPLOYBYDEFAULT"))
    }
    // Requires g5i4-tests to be loaded (TestOutcome runs first).
    set module = ##class(%IPM.Storage.Module).NameOpen("g5i4-tests")
    set proc = ""
    for i=1:1:module.Resources.Count() {
        set candidate = module.Resources.GetAt(i).Processor
        if $isobject(candidate) && candidate.%IsA("%IPM.ResourceProcessor.Test") && (candidate.Package = "G5I4.Pass") {
            set proc = candidate
        }
    }
    for phase = "test", "TEST" {
        set proc.Phase = $listbuild(phase)
        set sc = proc.%ValidateObject()
        do ..Obs("P-stored-"_phase, "valid", $system.Status.IsOK(sc))
        kill params
        do ..Obs("P-stored-"_phase, "runs_for_TEST_request", proc.TestsShouldRun("TEST", .params))
    }
}

ClassMethod RegistryCase(scenario As %String, location As %String = "http://localhost:18501/")
{
    set svc = ##class(%IPM.Repo.Remote.PackageService).%New()
    set svc.Location = location
    set svc.Username = "fixture-user"
    set svc.Password = "fixture-pass"
    set svc.Token = "fixture-key"
    set ref = ##class(%IPM.Storage.ModuleInfo).%New()
    set ref.Name = scenario
    set ref.VersionString = "1.0.0"
    set ref.Deployed = 0
    kill ^IRIS.TempZPMRInst(scenario)
    try {
        set stream = svc.GetModule(ref, .asArchive)
        do ..Obs("R-"_scenario, "ok", 1)
        do ..Obs("R-"_scenario, "size", stream.Size)
        do ..Obs("R-"_scenario, "as_archive", asArchive)
    } catch e {
        do ..Obs("R-"_scenario, "ok", 0)
        do ..Obs("R-"_scenario, "error", $system.Status.GetErrorText(e.AsStatus()))
    }
    do ..Obs("R-"_scenario, "token", $get(^IRIS.TempZPMRInst(scenario, "1.0.0")))
}

ClassMethod Registry()
{
    for scenario = "same", "cross", "mismatch", "nohash", "upper", "dl404", "meta404" {
        do ..RegistryCase(scenario)
    }
    // Analytics consumer: the stored token is sent once and then removed.
    set svc = ##class(%IPM.Repo.Remote.PackageService).%New()
    set svc.Location = "http://localhost:18501/"
    try {
        do svc.SendAnalyticsToRegistry("install", {"name":"same", "version":"1.0.0"})
        do ..Obs("R-analytics", "ok", 1)
    } catch e {
        do ..Obs("R-analytics", "ok", 0)
    }
    do ..Obs("R-analytics", "token_after", $data(^IRIS.TempZPMRInst("same", "1.0.0")))
}

/// Full install workflow through a registry repository definition.
ClassMethod RegistryInstall()
{
    do ..Cmd("I0-repo", "repo -n g5i4reg -r -url http://localhost:18501/ -user fixture-user -pass fixture-pass")
    do ..Cmd("I1-install-good", "install g5i4-good")
    do ..Obs("I1-install-good", "installed", ##class(%IPM.Storage.Module).NameExists("g5i4-good"))
    do ..Cmd("I2-install-bad", "install g5i4-bad")
    do ..Obs("I2-install-bad", "installed", ##class(%IPM.Storage.Module).NameExists("g5i4-bad"))
    do ..Obs("I2-install-bad", "token", $get(^IRIS.TempZPMRInst("g5i4-bad", "1.0.0")))
    // Empty registry hash with bytes that are not an archive.
    do ..Cmd("I3-install-junk", "install g5i4-junk")
    do ..Obs("I3-install-junk", "installed", ##class(%IPM.Storage.Module).NameExists("g5i4-junk"))
}

}
