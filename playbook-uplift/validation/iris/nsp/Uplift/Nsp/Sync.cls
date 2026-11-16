/// Native checks N18-N22 for module 09 (developer sync). Run in USER. Fixtures are copied to /tmp first.
Class Uplift.Nsp.Sync
{

Parameter DATA = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/";

Parameter DIR = "/tmp/nc-sync/";

Parameter SUPER = "src/cls/SyncTest/SuperClass.cls";

ClassMethod U() As %String [ CodeMode = expression ]
{
"Uplift.Nsp.Util"
}

/// Uninstall sync fixtures and put a fresh copy of sync-test in DIR.
ClassMethod Fresh()
{
    set u = ..U()
    for m = "sync-test", "sync-dep-c" {
        if $classmethod(u, "Mod", m) '= "-" { do $classmethod(u, "Run", "uninstall "_m_" -force", .o, 0) }
    }
    for c = "SyncTest.SuperClass", "SyncTest.SubClass", "SyncTest.Consumer", "SyncDepC.Base" {
        do:##class(%Dictionary.ClassDefinition).%ExistsId(c) $system.OBJ.Delete(c, "-d")
    }
    do ##class(%IPM.Storage.FileHash).%KillExtent()
    do $classmethod(u, "CopyDir", ..#DATA_"sync-test/", ..#DIR)
}

ClassMethod Super() As %String
{
    set c = ##class(%Dictionary.ClassDefinition).%OpenId("SyncTest.SuperClass")
    quit $select($isobject(c):$select(c.Super="":"<none>", 1:c.Super), 1:"<no-class>")
}

ClassMethod Value() As %String
{
    set v = "<error>"
    try { set v = $classmethod("SyncTest.SuperClass", "GetValue") } catch e { set v = "<"_e.Name_">" }
    quit v
}

ClassMethod Row() As %String
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Hash, SyncFailed, TestPending FROM %IPM_Storage.FileHash WHERE ModuleName = 'sync-test' AND RelativePath LIKE '%SuperClass.cls'")
    quit:'rs.%Next() "<no-row>"
    quit "hash:"_$extract(rs.%Get("Hash"), 1, 10)_" syncFailed:"_rs.%Get("SyncFailed")_" testPending:"_rs.%Get("TestPending")
}

ClassMethod Break(path As %String) As %Status
{
    quit ##class(Uplift.Nsp.Util).Replace(path, "Class SyncTest.SuperClass"_$char(10), "Class SyncTest.SuperClass Extends NcMissing.Base"_$char(10))
}

/// N18 (SYNC-B15, B22 / F-SYNC-1): failed compile outside sync, exact-byte revert, then sync.
ClassMethod N18()
{
    set u = ..U()
    do ..Fresh()
    set f = ..#DIR_..#SUPER
    set sc = $classmethod(u, "Run", "load "_..#DIR_" -dev", .o1)
    set kv("loadDev") = $classmethod(u, "St", sc)
    set kv("valueAfterLoad") = ..Value()
    set sha0 = $classmethod(u, "FileSha", f)
    set kv("rowAfterLoad") = ..Row()
    do ..Break(f)
    set kv("brokenShaDiffers") = ($classmethod(u, "FileSha", f) '= sha0)
    set sc = $classmethod(u, "Run", "compile sync-test", .o2, 0)
    set kv("compile") = $classmethod(u, "St", sc)
    set kv("superAfterCompile") = ..Super()
    set kv("stateAfterCompile") = $classmethod(u, "ClassState", "SyncTest.SuperClass")
    set kv("rowAfterCompile") = ..Row()
    // restore the exact earlier bytes
    set ok = ##class(%File).CopyFile(..#DATA_"sync-test/"_..#SUPER, f, 1)
    set kv("restoredExactBytes") = ok && ($classmethod(u, "FileSha", f) = sha0)
    set sc = $classmethod(u, "Run", "sync sync-test -verbose", .o3)
    set kv("sync") = $classmethod(u, "St", sc)
    set kv("nothingToSync") = $classmethod(u, "Has", .o3, "Nothing to sync")
    set kv("updatedLine") = $classmethod(u, "Line", .o3, "Updated:")
    set kv("superAfterSync") = ..Super()
    set kv("stateAfterSync") = $classmethod(u, "ClassState", "SyncTest.SuperClass")
    set kv("valueAfterSync") = ..Value()
    set kv("rowAfterSync") = ..Row()
    do $classmethod(u, "Emit", "nc-sync-N18-compile-fail-revert", .kv)
}

/// N19 (SYNC-B15, B40, B41 / F-SYNC-2): load -dev -skip-sync, edit, sync, sync.
ClassMethod N19()
{
    set u = ..U()
    do ..Fresh()
    set f = ..#DIR_..#SUPER
    set sc = $classmethod(u, "Run", "load "_..#DIR_" -dev -skip-sync", .o1)
    set kv("loadDevSkipSync") = $classmethod(u, "St", sc)
    set kv("hasBaseline") = ##class(%IPM.Storage.FileHash).HasBaseline("sync-test")
    do $classmethod(u, "Replace", f, """original""", """edited""")
    set sc = $classmethod(u, "Run", "sync sync-test -verbose", .o2)
    set kv("sync1") = $classmethod(u, "St", sc)
    set kv("sync1Baseline") = $classmethod(u, "Has", .o2, "Baseline established")
    set kv("valueAfterSync1") = ..Value()
    set sc = $classmethod(u, "Run", "sync sync-test -verbose", .o3)
    set kv("sync2") = $classmethod(u, "St", sc)
    set kv("sync2Nothing") = $classmethod(u, "Has", .o3, "Nothing to sync")
    set kv("valueAfterSync2") = ..Value()
    set kv("diskHasEdited") = ($classmethod(u, "ReadFile", f) [ """edited""")
    do $classmethod(u, "Emit", "nc-sync-N19-skip-sync-first-sync", .kv)
}

/// N20 (SYNC-B43, B45, B46 / DEC-SYNC-3): sync of all dev modules, one broken.
ClassMethod N20()
{
    set u = ..U()
    do ..Fresh()
    set cdir = "/tmp/nc-sdep-c/"
    do $classmethod(u, "CopyDir", ..#DATA_"sync-dep/c/", cdir)
    set sc1 = $classmethod(u, "Run", "load "_..#DIR_" -dev", .o1, 0)
    set sc2 = $classmethod(u, "Run", "load "_cdir_" -dev", .o2, 0)
    set kv("loads") = $classmethod(u, "St", sc1)_" / "_$classmethod(u, "St", sc2)
    do ..Break(..#DIR_..#SUPER)
    do $classmethod(u, "Replace", cdir_"src/cls/SyncDepC/Base.cls", """c-original""", """c-edited""")
    set sc = $classmethod(u, "Run", "sync", .o3)
    set kv("syncAll") = $classmethod(u, "St", sc)
    set kv("errorLine") = $classmethod(u, "Line", .o3, "ERROR")
    set kv("summaryLine") = $classmethod(u, "Line", .o3, "sync-test")
    set kv("depCValue") = $classmethod("SyncDepC.Base", "Version")
    set kv("syncTestRow") = ..Row()
    set sc = $classmethod(u, "Run", "sync sync-test", .o4, 0)
    set kv("syncOne") = $classmethod(u, "St", sc)
    do $classmethod(u, "Emit", "nc-sync-N20-sync-all-one-fails", .kv)
}

/// N21 (SYNC-B51 / DEC-SYNC-4): reload of a module not in developer mode.
ClassMethod N21()
{
    set u = ..U()
    do ..Fresh()
    set cdir = "/tmp/nc-reload-c/"
    do $classmethod(u, "CopyDir", ..#DATA_"sync-dep/c/", cdir)
    set sc = $classmethod(u, "Run", "load "_cdir, .o1, 0)
    set kv("load") = $classmethod(u, "St", sc)
    set kv("record") = $classmethod(u, "Mod", "sync-dep-c")
    do $classmethod(u, "Replace", cdir_"src/cls/SyncDepC/Base.cls", """c-original""", """c-reloaded""")
    set sc = $classmethod(u, "Run", "reload sync-dep-c", .o2)
    set kv("reload") = $classmethod(u, "St", sc)
    set kv("reloadLine") = $classmethod(u, "Line", .o2, "Reload")
    set m = ##class(%Dictionary.MethodDefinition).%OpenId("SyncDepC.Base||Version")
    set kv("serverSourceHasReloaded") = $select($isobject(m):(m.Implementation.Read(1000) [ "c-reloaded"), 1:"<no-method>")
    set kv("value") = $classmethod("SyncDepC.Base", "Version")
    do $classmethod(u, "Emit", "nc-sync-N21-reload-non-dev", .kv)
    do $classmethod(u, "Run", "uninstall sync-dep-c", .o3, 0)
}

/// N22 (SYNC-B17, B19, B20): failed sync sets a retry mark; a good compile keeps it; next sync retries.
ClassMethod N22()
{
    set u = ..U()
    do ..Fresh()
    set f = ..#DIR_..#SUPER
    set sc = $classmethod(u, "Run", "load "_..#DIR_" -dev", .o1, 0)
    set kv("loadDev") = $classmethod(u, "St", sc)
    do ..Break(f)
    set sc = $classmethod(u, "Run", "sync sync-test", .o2)
    set kv("brokenSync") = $classmethod(u, "St", sc)
    set kv("rowAfterFailedSync") = ..Row()
    // fix to new bytes
    do ##class(%File).CopyFile(..#DATA_"sync-test/"_..#SUPER, f, 1)
    do $classmethod(u, "Replace", f, """original""", """fixed""")
    set sc = $classmethod(u, "Run", "compile sync-test", .o3, 0)
    set kv("compile") = $classmethod(u, "St", sc)
    set kv("valueAfterCompile") = ..Value()
    set kv("rowAfterCompile") = ..Row()
    set sc = $classmethod(u, "Run", "sync sync-test -verbose", .o4)
    set kv("sync") = $classmethod(u, "St", sc)
    set kv("retryLine") = $classmethod(u, "Line", .o4, "Retrying")
    set kv("updatedLine") = $classmethod(u, "Line", .o4, "Updated:")
    set kv("rowAfterSync") = ..Row()
    set sc = $classmethod(u, "Run", "sync sync-test -verbose", .o5)
    set kv("syncAgainNothing") = $classmethod(u, "Has", .o5, "Nothing to sync")
    do $classmethod(u, "Emit", "nc-sync-N22-retry-mark-after-compile", .kv)
    do ..Fresh()
}

}
