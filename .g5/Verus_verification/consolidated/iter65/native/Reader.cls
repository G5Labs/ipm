/// Test-only observer for iteration65. Not IPM code.
Class IPMCont.ModeReader
{

Parameter ROOT = "/tmp/iter65/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Put(pPath As %String, pLines As %List)
{
    do ##class(%File).CreateDirectoryChain(##class(%File).GetDirectory(pPath))
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(pPath))
    do s.Clear()
    set p = 0
    while $listnext(pLines,p,l) { do s.WriteLine(l) }
    $$$ThrowOnError(s.%Save())
}

/// Writes every source of module pMod (prefix pX = D or N) with value pV.
ClassMethod Sources(pMod As %String, pX As %String, pV As %Integer)
{
    set r = ..#ROOT_pMod_"/", k = "K65"_pX
    do ..Put(r_"src/cls/"_k_"/Main.cls",$lb("Class "_k_".Main","{","","ClassMethod Get() As %Integer","{","    quit "_pV,"}","","}"))
    do ..Put(r_"src/inc/"_k_"Inc.inc",$lb("ROUTINE "_k_"Inc [Type=INC]","#define K65V "_pV))
    do ..Put(r_"src/rtn/"_k_"Rtn.mac",$lb("ROUTINE "_k_"Rtn","Get() quit "_pV))
    do ..Put(r_"src/gbl/"_k_"G.xml",$lb("<?xml version=""1.0"" encoding=""UTF-8""?>","<Export generator=""IRIS"" version=""26"">","<Global>","<Node><Sub>^"_k_"G</Sub>","<Node><Sub>v</Sub>","<Data>"_pV_"</Data>","</Node>","</Node>","</Global>","</Export>"))
    do ..Put(r_"static"_pMod_"/f.txt",$lb(pV))
    do ..Put(r_"src/localize/"_k_"/Msgs.LOC",$lb("<?xml version=""1.0"" encoding=""UTF-8""?>","<Export generator=""IRIS"" version=""26"">","<Document name="""_k_".Msgs.LOC""><MsgFile Language=""en"">","<MsgDomain Domain="""_k_"Dom"">","<Message Id=""A"">v"_pV_"</Message>","</MsgDomain>","</MsgFile>","</Document></Export>"))
    do ..Put(r_"src/dfi/IPMCont"_k_"/Dash.dashboard.xml",$lb("<?xml version=""1.0"" encoding=""UTF-8""?>","<Export generator=""IRIS"" version=""26"">","<Document name=""IPMCont"_k_"-Dash.dashboard.DFI"">","<dashboard xmlns=""http://www.intersystems.com/deepsee/library"" name=""Dash"" folderName=""IPMCont"_k_""" title=""T"_pV_""" description="""" keywords="""" owner="""" shared=""true"" public=""true"" locked=""false"" resource="""" timeCreated=""2026-11-16T12:00:00.000Z"" createdBy=""_SYSTEM"" category="""" bookCover="""" scheme="""" worklistCount=""0"" snapTo=""true"" snapGrid=""true"" gridRows=""10"" gridCols=""10"" canResize=""true"" canModify=""true"" showTitleBar=""true"" companyName="""" companyLogo="""" companyStyle="""" backgroundColor=""white"" backgroundImage=""none"" backgroundRepeat=""no-repeat"" backgroundSize=""100% 100%"" backgroundOpacity=""1"">","</dashboard>","</Document></Export>"))
    do ..Put(r_"module.xml",$lb("<?xml version=""1.0"" encoding=""UTF-8""?>","<Export generator=""IRIS"" version=""26"">","<Document name="""_pMod_".ZPM"">","<Module>","<Name>"_pMod_"</Name>","<Version>1.0.0</Version>","<Packaging>module</Packaging>","<SourcesRoot>src</SourcesRoot>","<Resource Name="""_k_".PKG""/>","<Resource Name="""_k_"Inc.INC""/>","<Resource Name="""_k_"Rtn.MAC""/>","<Resource Name="""_k_"G.GBL""/>","<Resource Name="""_k_".Msgs.LOC""/>","<Resource Name=""IPMCont"_k_"-Dash.dashboard.DFI""/>","<FileCopy Name=""static"_pMod_"/"" Target=""/tmp/iter65out/"_pMod_"/""/>","<WebApplication Name=""/ipmcont65"_$zconvert(pX,"L")_""" NameSpace=""{$namespace}"" Description=""v"_pV_""" Path=""/tmp/iter65/"_pMod_"/static"_pMod_"/"" AutheEnabled=""64""/>","</Module>","</Document>","</Export>"))
}

ClassMethod WebDesc(pName As %String) As %String
{
    new $namespace
    set $namespace = "%SYS"
    set app = ##class(Security.Applications).%OpenId(pName)
    quit $select($isobject(app):app.Description,1:"")
}

ClassMethod Num(pText As %String) As %Integer
{
    set n = $zstrip(pText,"*E'N")
    quit $select(n="":-1,1:+n)
}

ClassMethod State(pCase As %String, pMod As %String, pX As %String)
{
    set k = "K65"_pX, p = pCase_"."_pX_"."
    do ..Obs(p_"cls",$select(##class(%Dictionary.CompiledMethod).%ExistsId(k_".Main||Get"):$classmethod(k_".Main","Get"),1:-1))
    set inc = ##class(%Routine).%OpenId(k_"Inc.INC"), t = ""
    if $isobject(inc) { set t = inc.Read(10000) }
    do ..Obs(p_"inc",..Num($piece(t,"K65V",2)))
    set v = -1
    try { set v = $$Get^@(k_"Rtn") } catch {}
    do ..Obs(p_"mac",v)
    do ..Obs(p_"gbl",$get(@("^"_k_"G(""v"")"),-1))
    set f = ##class(%Stream.FileCharacter).%New(), ft = -1
    if ##class(%File).Exists("/tmp/iter65out/"_pMod_"/f.txt") { do f.LinkToFile("/tmp/iter65out/"_pMod_"/f.txt") set ft = +f.ReadLine() }
    do ..Obs(p_"filecopy",ft)
    do ..Obs(p_"webapp",..Num(..WebDesc("/ipmcont65"_$zconvert(pX,"L"))))
    do ..Obs(p_"loc",..Num($get(^IRIS.Msg(k_"Dom","en","A"))))
    set obj = ##class(%DeepSee.UserLibrary.Utils).%OpenFolderItem("IPMCont"_k_"/Dash.dashboard",.sc)
    do ..Obs(p_"dfi",$select($isobject(obj):..Num(obj.title),1:-1))
}

ClassMethod Shell(pCase As %String, pCommand As %String) As %Boolean
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set note = 0
    for i=1:1:$get(lines) { set l = $zstrip($get(lines(i)),"*C") write !,"OUT|",pCase,"|",l set:l["no sync support" note = note + 1 }
    do ..Obs(pCase_".note",note)
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod Both(pCase As %String, pD As %String, pN As %String)
{
    do ..Obs(pCase_".D.ok",..Shell(pCase_"D",pD))
    do ..Obs(pCase_".N.ok",..Shell(pCase_"N",pN))
    do ..State(pCase,"md65","D")
    do ..State(pCase,"mn65","N")
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Sources("md65","D",1), ..Sources("mn65","N",1)
    do ..Both("s0","load "_r_"md65 -dev","load "_r_"mn65")
    do ..Sources("md65","D",2), ..Sources("mn65","N",2)
    do ..Both("a","load "_r_"md65","load "_r_"mn65")
    do ..Sources("md65","D",3), ..Sources("mn65","N",3)
    do ..Both("b1","md65 reload","mn65 reload")
    do ..State("b1x","md65","D"), ..State("b1x","mn65","N")
    do ..Both("b2","md65 compile","mn65 compile")
    do ..Sources("md65","D",4), ..Sources("mn65","N",4)
    do ..Both("c","sync md65","sync mn65")
    do ..Sources("md65","D",5)
    do ..Obs("v.D.ok",..Shell("vD","sync md65 -verbose"))
    do ..State("v","md65","D")
    write !,"NATIVE-END",!
}

}
