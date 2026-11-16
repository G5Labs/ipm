/// Test-only observer for iteration37. Not IPM code.
Class IPMCont.CopyReader
{

Parameter ROOT = "/tmp/iter37/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

/// Runs in %SYS without calling back into this class.
ClassMethod MakeNamespace() As %Status
{
    new $namespace
    set $namespace = "%SYS"
    set dir = "/usr/irissys/mgr/cpsrc/"
    do ##class(%File).CreateDirectoryChain(dir)
    set sc = ##class(SYS.Database).CreateDatabase(dir)
    quit:'sc sc
    kill p set p("Directory") = dir
    set sc = ##class(Config.Databases).Create("CPSRC",.p)
    quit:'sc sc
    kill p set p("Globals") = "CPSRC", p("Routines") = "CPSRC"
    quit ##class(Config.Namespaces).Create("CPSRC",.p)
}

/// Writes and compiles a class in namespace pNamespace (file written from USER first).
ClassMethod Define(pNamespace As %String, pClass As %String, pValue As %Integer) As %Status
{
    set file = ..#ROOT_"defs/"_pNamespace_"-"_pClass_".cls"
    do ##class(%File).CreateDirectoryChain(..#ROOT_"defs/")
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(file))
    for line="Class "_pClass,"{","","ClassMethod Get() As %Integer","{","    quit "_pValue,"}","","}" { do s.WriteLine(line) }
    $$$ThrowOnError(s.%Save())
    new $namespace
    set $namespace = pNamespace
    quit $system.OBJ.Load(file,"ck-d")
}

ClassMethod Value(pClass As %String) As %Integer
{
    quit:'##class(%Dictionary.CompiledMethod).%ExistsId(pClass_"||Get") -1
    quit $classmethod(pClass,"Get")
}

ClassMethod Defined(pClass As %String) As %Integer
{
    quit ##class(%Dictionary.ClassDefinition).%ExistsId(pClass)
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

ClassMethod State(pCase As %String)
{
    for cls="CopyA.C1","CopyA.C2","CopyA.C3","CopyB.D1","CopyB.D2","CopyC.N1","CopyD.Owned","CopyD.Free" {
        do ..Obs(pCase_".defined."_cls,..Defined(cls))
        do ..Obs(pCase_".value."_cls,..Value(cls))
    }
}

ClassMethod All()
{
    set r = ..#ROOT, ok = 1
    set sc = ..MakeNamespace() set ok = ok && sc
    if 'sc { write !,"ERR|ns|",$system.Status.GetErrorText(sc),! }
    // Target items that predate the source: CopyA.C2 and CopyB.D1.
    set ok = ok && ..Define("USER","CopyA.C2",2) && ..Define("USER","CopyB.D1",4)
    hang 1.5
    for item=$lb("CopyA.C1",10),$lb("CopyA.C2",20),$lb("CopyA.C3",30),$lb("CopyB.D1",40),$lb("CopyB.D2",41),$lb("CopyC.N1",50),$lb("CopyD.Owned",60),$lb("CopyD.Free",61) {
        set ok = ok && ..Define("CPSRC",$list(item,1),$list(item,2))
    }
    hang 1.5
    // A target item edited after the source: CopyC.N1.
    set ok = ok && ..Define("USER","CopyC.N1",5)
    set ok = ok && ..Shell("setup","load "_r_"ownmod")
    do ..Obs("setup.ok",ok)
    do ..State("setup")
    do ..Obs("o1.ok",..Shell("o1","load "_r_"cpa"))
    do ..State("o1")
    do ..Obs("o2.ok",..Shell("o2","load "_r_"cpb"))
    do ..State("o2")
    do ..Obs("o3.ok",..Shell("o3","load "_r_"cpc"))
    do ..State("o3")
    do ..Obs("o4.ok",..Shell("o4","load "_r_"cpd"))
    do ..State("o4")
    do ..Obs("u1.ok",..Shell("u1","uninstall cpa"))
    do ..State("u1")
    do ..Obs("u2.ok",..Shell("u2","uninstall cpb"))
    do ..State("u2")
    do ..Obs("u4.ok",..Shell("u4","uninstall cpd"))
    do ..State("u4")
    write !,"NATIVE-END",!
}

}
