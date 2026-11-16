Class Consolidated.IndependentThin
{
ClassMethod Put(path As %String, text As %String)
{
    set s=##class(%Stream.FileCharacter).%New()
    set s.Filename=path
    do s.Write(text)
    set sc=s.%Save()
    if $system.Status.IsError(sc) { do $system.Status.DisplayError(sc) }
}
ClassMethod CopyRollback(id As %String, missing As %Boolean = 0)
{
    set root="/tmp/g5-independent-thin/", source=root_"source.txt", target=root_id_".txt"
    do ##class(%File).CreateDirectoryChain(root)
    do ..Put(source,"held-out-data")
    do ##class(%File).Delete(target)
    set m=##class(%IPM.Storage.Module).%New(), m.Root=root
    set r=##class(%IPM.Storage.ResourceReference).%New(), r.Module=m, r.Name="source.txt"
    set p=##class(%IPM.ResourceProcessor.FileCopy).%New(r)
    kill params
    tstart
    set ^G5IndependentThin("rolledback")=1
    set sc=p.DoCopy($select(missing:root_"missing",1:source),target,.params)
    trollback
    write !,"OBS ",id," status=",$system.Status.IsOK(sc)," target=",##class(%File).Exists(target)," db=",$data(^G5IndependentThin("rolledback"))," tlevel=",$tlevel,!
}
ClassMethod Clean(id As %String, keep As %Boolean)
{
    set root="/tmp/g5-independent-thin/", target=root_id_".txt"
    do ..Put(target,"kept-or-deleted")
    set m=##class(%IPM.Storage.Module).%New(), m.Root=root
    set r=##class(%IPM.Storage.ResourceReference).%New(), r.Module=m, r.Name="source.txt"
    set p=##class(%IPM.ResourceProcessor.FileCopy).%New(r), p.Name="source.txt", p.InstallDirectory=target, p.KeepOnUninstall=keep
    set sc=p.OnPhase("Clean",.params,.handled)
    write !,"OBS ",id," status=",$system.Status.IsOK(sc)," target=",##class(%File).Exists(target)," handled=",handled,!
}
ClassMethod LockRollback(id As %String)
{
    set root="/tmp/g5-independent-thin/", path=root_"module-lock.json"
    do ##class(%File).Delete(path)
    set m=##class(%IPM.Storage.Module).%New(),m.Name="g5-independent-thin",m.VersionString="1.2.3",m.Root=root
    set deps=##class(%ListOfObjects).%New()
    tstart
    set ^G5IndependentThin("rolledback")=1
    try { do ##class(%IPM.General.LockFile).CreateLockFileForModule(m,.deps,.params) set ok=1 } catch ex { set ok=0 do ex.Display() }
    trollback
    write !,"OBS ",id," status=",ok," file=",##class(%File).Exists(path)," db=",$data(^G5IndependentThin("rolledback"))," tlevel=",$tlevel,!
}
ClassMethod History(id As %String, deleted As %Boolean)
{
    set pkg="g5-independent-"_id
    set filter("Package")=pkg
    set n=##class(%IPM.General.History).DeleteHistory(.filter)
    set h=##class(%IPM.General.HistoryTemp).LoadInit(pkg)
    set twinid=h.PersistedTwin.%Id()
    write !,"OBS ",id," initial=",##class(%IPM.General.History).%ExistsId(twinid),!
    if deleted {
        set n=##class(%IPM.General.History).DeleteHistory(.filter)
        write "OBS ",id," deleted=",n," afterdelete=",##class(%IPM.General.History).%ExistsId(twinid),!
    }
    set f=h.Finalize(1)
    try { set sc=h.PersistToTwin() } catch ex { set sc=ex.AsStatus() }
    write "OBS ",id," finalize=",$system.Status.IsOK(f)," persist=",$system.Status.IsOK(sc)," endrow=",##class(%IPM.General.History).%ExistsId(twinid),!
    if $system.Status.IsError(sc) { write "OBS ",id," error=",$system.Status.GetErrorText(sc),! }
    set h=""
    write "OBS ",id," afterclose=",##class(%IPM.General.History).%ExistsId(twinid),!
    set n=##class(%IPM.General.History).DeleteHistory(.filter)
}
ClassMethod DeleteChild(pkg As %String)
{
    set filter("Package")=pkg
    try { set n=##class(%IPM.General.History).DeleteHistory(.filter) set ^G5IndependentThin("childDeleteCount")=n } catch ex { set ^G5IndependentThin("childError")=ex.DisplayString() }
    set ^G5IndependentThin("childDone")=1
}
ClassMethod HistoryConcurrent()
{
    set pkg="g5-independent-concurrent"
    kill ^G5IndependentThin("childDeleteCount"),^G5IndependentThin("childError"),^G5IndependentThin("childDone")
    set filter("Package")=pkg, n=##class(%IPM.General.History).DeleteHistory(.filter)
    set h=##class(%IPM.General.HistoryTemp).LoadInit(pkg),id=h.PersistedTwin.%Id()
    set rs=##class(%IPM.General.History).GetHistory(.filter),before=rs.%Next()
    job ..DeleteChild(pkg)
    for i=1:1:100 {
        quit:$get(^G5IndependentThin("childDone"))
        hang .05
    }
    write !,"OBS H-CONCURRENT before=",before," childDone=",$get(^G5IndependentThin("childDone"))," deleted=",$get(^G5IndependentThin("childDeleteCount"))," error=",$get(^G5IndependentThin("childError"))," afterdelete=",##class(%IPM.General.History).%ExistsId(id),!
    set f=h.Finalize(1)
    try { set sc=h.PersistToTwin() } catch ex { set sc=ex.AsStatus() }
    set h="",rs=""
    set rs=##class(%IPM.General.History).GetHistory(.filter),after=rs.%Next()
    write !,"OBS H-CONCURRENT finalize=",$system.Status.IsOK(f)," persist=",$system.Status.IsOK(sc)," afterclose=",after,!
    set rs="",n=##class(%IPM.General.History).DeleteHistory(.filter)
}
ClassMethod Inspection()
{
    set sc=##class(%IPM.Main).Shell("load /tmp/g5-independent-thin/REPORT -dev")
    set m=##class(%IPM.Storage.Module).NameOpen("g5-independent-report"),id=m.%Id(),v=m.VersionString
    for name="g5-independent-report","g5-independent-report-alias","g5-independent-report-missing" {
        set sc=##class(%IPM.Main).Shell("show "_name)
        set after=##class(%IPM.Storage.Module).%OpenId(id)
        write !,"OBS INSPECT-",name," status=",$system.Status.IsOK(sc)," sameid=",(after.%Id()=id)," sameversion=",(after.VersionString=v),!
    }
}
ClassMethod LockWriteConsumers()
{
    for id="LOCKFAIL","LOCKOK" {
        set root="/tmp/g5-independent-thin/"_id_"/",path=root_"module-lock.json"
        set sc=##class(%IPM.Main).Shell("load "_root_" -dev -lock")
        write !,"OBS ",id," status=",$system.Status.IsOK(sc)," file=",(##class(%File).Exists(path) && '##class(%File).DirectoryExists(path))," directory=",##class(%File).DirectoryExists(path),!
    }
}
ClassMethod Fail() As %Status
{
    return $system.Status.Error(5001,"independent held-out activation failure")
}
ClassMethod LoadConsumer(id As %String)
{
    set root="/tmp/g5-independent-thin/"_id_"/", target="/tmp/g5-independent-thin/target-"_id_"/payload.txt"
    do ##class(%File).RemoveDirectoryTree("/tmp/g5-independent-thin/target-"_id_"/")
    set command="load "_root_" -lock"
    set sc=##class(%IPM.Main).Shell(command)
    set m=##class(%IPM.Storage.Module).NameOpen("g5-independent-"_$zconvert(id,"L"))
    write !,"OBS ",id," status=",$system.Status.IsOK(sc)," module=",$isobject(m)," copied=",##class(%File).Exists(target)," rootlock=",##class(%File).Exists(root_"module-lock.json")," cwdlock=",##class(%File).Exists(##class(%File).NormalizeFilename("module-lock.json"))," tlevel=",$tlevel,!
    write "OBS ",id," error=",$system.Status.GetErrorText(sc),!
    set stream=##class(%Stream.FileCharacter).%New()
    set fs=stream.LinkToFile(##class(%File).NormalizeFilename("module-lock.json"))
    if $system.Status.IsOK(fs) { set obj=##class(%DynamicObject).%FromJSON(stream.Read(32000)) write "OBS ",id," lockname=",obj.name,! }
}
ClassMethod Availability()
{
    for port=18171:1:18174 {
        set p=##class(%IPM.Repo.Oras.PackageService).%New(),p.Location="http://127.0.0.1:"_port
        try { set ok=p.IsAvailable() } catch e { set ok=-1 write "OBS ORAS-",port," error=",e.DisplayString(),! }
        write !,"OBS ORAS-",port," available=",ok,!
    }
}
ClassMethod Report()
{
    set file="/tmp/g5-independent-thin/report.json"
    do ##class(%File).Delete(file)
    set sc=##class(%IPM.Main).Shell("load /tmp/g5-independent-thin/REPORT -dev")
    write !,"OBS REPORT-LOAD status=",$system.Status.IsOK(sc),!
    set sc=##class(%IPM.Main).Shell("test g5-independent-report -only -output-file "_file_" -format unknown-independent-format")
    write !,"OBS REPORT-COMPOSE status=",$system.Status.IsOK(sc)," file=",##class(%File).Exists(file),!
    write !,"OBS REPORT-COMPOSE error=",$system.Status.GetErrorText(sc),!
    set file="/tmp/g5-independent-thin/report-neighbor.json"
    do ##class(%File).Delete(file)
    set sc=##class(%IPM.Main).Shell("test g5-independent-report -only -output-file "_file_" -format json")
    write !,"OBS REPORT-NEIGHBOR status=",$system.Status.IsOK(sc)," file=",##class(%File).Exists(file),!

}
ClassMethod Run()
{
    do ..CopyRollback("F-YES",0)
    do ..CopyRollback("F-MISSING",1)
    do ..Clean("C-KEEP",1)
    do ..Clean("C-DELETE",0)
    do ..LockRollback("L-YES")
    do ..History("H-BASE",0)
    do ..History("H-DELETE",1)
    do ..LoadConsumer("IMMEDIATE")
    do ..LoadConsumer("DEFERRED")
    do ..Availability()
    do ..Report()
    write "NATIVE-END",!
}
}
