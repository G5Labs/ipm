/// Test-only observer for iteration59. Not IPM code.
Class IPMCont.MapReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Runs in %SYS without calling back into this class. Returns $lb(mapExists, mapDBcode, destCode, definedInSys).
ClassMethod Probe(pPkg As %String, pClass As %String) As %List
{
    new $namespace
    set $namespace = "%SYS"
    set exists = ##class(Config.MapPackages).Exists("USER",pPkg)
    set dbcode = 0
    if exists {
        kill p
        do ##class(Config.MapPackages).Get("USER",pPkg,.p)
        set db = $get(p("Database"))
        set dbcode = $case(db,"USER":1,"IRISSYS":2,"DB59":3,:9)
    }
    set dest = $piece(##class(%SYS.Namespace).GetPackageDest("USER",pPkg),"^",2,*)
    set destcode = $case(1,dest["/mgr/user/":1,dest["/mgr/db59/":3,dest["/mgr/irissys/":2,dest["/mgr/":4,:0)
    set insys = ##class(%Dictionary.CompiledClass).%ExistsId(pClass)
    quit $lb(exists,dbcode,destcode,insys)
}

/// Creates DB59 and maps package P59 in USER to it; runs in %SYS only.
ClassMethod AdminMapping() As %Status
{
    new $namespace
    set $namespace = "%SYS"
    set dir = "/usr/irissys/mgr/db59/"
    do ##class(%File).CreateDirectoryChain(dir)
    set sc = ##class(SYS.Database).CreateDatabase(dir)
    quit:'sc sc
    kill p set p("Directory") = dir
    set sc = ##class(Config.Databases).Create("DB59",.p)
    quit:'sc sc
    kill p set p("Database") = "DB59"
    set sc = ##class(Config.MapPackages).Create("USER","P59",.p)
    quit:'sc sc
    quit ##class(Config.CPF).Activate()
}

ClassMethod State(pCase As %String)
{
    for item=$lb("%ZI59","%ZI59.Main","a"),$lb("P59.Sub","P59.Sub.Main","b"),$lb("%ZI59C","%ZI59C.Main","c"),$lb("N59","N59.Main","d"),$lb("%ZI59G","%ZI59G.Main","e") {
        set pkg = $list(item,1), cls = $list(item,2), k = $list(item,3)
        set r = ..Probe(pkg,cls)
        do ..Obs(pCase_"."_k_".map",$list(r,1))
        do ..Obs(pCase_"."_k_".mapdb",$list(r,2))
        do ..Obs(pCase_"."_k_".dest",$list(r,3))
        do ..Obs(pCase_"."_k_".insys",$list(r,4))
        do ..Obs(pCase_"."_k_".inuser",##class(%Dictionary.CompiledClass).%ExistsId(cls))
        do ..Obs(pCase_"."_k_".module",##class(%IPM.Storage.Module).NameExists("m59"_k))
    }
}

ClassMethod Shell(pCase As %String, pCommand As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    for i=1:1:$get(lines) { write !,"OUT|",pCase,"|",$zstrip($get(lines(i)),"*C") }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..State(pCase)
}

ClassMethod All()
{
    set r = "/tmp/iter59/"
    do ..State("s0")
    do ..Shell("a1","load "_r_"m59a")
    do ..Shell("a2","uninstall m59a")
    set sc = ..AdminMapping()
    if 'sc { write !,"ERR|admin|",$system.Status.GetErrorText(sc),! }
    do ..Obs("admin.ok",$system.Status.IsOK(sc))
    do ..State("s1")
    do ..Shell("b1","load "_r_"m59b")
    do ..Shell("b2","uninstall m59b")
    do ..Shell("c1","load "_r_"m59c -DNoMapping=1")
    do ..Shell("c2","uninstall m59c")
    do ..Shell("d1","load "_r_"m59d")
    do ..Shell("e1","load "_r_"m59e")
    do ..Shell("e2","uninstall m59e")
    do ..Shell("e3","uninstall m59e -global")
    do ..Shell("c3","load "_r_"m59c -DNoMapping=1")
    do ..Shell("c4","uninstall m59c -global")
    write !,"NATIVE-END",!
}

}
