/// Native checks #1-#20 of intents/03-04-seed-challenge.md ("Candidate native checks") for modules 03 (INST) and 04 (DEP).
/// Each check resets the namespace, runs real IPM shell commands and prints OBS|nc-inst-<n>-...| or OBS|nc-dep-<n>-...| lines
/// through Uplift.Probe.Env:Emit. Fixtures: /tmp/uplift/inst/fixtures (copied from validation/iris/inst/fixtures) and the
/// IPM test data under /home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/.
Class Uplift.Inst.Checks
{

Parameter FIX = "/tmp/uplift/inst/fixtures/";

Parameter DATA = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/";

Parameter CAPTURE = "/tmp/nc-inst-capture.txt";

Parameter STEPFILE = "/tmp/nc-inst-m-steps.txt";

// ===== helpers =====

ClassMethod Reset()
{
    while $tlevel { trollback }
    do ..Run("uninstall -all")
    // uninstall -all can leave modules behind after half-finished loads
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %IPM_Storage.ModuleItem")
    while rs.%Next() {
        set n = rs.%Get("Name")
        continue:(n = "zpm")||(n = "ipm")
        do ..Run("uninstall "_n_" -force")
    }
    do ..Run("repo -delete-all")
    do ##class(%IPM.Storage.UpdateStep.AnyMember).%KillExtent()
    do ##class(%IPM.Storage.UpdateStep.PrimaryOnly).%KillExtent()
    for p = "NcInstM", "NcDepLf", "NcDepCo", "UpdatePackage", "UpdateDeps", "UpdateDepsAdded", "UpdateStepErrors" {
        do $system.OBJ.DeletePackage(p, "-d")
    }
    kill ^NcInstM, ^NcInstMx, ^NcInstMy, ^NcInstLock
    do ##class(%File).Delete(..#STEPFILE)
}

/// Runs one IPM shell command with its output captured; echoes the output and a CMD line to the session log.
ClassMethod Run(cmd As %String, Output out As %String) As %Status
{
    set out = "", io = $io, f = ..#CAPTURE
    open f:("WNS"):2
    if '$test {
        write !, "CMD (no capture) ", cmd, !
        set sc = ##class(%IPM.Main).Shell(cmd)
        quit sc
    }
    use f
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
    } catch e {
        set sc = e.AsStatus()
    }
    close f
    use io
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(f)
    while 's.AtEnd {
        set line = s.ReadLine()
        write line, !
        set out = out_line_" "
    }
    write "CMD ", cmd, " => ", $select($system.Status.IsOK(sc): "OK", 1: $system.Status.GetErrorText(sc)), !
    quit sc
}

ClassMethod Clean(v As %String, len As %Integer = 220) As %String
{
    quit $zstrip($translate($extract(v, 1, len), ";|="_$char(9, 10, 13), ",/:   "), "<=>W")
}

ClassMethod Put(ByRef kv, key As %String, sc As %Status)
{
    set kv(key) = $select($system.Status.IsOK(sc): "ok", 1: "err")
    if '$system.Status.IsOK(sc) {
        set kv(key_"Err") = ..Clean($system.Status.GetErrorText(sc))
    }
}

ClassMethod Obs(name As %String, ByRef kv)
{
    set k = ""
    for {
        set k = $order(kv(k))
        quit:k=""
        set kv(k) = ..Clean(kv(k), 400)
    }
    do ##class(Uplift.Probe.Env).Emit(name, .kv)
}

/// Installed version of a module, with /dev in developer mode, or "absent".
ClassMethod Ver(name As %String) As %String
{
    if '##class(%IPM.Storage.Module).NameExists(name) { quit "absent" }
    set mod = ##class(%IPM.Storage.Module).NameOpen(name, 0)
    set v = mod.VersionString_$select(mod.DeveloperMode: "/dev", 1: "")
    kill mod
    quit v
}

