/// Test-only observer for iteration31. Not IPM code.
Class Iter31.Reader
{

ClassMethod Code(pVersion As %String) As %Integer
{
    quit:pVersion="" 0
    quit ($piece(pVersion,".",1)*100) + ($piece(pVersion,".",2)*10) + $piece(pVersion,".",3)
}

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Installed(pName As %String) As %String
{
    quit:'##class(%IPM.Storage.Module).NameExists(pName) ""
    quit ##class(%IPM.Storage.Module).NameOpen(pName).VersionString
}

ClassMethod Cmd(pLabel As %String, pCommand As %String)
{
    write !,"BEGIN|",pLabel,"|",pCommand
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set text = ""
    for i=1:1:$get(lines) { set line = $zstrip($get(lines(i)),"*C") write !,"OUT|",pLabel,"|",line set text = text_" "_line }
    set err = $select($system.Status.IsOK(sc):"",1:$system.Status.GetErrorText(sc))
    if err'="" { write !,"ERR|",pLabel,"|",err }
    set all = text_" "_err
    do ..Obs(pLabel_".ok",$system.Status.IsOK(sc))
    do ..Obs(pLabel_".msg-installed-req",all["does not satisfy the requirements of other modules installed")
    do ..Obs(pLabel_".msg-no-satisfactory",all["Could not find satisfactory version")
    do ..State(pLabel)
    write !,"END|",pLabel
}

ClassMethod State(pLabel As %String)
{
    do ..Obs(pLabel_".b",..Code(..Installed("i31b")))
    set code = 0
    if ##class(%Dictionary.CompiledMethod).%ExistsId("Iter31B.Info||Version") { set code = ..Code($classmethod("Iter31B.Info","Version")) }
    do ..Obs(pLabel_".bcode",code)
    do ..Obs(pLabel_".a",..Installed("i31a")'="")
    do ..Obs(pLabel_".p",..Installed("i31p")'="")
}

ClassMethod Reset(pLabel As %String)
{
    for name="i31p","i31a","i31b" {
        if ##class(%IPM.Main).Shell("uninstall "_name_" -force") {}
    }
    do ..State(pLabel)
}

ClassMethod Run()
{
    do ..Cmd("repo","repo -fs -name r31 -depth 1 -path /tmp/iter31/repo/")
    // S1: no installed requirement on B
    do ..Cmd("s1-b15","install i31b 1.5.0")
    do ..Cmd("s1-p","install i31p")
    do ..Reset("s1-reset")
    // S2: A installed with requirement >=1.0.0 <2.0.0 on B
    do ..Cmd("s2-b15","install i31b 1.5.0")
    do ..Cmd("s2-a","install i31a")
    do ..Cmd("s2-p","install i31p")
    // S3/S4/S5 on S2 state: explicit 2.0.0, unversioned, explicit satisfying 1.7.0
    do ..Cmd("s3-b20","install i31b 2.0.0")
    do ..Cmd("s4-bany","install i31b")
    do ..Cmd("s5-b17","install i31b 1.7.0")
    // S6: directory load of B 2.0.0 with A and P still installed
    do ..Cmd("s6-load20","load /tmp/iter31/loose/i31b-2.0.0/")
    do ..Reset("s6-reset")
    // S7: requirement removed by uninstalling A before explicit 2.0.0
    do ..Cmd("s7-b15","install i31b 1.5.0")
    do ..Cmd("s7-a","install i31a")
    do ..Cmd("s7-una","uninstall i31a")
    do ..Cmd("s7-b20","install i31b 2.0.0")
    write !,"NATIVE-END",!
}

}
