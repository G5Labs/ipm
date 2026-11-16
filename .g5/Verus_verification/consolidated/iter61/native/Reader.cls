/// Test-only observer for iteration61. Not IPM code.
Class IPMCont.WebAppReader
{

Parameter ROOT = "/tmp/iter61/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Reads one application in %SYS without calling back into this class.
ClassMethod App(pName As %String, Output pProps) As %Boolean
{
    new $namespace
    kill pProps
    set $namespace = "%SYS"
    quit:'##class(Security.Applications).Exists(pName) 0
    do ##class(Security.Applications).Get(pName,.pProps)
    quit 1
}

ClassMethod Edit(pName As %String, pDescription As %String) As %Status
{
    new $namespace
    set $namespace = "%SYS"
    set p("Description") = pDescription
    quit ##class(Security.Applications).Modify(pName,.p)
}

ClassMethod WriteManifest(pModule As %String, pAttrs As %String)
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(..#ROOT_pModule_"/module.xml"))
    do s.Clear()
    for line="<?xml version=""1.0"" encoding=""UTF-8""?>","<Export generator=""IRIS"" version=""26"">","<Document name="""_pModule_".ZPM"">","<Module>","<Name>"_pModule_"</Name>","<Version>1.0.0</Version>","<Packaging>module</Packaging>","<SourcesRoot>src</SourcesRoot>","<WebApplication "_pAttrs_"/>","</Module>","</Document>","</Export>" { do s.WriteLine(line) }
    $$$ThrowOnError(s.%Save())
}

ClassMethod Shell(pCase As %String, pCommand As %String) As %Boolean
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    for i=1:1:$get(lines) { write !,"OUT|",pCase,"|",$zstrip($get(lines(i)),"*C")}
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod State(pCase As %String)
{
    set a = ..App("/ipmcont61a",.p)
    do ..Obs(pCase_".a.exists",a)
    do ..Obs(pCase_".a.desc.first",$get(p("Description"))="first")
    do ..Obs(pCase_".a.desc.second",$get(p("Description"))="second")
    do ..Obs(pCase_".a.desc.manual",$get(p("Description"))="manual")
    do ..Obs(pCase_".a.desc.other",$get(p("Description"))="other")
    do ..Obs(pCase_".a.timeout900",$get(p("Timeout"))=900)
    do ..Obs(pCase_".a.ns.user",$get(p("NameSpace"))="USER")
    do ..Obs(pCase_".a.path.mgr",($get(p("Path"))["/usr/irissys/mgr/")&&($get(p("Path"))'["{"))
    do ..Obs(pCase_".upper.exists",..App("/IPMCONT61A",.q))
    do ..Obs(pCase_".c.slash",..App("/ipmcont61c/",.q))
    do ..Obs(pCase_".c.noslash",..App("/ipmcont61c",.q))
    do ..Obs(pCase_".d.exists",..App("/ipmcont61d",.q))
    for m="m61a","m61b","m61c","m61d" { do ..Obs(pCase_".mod."_m,##class(%IPM.Storage.Module).NameExists(m)) }
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Obs("w1.ok",..Shell("w1","load "_r_"m61a")) do ..State("w1")
    do ..WriteManifest("m61a","Name=""/ipmcont61a"" Path=""{$mgrdir}ipmcont61a"" Timeout=""900""")
    do ..Obs("w2.ok",..Shell("w2","load "_r_"m61a")) do ..State("w2")
    do ..WriteManifest("m61a","Name=""/ipmcont61a"" Description=""second"" Path=""{$mgrdir}ipmcont61a"" Timeout=""900""")
    do ..Obs("w3.ok",..Shell("w3","load "_r_"m61a")) do ..State("w3")
    do ..Obs("c0.ok",..Edit("/ipmcont61a","manual"))
    do ..Obs("c1.ok",..Shell("c1","m61a compile -only")) do ..State("c1")
    do ..Obs("c2.ok",..Shell("c2","m61a activate -only")) do ..State("c2")
    do ..Obs("n1.ok",..Shell("n1","load "_r_"m61b")) do ..State("n1")
    do ..Obs("n2.ok",..Shell("n2","uninstall m61b")) do ..State("n2")
    do ..Obs("t1.ok",..Shell("t1","load "_r_"m61c")) do ..State("t1")
    do ..Obs("f1.ok",..Shell("f1","load "_r_"m61d")) do ..State("f1")
    do ..Obs("u1.ok",..Shell("u1","uninstall m61a")) do ..State("u1")
    write !,"NATIVE-END",!
}

}