ClassMethod Prop(name As %String, prop As %String) As %String
{
    if '##class(%IPM.Storage.Module).NameExists(name) { quit "absent" }
    set mod = ##class(%IPM.Storage.Module).NameOpen(name, 0)
    set v = $property(mod, prop)
    kill mod
    quit $select(v = "": "<empty>", 1: v)
}

/// Step record state: none, or start:<0/1>/end:<0/1>/ok:<0/1>. start:0 with end:1 is a seeded step.
ClassMethod Step(cls As %String, meth As %String, primaryOnly As %Boolean) As %String
{
    set step = $select(primaryOnly: ##class(%IPM.Storage.UpdateStep.PrimaryOnly).GetUpdateStep(cls, meth), 1: ##class(%IPM.Storage.UpdateStep.AnyMember).GetUpdateStep(cls, meth))
    if '$isobject(step) || (step.%Id() = "") { quit "none" }
    quit "start:"_(step.TimeStampStart '= "")_"/end:"_(step.TimeStampEnd '= "")_"/ok:"_$system.Status.IsOK(step.Status)
}

/// Every step record whose class starts with prefix, from both tables: key <tag>any:V1.MethodA or <tag>pri:V1.MethodA,
/// value start:<0/1>/end:<0/1>/ok:<0/1>. A step with no record has no key. The fixtures move methods between the
/// all-member and primary-only lists from one version to the next, so records are read generically.
ClassMethod SimpleSteps(ByRef kv, prefix As %String = "UpdatePackage", tag As %String = "")
{
    set n = 0
    for t = "AnyMember", "PrimaryOnly" {
        set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT ClassName, MethodName, TimeStampStart, TimeStampEnd, Status FROM %IPM_Storage_UpdateStep."_t_" WHERE ClassName %STARTSWITH ?", prefix_".V")
        while rs.%Next() {
            set n = n + 1
            set cls = $piece(rs.%Get("ClassName"), ".", *)
            set kv(tag_$select(t = "AnyMember": "any:", 1: "pri:")_cls_"."_rs.%Get("MethodName")) = "start:"_(rs.%Get("TimeStampStart") '= "")_"/end:"_(rs.%Get("TimeStampEnd") '= "")_"/ok:"_$system.Status.IsOK(rs.%Get("Status"))
        }
    }
    set kv(tag_"stepRows") = n
}

ClassMethod Repo(name As %String, path As %String) As %Status
{
    quit ..Run("repo -n "_name_" -fs -path "_path)
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

ClassMethod FileText(path As %String) As %String
{
    if '##class(%File).Exists(path) { quit "" }
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(path)
    quit s.Read(1000000)
}

// ===== checks =====

/// #1 INST-B17, B28, B29 (+ B24/B26/B27 on the way): forced install over a version with an update package seeds the skipped steps.
ClassMethod C1()
{
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "install211", ..Run("install update-simple 2.1.1"))
    do ..Put(.kv, "plainInstall350", ..Run("install update-simple 3.5.0"))
    do ..Put(.kv, "forceInstall121", ..Run("install -force update-simple 1.2.1"))
    do ..Put(.kv, "forceInstall350", ..Run("install -force update-simple 3.5.0"))
    set kv("version") = ..Ver("update-simple")
    do ..SimpleSteps(.kv)
    do ..Obs("nc-inst-1-force-install-seeds", .kv)
}

/// #2 INST-B30, B18, B55: dev install does not seed; a later update runs the unseeded steps.
ClassMethod C2()
{
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "install211", ..Run("install update-simple 2.1.1"))
    do ..Put(.kv, "devInstall350", ..Run("install -dev update-simple 3.5.0"))
    set kv("versionAfterDev") = ..Ver("update-simple")
    do ..SimpleSteps(.kv, , "afterDev-")
    do ..Put(.kv, "update350", ..Run("update update-simple 3.5.0"))
    set kv("versionAfterUpdate") = ..Ver("update-simple")
    do ..SimpleSteps(.kv)
    do ..Obs("nc-inst-2-dev-install-then-update", .kv)

    kill kv
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "devInstall121", ..Run("install -dev update-simple 1.2.1"))
    do ..SimpleSteps(.kv, , "afterDev-")
    do ..Put(.kv, "update211", ..Run("update update-simple 2.1.1"))
    set kv("versionAfterUpdate") = ..Ver("update-simple")
    do ..SimpleSteps(.kv)
    do ..Obs("nc-inst-2b-dev-fresh-then-update", .kv)
}

