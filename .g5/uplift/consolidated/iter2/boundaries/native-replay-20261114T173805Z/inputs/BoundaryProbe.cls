Class Iter2.BoundaryProbe Extends %RegisteredObject
{
ClassMethod Hold(tag As %String)
{
    set h=##class(%IPM.General.HistoryTemp).LoadInit("iter2-"_tag)
    do h.SetName("iter2-"_tag_"-changed")
    set ^Iter2Boundary(tag,"job")=$job,^Iter2Boundary(tag,"temp")=h.%Id(),^Iter2Boundary(tag,"twin")=h.PersistedTwin.%Id(),^Iter2Boundary(tag,"ready")=1
    for { hang 1 }
}
ClassMethod Read(tag As %String)
{
    set tid=$get(^Iter2Boundary(tag,"temp")),pid=$get(^Iter2Boundary(tag,"twin"))
    write !,"OBS ",tag," tid=",tid," pid=",pid," temp=",##class(%IPM.General.HistoryTemp).%ExistsId(tid)," permanent=",##class(%IPM.General.History).%ExistsId(pid)
    set p=##class(%IPM.General.History).%OpenId(pid)
    if $isobject(p) { write " package=",p.Package," final=",p.Finalized," timeend=",(p.TimeEnd'="") }
    write !
}
ClassMethod DeleteChild(tag As %String, pkg As %String)
{
    set ^Iter2Boundary(tag,"started")=1
    set filter("Package")=pkg
    try { set n=##class(%IPM.General.History).DeleteHistory(.filter),^Iter2Boundary(tag,"count")=n } catch ex { set ^Iter2Boundary(tag,"error")=ex.DisplayString() }
    set ^Iter2Boundary(tag,"done")=1
}
ClassMethod DeletePair(locked As %Boolean)
{
    set tag="delete-"_locked,pkg="iter2-"_tag
    kill ^Iter2Boundary(tag)
    set filter("Package")=pkg,n=##class(%IPM.General.History).DeleteHistory(.filter)
    set h=##class(%IPM.General.HistoryTemp).LoadInit(pkg),id=h.PersistedTwin.%Id()
    if locked {
        tstart
        set sc=##class(%IPM.General.History).%LockId(id,0,0)
    }
    job ..DeleteChild(tag,pkg)
    for i=1:1:100 { quit:$get(^Iter2Boundary(tag,"started"))
        hang .02 }
    hang 1
    write !,"OBS ",tag," done_before_finalize=",+$get(^Iter2Boundary(tag,"done"))," row_before_finalize=",##class(%IPM.General.History).%ExistsId(id)
    set f=h.Finalize(1)
    try { set sc=h.PersistToTwin() } catch ex { set sc=ex.AsStatus() }
    write " finalize=",$system.Status.IsOK(f)," persist=",$system.Status.IsOK(sc)
    if locked {
        do ##class(%IPM.General.History).%UnlockId(id,0,1)
        tcommit
    }
    for i=1:1:200 { quit:$get(^Iter2Boundary(tag,"done"))
        hang .02 }
    write " done=",+$get(^Iter2Boundary(tag,"done"))," deleted=",+$get(^Iter2Boundary(tag,"count"))," finalrow=",##class(%IPM.General.History).%ExistsId(id)," error=",$get(^Iter2Boundary(tag,"error")),!
    set h=""
    write "OBS ",tag," afterclose=",##class(%IPM.General.History).%ExistsId(id)," originalid=",id
    set final=##class(%IPM.General.History).%OpenId(id)
    if $isobject(final) { write " package=",final.Package," finalized=",final.Finalized," ended=",(final.TimeEnd'="")," success=",$system.Status.IsOK(final.Success) }
    write !
}
ClassMethod LockChild(tag As %String, cls As %String, id As %String)
{
    set sc=$classmethod(cls,"%LockId",id,0,0)
    set ^Iter2Boundary(tag,"acquired")=$system.Status.IsOK(sc),^Iter2Boundary(tag,"done")=1
    if $system.Status.IsOK(sc) { do $classmethod(cls,"%UnlockId",id,0,1) }
}
ClassMethod TestOther(tag As %String, cls As %String, id As %String) As %Boolean
{
    kill ^Iter2Boundary(tag)
    job ..LockChild(tag,cls,id)
    for i=1:1:100 { quit:$get(^Iter2Boundary(tag,"done"))
        hang .02 }
    return +$get(^Iter2Boundary(tag,"acquired"))
}
ClassMethod LockPair(mode As %Boolean)
{
    set r=##class(%IPM.Repo.Filesystem.Definition).%New(),r.Name="iter2-lock-"_$increment(^Iter2Boundary("seq")),r.Root="/tmp/iter2-lock-"_$get(^Iter2Boundary("seq"))_"/"
    set sc=r.%Save(),id=r.%Id(),cls=$classname(r)

    tstart
    set m=##class(%IPM.Utils.LockManager).%New(mode),sc=m.LockClassId(cls,id,0,0)
    set before=..TestOther("lock-before-"_mode,cls,id)
    set m=""
    set closed=..TestOther("lock-close-"_mode,cls,id)
    tcommit
    set committed=..TestOther("lock-commit-"_mode,cls,id)
    write !,"OBS lock-",mode," saved=",$system.Status.IsOK(sc)," acquired_before_close=",before," acquired_after_close=",closed," acquired_after_commit=",committed,!
}

ClassMethod CachePair(outer As %Boolean)
{
    set r=##class(%IPM.Repo.Filesystem.Definition).%New(),r.Name="iter2-cache-"_$increment(^Iter2Boundary("seq")),r.Root="/tmp/iter2-empty-"_$get(^Iter2Boundary("seq"))_"/"
    set sc=##class(%File).CreateDirectory(r.Root)
    set sc=r.%Save(),id=r.%Id(),cls=$classname(r)
    write !,"OBS setup save=",$system.Status.IsOK(sc)," id=",id," text=",$system.Status.GetErrorText(sc),!
    if outer { tstart }
    set sc=r.BuildCache(1,0,0)
    set after=..TestOther("cache-return-"_outer,cls,id)
    if outer { tcommit }
    set committed=..TestOther("cache-commit-"_outer,cls,id)
    write !,"OBS cache-",outer," status=",$system.Status.IsOK(sc)," acquired_after_return=",after," acquired_after_commit=",committed,!
}

ClassMethod RoleExists(role As %String) As %Boolean
{
    new $namespace
    set $namespace="%SYS"
    return ##class(Security.Roles).Exists(role)
}
ClassMethod CPFPair(which As %String)
{
    set role="Iter2BoundaryCPF",before=..RoleExists(role)
    set m=##class(%IPM.Storage.Module).%New(),m.Root="/tmp/iter2-cpf/",m.SourcesRoot=""
    set rr=##class(%IPM.Storage.ResourceReference).%New(),rr.Module=m
    do rr.Attributes.SetAt(which_".cpf","Name"),rr.Attributes.SetAt("","Directory")
    set p=##class(%IPM.ResourceProcessor.CPF).%New(rr)
    kill ^Iter2Boundary("cpfsentinel")
    tstart
    set ^Iter2Boundary("cpfsentinel")=1
    set sc=p.DoMerge(.params),during=..RoleExists(role)
    trollback
    set after=..RoleExists(role)
    write !,"OBS cpf-",which," status=",$system.Status.IsOK(sc)," before=",before," during=",during," afterrollback=",after," dbsentinel=",$data(^Iter2Boundary("cpfsentinel")),!
    if $system.Status.IsError(sc) { write "OBS cpf-error ",$system.Status.GetErrorText(sc),! }
}
}
