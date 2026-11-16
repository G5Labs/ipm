/// Test-only observer for iteration56. Not IPM code.
Class IPMCont.LocReader
{

Parameter SRC = "/tmp/iter56/src/";

Parameter WORK = "/tmp/iter56work/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Code(pText As %String) As %Integer
{
    quit $case(pText,"":0,"alpha":1,"alpha2":2,"beta":3,"gamma":4,:99)
}

/// Copy a source tree into a directory this process owns, so permissions can be changed.
ClassMethod CopyTree(pFrom As %String, pTo As %String) As %Status
{
    quit ##class(%File).CopyDir(pFrom,pTo,1)
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

ClassMethod State(pCase As %String, pDomain As %String, pRoot As %String, pModule As %String)
{
    for id="A","B","C" { do ..Obs(pCase_".msg."_id,..Code($get(^IRIS.Msg(pDomain,"en",id)))) }
    do ..Obs(pCase_".doc",##class(%RoutineMgr).Exists($select(pModule="loc56":"D56.Msgs.LOC",1:"D56R.Msgs.LOC")))
    do ..Obs(pCase_".module",##class(%IPM.Storage.Module).NameExists(pModule))
    set file = pRoot_"localize/D56Msgs.xml"
    set:pModule="loc56ro" file = pRoot_"localize/D56RMsgs.xml"
    do ..Obs(pCase_".exportfile",##class(%File).Exists(file))
}

ClassMethod WriteLoc(pDir As %String, pName As %String, pDomain As %String, pLines As %List)
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(pDir))
    do s.Clear()
    do s.WriteLine("<?xml version=""1.0"" encoding=""UTF-8""?>")
    do s.WriteLine("<Export generator=""IRIS"" version=""26"">")
    do s.WriteLine("<Document name="""_pName_"""><MsgFile Language=""en"">")
    do s.WriteLine("<MsgDomain Domain="""_pDomain_""">")
    set ptr = 0
    while $listnext(pLines,ptr,line) { do s.WriteLine(line) }
    do s.WriteLine("</MsgDomain>")
    do s.WriteLine("</MsgFile>")
    do s.WriteLine("</Document></Export>")
    $$$ThrowOnError(s.%Save())
}

ClassMethod All()
{
    set w = ..#WORK
    do ##class(%File).CreateDirectoryChain(w)
    // Writable copies.
    set ok = ..CopyTree(..#SRC_"loc56/",w_"loc56/") && ..CopyTree(..#SRC_"plain56/",w_"plain56ro/")
    // Read-only module with a .LOC: same layout, other names.
    set ok = ok && ..CopyTree(..#SRC_"loc56/",w_"loc56ro/")
    $$$ThrowOnError(##class(%File).Delete(w_"loc56ro/localize/D56/Msgs.LOC"))
    do ##class(%File).CreateDirectoryChain(w_"loc56ro/localize/D56R/")
    do ##class(%File).RemoveDirectory(w_"loc56ro/localize/D56/")
    do ..WriteLoc(w_"loc56ro/localize/D56R/Msgs.LOC","D56R.Msgs.LOC","D56RDom",$lb("<Message Id=""A"">alpha</Message>"))
    set x = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(x.LinkToFile(w_"loc56ro/module.xml"))
    set text = $replace($replace(x.Read(100000),"loc56","loc56ro"),"D56.Msgs.LOC","D56R.Msgs.LOC")
    do x.Clear() do x.Write(text) $$$ThrowOnError(x.%Save())
    set x = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(x.LinkToFile(w_"plain56ro/module.xml"))
    set text = $replace(x.Read(100000),"plain56","plain56ro")
    do x.Clear() do x.Write(text) $$$ThrowOnError(x.%Save())
    do $zf(-100,"/SHELL","chmod","-R","a-w",w_"loc56ro")
    do $zf(-100,"/SHELL","chmod","-R","a-w",w_"plain56ro")
    do ..Obs("setup.ok",ok)
    do ..Obs("setup.rowritable",##class(%File).Writeable(w_"loc56ro/"))
    do ..Obs("l1.ok",..Shell("l1","load "_w_"loc56"))
    do ..State("l1","D56Dom",w_"loc56/","loc56")
    do ..WriteLoc(w_"loc56/localize/D56/Msgs.LOC","D56.Msgs.LOC","D56Dom",$lb("<Message Id=""A"">alpha2</Message>","<Message Id=""C"">gamma</Message>"))
    do ..Obs("l2.ok",..Shell("l2","load "_w_"loc56"))
    do ..State("l2","D56Dom",w_"loc56/","loc56")
    do ..Obs("u1.ok",..Shell("u1","uninstall loc56"))
    do ..State("u1","D56Dom",w_"loc56/","loc56")
    do ..Obs("r1.ok",..Shell("r1","load "_w_"loc56ro"))
    do ..State("r1","D56RDom",w_"loc56ro/","loc56ro")
    do ..Obs("n1.ok",..Shell("n1","load "_w_"plain56ro"))
    do ..Obs("n1.module",##class(%IPM.Storage.Module).NameExists("plain56ro"))
    write !,"NATIVE-END",!
}

}
