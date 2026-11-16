/// Native checks N12-N17 for module 08 (resources and author code). Run in USER.
Class Uplift.Nsp.Res
{

Parameter FIX = "/tmp/uplift/nsp/fixtures/";

Parameter DATA = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/";

ClassMethod DeployedFlag(cls As %String) As %String
{
    set c = ##class(%Dictionary.ClassDefinition).%OpenId(cls)
    quit:'$isobject(c) "<no-class>"
    quit c.Deployed
}

/// 1 when the method implementation of cls:method can be read back from the class definition.
ClassMethod SourceReadable(cls As %String, method As %String) As %String
{
    set m = ##class(%Dictionary.MethodDefinition).%OpenId(cls_"||"_method)
    quit:'$isobject(m) "<no-method>"
    quit (m.Implementation.Size > 0)
}

ClassMethod ModDeployed(name As %String) As %String
{
    quit:'##class(%IPM.Storage.Module).NameExists(name) "-"
    quit ##class(%IPM.Storage.Module).NameOpen(name).Deployed
}

/// N12 (RES-B23 / DEC-RES-3, RES-B31 / F-RES-3, RES-B20 / F-RES-4)
ClassMethod N12()
{
    set u = "Uplift.Nsp.Util"
    set dir = "/tmp/nc-dm/"
    do $classmethod(u, "CopyDir", ..#DATA_"deployed-mappings/", dir)
    // (a) makedeployed in developer mode
    set sc = $classmethod(u, "Run", "load -dev "_dir, .o1)
    set kv("loadDev") = $classmethod(u, "St", sc)
    set kv("deployedBefore") = ..DeployedFlag("DeployMapA.First")
    set kv("sourceBefore") = ..SourceReadable("DeployMapA.First", "Hello")
    set sc = $classmethod(u, "Run", "makedeployed deployed-mappings -v", .o2)
    set kv("makedeployed") = $classmethod(u, "St", sc)
    set kv("devMessage") = $classmethod(u, "Line", .o2, "developer mode")
    set kv("deployedAfter") = ..DeployedFlag("DeployMapA.First")
    set kv("sourceAfter") = ..SourceReadable("DeployMapA.First", "Hello")
    set kv("moduleDeployedFlag") = ..ModDeployed("deployed-mappings")
    set kv("moduleRecord") = $classmethod(u, "Mod", "deployed-mappings")
    set kv("callAfter") = $classmethod("DeployMapA.First", "Hello")
    do $classmethod(u, "Emit", "nc-res-N12a-makedeployed-dev", .kv)
    kill kv
    set sc = $classmethod(u, "Run", "uninstall deployed-mappings -force", .o3)
    set kv("uninstall") = $classmethod(u, "St", sc)
    set kv("classAfterUninstall") = $classmethod(u, "ClassState", "DeployMapA.First")
    // (b) package in developer mode, without -v
    do $classmethod(u, "CopyDir", ..#DATA_"deployed-mappings/", dir)
    do ##class(%File).RemoveDirectoryTree("/tmp/nc-pkg/")
    set sc = $classmethod(u, "Run", "load -dev "_dir, .o4)
    set kv("loadDev") = $classmethod(u, "St", sc)
    set kv("deployedBefore") = ..DeployedFlag("DeployMapA.First")
    set sc = $classmethod(u, "Run", "package deployed-mappings -path /tmp/nc-pkg/dm", .o5)
    set kv("package") = $classmethod(u, "St", sc)
    set kv("tgzExists") = ##class(%File).Exists("/tmp/nc-pkg/dm.tgz")
    set kv("generatedLineShown") = $classmethod(u, "Has", .o5, "Module package generated")
    set kv("outputLines") = +$get(o5)
    set kv("deployedAfter") = ..DeployedFlag("DeployMapA.First")
    set kv("sourceAfter") = ..SourceReadable("DeployMapA.First", "Hello")
    set kv("moduleRecord") = $classmethod(u, "Mod", "deployed-mappings")
    do $classmethod(u, "Emit", "nc-res-N12b-package-dev", .kv)
    kill kv
    set sc = $classmethod(u, "Run", "package deployed-mappings -v -path /tmp/nc-pkg/dmv", .o6)
    set kv("package") = $classmethod(u, "St", sc)
    set kv("generatedLine") = $classmethod(u, "Line", .o6, "Module package generated")
    do $classmethod(u, "Emit", "nc-res-N12c-package-verbose", .kv)
    do $classmethod(u, "Run", "uninstall deployed-mappings -force", .o7)
}

/// N13 (RES-B49 / F-RES-2)
ClassMethod N13()
{
    set u = "Uplift.Nsp.Util"
    set sc = $classmethod(u, "Run", "load "_..#DATA_"simplest-module/1.0.0/", .o1)
    set kv("load") = $classmethod(u, "St", sc)
    set sc = $classmethod(u, "Run", "simplest-module register", .o2)
    set kv("register") = $classmethod(u, "St", sc)
    set kv("registerErrLine") = $classmethod(u, "Line", .o2, "ERROR")
    set sc = $classmethod(u, "Run", "simplest-module clean register", .o3)
    set kv("helpExample") = $classmethod(u, "St", sc)
    set sc = $classmethod(u, "Run", "simplest-module nosuchphase", .o4)
    set kv("unknownPhase") = $classmethod(u, "St", sc)
    do $classmethod(u, "Emit", "nc-res-N13-register", .kv)
    do $classmethod(u, "Run", "uninstall simplest-module", .o5)
}

/// N14 (RES-B60, B63, B66, B67, B69)
ClassMethod N14()
{
    set u = "Uplift.Nsp.Util"
    set dir = "/tmp/nc-lune/"
    // first attempt (log 890-nsp-res-checks) stopped in PyVersion after the bypass load; start from no module record
    if $classmethod(u, "Mod", "lune-wheel") '= "-" { do $classmethod(u, "Run", "uninstall lune-wheel -force", .oz) }
    do $classmethod(u, "CopyDir", ..#DATA_"python-deps-tests/lune-wheel-reqs-offline/", dir)
    set kv("reqs") = $translate($classmethod(u, "ReadFile", dir_"requirements.txt"), $char(10), " ")
    // control: without the flag the offline requirements.txt (lune==1.6.4, --no-index) cannot be met
    set sc = $classmethod(u, "Run", "load -v "_dir, .o1)
    set kv("loadNoFlag") = $classmethod(u, "St", sc)
    set kv("noFlagRecord") = $classmethod(u, "Mod", "lune-wheel")
    if kv("noFlagRecord") '= "-" { do $classmethod(u, "Run", "uninstall lune-wheel -force", .o0) }
    set sc = $classmethod(u, "Run", "load -v -bypass-py-deps "_dir, .o2)
    set kv("loadBypass") = $classmethod(u, "St", sc)
    set kv("bypassRecord") = $classmethod(u, "Mod", "lune-wheel")
    set kv("bypassWheelLines") = ..Count(.o2, "lune-1.6.2-py3-none-any.whl")
    set kv("bypassReqsLines") = ..Count(.o2, "requirements.txt")
    set kv("bypassPipLine") = $classmethod(u, "Line", .o2, "lune-1.6.2-py3-none-any.whl")
    set kv("luneImportVersion") = ..PyVersion("lune")
    set sc = $classmethod(u, "Run", "lune-wheel compile -v", .o3)
    set kv("compile") = $classmethod(u, "St", sc)
    set kv("compileWheelLines") = ..Count(.o3, "lune-1.6.2-py3-none-any.whl")
    set kv("compilePipLine") = $classmethod(u, "Line", .o3, "lune-1.6.2-py3-none-any.whl")
    set kv("compileReqsLines") = ..Count(.o3, "requirements.txt")
    do $classmethod(u, "Emit", "nc-res-N14-bypass-py-deps", .kv)
    do $classmethod(u, "Run", "uninstall lune-wheel", .o4)
}

ClassMethod Count(ByRef out, str As %String) As %Integer
{
    set n = 0
    for i=1:1:$get(out) { set:out(i)[str n = n + 1 }
    quit n
}

ClassMethod PyVersion(pkg As %String) As %String
{
    set v = "<not-importable>"
    try {
        set md = ##class(%SYS.Python).Import("importlib.metadata")
        set v = md.version(pkg)
    } catch e {
    }
    quit v
}

/// N15 (RES-B39, B41, B42): a throwing hook with CheckStatus 0; a missing method on a custom phase.
ClassMethod N15()
{
    set u = "Uplift.Nsp.Util"
    kill ^IRIS.Temp.NcHookRan
    set sc = $classmethod(u, "Run", "load "_..#FIX_"nc-invoke-throw/", .o1)
    set kv("throwLoad") = $classmethod(u, "St", sc)
    set kv("hookRan") = +$get(^IRIS.Temp.NcHookRan)
    set kv("throwRecord") = $classmethod(u, "Mod", "nc-invoke-throw")
    do $classmethod(u, "Run", "uninstall nc-invoke-throw -force", .o2)
    set sc = $classmethod(u, "Run", "load "_..#FIX_"nc-invoke-badname/", .o3)
    set kv("badnameLoad") = $classmethod(u, "St", sc)
    set kv("badnameRecord") = $classmethod(u, "Mod", "nc-invoke-badname")
    set sc = $classmethod(u, "Run", "nc-invoke-badname NcPhase", .o4)
    set kv("badnamePhase") = $classmethod(u, "St", sc)
    set sc = $classmethod(u, "Run", "nc-invoke-badname ncphase", .o5)
    set kv("badnamePhaseLowercase") = $classmethod(u, "St", sc)
    do $classmethod(u, "Run", "uninstall nc-invoke-badname", .o6)
    do $classmethod(u, "Emit", "nc-res-N15-hook-throw-and-badname", .kv)
}

/// N16 (RES-B73 / DEC-RES-1): no `new $namespace` here, so a lasting switch would show.
/// No output capture: the capture routine is not visible in %SYS (first attempt, log 890-nsp-res-checks, failed with NOROUTINE).
ClassMethod N16()
{
    set u = "Uplift.Nsp.Util"
    set kv("before") = $namespace
    write !, ">>> zpm ""cos set $namespace=""""%SYS"""" write """"INSIDE:"""",$namespace"" (not captured)", !
    set sc = ##class(%IPM.Main).Shell("cos set $namespace=""%SYS"" write ""INSIDE:"",$namespace")
    set kv("status") = $classmethod(u, "St", sc)
    set kv("after") = $namespace
    set $namespace = kv("before")
    do $classmethod(u, "Emit", "nc-res-N16-exec-namespace-v2", .kv)
}

/// N17 (RES-B74..B78 / F-RES-5 / DEC-RES-2)
ClassMethod N17()
{
    set u = "Uplift.Nsp.Util"
    set d = ..#FIX_"rff2/"
    // v2: the first run (log 890-nsp-res-checks) reused path /tmp/, which an existing filesystem repo already holds
    for p = "a", "b", "o", "c1", "c3" { do ##class(%File).CreateDirectoryChain("/tmp/nc-rff/"_p_"/") }
    for r = "nc-rff-a", "nc-rff-b", "nc-rff-o", "nc-rff-c1", "nc-rff-c3" {
        do:##class(%IPM.Repo.Definition).ServerDefinitionKeyExists(r) ##class(%IPM.Repo.Definition).ServerDefinitionKeyDelete(r)
    }
    set sc = $classmethod(u, "Run", "run-from-file "_d_"cmds.text", .o1)
    set kv("a.text") = $classmethod(u, "St", sc)
    set kv("a.repoMade") = ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-rff-a")
    set sc = $classmethod(u, "Run", "run-from-file "_d_"cmds-array.json", .o2)
    set kv("b.array") = $classmethod(u, "St", sc)
    set kv("b.repoMade") = ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-rff-b")
    set sc = $classmethod(u, "Run", "run-from-file "_d_"cmds-object.json", .o3)
    set kv("o.object") = $classmethod(u, "St", sc)
    set kv("o.repoMade") = ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-rff-o")
    set sc = $classmethod(u, "Run", "run-from-file "_d_"cmds3.txt", .o4)
    set kv("c.txt") = $classmethod(u, "St", sc)
    set kv("c.line1Ran") = ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-rff-c1")
    set kv("c.line3Ran") = ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-rff-c3")
    set sc = $classmethod(u, "Run", "run-from-file -dry-run "_d_"cmds3.txt", .o5)
    set kv("dryRun") = $classmethod(u, "St", sc)
    set kv("dryRunShowsLine3") = $classmethod(u, "Has", .o5, "nc-rff-c3")
    for r = "nc-rff-a", "nc-rff-b", "nc-rff-o", "nc-rff-c1", "nc-rff-c3" {
        do:##class(%IPM.Repo.Definition).ServerDefinitionKeyExists(r) ##class(%IPM.Repo.Definition).ServerDefinitionKeyDelete(r)
    }
    do $classmethod(u, "Emit", "nc-res-N17-run-from-file-v2", .kv)
}

}
