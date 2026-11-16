/// Test-only observer for iteration34. Not IPM code.
Class IPMCont.VersionReader
{

Parameter ROOT = "/tmp/iter34/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Code(pVersion As %String) As %Integer
{
    quit $case(pVersion,"":0,"1.9.0":1,"1.10.0":2,"2.0.0-beta.1":3,"0.9.0":4,"1.0.0-rc.2":5,"1.0.0-rc.10":6,"2.0.0":7,:99)
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

ClassMethod Installed(pName As %String) As %String
{
    set mod = ##class(%IPM.Storage.Module).NameOpen(pName)
    quit $select($isobject(mod):mod.VersionString,1:"")
}

/// Order of the filesystem cache query that PackageService.ListModules reads.
ClassMethod Order(pCase As %String, pRoot As %String, pName As %String, pExpr As %String, pParams As %String)
{
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select VersionString from %IPM_Repo_Filesystem.Cache_OrderedMatches(?,?,?,?)",pRoot,pName,pExpr,pParams)
    if rs.%SQLCODE < 0 { write !,"ERR|",pCase,"|SQL ",rs.%SQLCODE," ",rs.%Message,! quit }
    set n = 0
    while rs.%Next() { set n = n + 1 do ..Obs(pCase_".pos."_..Code(rs.%Get("VersionString")),n) }
    do ..Obs(pCase_".count",n)
}

ClassMethod Install(pCase As %String, pCommand As %String, pName As %String)
{
    do ..Obs(pCase_".ok",..Shell(pCase,pCommand))
    do ..Obs(pCase_".version",..Code(..Installed(pName)))
    if ..Installed(pName) '= "" { do ..Shell(pCase_"-cleanup","uninstall "_pName) }
    do ..Obs(pCase_".after",..Code(..Installed(pName)))
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Obs("setup.ok",..Shell("setup","repo -fs -name r34a -path "_r_"repoA/"))
    set root = ##class(%IPM.Repo.Filesystem.Definition).%OpenId(##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen("r34a").%Id()).Root
    do ..Order("q1",root,"i34b","*","")
    do ..Order("q2",root,"i34b","*","prerelease")
    do ..Install("a2","install i34b","i34b")
    do ..Install("a3","install i34b 1.x","i34b")
    do ..Install("a4","install i34b 2.0.0-beta.1","i34b")
    do ..Obs("setup2.ok",..Shell("setup2","repo -delete -name r34a"))
    do ..Obs("setup3.ok",..Shell("setup3","repo -fs -name r34p -path "_r_"repoA/ -prereleases 1"))
    do ..Install("a5","install i34b 2.0.0-beta.1","i34b")
    do ..Install("a6","install i34b","i34b")
    do ..Install("a7","install i34b 1.x","i34b")
    do ..Install("a8","install i34b >=1.0","i34b")
    do ..Obs("setup4.ok",..Shell("setup4","repo -fs -name r34c -path "_r_"repoC/ -prereleases 1"))
    set rootc = ##class(%IPM.Repo.Filesystem.Definition).%OpenId(##class(%IPM.Repo.Definition).ServerDefinitionKeyOpen("r34c").%Id()).Root
    do ..Order("q3",rootc,"i34c","*","prerelease")
    do ..Order("q4",rootc,"i34c","*","")
    do ..Install("c1","install i34c","i34c")
    do ..Install("c2","install i34c 1.0.0-rc.2","i34c")
    do ..Order("q5",rootc,"i34d","*","prerelease")
    do ..Install("d1","install i34d","i34d")
    do ..Install("d2","install i34d 2.0.0","i34d")
    write !,"NATIVE-END",!
}

}
