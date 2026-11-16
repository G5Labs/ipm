/// Test-only observer for iteration30. Not IPM code.
Class IPMCont.PhaseReader
{

Parameter ROOT = "/tmp/iter30/phasecase/";

Parameter PHASES = "Clean,Initialize,Reload,Validate,ExportData,Compile,Activate,Document,MakeDeployed,Test,Package,Configure,MyCustom,Bogus,Compile+Activate,Mycustom+Compile";

ClassMethod WriteValue(pValue As %Integer) As %Status
{
    set file = ##class(%Stream.FileCharacter).%New()
    $$$ThrowOnError(file.LinkToFile(..#ROOT_"src/PhaseCase/Value.cls"))
    do file.Clear()
    do file.WriteLine("Class PhaseCase.Value")
    do file.WriteLine("{")
    do file.WriteLine("")
    do file.WriteLine("ClassMethod Get() As %Integer")
    do file.WriteLine("{")
    do file.WriteLine("    quit "_pValue)
    do file.WriteLine("}")
    do file.WriteLine("")
    do file.WriteLine("}")
    quit file.%Save()
}

ClassMethod Value() As %Integer
{
    quit:'##class(%Dictionary.CompiledMethod).%ExistsId("PhaseCase.Value||Get") -1
    quit $classmethod("PhaseCase.Value","Get")
}

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Case(pCase As %String, pCommand As %String, pDisk As %Integer)
{
    $$$ThrowOnError(..WriteValue(pDisk))
    for name="Compile","Activate","MyCustom" { set before(name) = $get(^IPMCONT("hook",name)) }
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set starts = 0, failures = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        if line [ " START" {
            set phase = $piece($piece(line,$char(9),2)," START",1)
            if phase = "" { set phase = $piece($piece(line," START",1),"]",*) }
            set phase = $zstrip(phase,"<>W")
            set starts = starts + 1
            set started($translate(phase,",","+")) = starts
        }
        if line [ " FAILURE" { set failures = failures + 1 }
    }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".starts",starts)
    do ..Obs(pCase_".failures",failures)
    for i=1:1:$length(..#PHASES,",") {
        set phase = $piece(..#PHASES,",",i)
        do ..Obs(pCase_".start."_phase,$get(started(phase)))
    }
    for name="Compile","Activate","MyCustom" { do ..Obs(pCase_".hook."_name,$get(^IPMCONT("hook",name)) - before(name)) }
    do ..Obs(pCase_".value",..Value())
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
}

ClassMethod All()
{
    kill ^IPMCONT
    do ..Case("setup","load "_..#ROOT_" -dev",1)
    do ..Case("c1","phasecase compile -only",11)
    do ..Case("c2","phasecase compile",12)
    do ..Case("c3","phasecase compile activate -only",13)
    do ..Case("c4","phasecase reload mycustom",14)
    do ..Case("c5","phasecase MYCUSTOM",15)
    do ..Case("c6","phasecase compile bogus",16)
    do ..Case("c7","phasecase bogus",17)
    do ..Case("c8","phasecase activate initialize",18)
    do ..Case("c9","compile phasecase -only",19)
    do ..Case("c10","module-action phasecase mycustom compile -only",20)
    do ..Case("c11","phasecase activate -only",21)
    write !,"NATIVE-END",!
}

}
