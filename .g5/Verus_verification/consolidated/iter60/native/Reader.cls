/// Test-only observer for iteration60. Not IPM code.
Class IPMCont.LegacyLocReader
{

Parameter ROOT = "/tmp/iter60/";

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

/// 1 when the include exists and its text contains pText; 0 when it exists without it; -1 when absent.
ClassMethod IncHas(pInc As %String, pText As %String) As %Integer
{
    quit:'##class(%RoutineMgr).Exists(pInc_".INC") -1
    set r = ##class(%Routine).%OpenId(pInc_".INC")
    quit:'$isobject(r) -1
    set text = r.Read(1000000)
    quit (text[pText)
}

ClassMethod Msg(pDomain As %String, pId As %String) As %Integer
{
    quit ''$data(^IRIS.Msg(pDomain,"en",pId))
}

ClassMethod State(pCase As %String)
{
    do ..Obs(pCase_".msg.Dom1.1",..Msg("L60Dom1",1))
    do ..Obs(pCase_".msg.Dom1.2",..Msg("L60Dom1",2))
    do ..Obs(pCase_".msg.Dom1.98",..Msg("L60Dom1",98))
    do ..Obs(pCase_".text.Dom1.1.uno",$get(^IRIS.Msg("L60Dom1","en",1))="uno")
    do ..Obs(pCase_".msg.DomA.1",..Msg("L60DomA",1))
    do ..Obs(pCase_".msg.DomB.1",..Msg("L60DomB",1))
    do ..Obs(pCase_".msg.DomM.1",..Msg("L60DomM",1))
    do ..Obs(pCase_".msg.DomM.99",..Msg("L60DomM",99))
    do ..Obs(pCase_".inc1.one",..IncHas("L60Inc1","L60ErrOne"))
    do ..Obs(pCase_".inc1.two",..IncHas("L60Inc1","L60ErrTwo"))
    do ..Obs(pCase_".incA.A",..IncHas("L60IncA","L60ErrA"))
    do ..Obs(pCase_".incA.B",..IncHas("L60IncA","L60ErrB"))
    do ..Obs(pCase_".incB.B",..IncHas("L60IncB","L60ErrB"))
    do ..Obs(pCase_".incB.A",..IncHas("L60IncB","L60ErrA"))
    do ..Obs(pCase_".incM.M",..IncHas("L60IncM","L60ErrM"))
}

ClassMethod WriteMsgs(pPath As %String, pBody As %String)
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(pPath))
    do s.Clear()
    do s.WriteLine("<?xml version=""1.0"" encoding=""UTF-8""?>")
    do s.WriteLine("<MsgFile Language=""en"">")
    do s.Write(pBody)
    do s.WriteLine("</MsgFile>")
    $$$ThrowOnError(s.%Save())
}

/// Lists the resolved children the module's resource reports (what Clean will delete).
ClassMethod Children(pCase As %String, pModule As %String)
{
    set mod = ##class(%IPM.Storage.Module).NameOpen(pModule)
    quit:'$isobject(mod)
    set ref = mod.Resources.GetAt(1)
    kill arr
    do ref.ResolveChildren(.arr)
    do ..Obs(pCase_".child.IncA",$data(arr("L60IncA.INC"))>0)
    do ..Obs(pCase_".child.IncB",$data(arr("L60IncB.INC"))>0)
    do ..Obs(pCase_".child.Inc1",$data(arr("L60Inc1.INC"))>0)
    set n = 0, comma = 0, k = ""
    for { set k = $order(arr(k)) quit:k=""  set n = n + 1 write !,"CHILD|",pCase,"|",k set:k["," comma = 1 }
    do ..Obs(pCase_".child.count",n)
    do ..Obs(pCase_".child.commaname",comma)
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Obs("p1.ok",..Shell("p1","load "_r_"ls60"))
    do ..State("p1")
    do ..Children("p1","ls60")
    do ..Obs("p2.ok",..Shell("p2","load "_r_"lm60"))
    do ..State("p2")
    do ..Children("p2","lm60")
    set ^IRIS.Msg("L60Dom1","en",98) = "pre"
    do ..WriteMsgs(r_"ls60/msgs/errors.xml","<MsgDomain Domain=""L60Dom1"">"_$c(10)_"<Message Id=""1"" Name=""L60ErrOne"">uno</Message>"_$c(10)_"</MsgDomain>"_$c(10))
    do ..Obs("p3.ok",..Shell("p3","load "_r_"ls60"))
    do ..State("p3")
    set ^IRIS.Msg("L60DomM","en",99) = "pre"
    do ..Obs("p4.ok",..Shell("p4","load "_r_"lg60"))
    do ..State("p4")
    do ..Obs("u1.ok",..Shell("u1","uninstall lm60"))
    do ..State("u1")
    do ..Obs("u2.ok",..Shell("u2","uninstall ls60"))
    do ..State("u2")
    write !,"NATIVE-END",!
}

}