ClassMethod HoldLock(name As %String, secs As %Integer = 90)
{
    lock +^IPM.UpdateLock(name):5
    set ^NcInstLock("held") = $test
    for i = 1:1:(secs * 5) {
        quit:$get(^NcInstLock("release"))
        hang 0.2
    }
    lock -^IPM.UpdateLock(name)
    set ^NcInstLock("done") = 1
}

/// #3 INST-B70, B71, F-INST-1: update while another process holds the update lock.
ClassMethod C3()
{
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "install121", ..Run("install update-simple 1.2.1"))
    job ##class(Uplift.Inst.Checks).HoldLock("update-simple")
    for i = 1:1:50 { quit:$data(^NcInstLock("held"))  hang 0.2 }
    set kv("lockHeldByJob") = $get(^NcInstLock("held"), "timeout")
    set t0 = $zhorolog
    set sc = ..Run("update update-simple 2.1.1", .out)
    set kv("seconds") = $justify($zhorolog - t0, 0, 1)
    do ..Put(.kv, "update211", sc)
    set kv("printedLockOrWarning") = (out [ "lock") || (out [ "WARN") || (out [ "skip")
    set kv("version") = ..Ver("update-simple")
    do ..SimpleSteps(.kv)
    set ^NcInstLock("release") = 1
    for i = 1:1:50 { quit:$data(^NcInstLock("done"))  hang 0.2 }
    set kv("jobReleased") = $get(^NcInstLock("done"), 0)
    do ..Obs("nc-inst-3-update-lock-busy", .kv)

    kill kv
    do ..Put(.kv, "update211again", ..Run("update update-simple 2.1.1"))
    set kv("version") = ..Ver("update-simple")
    do ..SimpleSteps(.kv)
    do ..Obs("nc-inst-3b-update-after-lock-free", .kv)
}

ClassMethod MState(ByRef kv)
{
    set kv("version") = ..Ver("nc-inst-m")
    set kv("Mx") = $get(^NcInstMx, "undef")
    set kv("My") = $get(^NcInstMy, "undef")
    set kv("fileLines") = ..FileLines(..#STEPFILE)
    set kv("V1.S1runs") = $get(^NcInstM("V1.S1"), 0)
    set kv("V1.S1") = ..Step("NcInstM.Upd.V1", "S1", 0)
    set kv("V2.A") = ..Step("NcInstM.Upd.V2", "A", 0)
    set kv("V2.B") = ..Step("NcInstM.Upd.V2", "B", 0)
}

/// #4 INST-B64..B68, DEC-INST-5: effects of a failed update step and of the retry, without and with developer mode.
ClassMethod C4()
{
    do ..Reset()
    do ..Put(.kv, "load100", ..Run("load "_..#FIX_"m/1.0.0/"))
    do ..Put(.kv, "updateBad", ..Run("update nc-inst-m -path "_..#FIX_"m/2.0.0-bad/"))
    do ..MState(.kv)
    do ..Obs("nc-inst-4-nondev-failed-update", .kv)
    kill kv
    do ..Put(.kv, "updateGood", ..Run("update nc-inst-m -path "_..#FIX_"m/2.0.0-good/"))
    do ..MState(.kv)
    do ..Obs("nc-inst-4b-nondev-retry", .kv)

    kill kv
    do ..Reset()
    do ..Put(.kv, "devLoad100", ..Run("load -dev "_..#FIX_"m/1.0.0/"))
    do ..Put(.kv, "devUpdateBad", ..Run("update -dev nc-inst-m -path "_..#FIX_"m/2.0.0-bad/"))
    do ..MState(.kv)
    do ..Obs("nc-inst-4c-dev-failed-update", .kv)
    kill kv
    do ..Put(.kv, "devUpdateGood", ..Run("update -dev nc-inst-m -path "_..#FIX_"m/2.0.0-good/"))
    do ..MState(.kv)
    do ..Obs("nc-inst-4d-dev-retry", .kv)

    // 4e: update -dev from a filesystem registry (not -path): is the update run in developer mode?
    kill kv
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "install121", ..Run("install update-simple 1.2.1"))
    do ..Put(.kv, "devUpdate211", ..Run("update -dev update-simple 2.1.1"))
    set kv("version") = ..Ver("update-simple")
    do ..Obs("nc-inst-4e-dev-update-from-registry", .kv)
}

/// #5 INST-B64, B65, F-INST-3: failed update of update-step-errors.
ClassMethod C5()
{
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "install121", ..Run("install update-step-errors 1.2.1"))
    do ..Put(.kv, "update211", ..Run("update update-step-errors 2.1.1"))
    set kv("version") = ..Ver("update-step-errors")
    do ..SimpleSteps(.kv, "UpdateStepErrors.UpdatePackage")
    do ..Obs("nc-inst-5-failed-step-update", .kv)
}

/// #6 INST-B53, DEC-INST-8: a module first brought in by update runs its whole step history.
ClassMethod C6()
{
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "install121", ..Run("install update-deps-added 1.2.1"))
    set kv("simpleBefore") = ..Ver("update-simple")
    do ..Put(.kv, "update211", ..Run("update update-deps-added 2.1.1"))
    set kv("parentVersion") = ..Ver("update-deps-added")
    set kv("simpleVersion") = ..Ver("update-simple")
    do ..SimpleSteps(.kv)
    do ..Obs("nc-inst-6-dep-added-by-update", .kv)

    kill kv
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "install121", ..Run("install update-simple 1.2.1"))
    do ..SimpleSteps(.kv)
    do ..Obs("nc-inst-6b-fresh-install-control", .kv)
}

/// #7 INST-B33..B38: the developer-mode guard of install.
ClassMethod C7()
{
    set dir = ..#DATA_"update-test/update-modules-1.x/update-simple/"
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "loadDev", ..Run("load -dev "_dir))
    set kv("v0") = ..Ver("update-simple")
    do ..Put(.kv, "install", ..Run("install update-simple 1.2.1"))
    do ..Put(.kv, "installForce", ..Run("install -force update-simple 1.2.1"))
    do ..Put(.kv, "reinstall", ..Run("reinstall update-simple"))
    set kv("v1") = ..Ver("update-simple")
    do ..Put(.kv, "reinstallDev", ..Run("reinstall -dev update-simple"))
    set kv("v2") = ..Ver("update-simple")
    do ..Put(.kv, "installPermitDowngrade", ..Run("install -DPermitDowngrade=1 update-simple 1.2.1"))
    set kv("v3") = ..Ver("update-simple")
    do ..Put(.kv, "reloadDev", ..Run("load -dev "_dir))
    set kv("v4") = ..Ver("update-simple")
    do ..Put(.kv, "update211", ..Run("update update-simple 2.1.1"))
    set kv("v5") = ..Ver("update-simple")
    do ..Put(.kv, "updatePermitDowngrade", ..Run("update -DPermitDowngrade=1 update-simple 2.1.1"))
    do ..Obs("nc-inst-7-dev-mode-guard", .kv)
}

/// #8 INST-B35, DEC-INST-3: a needed module in developer mode is replaced silently.
ClassMethod C8()
{
    set bdg = ..#DATA_"build-dependency-graph/"
    do ..Reset(), ..Repo("bdg", bdg)
    do ..Put(.kv, "loadDevC200", ..Run("load -dev "_bdg_"complex-c/2.0.0/"))
    set kv("cBefore") = ..Ver("complex-c")
    do ..Put(.kv, "installB200", ..Run("install complex-b 2.0.0"))
    set kv("cAfter") = ..Ver("complex-c")
    set kv("bAfter") = ..Ver("complex-b")
    do ..Obs("nc-inst-8-dev-dependency-replaced", .kv)
}

/// #9 INST-B39, DEC-INST-4: reinstall keeps the installed version.
ClassMethod C9()
{
    do ..Reset(), ..Repo("bdg", ..#DATA_"build-dependency-graph/")
    do ..Put(.kv, "installC200", ..Run("install complex-c 2.0.0"))
    do ..Put(.kv, "reinstall", ..Run("reinstall complex-c", .out))
    set kv("printedReinstalling") = $piece($piece(out, "Reinstalling ", 2), " ", 1, 2)
    set kv("versionAfter") = ..Ver("complex-c")
    do ..Obs("nc-inst-9-reinstall-version", .kv)
}

ClassMethod RepoInfo(ByRef kv, name As %String)
{
    if ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists(name, .id) {
        set kv("repo:"_name) = "id:"_id_"/sortOrder:"_##class(%IPM.Repo.Definition).SortOrder(id)
    }
}

/// #10 INST-B8, B9 (and INST-B4): registry prefix limits only the named module; tie-break between registries.
ClassMethod C10()
{
    do ..Reset()
    do ..Repo("nc-inst-r1", ..#FIX_"r1/"), ..Repo("nc-inst-r2", ..#FIX_"r2/")
    do ..RepoInfo(.kv, "nc-inst-r1"), ..RepoInfo(.kv, "nc-inst-r2")
    do ..Put(.kv, "installR2P", ..Run("install nc-inst-r2/nc-inst-p"))
    set kv("p.repo") = ..Prop("nc-inst-p", "Repository")
    set kv("x.repo") = ..Prop("nc-inst-x", "Repository")
    do ..Obs("nc-inst-10-registry-prefix", .kv)

    // Tie-break: r2 created first (lower ID, default sort order), r1 created second with a lower (earlier) sort order.
    kill kv
    do ..Reset()
    do ..Repo("nc-inst-r2", ..#FIX_"r2/"), ..Repo("nc-inst-r1", ..#FIX_"r1/")
    if ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-inst-r1", .id) {
        set r = ##class(%IPM.Repo.Definition).%OpenId(id)
        set r.OverriddenSortOrder = 1
        do ..Put(.kv, "setSortOrder", r.%Save())
        kill r
    }
    do ..RepoInfo(.kv, "nc-inst-r1"), ..RepoInfo(.kv, "nc-inst-r2")
    do ..Put(.kv, "installX", ..Run("install nc-inst-x"))
    set kv("x.repo") = ..Prop("nc-inst-x", "Repository")
    do ..Obs("nc-inst-10b-registry-tie-break", .kv)

    // Same, with r1 given a higher (later) sort order: if the source still follows creation order, sort order plays no part.
    kill kv
    do ..Reset()
    do ..Repo("nc-inst-r2", ..#FIX_"r2/"), ..Repo("nc-inst-r1", ..#FIX_"r1/")
    if ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-inst-r1", .id) {
        set r = ##class(%IPM.Repo.Definition).%OpenId(id)
        set r.OverriddenSortOrder = 3
        do ..Put(.kv, "setSortOrder", r.%Save())
        kill r
    }
    do ..RepoInfo(.kv, "nc-inst-r1"), ..RepoInfo(.kv, "nc-inst-r2")
    do ..Put(.kv, "installX", ..Run("install nc-inst-x"))
    set kv("x.repo") = ..Prop("nc-inst-x", "Repository")
    do ..Obs("nc-inst-10c-registry-tie-break-high", .kv)
}

/// #11 DEP-B12, B13, DEC-DEP-1: greedy resolution fails on a solvable input.
ClassMethod C11()
{
    do ..Reset(), ..Repo("nc-dep-r11", ..#FIX_"dep11/")
    do ..Put(.kv, "installRoot", ..Run("install nc-dep-root"))
    for m = "nc-dep-root", "nc-dep-a", "nc-dep-b", "nc-dep-c" { set kv(m) = ..Ver(m) }
    do ..Obs("nc-dep-11-greedy-resolution", .kv)

    // Control: the same input has a solution (A 1.0.0 with C 1.0.0).
    kill kv
    do ..Reset(), ..Repo("nc-dep-r11", ..#FIX_"dep11/")
    do ..Put(.kv, "installA100", ..Run("install nc-dep-a 1.0.0"))
    do ..Put(.kv, "installRoot", ..Run("install nc-dep-root"))
    for m = "nc-dep-root", "nc-dep-a", "nc-dep-b", "nc-dep-c" { set kv(m) = ..Ver(m) }
    do ..Obs("nc-dep-11b-solvable-with-pin", .kv)
}

/// #12 DEP-B24, B25, F-DEP-1: PermitDowngrade relaxes the resolver but the load refuses.
ClassMethod C12()
{
    set bdg = ..#DATA_"build-dependency-graph/"
    do ..Reset(), ..Repo("bdg", bdg)
    do ..Put(.kv, "loadC300", ..Run("load "_bdg_"complex-c/3.0.0/"))
    do ..Put(.kv, "loadA", ..Run("load "_bdg_"complex-a/1.0.0/"))
    set kv("cAfterPlain") = ..Ver("complex-c")
    do ..Put(.kv, "loadAPermitDowngrade", ..Run("load "_bdg_"complex-a/1.0.0/ -DPermitDowngrade=1"))
    set kv("cAfterPermit") = ..Ver("complex-c")
    set kv("aAfter") = ..Ver("complex-a")
    do ..Obs("nc-dep-12-permit-downgrade", .kv)
}

ClassMethod LockKeys(path As %String) As %String
{
    set text = ..FileText(path)
    quit:text="" "<nofile>"
    set obj = ##class(%DynamicAbstractObject).%FromJSON(text)
    set deps = obj.%Get("dependencies")
    quit:'$isobject(deps) "<none>"
    set it = deps.%GetIterator(), keys = ""
    while it.%GetNext(.k) { set keys = keys_$select(keys = "": "", 1: " ")_k }
    quit keys
}

/// #13 DEP-B14, B39, F-DEP-2: order of dependencies in the lock file.
ClassMethod C13()
{
    set lf = ..#FIX_"dep13/lroot/module-lock.json"
    do ##class(%File).Delete(lf)
    do ..Reset(), ..Repo("nc-dep-r13", ..#FIX_"dep13/")
    do ..Put(.kv, "install", ..Run("install -create-lockfile nc-dep-lroot", .out))
    set kv("installOrder") = $select($find(out, "nc-dep-zz-base") < $find(out, "nc-dep-aa-mid"): "zz-base first in output", 1: "aa-mid first in output")
    set root = ..Prop("nc-dep-lroot", "Root")
    set kv("root") = root
    set kv("lockFileAt") = $select(##class(%File).Exists(root_"module-lock.json"): "module root", 1: "missing")
    set kv("dependencyKeys") = ..LockKeys(root_"module-lock.json")
    write !, "LOCKFILE ", ..FileText(root_"module-lock.json"), !
    do ..Obs("nc-dep-13-lockfile-order", .kv)
}

ClassMethod FileSig(path As %String) As %String
{
    quit:'##class(%File).Exists(path) "absent"
    quit "size:"_##class(%File).GetFileSize(path)_"/md5:"_$extract(##class(%xsd.hexBinary).LogicalToXSD($system.Encryption.MD5Hash(..FileText(path))), 1, 12)_"/mtime:"_##class(%File).GetFileDateModified(path)
}

/// #14 DEP-B38, B41, DEC-DEP-3: lock file when a needed module came from a folder.
ClassMethod C14()
{
    set base = ..#DATA_"lock-test/mods-base-cases/"
    set lf = base_"lock-mod-e-1-dep-0-transient/module-lock.json"
    set saved = ..FileText(lf)
    do ..Reset(), ..Repo("lock-file-base", base)
    do ..Put(.kv, "loadA", ..Run("load "_base_"lock-mod-a-no-deps/"))
    set kv("a.repo") = ..Prop("lock-mod-a-no-deps", "Repository")
    set kv("fileBefore") = ..FileSig(lf)
    hang 1.1
    do ..Put(.kv, "installE", ..Run("install -create-lockfile lock-mod-e-1-dep-0-transient", .out))
    set kv("e.version") = ..Ver("lock-mod-e-1-dep-0-transient")
    set kv("printedLockError") = (out [ "Error creating lock file")
    set kv("lockErrorText") = $extract(out, $find(out, "Error creating lock file") - 24, $find(out, "Error creating lock file") + 140)
    set kv("fileAfter") = ..FileSig(lf)
    set kv("fileChanged") = (kv("fileBefore") '= kv("fileAfter"))
    if kv("fileChanged") && (saved '= "") {
        // put the committed fixture back
        set s = ##class(%Stream.FileCharacter).%New()
        do s.LinkToFile(lf), s.Write(saved), s.%Save()
    }
    do ..Obs("nc-dep-14-lockfile-folder-dependency", .kv)
}

/// #15 DEP-B40, DEC-DEP-2: lock file left behind by a failed install.
ClassMethod C15()
{
    set lf = ..#FIX_"dep15/lf-fail/module-lock.json"
    do ##class(%File).Delete(lf)
    do ..Reset(), ..Repo("nc-dep-r15", ..#FIX_"dep15/")
    set kv("fileBefore") = ##class(%File).Exists(lf)
    do ..Put(.kv, "install", ..Run("install -create-lockfile nc-dep-lf-fail"))
    set kv("fileAfter") = ##class(%File).Exists(lf)
    set kv("dependencyKeys") = ..LockKeys(lf)
    set kv("failVersion") = ..Ver("nc-dep-lf-fail")
    set kv("baseVersion") = ..Ver("nc-dep-lf-base")
    do ..Obs("nc-dep-15-lockfile-failed-install", .kv)
}

/// #16 DEP-B44, B49, F-DEP-3: list-dependents with lower and mixed case.
ClassMethod C16()
{
    do ..Reset(), ..Repo("u", ..#DATA_"update-test/")
    do ..Put(.kv, "installDeps211", ..Run("install update-deps 2.1.1"))
    do ..Put(.kv, "lower", ..Run("list-dependents update-simple", .out1))
    set kv("lowerListsUpdateDeps") = (out1 [ "update-deps")
    set kv("lowerOut") = out1
    do ..Put(.kv, "mixed", ..Run("list-dependents Update-Simple", .out2))
    set kv("mixedListsUpdateDeps") = (out2 [ "update-deps")
    set kv("mixedOut") = $select($zstrip(out2, "<>W") = "": "<empty>", 1: out2)
    do ..Obs("nc-dep-16-list-dependents-case", .kv)
}

/// #17 DEP-B15..B19: a test-scoped clash shows only when the test phase runs.
ClassMethod C17()
{
    set bdg = ..#DATA_"build-dependency-graph/"
    do ..Reset(), ..Repo("bdg", bdg)
    do ..Put(.kv, "loadScopedRoot", ..Run("load "_bdg_"scoped-root/2.0.0/"))
    for m = "scoped-root", "loop-a", "scoped-b", "loop-c" { set kv(m) = ..Ver(m) }
    do ..Put(.kv, "testOnly", ..Run("scoped-root test -only"))
    set kv("testErrNamesLoopC") = ($get(kv("testOnlyErr")) [ "loop-c")
    set kv("loopCAfter") = ..Ver("loop-c")
    do ..Obs("nc-dep-17-scoped-clash", .kv)
}

/// #18 DEP-B22, B23: the named module must satisfy installed modules' requirements.
ClassMethod C18()
{
    do ..Reset(), ..Repo("bdg", ..#DATA_"build-dependency-graph/")
    do ..Put(.kv, "installB100", ..Run("install complex-b 1.0.0"))
    set kv("cBefore") = ..Ver("complex-c")
    do ..Put(.kv, "installC300", ..Run("install complex-c 3.0.0"))
    set kv("cAfter") = ..Ver("complex-c")
    do ..Obs("nc-dep-18-installed-requirements", .kv)
}

ClassMethod ABCD(ByRef kv)
{
    for m = "module-a", "module-b", "module-c", "module-d" { set kv(m) = ..Ver(m) }
}

/// #19 DEP-B27, B30, B31: versions after failed multi-module updates.
ClassMethod C19()
{
    do ..Reset(), ..Repo("update-test-modules", ..#DATA_"update-test/")
    do ..Put(.kv, "installA200", ..Run("install module-a 2.0.0"))
    do ..Put(.kv, "installD", ..Run("install module-d"))
    do ..ABCD(.kv)
    do ..Obs("nc-dep-19-before", .kv)
    kill kv
    do ..Put(.kv, "updateA210", ..Run("update module-a 2.1.0+snapshot"))
    do ..ABCD(.kv)
    do ..Obs("nc-dep-19-blocked-update", .kv)
    kill kv
    do ..Put(.kv, "uninstallD", ..Run("uninstall module-d"))
    do ..Put(.kv, "updateA205", ..Run("update module-a 2.0.5+snapshot"))
    do ..ABCD(.kv)
    do ..Obs("nc-dep-19b-failed-midway", .kv)
    kill kv
    do ..Put(.kv, "updateA210", ..Run("update module-a 2.1.0+snapshot"))
    do ..ABCD(.kv)
    do ..Obs("nc-dep-19c-coordinated-update", .kv)

    // The prescribed steps fail before any module moves. Here a needed module moves first, then the parent's Activate fails.
    // 19d: plain update from the registry. 19e: update -dev from a folder (-path). 19f: update -dev from the registry.
    for variant = "d", "e", "f" {
        kill kv
        do ..Reset(), ..Repo("nc-dep-r19", ..#FIX_"dep19/")
        do ..Put(.kv, "installP100", ..Run("install nc-dep-co-p 1.0.0"))
        set kv("pBefore") = ..Ver("nc-dep-co-p"), kv("dBefore") = ..Ver("nc-dep-co-d")
        set cmd = $case(variant, "d": "update nc-dep-co-p 2.0.0", "e": "update -dev nc-dep-co-p -path "_..#FIX_"dep19/p-2.0.0/", : "update -dev nc-dep-co-p 2.0.0")
        set kv("cmd") = cmd
        do ..Put(.kv, "update", ..Run(cmd, .out))
        set kv("dLoadedDuringUpdate") = (out [ "nc-dep-co-d")
        set kv("pAfter") = ..Ver("nc-dep-co-p"), kv("dAfter") = ..Ver("nc-dep-co-d")
        do ..Obs($case(variant, "d": "nc-dep-19d-rollback-after-dep-moved", "e": "nc-dep-19e-dev-path-update-dep-kept", : "nc-dep-19f-dev-registry-update"), .kv)
    }
}

/// #20 INST-B72..B74: needs a mirror; record why it cannot run here.
ClassMethod C20()
{
    set kv("isMirrorMember") = $system.Mirror.IsMember()
    set kv("isPrimary") = $system.Mirror.IsPrimary()
    set kv("reason") = "single non-mirrored instance, no second member to fail over to"
    do ..Obs("nc-inst-20-no-mirror", .kv)
}

ClassMethod RunList(list As %List)
{
    set ptr = 0
    while $listnext(list, ptr, n) {
        write !, "===== CHECK ", n, " =====", !
        try {
            do $classmethod("Uplift.Inst.Checks", "C"_n)
        } catch e {
            write "OBS|error-nc-check-"_n_"|error="_..Clean(e.DisplayString())_";", !
        }
        while $tlevel { trollback }
    }
}

}
