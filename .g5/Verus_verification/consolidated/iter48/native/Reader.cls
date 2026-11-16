/// Test-only observer for iteration48. Not IPM code.
Class IPMCont.CpfReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Mapping existence, read in %SYS without calling back into this class.
ClassMethod Mapped(pPackage As %String) As %Boolean
{
    new $namespace
    set $namespace = "%SYS"
    quit ##class(Config.MapPackages).Exists("USER",pPackage)
}

ClassMethod Case(pCase As %String, pCommand As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set details = 0
    for i=1:1:$get(lines) { set line = $zstrip($get(lines(i)),"*C") write !,"OUT|",pCase,"|",line }
    if 'sc { set text = $system.Status.GetErrorText(sc) write !,"ERR|",pCase,"|",text,! set details = (text["Error merging CPF file")&&(text["messages.log") }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".details",details)
    do ..Obs(pCase_".mapA",..Mapped("IPMCONTA48"))
    do ..Obs(pCase_".mapB",..Mapped("IPMCONTB48"))
}

ClassMethod All()
{
    do ..Obs("pre.mapA",..Mapped("IPMCONTA48"))
    do ..Case("l1","load /tmp/iter48/m48")
    do ..Case("c1","m48 compile -only")
    do ..Case("p1","m48 map48")
    do ..Case("x1","m48 bad48")
    write !,"NATIVE-END",!
}

}
