/// Test-only observer for iteration50. Not IPM code.
Class IPMCont.DefModReader
{

Parameter ROOT = "/tmp/iter50/m50";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod ModeCode(pMode As %String) As %Integer
{
    quit $case(pMode,"":0,"stored":1,"second":2,"cmd":3,"manifest":4,:99)
}

ClassMethod Run(pCase As %String, pCommand As %String, ByRef pLines) As %Boolean
{
    kill pLines
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.pLines)
    for i=1:1:$get(pLines) { set pLines(i) = $zstrip($get(pLines(i)),"*C") write !,"OUT|",pCase,"|",pLines(i) }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod Has(ByRef pLines, pText As %String, pExact As %Boolean = 0) As %Integer
{
    set found = 0
    for i=1:1:$get(pLines) {
        set line = $zstrip(pLines(i),"<>W")
        if pExact { set:line=pText found = 1 } else { set:line[pText found = 1 }
    }
    quit found
}

ClassMethod Get(pCase As %String)
{
    set ok = ..Run(pCase_"-get","default-modifiers -get",.lines)
    do ..Obs(pCase_".get.ok",ok)
    do ..Obs(pCase_".get.none",..Has(.lines,"No default modifiers currently set"))
    do ..Obs(pCase_".get.modestored",..Has(.lines,"-DMode=stored",1))
    do ..Obs(pCase_".get.modesecond",..Has(.lines,"-DMode=second",1))
    do ..Obs(pCase_".get.othersub",..Has(.lines,"-DOther.Sub=x",1))
    do ..Obs(pCase_".get.verbose",..Has(.lines,"-DVerbose=1",1))
}

ClassMethod Cmd(pCase As %String, pCommand As %String)
{
    do ..Obs(pCase_".ok",..Run(pCase,pCommand,.lines))
}

ClassMethod Load(pCase As %String, pExtra As %String = "")
{
    kill ^IPMCONT50
    do ..Run(pCase_"-un","uninstall m50",.ignore)
    set ok = ..Run(pCase,"load "_..#ROOT_" "_pExtra,.lines)
    do ..Obs(pCase_".ok",ok)
    do ..Obs(pCase_".mode",..ModeCode($get(^IPMCONT50("mode"))))
    do ..Obs(pCase_".verbose",..Has(.lines,"Loading file"))
}

ClassMethod All()
{
    do ..Get("g0")
    do ..Cmd("e1","default-modifiers")
    do ..Cmd("e2","default-modifiers -set -get")
    do ..Cmd("s0","default-modifiers -set")
    do ..Get("s0")
    do ..Cmd("s1","default-modifiers -set -DMode=stored")
    do ..Cmd("s2","default-modifiers -set -DOther.Sub=x")
    do ..Get("s2")
    do ..Cmd("s3","default-modifiers -set -DMode=second")
    do ..Get("s3")
    do ..Load("l1")
    do ..Load("l2","-DMode=cmd")
    do ..Get("l2")
    do ..Cmd("v0","default-modifiers -set -DVerbose=1")
    do ..Load("v1")
    do ..Load("v2","-quiet")
    do ..Cmd("f1","default-modifiers -set -verbose")
    do ..Get("f1")
    do ..Cmd("d0","default-modifiers -delete")
    do ..Get("d0")
    do ..Load("d1")
    write !,"NATIVE-END",!
}

}
