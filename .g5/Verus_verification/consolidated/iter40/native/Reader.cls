/// Test-only observer for iteration40. Not IPM code.
Class IPMCont.TestReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
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

ClassMethod Case(pCase As %String, pCommand As %String)
{
    set ran = $get(^IPMCONT40("ran")), vran = $get(^IPMCONT40("vran"))
    do ..Obs(pCase_".ok",..Shell(pCase,pCommand))
    do ..Obs(pCase_".ran",$get(^IPMCONT40("ran")) - ran)
    do ..Obs(pCase_".vran",$get(^IPMCONT40("vran")) - vran)
    do ..Obs(pCase_".tests",##class(%Dictionary.ClassDefinition).%ExistsId("T40.Tests.Basic"))
    do ..Obs(pCase_".vtests",##class(%Dictionary.ClassDefinition).%ExistsId("T40.VTests.Basic"))
    do ..Obs(pCase_".testscompiled",##class(%Dictionary.CompiledClass).%ExistsId("T40.Tests.Basic"))
    do ..Obs(pCase_".main",##class(%Dictionary.CompiledClass).%ExistsId("T40.Main"))
    do ..Obs(pCase_".record",##class(%IPM.Storage.Module).NameExists("t40"))
}

ClassMethod All()
{
    kill ^IPMCONT40
    do ..Case("n0","load /tmp/iter40/t40")
    do ..Case("n1","test t40 -only")
    do ..Case("n2","uninstall t40")
    do ..Case("d0","load /tmp/iter40/t40 -dev")
    do ..Case("d1","test t40 -only")
    do ..Case("d2","uninstall t40")
    write !,"NATIVE-END",!
}

}
