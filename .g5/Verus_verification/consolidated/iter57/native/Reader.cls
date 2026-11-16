/// Test-only observer for iteration57. Not IPM code.
Class IPMCont.CspReader
{

Parameter ROOT = "/tmp/iter57/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Application properties, read in %SYS without calling back into this class.
ClassMethod App(pUrl As %String, ByRef pProps) As %Boolean
{
    new $namespace
    set $namespace = "%SYS"
    kill pProps
    quit:'##class(Security.Applications).Exists(pUrl) 0
    do ##class(Security.Applications).Get(pUrl,.pProps)
    quit 1
}

ClassMethod MakeAdminApp() As %Status
{
    new $namespace
    set $namespace = "%SYS"
    set p("NameSpace") = "USER", p("Description") = "admin desc", p("Path") = "/tmp/iter57/adminpath/", p("AutheEnabled") = 32, p("Timeout") = 1234
    quit ##class(Security.Applications).Create("/ipmcont57c",.p)
}

ClassMethod FileText(pPath As %String) As %Integer
{
    quit:'##class(%File).Exists(pPath) 0
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(pPath)
    set t = $zstrip(s.ReadLine(),"<>W")
    quit $case(t,"v1":1,"v2":2,:9)
}

ClassMethod Shell(pCase As %String, pCommand As %String, Output pWarn As %Integer) As %Boolean
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set pWarn = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        set:line["<CSPApplication></CSPApplication> resource tag is deprecated" pWarn = pWarn + 1
    }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod State(pCase As %String)
{
    set r = ..#ROOT
    for u="a","b","c" {
        set ex = ..App("/ipmcont57"_u,.p)
        do ..Obs(pCase_".app"_u,ex)
        if ex {
            do ..Obs(pCase_".app"_u_".pathdeploy",$get(p("Path"))[("/deploy"_$zconvert(u,"U")))
            do ..Obs(pCase_".app"_u_".pathsource",$get(p("Path"))[("/m57"_u_"/web"))
            do ..Obs(pCase_".app"_u_".pathadmin",$get(p("Path"))["/adminpath")
            do ..Obs(pCase_".app"_u_".descadmin",$get(p("Description"))="admin desc")
            do ..Obs(pCase_".app"_u_".timeout1234",$get(p("Timeout"))=1234)
            do ..Obs(pCase_".app"_u_".unauth",$zboolean(+$get(p("AutheEnabled")),64,1)=64)
            do ..Obs(pCase_".app"_u_".password",$zboolean(+$get(p("AutheEnabled")),32,1)=32)
        }
    }
    do ..Obs(pCase_".deployA",..FileText(r_"deployA/index.html"))
    do ..Obs(pCase_".deployC",..FileText(r_"deployC/index.html"))
    do ..Obs(pCase_".sourceA",..FileText(r_"m57a/web/index.html"))
    do ..Obs(pCase_".sourceB",..FileText(r_"m57b/web/index.html"))
    do ..Obs(pCase_".sourceBdir",##class(%File).DirectoryExists(r_"m57b/web/"))
}

ClassMethod Case(pCase As %String, pCommand As %String)
{
    do ..Obs(pCase_".ok",..Shell(pCase,pCommand,.warn))
    do ..Obs(pCase_".warn",warn)
    do ..State(pCase)
}

ClassMethod WriteSource(pModule As %String, pText As %String)
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(..#ROOT_pModule_"/web/index.html"))
    do s.Clear() do s.WriteLine(pText) $$$ThrowOnError(s.%Save())
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..State("s0")
    do ..Case("a1","load "_r_"m57a")
    do ..WriteSource("m57a","v2")
    do ..Case("a2","load "_r_"m57a")
    do ..Case("a3","uninstall m57a")
    do ..WriteSource("m57a","v1")
    do ..Case("a4","load "_r_"m57a -dev")
    do ..Case("b1","load "_r_"m57b")
    do ..Case("b2","uninstall m57b")
    set sc = ..MakeAdminApp()
    do ..Obs("c0.ok",$system.Status.IsOK(sc))
    do ..State("c0")
    do ..Case("c1","load "_r_"m57c")
    do ..Case("c2","uninstall m57c")
    write !,"NATIVE-END",!
}

}
