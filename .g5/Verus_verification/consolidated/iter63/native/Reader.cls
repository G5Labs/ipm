/// Test-only observer for iteration63. Not IPM code.
Class IPMCont.CleanReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Runs in %SYS without calling back into this class.
ClassMethod MakeDB() As %Status
{
    new $namespace
    set $namespace = "%SYS"
    set dir = "/usr/irissys/mgr/u63db/"
    do ##class(%File).CreateDirectoryChain(dir)
    set sc = ##class(SYS.Database).CreateDatabase(dir)
    quit:'sc sc
    kill p set p("Directory") = dir
    quit ##class(Config.Databases).Create("U63DB",.p)
}

/// Returns $lb(globalMapped, packageMapped), read in %SYS.
ClassMethod Mapped() As %List
{
    new $namespace
    set $namespace = "%SYS"
    quit $lb(##class(Config.MapGlobals).Exists("USER","U63M"),##class(Config.MapPackages).Exists("USER","U63MP"))
}

ClassMethod Shell(pCase As %String, pCommand As %String) As %Boolean
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    for i=1:1:$get(lines) { write !,"OUT|",pCase,"|",$zstrip($get(lines(i)),"*C") }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod State(pCase As %String)
{
    do ..Obs(pCase_".record",##class(%IPM.Storage.Module).NameExists("u63"))
    do ..Obs(pCase_".stream",##class(%IPM.StudioDocument.ModuleStream).NameExists("u63"))
    do ..Obs(pCase_".cls.one",##class(%Dictionary.ClassDefinition).%ExistsId("U63.One"))
    do ..Obs(pCase_".pkg.a",##class(%Dictionary.ClassDefinition).%ExistsId("U63P.A"))
    do ..Obs(pCase_".pkg.b",##class(%Dictionary.ClassDefinition).%ExistsId("U63P.B"))
    do ..Obs(pCase_".inc",##class(%RoutineMgr).Exists("U63Inc.INC"))
    do ..Obs(pCase_".mac",##class(%RoutineMgr).Exists("U63R.MAC"))
    do ..Obs(pCase_".int",##class(%RoutineMgr).Exists("U63R.INT"))
    do ..Obs(pCase_".obj",##class(%RoutineMgr).Exists("U63R.OBJ"))
    do ..Obs(pCase_".gbl.nameonly",$data(^U63N)>0)
    do ..Obs(pCase_".gbl.attr",$data(^U63G)>0)
    set m = ..Mapped()
    do ..Obs(pCase_".map.gbl",$list(m,1))
    do ..Obs(pCase_".map.pkg",$list(m,2))
    do ..Obs(pCase_".file.a",##class(%File).Exists("/tmp/iter63t/a.txt"))
    do ..Obs(pCase_".file.dir",##class(%File).Exists("/tmp/iter63t/dir/d.txt"))
    do ..Obs(pCase_".file.keepdir",##class(%File).Exists("/tmp/iter63t/keep/kk.txt"))
    do ..Obs(pCase_".file.keepfile",##class(%File).Exists("/tmp/iter63t/k.txt"))
    // Developer-mode bookkeeping of u63d.
    do ..Obs(pCase_".dev.record",##class(%IPM.Storage.Module).NameExists("u63d"))
    do ..Obs(pCase_".dev.cls",##class(%Dictionary.ClassDefinition).%ExistsId("U63D.X"))
    do ..Obs(pCase_".dev.sourcesmodule",$data(^Sources("MODULE","u63d"))>0)
    set n = 0, k = ""
    for { set k = $order(^Sources("CLS",k)) quit:k=""  set:k["U63D" n = n + 1 }
    do ..Obs(pCase_".dev.sourcescls",n)
    set n = 0, ref = "^Sources"
    for { set ref = $query(@ref) quit:ref=""  if ($zconvert(ref,"U")["U63D") { set n = n + 1 write !,"SRC|",pCase,"|",ref,"=",@ref } }
    do ..Obs(pCase_".dev.sourcesnodes",n)
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select count(*) c from %IPM_Storage.FileHash where ModuleName = 'u63d'")
    do rs.%Next()
    do ..Obs(pCase_".dev.filehash",rs.%Get("c"))
}

ClassMethod All()
{
    set sc = ..MakeDB()
    if 'sc { write !,"ERR|db|",$system.Status.GetErrorText(sc),! }
    do ..Obs("setup.db",$system.Status.IsOK(sc))
    do ##class(%File).CreateDirectoryChain("/tmp/iter63t/")
    do ..Obs("i1.ok",..Shell("i1","load /tmp/iter63/u63"))
    do ..State("i1")
    do ..Obs("x1.ok",..Shell("x1","uninstall u63"))
    do ..State("x1")
    do ..Obs("i2.ok",..Shell("i2","load /tmp/iter63/u63d -dev"))
    do ..State("i2")
    do ..Obs("x2.ok",..Shell("x2","uninstall u63d"))
    do ..State("x2")
    do ..Obs("i3.ok",..Shell("i3","load /tmp/iter63/u63"))
    do ..Obs("x3.ok",..Shell("x3","uninstall u63 -purge"))
    do ..State("x3")
    write !,"NATIVE-END",!
}

}
