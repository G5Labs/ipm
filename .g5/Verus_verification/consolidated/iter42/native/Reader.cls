/// Test-only observer for iteration42. Not IPM code.
Class IPMCont.PreloadReader
{

Parameter ROOT = "/tmp/iter42/";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Value(pClass As %String) As %Integer
{
    quit:'##class(%Dictionary.CompiledMethod).%ExistsId(pClass_"||Get") -1
    quit $classmethod(pClass,"Get")
}

ClassMethod WriteBoot(pValue As %Integer)
{
    set s = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(s.LinkToFile(..#ROOT_"m42/preload/cls/M42P/Boot.cls"))
    do s.Clear()
    for line="Class M42P.Boot","{","","ClassMethod Get() As %Integer","{","    quit "_pValue,"}","","}" { do s.WriteLine(line) }
    $$$ThrowOnError(s.%Save())
}

ClassMethod Case(pCase As %String, pCommand As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set initOk = 0, initFail = 0, skip = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        set:line["Initialize SUCCESS" initOk = initOk + 1
        set:line["Initialize FAILURE" initFail = initFail + 1
        set:line["Skipping preload" skip = skip + 1
    }
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".init.success",initOk)
    do ..Obs(pCase_".init.failure",initFail)
    do ..Obs(pCase_".skip",skip)
    do ..Obs(pCase_".boot",..Value("M42P.Boot"))
    do ..Obs(pCase_".main",..Value("M42.Main"))
    do ..Obs(pCase_".bbroken",##class(%Dictionary.CompiledClass).%ExistsId("M42B.Broken"))
    do ..Obs(pCase_".bok",..Value("M42BS.Ok"))
    do ..Obs(pCase_".bmodule",##class(%IPM.Storage.Module).NameExists("m42b"))
    do ..Obs(pCase_".cmodule",##class(%IPM.Storage.Module).NameExists("m42c"))
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Case("setup","load "_r_"m42 -dev -v")
    do ..WriteBoot(2)
    do ..Case("i1","m42 initialize -only -v")
    do ..Case("i2","m42 compile -v")
    do ..Case("i3","load "_r_"m42 -dev -v")
    do ..WriteBoot(3)
    do ..Case("i4","m42 initialize -only -v -DRootDirectory="_r_"m42/")
    do ..Case("b1","load "_r_"m42b -v")
    do ..Case("c1","load "_r_"m42c -v")
    write !,"NATIVE-END",!
}

}
