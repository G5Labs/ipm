/// Test-only observer for iteration51. Not IPM code.
Class IPMCont.InfoReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Info(pCase As %String, pCommand As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set after = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        if line["Currently installed top-level modules" { set after = 1 continue }
        continue:'after
        set l = $zconvert(line,"L")
        set:l["top51" seen("top51") = 1
        set:(l["dep51")&&(l'["mixeddep51") seen("dep51") = 1
        set:l["mixeddep51" seen("mixed") = 1
        set:l["ext51label" seen("ext") = 1
        set:l["ext51label (ext51)" seen("extfull") = 1
        set:(l["ext51label")&&(l["1.0.0+snapshot") seen("extbuild") = 1
        set:(l["ext51label")&&(l["1.0.0")&&(l'["+snapshot") seen("extnobuild") = 1
    }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    for k="top51","dep51","mixed","ext","extfull","extbuild","extnobuild" { do ..Obs(pCase_".listed."_k,$get(seen(k))) }
}

ClassMethod All()
{
    set r = "/tmp/iter51/"
    for m="dep51","mixed","ext51","top51" { set sc = ##class(%IPM.Main).Shell("load "_r_m) do ..Obs("load."_m,$system.Status.IsOK(sc)) }
    set mod = ##class(%IPM.Storage.Module).NameOpen("MixedDep51")
    do ..Obs("stored.mixedcase",$select($isobject(mod):(mod.Name="MixedDep51"),1:-1))
    do ..Info("i1","info")
    do ..Info("i2","info -v")
    write !,"NATIVE-END",!
}

}
