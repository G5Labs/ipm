/// Test-only observer for iteration67. Not IPM code.
Class IPMCont.CompReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Count recorded calls by extension and method, and whether every recorded name equals pName.
ClassMethod Calls(pCase As %String, pFrom As %Integer, pName As %String = "")
{
    set (p,o,bad) = 0
    for i=pFrom+1:1:$get(^IPMCONT67("log")) {
        set e = ^IPMCONT67("log",i)
        set:$piece(e,"|")="P" p = p + 1
        set:$piece(e,"|")="O" o = o + 1
        set:(pName'="")&&($piece(e,"|",3)'=pName) bad = bad + 1
    }
    do ..Obs(pCase_".p",p), ..Obs(pCase_".o",o)
    do:pName'="" ..Obs(pCase_".namecorrected",'bad)
}

ClassMethod Configure(pPrimary As %String, pOthers As %List) As %Status
{
    kill ^SYS("SourceControl","%IPM.ExtensionBase.Composite")
    set sc = ##class(%IPM.ExtensionBase.Composite).SetExtensionClasses(pPrimary,pOthers)
    quit:'sc sc
    quit ##class(%Studio.SourceControl.Interface).SourceControlClassSet("%IPM.ExtensionBase.Composite")
}

ClassMethod New(Output pSC) As %IPM.ExtensionBase.Composite
{
    set pSC = $$$OK
    try {
        set ext = ##class(%IPM.ExtensionBase.Composite).%New($listbuild($username,"",$job,""))
        if '$isobject(ext) { set pSC = $get(%objlasterror,$$$ERROR($$$GeneralError,"no object")) }
    } catch e { set ext = "", pSC = e.AsStatus() }
    quit ext
}

ClassMethod All()
{
    kill ^IPMCONT67
    set sc = ##class(%IPM.Main).Shell("load /tmp/iter67/m67") do ..Obs("setup.load",$system.Status.IsOK(sc))
    set sc = $system.OBJ.Load("/tmp/iter67/N67.cls","ck-d") do ..Obs("setup.n67",$system.Status.IsOK(sc))
    do ..Obs("setup.config",..Configure("IPMCont.P67",$listbuild("IPMCont.O67")))
    set ext = ..New(.sc) do ..Obs("setup.new",$system.Status.IsOK(sc))
    // a: per-item callbacks reach only the primary; names are corrected.
    for item=$lb("a1","M67.Main.cls"),$lb("a2","N67.X.cls"),$lb("a3","/M67/Main.cls") {
        set c = $list(item,1), n = $list(item,2), from = +$get(^IPMCONT67("log"))
        do ext.OnBeforeSave(n), ext.OnAfterSave(n), ext.OnBeforeLoad(n), ext.OnAfterCompile(n), ext.OnBeforeDelete(n)
        do ..Obs(c_".icon",ext.ItemIconState(n))
        do ..Calls(c,from,$select(c="a2":"N67.X.CLS",1:"M67.Main.CLS"))
    }
    // g: GetStatus combines Editable across extensions.
    for item=$lb("g1",1,0),$lb("g2",1,1),$lb("g3",0,1) {
        set ^IPMCONT67("cfg","P","editable") = $list(item,2), ^IPMCONT67("cfg","O","editable") = $list(item,3)
        set from = +$get(^IPMCONT67("log")), editable = "", insc = ""
        set sc = ext.GetStatus("M67.Main.cls",.insc,.editable)
        do ..Obs($list(item,1)_".ok",$system.Status.IsOK(sc)), ..Obs($list(item,1)_".editable",editable)
        do ..Calls($list(item,1),from)
    }
    set ^IPMCONT67("cfg","P","editable") = 1, ^IPMCONT67("cfg","O","editable") = 1
    // r: IsReadOnly combines read-only across extensions.
    for item=$lb("r1",0,1),$lb("r2",1,0),$lb("r3",0,0) {
        set ^IPMCONT67("cfg","P","ro") = $list(item,2), ^IPMCONT67("cfg","O","ro") = $list(item,3)
        set from = +$get(^IPMCONT67("log"))
        do ..Obs($list(item,1)_".ro",ext.IsReadOnly("M67.Main.cls"))
        do ..Calls($list(item,1),from)
    }
    set ^IPMCONT67("cfg","P","ro") = 0, ^IPMCONT67("cfg","O","ro") = 0
    // u: UserAction routing by type and menu.
    for item=$lb("u1",1,"%MenuX"),$lb("u2",0,"O67Menu,Do"),$lb("u3",0,"Unknown67,Do") {
        set from = +$get(^IPMCONT67("log"))
        set sc = ext.UserAction($list(item,2),$list(item,3),"M67.Main.cls","",.action,.target,.msg,.reload)
        do ..Obs($list(item,1)_".ok",$system.Status.IsOK(sc))
        do ..Calls($list(item,1),from)
    }
    // d: unknown methods dispatch to the primary.
    set from = +$get(^IPMCONT67("log"))
    do ..Obs("d1.result",ext.Custom67())
    do ..Calls("d1",from)
    // s: a nonexistent other-extension name.
    set sc = ..Configure("IPMCont.P67",$listbuild("IPMCont.O67","No.Such67"))
    do ..Obs("s1.config",$system.Status.IsOK(sc))
    set ext2 = ..New(.sc2) do ..Obs("s1.new",$system.Status.IsOK(sc2)), ..Obs("s1.object",$isobject(ext2))
    set stored = $get(^SYS("SourceControl","%IPM.ExtensionBase.Composite","SubExtensions"))
    do ..Obs("s1.stored.nosuch",$listfind(##class(%IPM.ExtensionBase.Composite).GetSubExtensionClasses(),"No.Such67")>0)
    // e: clearing the primary with an empty name.
    do ..Configure("IPMCont.P67",$listbuild("IPMCont.O67"))
    set sc = ##class(%IPM.ExtensionBase.Composite).SetExtensionClasses("",$listbuild("IPMCont.O67"))
    do ..Obs("e1.config",$system.Status.IsOK(sc))
    do ..Obs("e1.primary.p67",##class(%IPM.ExtensionBase.Composite).GetPrimaryExtensionClass()="IPMCont.P67")
    write !,"NATIVE-END",!
}

}
