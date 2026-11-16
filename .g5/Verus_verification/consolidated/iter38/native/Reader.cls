/// Test-only observer for iteration38. Not IPM code.
Class IPMCont.HistoryReader
{

Parameter ROOT = "/tmp/iter38/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Shell(pCase As %String, pCommand As %String, Output pLines) As %Boolean
{
    kill pLines
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.pLines)
    for i=1:1:$get(pLines) { set pLines(i) = $get(pLines(i)) write !,"OUT|",pCase,"|",pLines(i) }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit $system.Status.IsOK(sc)
}

ClassMethod MaxId() As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select max(ID) m from %IPM_General.History")
    quit:'rs.%Next() 0
    quit +rs.%Get("m")
}

ClassMethod Count() As %Integer
{
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select count(*) c from %IPM_General.History")
    quit:'rs.%Next() 0
    quit +rs.%Get("c")
}

ClassMethod PhaseCode(pName As %String) As %Integer
{
    quit $case(pName,"Clean":0,"Initialize":1,"Reload":2,"Validate":4,"Compile":6,"Activate":7,"":-1,:99)
}

/// Stored row fields, read directly.
ClassMethod Stored(pKey As %String, pId As %Integer)
{
    set obj = ##class(%IPM.General.History).%OpenId(pId)
    do ..Obs("st."_pKey_".exists",$isobject(obj))
    quit:'$isobject(obj)
    do ..Obs("st."_pKey_".action",$case(obj.Action,"load":1,"install":2,"uninstall":3,"update":4,:0))
    do ..Obs("st."_pKey_".package",obj.Package'="")
    do ..Obs("st."_pKey_".vunset",obj.Version.ToString()="0.0.0-0")
    do ..Obs("st."_pKey_".ok",$system.Status.IsOK(obj.Success))
    do ..Obs("st."_pKey_".committed",+obj.Committed)
    do ..Obs("st."_pKey_".ended",obj.TimeEnd'="")
    do ..Obs("st."_pKey_".source",obj.SourceName'="")
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select Phases from %IPM_General.History where ID = ?",pId)
    do rs.%Next()
    set phases = rs.%Get("Phases"), n = 0, ptr = 0
    while $listnext(phases,ptr,phase) {
        set n = n + 1, p = $listget(phase,1)
        do ..Obs("st."_pKey_".p"_n_".name",..PhaseCode($listget(p,1)))
        do ..Obs("st."_pKey_".p"_n_".ended",$listget(p,3)'="")
        set s = $listget(p,4)
        do ..Obs("st."_pKey_".p"_n_".status",$select(s="":2,$system.Status.IsOK(s):1,1:0))
    }
    do ..Obs("st."_pKey_".phases",n)
}

/// Rendered summary row and details for one record.
ClassMethod Rendered(pKey As %String, pId As %Integer, pVersion As %String)
{
    do ..Shell("find-"_pKey,"history find -limit 0",.lines)
    set row = ""
    for i=1:1:$get(lines) { if $piece($zstrip(lines(i),"<W")," ")=pId { set row = lines(i) } }
    do ..Obs("find."_pKey_".row",row'="")
    do ..Obs("find."_pKey_".vunset",row[" <unset> ")
    do ..Obs("find."_pKey_".vtext",(pVersion'="")&&(row[(" v"_pVersion_" ")))
    do ..Obs("find."_pKey_".success",row[" Success ")
    do ..Obs("find."_pKey_".committed",row[" Committed ")
    do ..Obs("find."_pKey_".uncommitted",row[" Uncommitted ")
    do ..Obs("find."_pKey_".na",$zstrip(row,">W")?.E1" n/a")
    do ..Obs("find."_pKey_".nosource",row["<no source set>")
    do ..Obs("find."_pKey_".dur",row[" sec) ")
    do ..Shell("det-"_pKey,"history details "_pId_" -phases",.lines)
    set inPhases = 0, n = 0
    for i=1:1:$get(lines) {
        set line = $zstrip(lines(i),"<>W")
        if line?1"Source: ".E { do ..Obs("det."_pKey_".source.na",line="Source: n/a") }
        if line?1"Version: ".E { do ..Obs("det."_pKey_".vunset",line="Version: <unset>") }
        if line?1"Time: ".E { do ..Obs("det."_pKey_".endedtext",'(line?.E1"Ended At")) }
        if line?1"Committed: ".E { do ..Obs("det."_pKey_".committed",line="Committed: Committed") }
        if line?1"Phases:".E { set inPhases = 1 continue }
        continue:'inPhases
        set name = $piece(line," ",1)
        continue:(name="")||(name="Name")
        set n = n + 1
        do ..Obs("det."_pKey_".p"_n_".name",..PhaseCode(name))
        do ..Obs("det."_pKey_".p"_n_".success",$piece(line," ",*)="Success")
        do ..Obs("det."_pKey_".p"_n_".dur",line[" sec)")
    }
    do ..Obs("det."_pKey_".phases",n)
}

ClassMethod Case(pKey As %String, pCommand As %String, pVersion As %String)
{
    set before = ..MaxId()
    do ..Obs(pKey_".ok",..Shell(pKey,pCommand,.lines))
    set id = ..MaxId()
    do ..Obs(pKey_".new",id>before)
    do ..Stored(pKey,id)
    do ..Rendered(pKey,id,pVersion)
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Obs("setup.color",..Shell("setup","config set ColorScheme none",.l))
    do ..Obs("setup.repo",..Shell("setup2","repo -fs -name r38 -path "_r_"repo/",.l))
    do ..Case("a","load "_r_"m38a","1.2.3")
    do ..Case("b","load "_r_"bad","")
    do ..Case("e","load "_r_"m38e","2.0.0")
    do ..Case("z","load "_r_"m38z","")
    do ..Case("i","install m38r","3.0.0")
    set c0 = ..Count()
    do ..Obs("ma.ok",..Shell("ma","m38a compile",.l))
    do ..Obs("ma.newrecords",..Count()-c0)
    do ..Case("u","uninstall m38a","1.2.3")
    write !,"NATIVE-END",!
}

}
