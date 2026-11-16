/// Test-only observer for iteration58. Not IPM code.
Class IPMCont.DfiReader
{

Parameter ROOT = "/tmp/iter58/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// 0 absent; otherwise the numeric suffix of the title (T1 -> 1, T2 -> 2).
ClassMethod Title(pFull As %String) As %Integer
{
    set obj = ##class(%DeepSee.UserLibrary.Utils).%OpenFolderItem(pFull,.sc)
    quit:'$isobject(obj) 0
    quit +$extract(obj.title,2,*)
}

ClassMethod State(pCase As %String)
{
    for item="IPMCont58/DashA","IPMCont58/DashB","IPMCont58W/W1","IPMCont58W/W2","IPMCont58W/W3","IPMCont58W/W4","IPMCont58N/DashN" {
        do ..Obs(pCase_".item."_$translate(item,"/","."),..Title(item_".dashboard"))
    }
}

ClassMethod Shell(pCase As %String, pCommand As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    for i=1:1:$get(lines) { write !,"OUT|",pCase,"|",$zstrip($get(lines(i)),"*C") }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..State(pCase)
}

ClassMethod SetTitle(pFile As %String, pTitle As %String)
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(..#ROOT_pFile))
    set text = s.Read(1000000)
    set text = $replace(text,"title=""T1""","title="""_pTitle_"""")
    do s.Clear() do s.Write(text) $$$ThrowOnError(s.%Save())
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Shell("n1","load "_r_"m58n")
    do ..Shell("d1","load "_r_"m58 -dev")
    do ..SetTitle("m58/src/dfi/IPMCont58/DashA.dashboard.xml","T2")
    do ..Shell("r1","m58 reload -only")
    do ##class(%File).Delete(r_"m58/src/dfi/IPMCont58/DashB.dashboard.xml")
    do ..Shell("r2","m58 reload -only")
    do ..Shell("u1","uninstall m58")
    do ..Shell("u2","uninstall m58n")
    write !,"NATIVE-END",!
}

}
