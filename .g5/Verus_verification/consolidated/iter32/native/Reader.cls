/// Test-only observer for iteration32. Not IPM code.
Class IPMCont.SyncReader
{

Parameter ROOT = "/tmp/iter32/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod WriteClass(pPath As %String, pClass As %String, pValue As %Integer)
{
    set file = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(file.LinkToFile(pPath))
    do file.Clear()
    for line="Class "_pClass,"{","","ClassMethod Get() As %Integer","{","    quit "_pValue,"}","","}" { do file.WriteLine(line) }
    $$$ThrowOnError(file.%Save())
}

ClassMethod Value(pClass As %String) As %Integer
{
    quit:'##class(%Dictionary.CompiledMethod).%ExistsId(pClass_"||Get") -1
    quit $classmethod(pClass,"Get")
}

ClassMethod Run(pCase As %String, pCommand As %String, ByRef pLines) As %Status
{
    kill pLines
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.pLines)
    for i=1:1:$get(pLines) { set pLines(i) = $zstrip($get(pLines(i)),"*C") write !,"OUT|",pCase,"|",pLines(i) }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    quit sc
}

ClassMethod Count(ByRef pLines, pText As %String) As %Integer
{
    set n = 0
    for i=1:1:$get(pLines) { set:pLines(i)[pText n = n + 1 }
    quit n
}

ClassMethod State(pCase As %String)
{
    for cls="SyncC.A","SyncC.B","SyncC.C","SyncD.D","SyncE.E","SyncF.A","SyncF.B" {
        do ..Obs(pCase_".exists."_cls,##class(%Dictionary.ClassDefinition).%ExistsId(cls))
        do ..Obs(pCase_".value."_cls,..Value(cls))
    }
}

ClassMethod SyncCase(pCase As %String, pCommand As %String)
{
    set sc = ..Run(pCase,pCommand,.lines)
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".nothing",..Count(.lines,"Nothing to sync."))
    do ..Obs(pCase_".notapplied",..Count(.lines,"detected but not applied"))
    do ..Obs(pCase_".deleted",..Count(.lines,"  Deleted: "))
    do ..Obs(pCase_".updated",..Count(.lines,"  Updated: "))
    do ..State(pCase)
}

ClassMethod Setup(pCommand As %String) As %Boolean
{
    set sc = ..Run("setup",pCommand,.lines)
    quit $system.Status.IsOK(sc)
}

ClassMethod All()
{
    set r = ..#ROOT, ok = 1
    for cmd="load "_r_"syncc -dev","load "_r_"syncf -dev","load "_r_"oy -dev","load "_r_"ox -dev","load "_r_"ty -dev","load "_r_"tz","load "_r_"tx -dev","uninstall oy -force","load "_r_"oy -dev","uninstall ty -force","load "_r_"ty -dev" {
        set ok = ok && ..Setup(cmd)
    }
    do ..Obs("setup.ok",ok)
    do ..State("setup")
    do ..SyncCase("s0","sync syncc")
    $$$ThrowOnError(##class(%File).Delete(r_"syncc/src/cls/SyncC/B.cls"))
    do ..SyncCase("s1","sync syncc")
    do ..SyncCase("s2","sync syncc -delete")
    do ..WriteClass(r_"syncc/src/cls/SyncC/A.cls","SyncC.A",2)
    $$$ThrowOnError(##class(%File).Delete(r_"syncc/src/cls/SyncC/C.cls"))
    do ..SyncCase("s3","sync syncc -delete")
    $$$ThrowOnError(##class(%File).Delete(r_"syncc/src/cls/SyncD/D.cls"))
    do ..SyncCase("s4","sync syncc -delete")
    $$$ThrowOnError(##class(%File).CreateDirectoryChain(r_"syncc/src/cls/SyncE/"))
    do ..WriteClass(r_"syncc/src/cls/SyncE/E.cls","SyncE.E",5)
    set xml = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(xml.LinkToFile(r_"syncc/module.xml"))
    set text = xml.Read(100000)
    set text = $replace(text,"<Resource Name=""SyncD.D.CLS""/>","<Resource Name=""SyncE.PKG""/>")
    do xml.Clear() do xml.Write(text) $$$ThrowOnError(xml.%Save())
    do ..SyncCase("s5","sync syncc")
    // Flat layout: classes directly under SourcesRoot load through the "" fallback.
    do ..WriteClass(r_"syncf/src/SyncF/A.cls","SyncF.A",2)
    do ..SyncCase("f1","sync syncf")
    $$$ThrowOnError(##class(%File).Delete(r_"syncf/src/SyncF/B.cls"))
    do ..SyncCase("f2","sync syncf -delete")
    do ..SyncCase("f3","load "_r_"syncf -dev")
    // Whole-namespace order: row order, helper order and Sync START order.
    set rs = ##class(%SQL.Statement).%ExecDirect(,"select Name from %IPM_Storage.ModuleItem where DeveloperMode = 1 order by ID")
    set n = 0
    while rs.%Next() { set n = n + 1 do ..Obs("rows."_rs.%Get("Name"),n) }
    set order = ##class(%IPM.Utils.Module).GetDevModeModulesInDependencyOrder()
    for i=1:1:$listlength(order) { do ..Obs("helper."_$list(order,i),i) }
    set sc = ..Run("o","sync",.lines)
    do ..Obs("o.ok",$system.Status.IsOK(sc))
    set n = 0
    for i=1:1:$get(lines) {
        if (lines(i)["Sync START") {
            set n = n + 1
            set name = $piece($piece(lines(i),"|",2),"]",1)
            do ..Obs("o.start."_name,n)
        }
    }
    do ..Obs("o.starts",n)
    write !,"NATIVE-END",!
}

}
