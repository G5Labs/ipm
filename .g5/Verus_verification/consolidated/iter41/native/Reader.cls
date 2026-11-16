/// Test-only observer for iteration41. Not IPM code.
Class IPMCont.ConfigReader
{

Parameter KEYS = "trackingId,analytics,ColorScheme,TerminalPrompt,PublishTimeout,PipCaller,UseStandalonePip,SemVerPostRelease,DefaultLogEntryLimit,HistoryRetain,TestReportFormat,ModuleRoot";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Small codes for stored text so observations stay integers.
ClassMethod Code(pText As %String) As %Integer
{
    quit:pText="" -1
    quit:pText?1.N +pText
    quit $case($zconvert(pText,"L"),"json":1001,"xml":1002,"yes":1003,"no":1004,:2000)
}

/// Raw node, default node and presence for one storage index, read in %SYS.
ClassMethod Raw(pIndex As %String, Output pValue, Output pDefault, Output pHasDefault, Output pHasValue)
{
    new $namespace
    set $namespace = "%SYS"
    set pHasValue = ($data(^IPM.settings(pIndex))#2)
    set pValue = $get(^IPM.settings(pIndex))
    set pHasDefault = ($data(^IPM.settings(pIndex,"default"))#2)
    set pDefault = $get(^IPM.settings(pIndex,"default"))
}

ClassMethod Key(pCase As %String, pKey As %String)
{
    set idx = $parameter("%IPM.Repo.UniversalSettings",pKey)
    do ..Raw(idx,.v,.def,.hasdef,.hasv)
    do ..Obs(pCase_"."_pKey_".value",..Code(v))
    do ..Obs(pCase_"."_pKey_".hasdefault",hasdef)
    do ..Obs(pCase_"."_pKey_".default",..Code(def))
}

ClassMethod Shell(pCase As %String, pCommand As %String, Output pLines) As %Boolean
{
    kill pLines
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.pLines)
    for i=1:1:$get(pLines) { set pLines(i) = $zstrip($get(pLines(i)),"*C") write !,"OUT|",pCase,"|",pLines(i) }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod Has(ByRef pLines, pText As %String) As %Integer
{
    set n = 0
    for i=1:1:$get(pLines) { set:pLines(i)[pText n = n + 1 }
    quit n
}

ClassMethod Case(pCase As %String, pCommand As %String, pKey As %String = "", Output lines)
{
    set ok = ..Shell(pCase,pCommand,.lines)
    do ..Obs(pCase_".ok",ok)
    do ..Obs(pCase_".notfound",..Has(.lines,"not found"))
    do ..Obs(pCase_".updated",..Has(.lines,"succesfully updated"))
    do ..Obs(pCase_".reset",..Has(.lines,"succesfully reset"))
    do ..Obs(pCase_".error",..Has(.lines,"ERROR"))
    if pKey '= "" { do ..Key(pCase,pKey) }
}

ClassMethod All()
{
    // Baseline after a fresh install: which keys IPM seeded with a default node.
    for i=1:1:$length(..#KEYS,",") { set k = $piece(..#KEYS,",",i) continue:k="ModuleRoot"  do ..Key("c0",k) }
    do ..Case("c1","config set HistoryRetain 5","HistoryRetain")
    do ..Case("c1g","config get HistoryRetain","",.lines)
    do ..Obs("c1g.shows5",..Has(.lines,"HistoryRetain:5"))
    do ..Case("c2","config delete HistoryRetain","HistoryRetain")
    do ..Case("c3","config set PublishTimeout 99","PublishTimeout")
    do ..Case("c3d","config delete PublishTimeout","PublishTimeout")
    do ..Case("c4","config set TestReportFormat xml","TestReportFormat")
    do ..Case("c4n","config set TestReportFormat json","TestReportFormat")
    do ..Case("c5","config set Bogus 1")
    do ..Raw("Bogus",.v,.def,.hasdef,.hasv) do ..Obs("c5.bogusnode",hasv)
    do ..Case("c6","config delete Bogus")
    do ..Case("c7","config set analytics 0","analytics")
    do ..Obs("c7.available",..Code(##class(%IPM.Repo.UniversalSettings).GetAnalyticsAvailable()))
    do ..Obs("c7.consumerdisabled",'##class(%IPM.Repo.UniversalSettings).GetAnalyticsAvailable())
    do ..Case("c8","config set analytics yes","analytics")
    do ..Obs("c8.available",..Code(##class(%IPM.Repo.UniversalSettings).GetAnalyticsAvailable()))
    do ..Obs("c8.consumerdisabled",'##class(%IPM.Repo.UniversalSettings).GetAnalyticsAvailable())
    set ok = ..Shell("c9","config list",.lines)
    do ..Obs("c9.ok",ok)
    for i=1:1:$length(..#KEYS,",") { set k = $piece(..#KEYS,",",i) do ..Obs("c9.listed."_k,..Has(.lines,k_": ")) }
    do ..Obs("c9.listed.registry",..Has(.lines,"registry"))
    write !,"NATIVE-END",!
}

}
