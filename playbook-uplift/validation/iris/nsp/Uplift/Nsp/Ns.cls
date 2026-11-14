/// Native checks N2-N10 for module 07 (namespace ownership). Run from USER, where IPM has a local install.
/// Scratch namespaces: NCA, NCB, NCD, NCE (empty), NCC (own local install, the "other source"), NCT (no IPM).
Class Uplift.Nsp.Ns
{

Parameter FIX = "/tmp/uplift/nsp/fixtures/";

Parameter DATA = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/";

ClassMethod U() As %String [ CodeMode = expression ]
{
"Uplift.Nsp.Util"
}

ClassMethod Setup()
{
    for ns = "NCA", "NCB", "NCC", "NCD", "NCE", "NCT" {
        set sc = ##class(Uplift.Nsp.Util).MkNs(ns)
        write !, "MkNs ", ns, " ", ##class(Uplift.Nsp.Util).St(sc)
        // The probe classes live in USER; map package Uplift so they run in every scratch namespace.
        write " mapUplift ", ##class(Uplift.Nsp.Util).St(..MapUplift(ns))
    }
}

/// %IPM.Test.Utils.CreateNamespace also maps the %IPM package (only) from the caller's namespace.
/// Record what the installer did in NCC under that pre-existing mapping, then rebuild clean namespaces.
ClassMethod FixSetup()
{
    set u = ..U()
    set kv("nccMaps") = $classmethod(u, "Maps", "NCC")
    set kv("nccPkgDest") = $classmethod(u, "PkgDest", "NCC")
    set kv("nccRtnDB") = $classmethod(u, "CodeDB", "NCC")
    set kv("userPkgDest") = $classmethod(u, "PkgDest", "USER")
    set kv("nccZpmRecord") = $classmethod(u, "Mod", "zpm", "NCC")
    set kv("nccInstallerClassDest") = ..ClassDest("NCC", "IPM.Installer")
    set kv("ncaMapsFromCreate") = $classmethod(u, "Maps", "NCA")
    do $classmethod(u, "Emit", "nc-nsp-installer-existing-pkg-mapping", .kv)
    write !, "DeleteNamespace NCC ", $classmethod(u, "St", ##class(%IPM.Test.Utils).DeleteNamespace("NCC"))
    write !, "MkNs NCC ", $classmethod(u, "St", $classmethod(u, "MkNs", "NCC"))
    for ns = "NCA", "NCB", "NCC", "NCD", "NCE", "NCT" {
        write !, "Unmap %IPM pkg in ", ns, " ", $classmethod(u, "St", ..DropIPMPkgMap(ns))
        write " mapUplift ", $classmethod(u, "St", ..MapUplift(ns))
        write " -> ", $classmethod(u, "Maps", ns), " hasIPM:", $classmethod(u, "HasIPM", ns)
    }
}

/// Does %Installer's ClassMapping re-point an existing %IPM package mapping? NCX is created by
/// %IPM.Test.Utils.CreateNamespace, which maps package %IPM (only) to USER's database; then the installer runs in NCX.
ClassMethod RepointProbe()
{
    set u = ..U()
    new $namespace
    set sc = $classmethod(u, "MkNs", "NCX")
    set kv("mkNs") = $classmethod(u, "St", sc)
    set kv("mapUplift") = $classmethod(u, "St", ..MapUplift("NCX"))
    set kv("ncxMapsBefore") = $classmethod(u, "Maps", "NCX")
    set kv("ncxPkgDestBefore") = $classmethod(u, "PkgDest", "NCX")
    set kv("cpfBefore") = ..CpfMap("NCX")
    set kv("userMainTimeBefore") = ..CompiledTime("USER", "%IPM.Main")
    set $namespace = "NCX"
    set sc = ..InstallHere()
    set kv("installerStatus") = $classmethod(u, "St", sc)
    set kv("ncxZpmRecord") = $classmethod(u, "Mod", "zpm", "NCX")
    set $namespace = "USER"
    set kv("ncxMapsAfter") = $classmethod(u, "Maps", "NCX")
    set kv("ncxPkgDestAfter") = $classmethod(u, "PkgDest", "NCX")
    set kv("ncxRtnDestAfter") = ..RtnDest("NCX", "%IPM.Main.1")
    set kv("cpfAfter") = ..CpfMap("NCX")
    set kv("userMainTimeAfter") = ..CompiledTime("USER", "%IPM.Main")
    set kv("ncxMainTimeAfter") = ..CompiledTime("NCX", "%IPM.Main")
    set kv("ncxCodeDB") = $classmethod(u, "CodeDB", "NCX")
    do $classmethod(u, "Emit", "nc-nsp-installer-repoint", .kv)
}

ClassMethod CompiledTime(ns As %String, cls As %String) As %String
{
    new $namespace
    set $namespace = ns
    set c = ##class(%Dictionary.CompiledClass).%OpenId(cls)
    quit $select($isobject(c):c.TimeChanged, 1:"<none>")
}

ClassMethod RtnDest(ns As %String, rtn As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(%SYS.Namespace).GetRoutineDest(ns, rtn)
}

/// The [Map.<ns>] lines of iris.cpf that mention IPM, joined.
ClassMethod CpfMap(ns As %String) As %String
{
    set f = ##class(%Stream.FileCharacter).%New()
    do f.LinkToFile($system.Util.InstallDirectory()_"iris.cpf")
    set in = 0, out = ""
    while 'f.AtEnd {
        set l = f.ReadLine()
        if $extract(l) = "[" { set in = (l = ("[Map."_ns_"]")) continue }
        if in && (l [ "IPM") { set out = out_$select(out="":"", 1:" , ")_$translate(l, "=", ":") }
    }
    quit $select(out="":"<none>", 1:out)
}

ClassMethod ClassDest(ns As %String, cls As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(%SYS.Namespace).GetPackageDest(ns, $piece(cls, ".", 1, *-1))
}

ClassMethod DropIPMPkgMap(ns As %String) As %Status
{
    new $namespace
    set $namespace = "%SYS"
    quit:'##class(Config.MapPackages).Exists(ns, "%IPM") $$$OK
    set sc = ##class(Config.MapPackages).Delete(ns, "%IPM")
    quit:'$system.Status.IsOK(sc) sc
    quit ##class(Config.CPF).Activate()
}

ClassMethod MapUplift(ns As %String) As %Status
{
    new $namespace
    set $namespace = "%SYS"
    quit:##class(Config.MapPackages).Exists(ns, "Uplift") $$$OK
    set p("Database") = "USER"
    set sc = ##class(Config.MapPackages).Create(ns, "Uplift", .p)
    quit:'$system.Status.IsOK(sc) sc
    quit ##class(Config.CPF).Activate()
}

/// Run in NCC: give NCC its own local install (as the README says: load the installer, run setup).
ClassMethod InstallHere() As %Status
{
    set sc = $system.OBJ.Load("/home/irisowner/zpm/preload/cls/IPM/Installer.cls", "ck")
    quit:'$system.Status.IsOK(sc) sc
    quit $classmethod("IPM.Installer", "setup", "/home/irisowner/zpm/", 3)
}

/// N2 (NSP-B18, B20, B29): enable -map -ns NCA,NCB,NCC from USER, where NCB is already mapped from NCC.
ClassMethod N2()
{
    set u = ..U()
    new $namespace
    set kv("userDB") = $classmethod(u, "CodeDB", "USER")
    set kv("nccDB") = $classmethod(u, "CodeDB", "NCC")
    set kv("nccOwnInstall") = $classmethod(u, "Mod", "zpm", "NCC")
    // NCB gets IPM by mapping from NCC first
    set $namespace = "NCC"
    set sc = $classmethod(u, "Run", "enable -map -ns NCB", .o0)
    set kv("prepNCBfromNCC") = $classmethod(u, "St", sc)
    set $namespace = "USER"
    set kv("ncaDestBefore") = $classmethod(u, "PkgDest", "NCA")
    set kv("ncbDestBefore") = $classmethod(u, "PkgDest", "NCB")
    set sc = $classmethod(u, "Run", "enable -map -ns NCA,NCB,NCC", .out)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("ncaDestAfter") = $classmethod(u, "PkgDest", "NCA")
    set kv("ncbDestAfter") = $classmethod(u, "PkgDest", "NCB")
    set kv("nccDestAfter") = $classmethod(u, "PkgDest", "NCC")
    set kv("ncaMaps") = $classmethod(u, "Maps", "NCA")
    set kv("ncbMaps") = $classmethod(u, "Maps", "NCB")
    set kv("printedMappingNCA") = $classmethod(u, "Has", .out, "Mapping %IPM package in USER equivalently to NCA")
    set kv("printedMappingNCB") = $classmethod(u, "Has", .out, "Mapping %IPM package in USER equivalently to NCB")
    set kv("printedSkipNCC") = $classmethod(u, "Line", .out, "Skipping IPM mapping of NCC")
    set kv("ncaHasIPM") = $classmethod(u, "HasIPM", "NCA")
    set kv("ncbStillOnNCC") = (kv("ncbDestAfter") = kv("ncbDestBefore"))
    do $classmethod(u, "Emit", "nc-nsp-N2-enable-map", .kv)
}

/// N3 (NSP-B23..B25): NCA has its own repo; NCB is mapped from NCC; NCD, NCE are empty.
ClassMethod N3()
{
    set u = ..U()
    new $namespace
    set $namespace = "NCA"
    set sc = $classmethod(u, "Run", "repo -fs -n nc-nsp-own -path /tmp/", .o0)
    set kv("ncaOwnRepo") = $classmethod(u, "St", sc)_" "_$classmethod(u, "RepoNames", "NCA")
    set $namespace = "USER"
    set sc = $classmethod(u, "Run", "repo -fs -n nc-nsp-user -path /tmp/", .o1)
    set sc = $classmethod(u, "Run", "enable -map -repos -ns NCA,NCB,NCD,NCE", .out)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("skipNCA") = $classmethod(u, "Line", .out, "repository mapping of NCA")
    set kv("skipNCB") = $classmethod(u, "Line", .out, "repository mapping of NCB")
    set kv("lineNCD") = $classmethod(u, "Line", .out, "repository mapping of NCD")
    set kv("ncaMaps") = $classmethod(u, "Maps", "NCA")
    set kv("ncbMaps") = $classmethod(u, "Maps", "NCB")
    set kv("ncdMaps") = $classmethod(u, "Maps", "NCD")
    set kv("ncaRepos") = $classmethod(u, "RepoNames", "NCA")
    set kv("ncdRepos") = $classmethod(u, "RepoNames", "NCD")
    do $classmethod(u, "Emit", "nc-nsp-N3-enable-map-repos", .kv)
    // Second run: does a namespace whose code mapping was just added get the repo mapping now?
    kill kv
    set sc = $classmethod(u, "Run", "enable -map -repos -ns NCD,NCE", .out2)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("lineNCD") = $classmethod(u, "Line", .out2, "repository mapping of NCD")
    set kv("ncdMaps") = $classmethod(u, "Maps", "NCD")
    set kv("nceMaps") = $classmethod(u, "Maps", "NCE")
    set kv("ncdRepos") = $classmethod(u, "RepoNames", "NCD")
    set kv("ncdRepoGlobalDest") = $classmethod(u, "GlobalDest", "NCD", "IPM.Repo.DefinitionD")
    do $classmethod(u, "Emit", "nc-nsp-N3-enable-map-repos-rerun", .kv)
}

/// N9 (NSP-B72, F-NSP-2): IPMVersion need in a mapped namespace (NCA) vs the source (USER).
ClassMethod N9()
{
    set u = ..U()
    new $namespace
    set dir = ..#FIX_"nc-version-req/"
    set $namespace = "USER"
    set sc = $classmethod(u, "Run", "load "_dir, .o1)
    set kv("userLoad") = $classmethod(u, "St", sc)
    set kv("userRecord") = $classmethod(u, "Mod", "nc-version-req", "USER")
    do $classmethod(u, "Run", "uninstall nc-version-req", .o2)
    set $namespace = "NCA"
    set kv("ncaPkgDest") = $classmethod(u, "PkgDest", "NCA")
    set kv("ncaZpmRecord") = $classmethod(u, "Mod", "zpm", "NCA")
    set sc = $classmethod(u, "Run", "load "_dir, .o3)
    set kv("ncaLoad") = $classmethod(u, "St", sc)
    set kv("ncaRecord") = $classmethod(u, "Mod", "nc-version-req", "NCA")
    set kv("ncaErrLine") = $classmethod(u, "Line", .o3, "Current IPM version")
    do $classmethod(u, "Run", "uninstall nc-version-req -force", .o4)
    do $classmethod(u, "Emit", "nc-nsp-N9-ipmversion-mapped", .kv)
}

/// N9 with a need the source's 0.10.11-SNAPSHOT satisfies (">=0.9" is not met by a prerelease under IPM's rules).
ClassMethod N9b()
{
    set u = ..U()
    new $namespace
    set dir = ..#FIX_"nc-version-req-snap/"
    set $namespace = "USER"
    set sc = $classmethod(u, "Run", "load "_dir, .o1)
    set kv("userLoad") = $classmethod(u, "St", sc)
    set kv("userRecord") = $classmethod(u, "Mod", "nc-version-req-snap", "USER")
    do $classmethod(u, "Run", "uninstall nc-version-req-snap", .o2)
    set $namespace = "NCA"
    set kv("ncaPkgDest") = $classmethod(u, "PkgDest", "NCA")
    set kv("ncaZpmRecord") = $classmethod(u, "Mod", "zpm", "NCA")
    set sc = $classmethod(u, "Run", "load "_dir, .o3)
    set kv("ncaLoad") = $classmethod(u, "St", sc)
    set kv("ncaRecord") = $classmethod(u, "Mod", "nc-version-req-snap", "NCA")
    if kv("ncaRecord") '= "-" { do $classmethod(u, "Run", "uninstall nc-version-req-snap -force", .o4) }
    do $classmethod(u, "Emit", "nc-nsp-N9-ipmversion-mapped-snap", .kv)
}

/// N7 (NSP-B47, B49, DEC-NSP-1, DEC-NSP-2) in NCD (code and repo settings mapped from USER):
/// install zpm, then run the installer, then unmap from USER.
ClassMethod N7()
{
    set u = ..U()
    new $namespace
    set kv("userDB") = $classmethod(u, "CodeDB", "USER")
    set kv("ncdDB") = $classmethod(u, "CodeDB", "NCD")
    set kv("ncdMapsBefore") = $classmethod(u, "Maps", "NCD")
    set kv("ncdDestBefore") = $classmethod(u, "PkgDest", "NCD")
    set $namespace = "NCD"
    set sc = $classmethod(u, "Run", "install zpm", .o1)
    set kv("installZpm") = $classmethod(u, "St", sc)
    set kv("zpmRecordAfterInstall") = $classmethod(u, "Mod", "zpm", "NCD")
    do $classmethod(u, "Emit", "nc-nsp-N7-install-zpm-mapped", .kv)
    kill kv
    set t0 = $zh
    set sc = ..InstallHere()
    set kv("installerStatus") = $classmethod(u, "St", sc)
    set kv("installerSeconds") = $justify($zh - t0, 0, 1)
    set kv("ncdDB") = $classmethod(u, "CodeDB", "NCD")
    set kv("ncdMapsAfter") = $classmethod(u, "Maps", "NCD")
    set kv("ncdDestAfter") = $classmethod(u, "PkgDest", "NCD")
    set kv("ncdRepoGlobalDestAfter") = $classmethod(u, "GlobalDest", "NCD", "IPM.Repo.DefinitionD")
    set kv("ncdZpmRecord") = $classmethod(u, "Mod", "zpm", "NCD")
    set kv("userZpmRecord") = $classmethod(u, "Mod", "zpm", "USER")
    set kv("userHasIPM") = $classmethod(u, "HasIPM", "USER")
    set kv("ncdRepos") = $classmethod(u, "RepoNames", "NCD")
    do $classmethod(u, "Emit", "nc-nsp-N7-installer-in-mapped", .kv)
    kill kv
    set $namespace = "USER"
    set sc = $classmethod(u, "Run", "unmap -ns NCD", .o2)
    set kv("unmapStatus") = $classmethod(u, "St", sc)
    set kv("unmapLine") = $classmethod(u, "Line", .o2, "NCD")
    set kv("noMappingFound") = $classmethod(u, "Has", .o2, "No mapping found for NCD")
    set kv("ncdMapsAfterUnmap") = $classmethod(u, "Maps", "NCD")
    do $classmethod(u, "Emit", "nc-nsp-N7-unmap-after-local-install", .kv)
}

/// N6 (NSP-B41..B44): unmap NCE (code and repo mapped), NCA (code mapped, own repo), then USER (source).
ClassMethod N6()
{
    set u = ..U()
    new $namespace
    set $namespace = "USER"
    set kv("nceMapsBefore") = $classmethod(u, "Maps", "NCE")
    set kv("ncaMapsBefore") = $classmethod(u, "Maps", "NCA")
    set sc = $classmethod(u, "Run", "unmap -ns NCE,NCA", .o1)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("nceMapsAfter") = $classmethod(u, "Maps", "NCE")
    set kv("ncaMapsAfter") = $classmethod(u, "Maps", "NCA")
    set kv("nceHasIPM") = $classmethod(u, "HasIPM", "NCE")
    set kv("ncaHasIPM") = $classmethod(u, "HasIPM", "NCA")
    set kv("ncaOwnRepoGlobalStill") = $classmethod(u, "GlobalDest", "NCA", "IPM.Repo.DefinitionD")
    do $classmethod(u, "Emit", "nc-nsp-N6-unmap-mapped", .kv)
    kill kv
    set kv("userMapsBefore") = $classmethod(u, "Maps", "USER")
    set sc = $classmethod(u, "Run", "unmap -ns USER", .o2)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("noMappingFound") = $classmethod(u, "Has", .o2, "No mapping found for USER")
    set kv("userMapsAfter") = $classmethod(u, "Maps", "USER")
    set kv("userHasIPM") = $classmethod(u, "HasIPM", "USER")
    do $classmethod(u, "Emit", "nc-nsp-N6-unmap-source", .kv)
}

/// N8 (NSP-B62, B66, DEC-NSP-5): load -dev of a module that needs %SYS, in USER; plain load as control.
ClassMethod N8()
{
    set u = ..U()
    set dir = ..#DATA_"sys-ns-module/"
    set sc = $classmethod(u, "Run", "load "_dir, .o1)
    set kv("plainLoad") = $classmethod(u, "St", sc)
    set kv("plainRecord") = $classmethod(u, "Mod", "sys-ns-module")
    if kv("plainRecord") '= "-" { do $classmethod(u, "Run", "uninstall sys-ns-module -force", .o0) }
    set sc = $classmethod(u, "Run", "load -dev "_dir, .o2)
    set kv("devLoad") = $classmethod(u, "St", sc)
    set kv("devRecord") = $classmethod(u, "Mod", "sys-ns-module")
    set sc = $classmethod(u, "Run", "list", .o3)
    set kv("listedAfterDev") = $classmethod(u, "Has", .o3, "sys-ns-module")
    set kv("sysClass") = $classmethod(u, "ClassState", "SysNS.module")
    do $classmethod(u, "Emit", "nc-nsp-N8-sysns-dev", .kv)
    do $classmethod(u, "Run", "uninstall sys-ns-module -force", .o4)
}

/// N10 (NSP-B57, F-NSP-3): a module named zpm-registry with a +build version, then `version` and `info`.
ClassMethod N10()
{
    set u = ..U()
    set sc = $classmethod(u, "Run", "load "_..#FIX_"nc-zpm-registry/", .o1)
    set kv("load") = $classmethod(u, "St", sc)
    set kv("record") = $classmethod(u, "Mod", "zpm-registry")
    set sc = $classmethod(u, "Run", "version", .o2)
    set kv("versionStatus") = $classmethod(u, "St", sc)
    set kv("versionLine") = $classmethod(u, "Line", .o2, "zpm-registry")
    set kv("versionShowsBuild") = $classmethod(u, "Has", .o2, "+ncbuild")
    set sc = $classmethod(u, "Run", "info", .o3)
    set kv("infoShowsBuild") = $classmethod(u, "Has", .o3, "+ncbuild")
    set kv("infoLine") = $classmethod(u, "Line", .o3, "zpm-registry")
    do $classmethod(u, "Emit", "nc-nsp-N10-version-build", .kv)
    do $classmethod(u, "Run", "uninstall zpm-registry -force", .o4)
}

/// N5 (NSP-B40, F-NSP-1): enable -ns NCT from a registry stub whose installer does not compile.
ClassMethod N5()
{
    set u = ..U()
    if ##class(%IPM.Repo.Definition).ServerDefinitionKeyExists("nc-nsp-stub") {
        do ##class(%IPM.Repo.Definition).ServerDefinitionKeyDelete("nc-nsp-stub")
    }
    set d = ##class(%IPM.Repo.Remote.Definition).%New()
    set d.Name = "nc-nsp-stub", d.URL = "http://127.0.0.1:8765/", d.Enabled = 1
    set kv("stubSaved") = $classmethod(u, "St", d.%Save())
    // the stub serves the installer for 9.9.9; check it is reachable
    set r = ##class(%Net.HttpRequest).%New(), r.Server = "127.0.0.1", r.Port = 8765
    set sc = r.Get("/packages/zpm/9.9.9/installer")
    set kv("stubHttp") = $select($system.Status.IsOK(sc):r.HttpResponse.StatusCode, 1:"err")
    set kv("nctHasIPMBefore") = $classmethod(u, "HasIPM", "NCT")
    // a first attempt (log 84-nsp-namespace2) stopped with FRAMESTACK under IPM's own output capture; start NCT clean
    new $namespace
    set $namespace = "NCT"
    do $system.OBJ.Delete("NcBroken.Installer", "-d")
    set kv("nctBrokenClassBefore") = $classmethod(u, "ClassState", "NcBroken.Installer")
    set $namespace = "USER"
    set sc = $classmethod(u, "Run", "enable -ns NCT -v 9.9.9 -remote nc-nsp-stub", .out)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("enabledLine") = $classmethod(u, "Line", .out, "IPM enabled for namespace")
    set kv("compileErrorShown") = $classmethod(u, "Has", .out, "ERROR")
    set kv("nctHasIPMAfter") = $classmethod(u, "HasIPM", "NCT")
    set $namespace = "NCT"
    set kv("nctBrokenClass") = $classmethod(u, "ClassState", "NcBroken.Installer")
    set $namespace = "USER"
    do $classmethod(u, "Emit", "nc-nsp-N5-enable-remote-broken", .kv)
}

/// Lock holder for N4 (run with JOB): holds ^IPM.Repo.DefinitionD until released or 120 s.
ClassMethod HoldLock()
{
    lock +^IPM.Repo.DefinitionD:5
    set ^IRIS.Temp.NcLock = $test
    for i=1:1:120 { quit:$get(^IRIS.Temp.NcRelease)  hang 1 }
    lock -^IPM.Repo.DefinitionD
    set ^IRIS.Temp.NcLock = "released"
}

/// N4 (NSP-B33, DEC-NSP-4): enable -community while another process holds the repo extent lock.
ClassMethod N4()
{
    set u = ..U()
    kill ^IRIS.Temp.NcLock, ^IRIS.Temp.NcRelease
    set kv("reposBefore") = $classmethod(u, "RepoNames", "USER")
    set kv("defaultRegistry") = ##class(%IPM.Repo.UniversalSettings).GetDefaultRegistry()
    job ##class(Uplift.Nsp.Ns).HoldLock()
    for i=1:1:20 { quit:$data(^IRIS.Temp.NcLock)  hang 0.25 }
    set kv("lockHeld") = $get(^IRIS.Temp.NcLock, "?")
    set t0 = $zh
    set sc = $classmethod(u, "Run", "enable -community", .out)
    set kv("seconds") = $justify($zh - t0, 0, 1)
    set ^IRIS.Temp.NcRelease = 1
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("deleteAllLine") = $classmethod(u, "Line", .out, "repo -delete-all")
    set kv("errorLine") = $classmethod(u, "Line", .out, "Error running command")
    set kv("resetLine") = $classmethod(u, "Line", .out, "Command ""repo -reset-defaults""")
    set kv("mapLine") = $classmethod(u, "Line", .out, "Command ""enable -map -repos -globally""")
    set kv("reposAfter") = $classmethod(u, "RepoNames", "USER")
    set kv("nctMaps") = $classmethod(u, "Maps", "NCT")
    set kv("sysMaps") = $classmethod(u, "Maps", "%SYS")
    do $classmethod(u, "Emit", "nc-nsp-N4-community-step-fails", .kv)
    hang 2
}

/// Undo mappings made by N4 directly in Config (unmap leaves the routine mapping; see N6).
ClassMethod CleanupMaps()
{
    new $namespace
    set $namespace = "%SYS"
    for ns = "%SYS", "NCA", "NCB", "NCE", "NCT" {
        do:##class(Config.MapPackages).Exists(ns, "%IPM") ##class(Config.MapPackages).Delete(ns, "%IPM")
        do:##class(Config.MapRoutines).Exists(ns, "%IPM.*") ##class(Config.MapRoutines).Delete(ns, "%IPM.*")
        for g = "D", "S", "I" { do:##class(Config.MapGlobals).Exists(ns, "IPM.Repo.Definition"_g) ##class(Config.MapGlobals).Delete(ns, "IPM.Repo.Definition"_g) }
    }
    write !, "Activate ", ##class(Config.CPF).Activate()
    set $namespace = "USER"
    for ns = "%SYS", "NCA", "NCB", "NCE", "NCT" { write !, ns, " ", ##class(Uplift.Nsp.Util).Maps(ns), " hasIPM:", ##class(Uplift.Nsp.Util).HasIPM(ns) }
}

/// MigrateRepoDefinitions on its success path (no legacy repo rows): what does it return?
ClassMethod MigrateRepoDefs()
{
    set u = ..U()
    set kv("legacyRows") = $data(^%ZPM.Client.ServerDefD)
    set kv("tlevelBefore") = $tlevel
    try {
        set sc = ##class(%IPM.Utils.Migration).MigrateRepoDefinitions(0)
        set kv("returned") = $select($data(sc)#2=0:"<nothing>", sc="":"<empty>", 1:$classmethod(u, "St", sc))
    } catch e {
        set kv("threw") = e.DisplayString()
    }
    set kv("tlevelAfter") = $tlevel
    do $classmethod(u, "Emit", "nc-nsp-migrate-repo-defs-success", .kv)
}

}
