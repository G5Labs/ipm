/// Test-only observer for iteration62. Not IPM code.
Class IPMCont.InteropReader
{

Parameter ROOT = "/tmp/iter62/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Runs in %SYS without calling back into this class.
ClassMethod Enable() As %Status
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(%EnsembleMgr).EnableNamespace("USER",1)
}

ClassMethod Value(pTable As %String, pKey As %String) As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select DataValue from Ens_Util.LookupTable where TableName=? and KeyName=?",pTable,pKey)
    quit:rs.%SQLCODE<0 -2
    quit:'rs.%Next() -1
    quit +rs.%Get("DataValue")
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
    for k="a","b","c" { do ..Obs(pCase_".I62A."_k,..Value("I62A",k)) }
    do ..Obs(pCase_".I62M.k",..Value("I62M","k"))
    do ..Obs(pCase_".I62X.k",..Value("I62X","k"))
    do ..Obs(pCase_".I62F.k",..Value("I62F","k"))
    do ..Obs(pCase_".doc.I62A",##class(%RoutineMgr).Exists("I62A.LUT"))
    do ..Obs(pCase_".doc.I62H",##class(%RoutineMgr).Exists("I62H.HL7"))
    do ..Obs(pCase_".schema.I62H",$data(^EnsHL7.Schema("I62H"))>0)
}

ClassMethod All()
{
    set r = ..#ROOT
    set sc = ..Enable()
    if 'sc { write !,"ERR|enable|",$system.Status.GetErrorText(sc),! }
    do ..Obs("enable.ok",$system.Status.IsOK(sc))
    do ..Obs("enable.interop",##class(%IPM.Storage.SystemRequirements).IsInteroperabilityEnabled())
    do ..Obs("a1.ok",..Shell("a1","load "_r_"i62a")) do ..State("a1")
    do ..Obs("m1.ok",..Shell("m1","load "_r_"i62m")) do ..State("m1")
    do ..Obs("x1.ok",..Shell("x1","load "_r_"i62x")) do ..State("x1")
    do ..Obs("f1.ok",..Shell("f1","load "_r_"i62f")) do ..State("f1")
    $$$ThrowOnError(##class(%File).CopyFile(r_"i62a/edit/I62A.xml",r_"i62a/src/i14y/I62A.xml",1))
    do ..Obs("a2.ok",..Shell("a2","load "_r_"i62a")) do ..State("a2")
    do ..Obs("a3.ok",..Shell("a3","uninstall i62a")) do ..State("a3")
    do ..Obs("m3.ok",..Shell("m3","uninstall i62m")) do ..State("m3")
    do ..Obs("h1.ok",..Shell("h1","load "_r_"i62h")) do ..State("h1")
    do ..Obs("h2.ok",..Shell("h2","uninstall i62h")) do ..State("h2")
    write !,"NATIVE-END",!
}

}
