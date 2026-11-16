/// Test-only observer for iteration36. Not IPM code.
Class IPMCont.InstallerReader
{

Parameter ROOT = "/tmp/iter36/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Code(pText As %String) As %Integer
{
    quit $case(pText,"":0,"param":1,"cli":2,"hello":3,"bye":4,"USER":5,:99)
}

ClassMethod Kind(pName As %String) As %Integer
{
    quit $case(pName,"Install":1,"Configure":2,"Unconfigure":3,"UnInstall":4,"Note":5,:99)
}

ClassMethod Case(pCase As %String, pCommand As %String, pModule As %String = "m36a")
{
    set before = +$get(^IPMCONT36("n"))
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    for i=1:1:$get(lines) { write !,"OUT|",pCase,"|",$zstrip($get(lines(i)),"*C") }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".installed",##class(%IPM.Storage.Module).NameExists(pModule))
    set after = +$get(^IPMCONT36("n"))
    do ..Obs(pCase_".calls",after-before)
    for n=before+1:1:after {
        set p = pCase_".call"_(n-before)
        // A record written inside a load that rolled back is gone; the $increment counter is not rolled back.
        if '$data(^IPMCONT36(n),rec) { do ..Obs(p_".kind",0) continue }
        set k = ..Kind($list(rec,1))
        do ..Obs(p_".kind",k)
        if k=1 { do ..Obs(p_".dbempty",$list(rec,2)="") do ..Obs(p_".dev",$list(rec,4)) do ..Obs(p_".upgrade",$list(rec,5)) }
        if k=2 { do ..Obs(p_".ns",..Code($list(rec,2))) do ..Obs(p_".mode",..Code($list(rec,4))) do ..Obs(p_".greeting",..Code($list(rec,5))) do ..Obs(p_".installdir",$list(rec,6)>0) do ..Obs(p_".lifecycle",$list(rec,7)) }
        if k=3 { do ..Obs(p_".purge",$list(rec,4)) }
        if k=4 { do ..Obs(p_".dbempty",$list(rec,2)="") }
        if k=5 { do ..Obs(p_".greeting",..Code($list(rec,2))) }
    }
}

ClassMethod All()
{
    kill ^IPMCONT36
    set r = ..#ROOT
    do ..Case("l1","load "_r_"m36a")
    do ..Case("l3","load "_r_"m36a -DGreeting=bye -DMode=cli")
    do ..Case("l5","load "_r_"m36a -DConfigure.IsUpgrade=1")
    do ..Case("u1","uninstall m36a")
    do ..Case("l2","load "_r_"m36a -dev")
    do ..Case("u2","uninstall m36a")
    set ^IPMCONT36("fail") = 1
    do ..Case("e1","load "_r_"m36a")
    kill ^IPMCONT36("fail")
    do ..Case("n1","load "_r_"m36n","m36n")
    write !,"NATIVE-END",!
}

}
