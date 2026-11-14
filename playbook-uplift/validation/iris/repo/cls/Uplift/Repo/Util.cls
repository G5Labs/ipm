/// Shared helpers for the native checks of modules 05 (UNIN) and 06 (REPO).
/// Observations print one line: OBS|<name>|<key>=<value>;...
Class Uplift.Repo.Util
{

Parameter FIX = "/tmp/uplift/repo/fixtures/";

Parameter IPMDATA = "/home/irisowner/zpm/tests/integration_tests/Test/PM/Integration/_data/";

Parameter OUT = "/tmp/uplift-repo-out.txt";

/// Make a value safe for an OBS line.
ClassMethod Safe(v As %String, max As %Integer = 160) As %String
{
    set v = $translate(v, ";|="_$char(13,10,9,27), ",/:   ")
    quit $extract($zstrip(v, "<>=W"), 1, max)
}

ClassMethod Emit(name As %String, ByRef kv)
{
    set out = "OBS|"_name_"|", k = ""
    for {
        set k = $order(kv(k), 1, v)
        quit:k=""
        set out = out_k_"="_..Safe(v, 300)_";"
    }
    write !, out, !
}

/// Run an IPM shell command with its output captured to a file. The output is echoed afterwards.
/// Do not use for commands that prompt.
ClassMethod Run(cmd As %String, Output out As %String) As %Status
{
    set out = "", f = ..#OUT
    set io = $io
    open f:("WNS"):5
    use f
    try {
        set sc = ##class(%IPM.Main).Shell(cmd)
    } catch e {
        set sc = e.AsStatus()
    }
    close f
    use io
    set stream = ##class(%Stream.FileCharacter).%New()
    do stream.LinkToFile(f)
    while 'stream.AtEnd { set out = out_stream.Read(32000) }
    write !, ">>> ", cmd, !, out, !
    write "<<< status=", $select($system.Status.IsOK(sc):"OK", 1:$system.Status.GetErrorText(sc)), !
    quit sc
}

ClassMethod Status(sc As %Status, ByRef kv, prefix As %String = "")
{
    set kv(prefix_"status") = $select($system.Status.IsOK(sc):"ok", 1:"err")
    set:'$system.Status.IsOK(sc) kv(prefix_"error") = $extract($system.Status.GetErrorText(sc), 1, 200)
}

ClassMethod Has(text As %String, sub As %String) As %Boolean
{
    quit (text [ sub)
}

ClassMethod Installed(name As %String) As %Boolean
{
    quit ##class(%IPM.Storage.Module).NameExists(name)
}

ClassMethod ClassState(cls As %String) As %String
{
    quit $select(##class(%Dictionary.CompiledClass).%ExistsId(cls):"compiled", ##class(%Dictionary.ClassDefinition).%ExistsId(cls):"defined", 1:"absent")
}

ClassMethod WebAppExists(app As %String) As %Boolean
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(Security.Applications).Exists(app)
}

ClassMethod DeleteWebApp(app As %String)
{
    new $namespace
    set $namespace = "%SYS"
    if ##class(Security.Applications).Exists(app) { do ##class(Security.Applications).Delete(app) }
}

/// Latest history row for a package: action/success.
ClassMethod LastHistory(pkg As %String) As %String
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT TOP 1 Action, Success, Committed FROM %IPM_General.History WHERE Package = ? ORDER BY ID DESC", pkg)
    if 'rs.%Next() { quit "<none>" }
    quit rs.%Get("Action")_"/"_$select($system.Status.IsOK(rs.%Get("Success")):"success", 1:"failure")
}

ClassMethod HistoryCount() As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(, "SELECT COUNT(*) AS N FROM %IPM_General.History")
    do rs.%Next()
    quit +rs.%Get("N")
}

/// Uninstall every test module, drop test classes, globals, web apps and repos.
ClassMethod Reset()
{
    if $tlevel { trollback }
    set mods = $listbuild("nc-web-top", "nc-web-dep", "nc-web-w", "nc-purge", "nc-ghost-b", "nc-ghost-a", "nc-gbl1", "nc-gbl2", "nc-fc-other", "fc-keep-on-uninstall", "nc-p", "nc-q", "nc-repo-pub", "nc-repo-dpl", "nc-fs1", "nc-fs2", "nc-scope-top")
    set ptr = 0
    while $listnext(mods, ptr, m) {
        if ..Installed(m) { do ..Run("uninstall "_m_" -force", .o) }
    }
    for c = "NCWebDep.Hello", "NCWebTop.Hello", "NCW.Hello", "NCW.Installer", "NCPurge.Item", "NCPurge.Installer", "NCGhostA.Hello", "NCGhostB.Hello", "NCP.Hello", "NCQ.Hello", "NCQ.Installer", "NC.Orphan", "NCEnv.Item", "NCRepoPub.Hello", "NCRepoDpl.Main", "NCFs1.Hello", "NCFs2.Hello" {
        if ##class(%Dictionary.ClassDefinition).%ExistsId(c) { do $system.OBJ.Delete(c, "-d") }
    }
    kill ^NCProbe, ^NC.Data, ^NC.Data2, ^NCPurge.ItemD, ^NCPurge.ItemI, ^NCEnv.ItemD, ^NCEnv.ItemI
    for a = "/nc-dep", "/nc-w" { do ..DeleteWebApp(a) }
    do ##class(%IPM.Repo.Definition).%DeleteExtent()
}

}
