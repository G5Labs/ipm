/// Round-4 research probes for the install/update lane (decisions/research/inst.md).
/// U17: does `update -dev` from an ORAS registry run in developer mode (F-INST-5 beyond filesystem registries)?
/// U22: an external kill after an update step method ends and before its step record is saved.
/// Reuses the helpers of Uplift.Inst.Checks (Run, Put, Ver, Step, FileLines) and prints OBS|r4-inst-...| lines.
/// Fixtures: /tmp/uplift/r4inst/fixtures (copied from validation/iris/r4/inst/fixtures).
Class Uplift.A2.Inst
{

Parameter FIX = "/tmp/uplift/r4inst/fixtures/";

Parameter ORASNS = "nc-a2-ns17";

Parameter U22FILE = "/tmp/nc-a2-u22-steps.txt";

ClassMethod C() As %String [ CodeMode = expression ]
{
"Uplift.Inst.Checks"
}

ClassMethod Reset()
{
    do $classmethod(..C(), "Reset")
    for p = "NcA2O17", "NcA2O17F", "NcA2U22" {
        do $system.OBJ.DeletePackage(p, "-d")
    }
    kill ^NcA2O17, ^NcA2O17F, ^NcA2U22, ^NcA2U22X, ^IRIS.TempNcA2
    do ##class(%File).Delete(..#U22FILE)
}

ClassMethod Run(cmd As %String, Output out As %String) As %Status
{
    quit $classmethod(..C(), "Run", cmd, .out)
}

ClassMethod Put(ByRef kv, key As %String, sc As %Status)
{
    do $classmethod(..C(), "Put", .kv, key, sc)
}

ClassMethod Obs(name As %String, ByRef kv)
{
    do $classmethod(..C(), "Obs", name, .kv)
}

ClassMethod Ver(name As %String) As %String
{
    quit $classmethod(..C(), "Ver", name)
}

ClassMethod Step(cls As %String, meth As %String) As %String
{
    quit $classmethod(..C(), "Step", cls, meth, 0)
}

/// Repository field of an installed module, or "absent".
ClassMethod RepoOf(name As %String) As %String
{
    quit $classmethod(..C(), "Prop", name, "Repository")
}

/// Tags of <path> in zot, space-separated; "<404>" when the repository is absent.
ClassMethod Tags(path As %String) As %String
{
    set req = ##class(%Net.HttpRequest).%New()
    set req.Server = "oras", req.Port = 5000
    set sc = req.Get("/v2/"_path_"/tags/list")
    if $$$ISERR(sc) { quit "<http error>" }
    if req.HttpResponse.StatusCode '= 200 { quit "<"_req.HttpResponse.StatusCode_">" }
    set obj = ##class(%DynamicObject).%FromJSON(req.HttpResponse.Data)
    set s = "", it = obj.tags.%GetIterator()
    while it.%GetNext(.k, .v) { set s = s_v_" " }
    quit $zstrip(s, ">W")
}

/// Publishes versions 1.0.0 and 2.0.0 of a fixture module (folder <dir>) to the ORAS registry nc-a2-o17.
ClassMethod Publish(ByRef kv, dir As %String, name As %String)
{
    for v = "1.0.0", "2.0.0" {
        do ..Put(.kv, "pub."_name_"."_v_".load", ..Run("load "_..#FIX_dir_"/"_v_"/"))
        do ..Put(.kv, "pub."_name_"."_v_".publish", ..Run("publish "_name_" -r nc-a2-o17"))
        do ..Put(.kv, "pub."_name_"."_v_".uninstall", ..Run("uninstall "_name))
    }
    set kv("pub."_name_".tags") = ..Tags(..#ORASNS_"/"_name)
}

/// U17. update -dev from an ORAS registry.
ClassMethod U17()
{
    do ..Reset()
    do ..Put(.kv, "repoOras", ..Run("repo -o -n nc-a2-o17 -url http://oras:5000 -ns "_..#ORASNS))
    do ..Publish(.kv, "o17", "nc-a2-o17")
    do ..Publish(.kv, "o17f", "nc-a2-o17f")
    // Start each check from no step records and no residue of the folder loads.
    do ##class(%IPM.Storage.UpdateStep.AnyMember).%KillExtent()
    do ##class(%IPM.Storage.UpdateStep.PrimaryOnly).%KillExtent()
    kill ^NcA2O17, ^NcA2O17F
    do ..Obs("r4-inst-u17-oras-setup", .kv)

    // (a) A good update: does -dev leave the module in developer mode?
    kill kv
    do ..Put(.kv, "install100", ..Run("install nc-a2-o17 1.0.0"))
    set kv("repoAfterInstall") = ..RepoOf("nc-a2-o17")
    set kv("versionBefore") = ..Ver("nc-a2-o17")
    do ..Put(.kv, "devUpdate200", ..Run("update -dev nc-a2-o17 2.0.0"))
    set kv("versionAfter") = ..Ver("nc-a2-o17")
    set kv("devModeAfter") = $classmethod(..C(), "Prop", "nc-a2-o17", "DeveloperMode")
    set kv("V2.A") = ..Step("NcA2O17.Upd.V2", "A")
    set kv("V2.Aruns") = $get(^NcA2O17("V2.A"), 0)
    do ..Obs("r4-inst-u17-oras-dev-update", .kv)

    // (b) A failing update: in developer mode it must not be undone (INST-B68).
    kill kv
    do ..Put(.kv, "install100", ..Run("install nc-a2-o17f 1.0.0"))
    set kv("versionBefore") = ..Ver("nc-a2-o17f")
    do ..Put(.kv, "devUpdate200", ..Run("update -dev nc-a2-o17f 2.0.0"))
    set kv("versionAfter") = ..Ver("nc-a2-o17f")
    set kv("V2.A") = ..Step("NcA2O17F.Upd.V2", "A")
    set kv("V2.B") = ..Step("NcA2O17F.Upd.V2", "B")
    set kv("V2.Aruns") = $get(^NcA2O17F("V2.A"), 0)
    do ..Obs("r4-inst-u17-oras-dev-failed-update", .kv)

    // Clean up: modules, then the published images.
    kill kv
    do ..Run("uninstall nc-a2-o17")
    do ..Run("uninstall nc-a2-o17f")
    do ..Put(.kv, "unpub.o17", ..Run("unpublish nc-a2-o17/nc-a2-o17 all -force"))
    do ..Put(.kv, "unpub.o17f", ..Run("unpublish nc-a2-o17/nc-a2-o17f all -force"))
    set kv("tags.o17") = ..Tags(..#ORASNS_"/nc-a2-o17")
    set kv("tags.o17f") = ..Tags(..#ORASNS_"/nc-a2-o17f")
    do ..Obs("r4-inst-u17-oras-cleanup", .kv)
    do ..Run("repo -delete -name nc-a2-o17")
}

/// Job body for U22: runs the update whose step A hands shake with the probe.
ClassMethod U22Job(dev As %Boolean)
{
    set ^IRIS.TempNcA2("jobPid") = $job
    set sc = ##class(%IPM.Main).Shell("update "_$select(dev: "-dev ", 1: "")_"nc-a2-u22 -path "_..#FIX_"u22/2.0.0/")
    // Reached only if the kill missed.
    set ^IRIS.TempNcA2("jobEnded") = $select($system.Status.IsOK(sc): "ok", 1: "err")
}

ClassMethod U22State(ByRef kv)
{
    set kv("version") = ..Ver("nc-a2-u22")
    set kv("V1.S1") = ..Step("NcA2U22.Upd.V1", "S1")
    set kv("V2.A") = ..Step("NcA2U22.Upd.V2", "A")
    set kv("V2.B") = ..Step("NcA2U22.Upd.V2", "B")
    set kv("X") = $get(^NcA2U22X, "undef")
    set kv("fileLines") = $classmethod(..C(), "FileLines", ..#U22FILE)
    set kv("aRuns") = $get(^IRIS.TempNcA2("aRuns"), 0)
}

/// U22. Kill the updating process after step V2.A returns and before its record is saved, then retry.
ClassMethod U22(dev As %Boolean)
{
    set tag = $select(dev: "dev", 1: "nondev")
    do ..Reset()
    do ..Put(.kv, "load100", ..Run("load "_$select(dev: "-dev ", 1: "")_..#FIX_"u22/1.0.0/"))
    set ^IRIS.TempNcA2("handshake") = 1
    job ##class(Uplift.A2.Inst).U22Job(dev)
    for i = 1:1:600 { quit:$get(^IRIS.TempNcA2("stepEnded"))  hang 0.1 }
    set pid = $get(^IRIS.TempNcA2("pid"))
    set kv("stepEndedSeen") = +$get(^IRIS.TempNcA2("stepEnded"))
    set kv("stepPidIsJob") = (pid '= "") && (pid = $get(^IRIS.TempNcA2("jobPid")))
    set step = ##class(%IPM.Storage.UpdateStep.AnyMember).GetUpdateStep("NcA2U22.Upd.V2", "A")
    set id = $select($isobject(step): step.%Id(), 1: "")
    kill step
    set kv("recordId") = id
    if id '= "" {
        lock +^IPM.UpdateStep.AnyMemberD(id):5
        set kv("probeHoldsRecordLock") = $test
    }
    set ^IRIS.TempNcA2("lockHeld") = 1
    hang 2
    // Where is the step process now? Read the record from disk, not from a cached object.
    set kv("beforeKill.V2.A") = ..Step("NcA2U22.Upd.V2", "A")
    try { set kv("beforeKill.processState") = $select(pid = "": "nopid", 1: $zstrip($system.Process.State(pid), "*C")) } catch { set kv("beforeKill.processState") = "<n/a>" }
    try { set kv("beforeKill.processRoutine") = $select(pid = "": "nopid", 1: $system.Process.CurrentLineAndRoutine(pid)) } catch { set kv("beforeKill.processRoutine") = "<n/a>" }
    set kv("beforeKill.jobEnded") = $get(^IRIS.TempNcA2("jobEnded"), "no")
    set kv("terminate") = $select(pid = "": "nopid", 1: $system.Process.Terminate(pid))
    for i = 1:1:100 { quit:'$data(^$JOB(pid))  hang 0.1 }
    set kv("processGone") = '$data(^$JOB(pid))
    if id '= "" { lock -^IPM.UpdateStep.AnyMemberD(id) }
    set kv("jobEnded") = $get(^IRIS.TempNcA2("jobEnded"), "no")
    do ..U22State(.kv)
    do ..Obs("r4-inst-u22-"_tag_"-kill-before-record-save", .kv)

    // Retry without the handshake.
    kill kv
    kill ^IRIS.TempNcA2("handshake"), ^IRIS.TempNcA2("lockHeld"), ^IRIS.TempNcA2("stepEnded")
    do ..Put(.kv, "retry", ..Run("update "_$select(dev: "-dev ", 1: "")_"nc-a2-u22 -path "_..#FIX_"u22/2.0.0/"))
    do ..U22State(.kv)
    do ..Obs("r4-inst-u22-"_tag_"-retry", .kv)
}

ClassMethod RunAll()
{
    try { do ..U17() } catch e { write !, "U17 EXCEPTION ", e.DisplayString(), ! }
    try { do ..U22(1) } catch e { write !, "U22 dev EXCEPTION ", e.DisplayString(), ! }
    try { do ..U22(0) } catch e { write !, "U22 nondev EXCEPTION ", e.DisplayString(), ! }
    do ..Reset()
}

}
