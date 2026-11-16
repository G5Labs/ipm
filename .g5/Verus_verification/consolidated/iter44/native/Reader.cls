/// Test-only observer for iteration44. Not IPM code.
Class IPMCont.CommentReader
{

Parameter ROOT = "/tmp/iter44/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod DocText(pName As %String) As %String
{
    set text = ""
    set doc = ##class(%RoutineMgr).%OpenId(pName_".ZPM")
    if $isobject(doc) { do doc.Code.Rewind() set text = doc.Code.Read(1000000) }
    quit text
}

ClassMethod FileText(pPath As %String) As %String
{
    set s = ##class(%Stream.FileCharacter).%New()
    do s.LinkToFile(pPath)
    quit s.Read(1000000)
}

ClassMethod Count(pText As %String, pNeedle As %String) As %Integer
{
    quit $length(pText,pNeedle) - 1
}

/// Code for the element that follows a comment: Name values 1 c44, 2 c44b, 3 c44a; resources 4 Z, 5 A; 9 other; 0 comment absent.
ClassMethod Next(pText As %String, pComment As %String) As %Integer
{
    quit:pText'[pComment 0
    set after = $piece(pText,pComment,2,*)
    set after = $piece(after,"-->",2,*)
    set tag = $piece($piece(after,"<",2),">",1)
    if $piece(tag," ")="Name" {
        set value = $piece($piece(after,">",2),"<",1)
        quit $case(value,"c44":1,"c44b":2,"c44a":3,:9)
    }
    quit:tag["C44Z.PKG" 4
    quit:tag["C44A.PKG" 5
    quit 9
}

ClassMethod State(pCase As %String, pName As %String)
{
    set text = ..DocText(pName)
    write !,"DOC|",pCase,"|",$replace($replace(text,$char(13),""),$char(10),"\n"),!
    for i=0:1:6 {
        set c = "C"_i_" "
        do ..Obs(pCase_".c"_i_".count",..Count(text,"<!-- "_c))
        do ..Obs(pCase_".c"_i_".next",..Next(text,"<!-- "_c))
    }
    do ..Obs(pCase_".comments",..Count(text,"<!--"))
    set pz = $find(text,"C44Z.PKG"), pa = $find(text,"C44A.PKG")
    do ..Obs(pCase_".zfirst",(pz>0)&&(pa>0)&&(pz<pa))
    set pb = $find(text,">c44b<"), pd = $find(text,">c44a<")
    do ..Obs(pCase_".bfirst",(pb>0)&&(pd>0)&&(pb<pd))
    set mod = ##class(%IPM.Storage.Module).NameOpen(pName)
    do ..Obs(pCase_".resources",$select($isobject(mod):mod.Resources.Count(),1:-1))
    do ..Obs(pCase_".deps",$select($isobject(mod):mod.Dependencies.Count(),1:-1))
    do ..Obs(pCase_".diskcomments",..Count(..FileText(..#ROOT_pName_"/module.xml"),"<!--"))
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

ClassMethod All()
{
    set r = ..#ROOT
    set ok = ..Shell("setup","load "_r_"c44a") && ..Shell("setup","load "_r_"c44b") && ..Shell("setup","load "_r_"c44 -dev") && ..Shell("setup","load "_r_"n44 -dev")
    do ..Obs("setup.ok",ok)
    do ..State("load","c44")
    do ..Obs("a1.ok",..Shell("a1","arrange c44"))
    do ..State("a1","c44")
    do ..Obs("a2.ok",..Shell("a2","arrange c44"))
    do ..State("a2","c44")
    do ..State("nload","n44")
    do ..Obs("n1.ok",..Shell("n1","arrange n44"))
    do ..State("n1","n44")
    // Neighbour: the namespace has a source-control class (the stock base class reports documents editable).
    set sc = ##class(%Studio.SourceControl.Interface).SourceControlClassSet("%Studio.SourceControl.Base",$namespace)
    if 'sc { write !,"ERR|scset|",$system.Status.GetErrorText(sc),! }
    do ..Obs("scset.ok",$system.Status.IsOK(sc))
    do ..Obs("s1.ok",..Shell("s1","arrange c44"))
    do ..State("s1","c44")
    do ..Obs("s2.ok",..Shell("s2","arrange c44"))
    do ..State("s2","c44")
    do ..Obs("s3.ok",..Shell("s3","arrange n44"))
    do ..State("s3","n44")
    write !,"NATIVE-END",!
}

}
