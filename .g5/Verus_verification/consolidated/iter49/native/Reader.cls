/// Test-only observer for iteration49. Not IPM code.
Class IPMCont.ImportReader
{

Parameter ROOT = "/tmp/iter49/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Value(pClass As %String) As %Integer
{
    quit:'##class(%Dictionary.CompiledMethod).%ExistsId(pClass_"||Get") -1
    quit $classmethod(pClass,"Get")
}

ClassMethod Case(pCase As %String, pCommand As %String, pClasses As %List)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set loaded = 0, exporting = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        set:line["item(s) loaded." loaded = loaded + 1
        set:line["Exporting modified files." exporting = exporting + 1
    }
    set err = ""
    if 'sc { set err = $system.Status.GetErrorText(sc) write !,"ERR|",pCase,"|",err,! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".errimport",err["Error in import")
    do ..Obs(pCase_".loadedline",loaded)
    do ..Obs(pCase_".exporting",exporting)
    set ptr = 0
    while $listnext(pClasses,ptr,cls) {
        do ..Obs(pCase_".defined."_cls,##class(%Dictionary.ClassDefinition).%ExistsId(cls))
        do ..Obs(pCase_".value."_cls,..Value(cls))
    }
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Case("i1","import "_r_"x/export.xml",$lb("I49X.A","I49X.B"))
    do ..Case("i2","import "_r_"d/",$lb("I49D.Root","I49D.RootX","I49D.Sub","I49D.SubX"))
    do ..Case("i3","import "_r_"n/ -norecurse",$lb("I49N.Root","I49N.RootX","I49N.Sub","I49N.SubX"))
    do ..Case("i4","import "_r_"c/ -nocompile",$lb("I49C.A","I49C.AX"))
    do ..Case("i5","import "_r_"e/",$lb("I49E.Good","I49E.Bad"))
    do ..Case("i6","import "_r_"missing/nothing.xml","")
    kill ^IPMCONT49
    set sc = $system.OBJ.Load(r_"SC49.cls","ck-d")
    set sc = sc && ##class(%Studio.SourceControl.Interface).SourceControlClassSet("IPMCont.SC49",$namespace)
    do ..Obs("sc.setup",$system.Status.IsOK(sc))
    do ..Case("i7","import "_r_"s/",$lb("I49S.Ctl","I49S.New"))
    do ..Obs("i7.useraction",$get(^IPMCONT49("UserAction",1,6)))
    do ..Obs("i7.uselist.ctl",$get(^IPMCONT49("UserActionList"))["I49S.Ctl")
    do ..Obs("i7.aftersave.ctl",$get(^IPMCONT49("OnAfterSave","I49S.Ctl.CLS")))
    do ..Obs("i7.add.new",$get(^IPMCONT49("AddToSourceControl","I49S.New.CLS")))
    do ..Obs("i7.add.ctl",$get(^IPMCONT49("AddToSourceControl","I49S.Ctl.CLS")))
    do ..Obs("i7.aftersave.new",$get(^IPMCONT49("OnAfterSave","I49S.New.CLS")))
    zwrite ^IPMCONT49
    write !,"NATIVE-END",!
}

}
