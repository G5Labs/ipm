/// Test-only observer for iteration39. Not IPM code.
Class IPMCont.DocReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Code(pVersion As %String) As %Integer
{
    quit $case(pVersion,"":0,"1.0.0":1,"1.1.0":2,"0.5.0":3,"0.0.1+snapshot":4,"1.2.0":5,:99)
}

ClassMethod Manifest(pName As %String, pVersion As %String) As %String
{
    set nl = $char(10)
    quit "<?xml version=""1.0"" encoding=""UTF-8""?>"_nl_"<Export generator=""IRIS"" version=""26"">"_nl_"<Document name=""m39.ZPM"">"_nl_"<Module>"_nl_"<Name>"_pName_"</Name>"_nl_"<Version>"_pVersion_"</Version>"_nl_"<Packaging>module</Packaging>"_nl_"<SourcesRoot>src</SourcesRoot>"_nl_"<Resource Name=""M39.PKG""/>"_nl_"</Module>"_nl_"</Document>"_nl_"</Export>"_nl
}

/// Saves new content into the m39.ZPM document through %RoutineMgr, as an editor would.
ClassMethod SaveDoc(pCase As %String, pText As %String) As %Boolean
{
    set doc = ##class(%RoutineMgr).%OpenId("m39.ZPM")
    if '$isobject(doc) { write !,"ERR|",pCase,"|open failed",! quit 0 }
    do doc.Code.Clear()
    do doc.Code.Write(pText)
    set sc = doc.%Save()
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod State(pCase As %String)
{
    set mod = ##class(%IPM.Storage.Module).NameOpen("m39")
    do ..Obs(pCase_".record",$isobject(mod))
    do ..Obs(pCase_".version",$select($isobject(mod):..Code(mod.VersionString),1:0))
    do ..Obs(pCase_".resources",$select($isobject(mod):mod.Resources.Count(),1:-1))
    do ..Obs(pCase_".other",##class(%IPM.Storage.Module).NameExists("m39other"))
    do ..Obs(pCase_".doc",##class(%RoutineMgr).Exists("m39.ZPM"))
    do ..Obs(pCase_".class",##class(%Dictionary.CompiledClass).%ExistsId("M39.Main"))
    set text = ""
    set doc = ##class(%RoutineMgr).%OpenId("m39.ZPM")
    if $isobject(doc) { do doc.Code.Rewind() set text = doc.Code.Read(100000) }
    do ..Obs(pCase_".doctext.other",text["<Name>m39other</Name>")
    do ..Obs(pCase_".doctext.v110",text["<Version>1.1.0</Version>")
    do ..Obs(pCase_".doctext.v120",text["<Version>1.2.0</Version>")
    do ..Obs(pCase_".doctext.v050",text["<Version>0.5.0</Version>")
    do ..Obs(pCase_".doctext.empty",(text=""))
}

ClassMethod All()
{
    set sc = ##class(%IPM.Main).Shell("load /tmp/iter39/m39")
    do ..Obs("setup.ok",$system.Status.IsOK(sc))
    do ..State("setup")
    do ..Obs("e1.ok",..SaveDoc("e1",..Manifest("m39","1.1.0")))
    do ..State("e1")
    do ..Obs("e2.ok",..SaveDoc("e2","<Export><Document name=""m39.ZPM""><Module><Name>m39</Name>"))
    do ..State("e2")
    do ..Obs("e3.ok",..SaveDoc("e3",..Manifest("m39other","1.2.0")))
    do ..State("e3")
    do ..Obs("e4.ok",..SaveDoc("e4",..Manifest("m39","0.5.0")))
    do ..State("e4")
    do ..Obs("e5.ok",..SaveDoc("e5",""))
    do ..State("e5")
    set sc = ##class(%RoutineMgr).Delete("m39.ZPM")
    if 'sc { write !,"ERR|e6|",$system.Status.GetErrorText(sc),! }
    do ..Obs("e6.ok",$system.Status.IsOK(sc))
    do ..State("e6")
    write !,"NATIVE-END",!
}

}
