/// Test-only observer for iteration45. Not IPM code.
Class IPMCont.ScanReader
{

Parameter ROOT = "/tmp/iter45/s45/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Write(pRel As %String, pLines As %List)
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(..#ROOT_pRel))
    do s.Clear()
    set ptr = 0
    while $listnext(pLines,ptr,line) { do s.WriteLine(line) }
    $$$ThrowOnError(s.%Save())
}

ClassMethod ClassLines(pClass As %String, pValue As %Integer) As %List
{
    quit $lb("Class "_pClass,"{","","ClassMethod Get() As %Integer","{","    quit "_pValue,"}","","}")
}

ClassMethod Value(pClass As %String) As %Integer
{
    quit:'##class(%Dictionary.CompiledMethod).%ExistsId(pClass_"||Get") -1
    quit $classmethod(pClass,"Get")
}

ClassMethod IncValue() As %Integer
{
    set r = ##class(%Routine).%OpenId("S45Inc.INC")
    quit:'$isobject(r) -1
    set text = r.Read(10000)
    quit +$piece($piece(text,"S45VALUE ",2),$char(10))
}

ClassMethod Case(pCase As %String, pCommand As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set updated = 0, nothing = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        set:line["  Updated: " updated = updated + 1
        set:line["Nothing to sync." nothing = nothing + 1
    }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".updated",updated)
    do ..Obs(pCase_".nothing",nothing)
    do ..Obs(pCase_".inner",..Value("S45.Deep.Inner"))
    do ..Obs(pCase_".top",..Value("S45.Top"))
    do ..Obs(pCase_".up",..Value("S45.Up"))
    do ..Obs(pCase_".inc",..IncValue())
}

ClassMethod All()
{
    do ..Case("setup","load "_..#ROOT_" -dev")
    do ..Case("f0","sync s45")
    do ..Write("src/cls/S45/Deep/Inner.cls",..ClassLines("S45.Deep.Inner",2))
    do ..Case("f1","sync s45")
    do ..Write("src/inc/S45Inc.inc",$lb("ROUTINE S45Inc [Type=INC]","#define S45VALUE 2"))
    do ..Case("f2","sync s45")
    do ..Write("src/cls/S45/Up.CLS",..ClassLines("S45.Up",3))
    do ..Case("f3","sync s45")
    do ..Write("src/cls/S45/Top.cls",..ClassLines("S45.Top",4))
    do ##class(%File).SetAttributes(..#ROOT_"src/cls/S45/Top.cls",0)
    do $zf(-100,"/SHELL","chmod","000",..#ROOT_"src/cls/S45/Top.cls")
    do ..Case("f4","sync s45")
    do $zf(-100,"/SHELL","chmod","644",..#ROOT_"src/cls/S45/Top.cls")
    do ..Case("f5","sync s45")
    write !,"NATIVE-END",!
}

}
