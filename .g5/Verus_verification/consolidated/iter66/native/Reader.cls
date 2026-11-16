/// Test-only observer for iteration66. Not IPM code.
Class IPMCont.DocPhaseReader
{

Parameter ROOT = "/tmp/iter66/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Pos(pName As %String) As %Integer
{
    for i=1:1:$get(^IPMCONT66("n")) { return:$get(^IPMCONT66("seq",i))=pName i }
    quit 0
}

ClassMethod Case(pCase As %String, pCommand As %String)
{
    kill ^IPMCONT66
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set starts = 0, notinstalled = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        set:line["Document START" starts = starts + 1
        set:line["Document START"&&(line["|d66]") start("d66") = starts
        set:line["Document START"&&(line["|m66]") start("m66") = starts
    }
    if 'sc {
        set text = $system.Status.GetErrorText(sc)
        write !,"ERR|",pCase,"|",text,!
        set notinstalled = (text["is not installed")
    }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".starts",starts)
    do ..Obs(pCase_".start.d66",$get(start("d66")))
    do ..Obs(pCase_".start.m66",$get(start("m66")))
    do ..Obs(pCase_".notinstalled",notinstalled)
    do ..Obs(pCase_".tlevel",$tlevel)
    for n="d66","m66","e66","f66" { do ..Obs(pCase_".hook."_n,..Pos(n)) }
}

ClassMethod All()
{
    set r = ..#ROOT, ok = 1
    for m="d66","m66","e66","f66","n66" { set ok = ok && ##class(%IPM.Main).Shell("load "_r_m) }
    do ..Obs("setup.ok",ok)
    do ..Case("c1","m66 document")
    do ..Case("c2","m66 document -DDocument.Recurse=0")
    do ..Case("c3","n66 document")
    do ..Case("c4","f66 document")
    do ##class(%IPM.Main).Shell("uninstall d66 -force")
    do ..Obs("c5pre.d66",##class(%IPM.Storage.Module).NameExists("d66"))
    do ..Case("c5","m66 document")
    do ##class(%IPM.Main).Shell("repo -fs -name r66 -path "_r_"repo/")
    do ..Case("c6","m66 document")
    do ..Obs("c6post.d66",##class(%IPM.Storage.Module).NameExists("d66"))
    write !,"NATIVE-END",!
}

}
