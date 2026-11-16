/// Round-4 intent probes for module 07 (namespace).
/// Collation: U24 / F-NSP-8. Does MapGlobalEquivalently(..., 1), whose 1 lands in AddGlobalMapping's
/// set-collation slot, change the collation of the new global mapping or of globals created through it?
/// Legacy: DEC-NSP-3. Does enable -map migrate legacy ZPM module records of the target?
/// Legacy ZPM is simulated in scratch namespace NCR4L only: a %ZPM.PackageManager class compiled into
/// NCR4L's own database through a namespace-local %ZPM package mapping, and one ^ZPM.Dev.ModuleD record.
/// Real ZPM 0.7.4 was not loaded: it installs %ZPM classes and the zpm language extension server-wide.
/// Scratch namespaces NCR4S, NCR4T, NCR4U, NCR4L (created here).
Class Uplift.R4Nsp.Probe
{

ClassMethod U() As %String [ CodeMode = expression ]
{
"Uplift.Nsp.Util"
}

/// Config.MapGlobals Collation of one mapping ("<none>" when absent, "" when not set).
ClassMethod MapColl(ns As %String, gbl As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    quit:'##class(Config.MapGlobals).Exists(ns, gbl) "<none>"
    kill p
    do ##class(Config.MapGlobals).Get(ns, gbl, .p)
    quit "db:"_$get(p("Database"))_" coll:"_$get(p("Collation"),"<unset>")
}

/// Collation of an existing global in a database directory.
ClassMethod GlobColl(gbl As %String, dir As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    try {
        set g = ##class(%GlobalEdit).Open(gbl, dir, , , .sc)
        if '$isobject(g) { return "err:"_$system.Status.GetErrorText($get(sc)) }
        return g.Collation
    } catch e {
        return "exc:"_e.DisplayString()
    }
}

ClassMethod DbDir(name As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    kill p
    do ##class(Config.Databases).Get(name, .p)
    quit $get(p("Directory"))
}

ClassMethod NewColl(dir As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    set db = ##class(SYS.Database).%OpenId(dir)
    quit:'$isobject(db) "<no-db>"
    quit db.NewGlobalCollation
}

ClassMethod SetNewColl(dir As %String, coll As %Integer) As %String
{
    new $namespace
    set $namespace = "%SYS"
    try {
        set db = ##class(SYS.Database).%OpenId(dir)
        set db.NewGlobalCollation = coll
        set sc = db.%Save()
        return $select($system.Status.IsOK(sc):"ok", 1:"err:"_$system.Status.GetErrorText(sc))
    } catch e {
        return "exc:"_e.DisplayString()
    }
}

/// Path that AddGlobalMapping's collation branch opens for database name db.
ClassMethod BranchPath(db As %String) As %String
{
    quit ##class(%Library.File).NormalizeDirectory($system.Util.InstallDirectory()_"mgr")_db
}

ClassMethod BranchOpens(db As %String) As %Boolean
{
    new $namespace
    set $namespace = "%SYS"
    quit $isobject(##class(SYS.Database).%OpenId(..BranchPath(db)))
}

/// Run 1 (log a4-nsp-collation) used NCR4S/T/U with the database collation set during global creation.
/// Run 2 (this version, NCR4S2/T2/U2): the mappings are made while the source database's new-global collation
/// is 133, then it is put back to 5 before globals are created through the mappings, so a collation
/// carried by the mapping itself can be told apart from the database default.
ClassMethod Collation(sfx As %String = "2")
{
    set u = ..U()
    new $namespace
    set $namespace = "USER"
    set S = "NCR4S"_sfx, T = "NCR4T"_sfx, V = "NCR4U"_sfx
    for ns = S, T, V { set kv("mk"_ns) = $classmethod(u, "St", ##class(Uplift.Nsp.Model).Clean(ns)) }
    set srcDir = ..DbDir(S)
    set kv("srcDir") = srcDir
    set kv("userDir") = ..DbDir("USER")
    set kv("branchPathUSER") = ..BranchPath("USER")
    set kv("branchOpensUSER") = ..BranchOpens("USER")
    set kv("branchOpensSrc") = ..BranchOpens(S)
    set kv("branchOpensNCC") = ..BranchOpens("NCC")
    set kv("userNewColl") = ..NewColl(kv("userDir"))
    set kv("srcNewCollBefore") = ..NewColl(srcDir)
    set kv("set133") = ..SetNewColl(srcDir, 133)
    set kv("srcNewCollWhileMapping") = ..NewColl(srcDir)
    // A: the exact IPM call, source USER (repo globals as in enable -map -repos)
    set kv("A_call") = $classmethod(u, "St", ##class(%IPM.Utils.Build).MapGlobalEquivalently("IPM.Repo.DefinitionD", "USER", T, 1))
    set kv("A_map") = ..MapColl(T, "IPM.Repo.DefinitionD")
    // C: exact IPM call shape, source S (database made by CreateNamespace: upper-case directory)
    set kv("C_call") = $classmethod(u, "St", ##class(%IPM.Utils.Build).MapGlobalEquivalently("NcR4C", S, T, 1))
    set kv("C_map") = ..MapColl(T, "NcR4C")
    // D: control, same database, set-collation 0 (the intended call)
    set kv("D_call") = $classmethod(u, "St", ##class(%IPM.Utils.Module).AddGlobalMapping(V, "NcR4D", S, 0, 0))
    set kv("D_map") = ..MapColl(V, "NcR4D")
    set kv("set5") = ..SetNewColl(srcDir, 5)
    set kv("srcNewCollWhileCreating") = ..NewColl(srcDir)
    // Create globals through the mappings (new processes see the activated mappings; this one too)
    set $namespace = T
    set ^NcR4C(1) = 1
    set $namespace = V
    set ^NcR4D(1) = 1
    set $namespace = S
    set ^NcR4E(1) = 1
    set $namespace = "USER"
    set kv("C_globDest") = $classmethod(u, "GlobalDest", T, "NcR4C")
    set kv("C_globColl") = ..GlobColl("NcR4C", srcDir)
    set kv("D_globColl") = ..GlobColl("NcR4D", srcDir)
    set kv("E_globColl") = ..GlobColl("NcR4E", srcDir)
    do $classmethod(u, "Emit", "r4-nsp-collation-slot-v2", .kv)
}

/// Same test as %IPM.Utils.Migration:HasLegacyZPMPackage (Migration.cls#L38-L42), run in ns.
ClassMethod HasLegacy(ns As %String) As %String
{
    new $namespace
    set $namespace = ns
    set old = "%ZPM.PackageManager."
    quit $order(^oddCOM(old)) [ old
}

/// Legacy record count and IPM record of the probe module in ns.
ClassMethod Records(ns As %String, prefix As %String, ByRef kv)
{
    set kv(prefix_"LegacyRows") = ..Count(ns, "ZPM.Dev.ModuleD")
    set kv(prefix_"IpmRows") = ..Count(ns, "IPM.Storage.ModuleD")
    set kv(prefix_"ProbeMod") = $classmethod(..U(), "Mod", "legacy-probe", ns)
}

ClassMethod Count(ns As %String, gbl As %String) As %Integer
{
    set n = 0, ref = "^|"""_ns_"""|"_gbl, id = ""
    for { set id = $order(@ref@(id)) quit:id=""  set n = n + 1 }
    quit n
}

ClassMethod Legacy()
{
    set u = ..U()
    new $namespace
    set $namespace = "USER"
    set kv("legacyUSERBefore") = ..HasLegacy("USER")
    set kv("mk") = $classmethod(u, "St", ##class(Uplift.Nsp.Model).Clean("NCR4L"))
    set kv("hasIPMBefore") = $classmethod(u, "HasIPM", "NCR4L")
    // Namespace-local %ZPM package mapping, so the fake legacy class lives in NCR4L's database only
    set kv("mapZPM") = $classmethod(u, "St", ##class(%IPM.Utils.Module).AddPackageMapping("NCR4L", "%ZPM", "NCR4L", 0))
    set $namespace = "NCR4L"
    set cd = ##class(%Dictionary.ClassDefinition).%New()
    set cd.Name = "%ZPM.PackageManager.NcR4Probe"
    set cd.Abstract = 1
    set scSave = cd.%Save()
    set scCompile = $system.OBJ.Compile("%ZPM.PackageManager.NcR4Probe", "ck-d")
    // One legacy module record. Layout copied from USER's own zpm record (IPM's storage is compatible
    // with the legacy one, Migration.cls#L119), renamed.
    kill ^ZPM.Dev.ModuleD
    merge ^ZPM.Dev.ModuleD(1) = ^|"USER"|IPM.Storage.ModuleD(1)
    set $list(^ZPM.Dev.ModuleD(1), 2) = "legacy-probe"
    set ^ZPM.Dev.ModuleD = 1
    set $namespace = "USER"
    set kv("saveCls") = $classmethod(u, "St", scSave)
    set kv("compileCls") = $classmethod(u, "St", scCompile)
    set kv("legacyNCR4L") = ..HasLegacy("NCR4L")
    set kv("legacyUSERAfterSetup") = ..HasLegacy("USER")
    set kv("legacyNCA") = ..HasLegacy("NCA")
    do ..Records("NCR4L", "before", .kv)
    do $classmethod(u, "Emit", "r4-nsp-legacy-setup", .kv)
    kill kv
    if (..HasLegacy("USER") '= 0) {
        write !, "ABORT: fake legacy class is visible in USER; cleaning up", !
        do ..LegacyCleanup()
        quit
    }
    // The command under test
    set sc = $classmethod(u, "Run", "enable -map -ns NCR4L", .out)
    set kv("cmdStatus") = $classmethod(u, "St", sc)
    set kv("migrateLine") = $classmethod(u, "Has", .out, "Migrat")
    set kv("mapLine") = $classmethod(u, "Line", .out, "equivalently to NCR4L")
    set kv("hasIPMAfter") = $classmethod(u, "HasIPM", "NCR4L")
    set kv("maps") = $classmethod(u, "Maps", "NCR4L")
    set kv("legacyNCR4LAfter") = ..HasLegacy("NCR4L")
    do ..Records("NCR4L", "after", .kv)
    set $namespace = "NCR4L"
    set sc = ##class(%IPM.Main).Shell("list")
    set $namespace = "USER"
    do $classmethod(u, "Emit", "r4-nsp-legacy-enable-map", .kv)
    kill kv
    // What the load path would have run in that namespace (RunAll runs this per namespace):
    // MigrateZPMToIPM, run in NCR4L only. Shows the record is reachable by the migration code.
    set $namespace = "NCR4L"
    try {
        do ##class(%IPM.Utils.Migration).MigrateZPMToIPM(1)
        set kv("migrateCall") = "ok"
    } catch e {
        set kv("migrateCall") = "err:"_$extract(e.DisplayString(), 1, 160)
    }
    set $namespace = "USER"
    do ..Records("NCR4L", "afterMigrate", .kv)
    do $classmethod(u, "Emit", "r4-nsp-legacy-migrate-direct", .kv)
    kill kv
    do ..LegacyCleanup()
}

/// Remove the fake legacy class, its mapping and the legacy record, and the IPM mappings, from NCR4L.
ClassMethod LegacyCleanup()
{
    set u = ..U()
    new $namespace
    set $namespace = "NCR4L"
    set scDel = $system.OBJ.Delete("%ZPM.PackageManager.NcR4Probe", "-d")
    kill ^ZPM.Dev.ModuleD
    set $namespace = "USER"
    set kv("delCls") = $classmethod(u, "St", scDel)
    set kv("unmapZPM") = $classmethod(u, "St", ##class(%IPM.Utils.Module).RemovePackageMapping("NCR4L", "%ZPM", 0))
    set kv("cleanIPM") = $classmethod(u, "St", ##class(Uplift.Nsp.Model).Clean("NCR4L"))
    set kv("legacyNCR4L") = ..HasLegacy("NCR4L")
    set kv("legacyUSER") = ..HasLegacy("USER")
    set kv("userShell") = ''$system.CLS.IsMthd("%IPM.Main", "Shell")
    set kv("userZpm") = $classmethod(u, "Mod", "zpm", "USER")
    do $classmethod(u, "Emit", "r4-nsp-legacy-cleanup", .kv)
}

/// DEC-NSP-2: the supported route to a local IPM in a mapped namespace is unmap, then install (PR #485).
/// After unmap (F-NSP-6 leaves the %IPM.* routine mapping), does install zpm's guard still refuse?
/// Reads CheckModuleNamespace() in the target only; nothing is installed. Scratch namespace NCR4P.
ClassMethod UnmapRoute()
{
    set u = ..U()
    new $namespace
    set $namespace = "USER"
    set kv("mk") = $classmethod(u, "St", ##class(Uplift.Nsp.Model).Clean("NCR4P"))
    set sc = $classmethod(u, "Run", "enable -map -ns NCR4P", .out)
    set kv("enableStatus") = $classmethod(u, "St", sc)
    set kv("mapsAfterEnable") = $classmethod(u, "Maps", "NCR4P")
    set sc = $classmethod(u, "Run", "unmap -ns NCR4P", .out)
    set kv("unmapStatus") = $classmethod(u, "St", sc)
    set kv("mapsAfterUnmap") = $classmethod(u, "Maps", "NCR4P")
    set kv("hasIPMAfterUnmap") = $classmethod(u, "HasIPM", "NCR4P")
    set $namespace = "NCR4P"
    set callable = ''$system.CLS.IsMthd("%IPM.Main", "CheckModuleNamespace")
    set scGuard = $select(callable:##class(%IPM.Main).CheckModuleNamespace(), 1:"")
    set $namespace = "USER"
    set kv("guardCallable") = callable
    set kv("guardAfterUnmap") = $select(callable:$classmethod(u, "St", scGuard), 1:"<not-callable>")
    set kv("cleanup") = $classmethod(u, "St", ##class(Uplift.Nsp.Model).Clean("NCR4P"))
    set kv("mapsAfterCleanup") = $classmethod(u, "Maps", "NCR4P")
    do $classmethod(u, "Emit", "r4-nsp-unmap-then-install-guard", .kv)
}

}
