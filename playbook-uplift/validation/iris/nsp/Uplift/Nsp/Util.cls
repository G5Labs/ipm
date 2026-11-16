/// Helpers for the native checks of modules 07-09 (namespace, resources, sync).
/// Observations are printed as single lines: OBS|<name>|<key>=<value>;...
Class Uplift.Nsp.Util
{

/// Print one observation line. Values are cleaned of ; | = characters.
ClassMethod Emit(name As %String, ByRef kv)
{
    set out = "OBS|"_name_"|", k = ""
    for {
        set k = $order(kv(k), 1, v)
        quit:k=""
        set out = out_k_"="_..Clean(v)_";"
    }
    write !, out, !
}

ClassMethod Clean(v As %String) As %String
{
    quit $translate($extract($zstrip(v, "*C"), 1, 300), ";|=", ",/:")
}

/// Run one IPM shell command with output captured; echo the output with a prefix; return the status.
/// pOut(n) holds the output lines; pOut is the count.
ClassMethod Run(cmd As %String, Output pOut, echo As %Boolean = 1) As %Status
{
    kill pOut
    write !, ">>> zpm """, cmd, """"
    do ##class(Uplift.Nsp.Cap).Begin()
    set sc = $$$OK
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
    } catch e {
        set sc = e.AsStatus()
    }
    do ##class(Uplift.Nsp.Cap).End(.pOut)
    if echo {
        for i=1:1:$get(pOut) { write !, "  | ", pOut(i) }
    }
    write !, "<<< status ", $select($system.Status.IsOK(sc):"OK", 1:"ERROR "_$extract($system.Status.GetErrorText(sc), 1, 300)), !
    quit sc
}

/// 1 when any output line contains str.
ClassMethod Has(ByRef pOut, str As %String) As %Boolean
{
    for i=1:1:$get(pOut) { return:(pOut(i) [ str) 1 }
    quit 0
}

/// First output line containing str, or "".
ClassMethod Line(ByRef pOut, str As %String) As %String
{
    for i=1:1:$get(pOut) { return:(pOut(i) [ str) $zstrip(pOut(i), "<>W") }
    quit ""
}

ClassMethod St(sc As %Status) As %String
{
    quit $select($system.Status.IsOK(sc):"ok", 1:"err:"_$extract($system.Status.GetErrorText(sc), 1, 160))
}

/// Where the %IPM package of ns comes from (database directory), read in %SYS.
ClassMethod PkgDest(ns As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(%SYS.Namespace).GetPackageDest(ns, "%IPM")
}

ClassMethod GlobalDest(ns As %String, gbl As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(%SYS.Namespace).GetGlobalDest(ns, gbl)
}

/// The routine (code) database directory of ns.
ClassMethod CodeDB(ns As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    set sc = ##class(Config.Namespaces).Get(ns, .p)
    quit:'$system.Status.IsOK(sc) "<none>"
    set db = $get(p("Routines"))
    set sc = ##class(Config.Databases).Get(db, .d)
    quit db_"@"_$get(d("Directory"))
}

/// Config.MapPackages / MapRoutines / MapGlobals state for ns: "pkg:<db>,rtn:<db>,repoD:<db>" ("-" when absent).
ClassMethod Maps(ns As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    set pkg = "-", rtn = "-", rep = "-"
    if ##class(Config.MapPackages).Exists(ns, "%IPM") { do ##class(Config.MapPackages).Get(ns, "%IPM", .p) set pkg = $get(p("Database")) }
    if ##class(Config.MapRoutines).Exists(ns, "%IPM.*") { do ##class(Config.MapRoutines).Get(ns, "%IPM.*", .r) set rtn = $get(r("Database")) }
    if ##class(Config.MapGlobals).Exists(ns, "IPM.Repo.DefinitionD") { do ##class(Config.MapGlobals).Get(ns, "IPM.Repo.DefinitionD", .g) set rep = $get(g("Database")) }
    quit "pkg:"_pkg_" rtn:"_rtn_" repoD:"_rep
}

/// 1 when %IPM.Main:Shell is callable in ns.
ClassMethod HasIPM(ns As %String) As %Boolean
{
    new $namespace
    set $namespace = ns
    quit ''$system.CLS.IsMthd("%IPM.Main", "Shell")
}

/// Module record in ns: version (and /dev), or "-".
ClassMethod Mod(name As %String, ns As %String = {$namespace}) As %String
{
    new $namespace
    set $namespace = ns
    quit:'$system.CLS.IsMthd("%IPM.Storage.Module", "NameExists") "<no-ipm>"
    quit:'##class(%IPM.Storage.Module).NameExists(name) "-"
    set m = ##class(%IPM.Storage.Module).NameOpen(name)
    quit m.VersionString_$select(m.DeveloperMode:"/dev", 1:"")
}

ClassMethod RepoNames(ns As %String = {$namespace}) As %String
{
    new $namespace
    set $namespace = ns
    set out = "", rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT Name FROM %IPM_Repo.Definition ORDER BY Name")
    while rs.%Next() { set out = out_$select(out="":"", 1:" ")_rs.%Get("Name") }
    quit $select(out="":"<none>", 1:out)
}

ClassMethod MkNs(ns As %String) As %Status
{
    if ##class(Uplift.Nsp.Util).NsExists(ns) { quit $$$OK }
    quit ##class(%IPM.Test.Utils).CreateNamespace(ns)
}

ClassMethod NsExists(ns As %String) As %Boolean
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(Config.Namespaces).Exists(ns)
}

ClassMethod ClassState(cls As %String) As %String
{
    quit $select(##class(%Dictionary.CompiledClass).%ExistsId(cls):"compiled", ##class(%Dictionary.ClassDefinition).%ExistsId(cls):"defined", 1:"absent")
}

/// Text file helpers (used to edit fixture copies inside the container).
ClassMethod ReadFile(path As %String) As %String
{
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(path)
    set out = ""
    while 's.AtEnd { set out = out_s.Read(32000) }
    quit out
}

ClassMethod WriteFile(path As %String, text As %String) As %Status
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$QuitOnError(s.LinkToFile(path))
    do s.Clear()
    do s.Write(text)
    quit s.%Save()
}

ClassMethod Replace(path As %String, old As %String, new As %String) As %Status
{
    set t = ..ReadFile(path)
    quit ..WriteFile(path, $replace(t, old, new))
}

/// SHA-1 of a file's bytes, hex.
ClassMethod FileSha(path As %String) As %String
{
    set s = ##class(%Stream.FileBinary).%New()
    do s.LinkToFile(path)
    quit ##class(%xsd.hexBinary).LogicalToXSD($system.Encryption.SHA1HashStream(s))
}

ClassMethod CopyDir(from As %String, to As %String) As %Boolean
{
    do ##class(%File).RemoveDirectoryTree(to)
    do ##class(%File).CreateDirectoryChain(to)
    quit ##class(%File).CopyDir(from, to, 1)
}

/// FileHash row flags for one module file: hash prefix, SyncFailed, TestPending.
ClassMethod HashRow(module As %String, relPath As %String) As %String
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT * FROM %IPM_Storage.FileHash WHERE ModuleName = ? AND RelativePath = ?", module, relPath)
    if rs.%SQLCODE < 0 { quit "sqlerr:"_rs.%SQLCODE_" "_rs.%Message }
    quit:'rs.%Next() "<no-row>"
    quit "hash:"_$extract(rs.%Get("Hash"), 1, 10)_" syncFailed:"_rs.%Get("SyncFailed")_" testPending:"_rs.%Get("TestPending")
}

}
