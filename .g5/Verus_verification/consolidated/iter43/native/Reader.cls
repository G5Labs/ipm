/// Test-only observer for iteration43. Not IPM code.
Class IPMCont.SCReader
{

Parameter ROOT = "/tmp/iter43/";

Parameter PHASES = "Clean,Initialize,Reload,Validate,Compile,Activate,Configure,Unconfigure";

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod Mode(pMode As %String) As %Status
{
    kill ^SYS("SourceControl","%IPM.ExtensionBase.Composite")
    if pMode = "none" { quit ##class(%Studio.SourceControl.Interface).SourceControlClassSet("") }
    if pMode = "direct" { quit ##class(%Studio.SourceControl.Interface).SourceControlClassSet("IPMCont.TestSC") }
    if pMode = "primary" {
        set sc = ##class(%IPM.ExtensionBase.Composite).SetExtensionClasses("IPMCont.TestSC","")
        quit:'sc sc
        quit ##class(%Studio.SourceControl.Interface).SourceControlClassSet("%IPM.ExtensionBase.Composite")
    }
    if pMode = "secondary" {
        set sc = ##class(%IPM.ExtensionBase.Composite).SetExtensionClasses("IPMCont.PlainSC",$listbuild("IPMCont.TestSC"))
        quit:'sc sc
        quit ##class(%Studio.SourceControl.Interface).SourceControlClassSet("%IPM.ExtensionBase.Composite")
    }
    quit $$$ERROR($$$GeneralError,"mode")
}

ClassMethod Case(pCase As %String, pCommand As %String)
{
    kill ^IPMCONT43("log")
    set sc = ##class(%IPM.Main).Shell(pCommand)
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Report(pCase,sc)
}

ClassMethod Report(pCase As %String, pSC As %Status)
{
    do ..Obs(pCase_".ok",$system.Status.IsOK(pSC))
    kill count
    set total = $get(^IPMCONT43("log"))
    for i=1:1:total {
        set e = ^IPMCONT43("log",i)
        write !,"LOG|",pCase,"|",i,"|",e
        set kind = $piece(e,"|",1), phase = $piece(e,"|",3)
        set count(kind_$select(phase'="":"."_phase,1:"")) = $get(count(kind_$select(phase'="":"."_phase,1:""))) + 1
        if (kind = "B") && '$data(first(phase)) { set first(phase) = i }
    }
    do ..Obs(pCase_".total",total)
    set mod = ##class(%IPM.Storage.Module).NameOpen("m43")
    do ..Obs(pCase_".rootcopy",$select($isobject(mod):(mod.Root["m43copy"),1:0))
    do ..Obs(pCase_".M",$get(count("M")))
    do ..Obs(pCase_".R",$get(count("R")))
    for i=1:1:$length(..#PHASES,",") {
        set p = $piece(..#PHASES,",",i)
        do ..Obs(pCase_".B."_p,$get(count("B."_p)))
        do ..Obs(pCase_".A."_p,$get(count("A."_p)))
    }
}

ClassMethod SaveDoc(pCase As %String)
{
    kill ^IPMCONT43("log")
    set doc = ##class(%RoutineMgr).%OpenId("m43.ZPM")
    do doc.Code.Rewind()
    set text = doc.Code.Read(100000)
    do doc.Code.Clear()
    do doc.Code.Write($replace(text,"<Version>1.0.0</Version>","<Version>1.0.1</Version>"))
    set sc = doc.%Save()
    if 'sc { write !,"ERR|",pCase,"|",$system.Status.GetErrorText(sc),! }
    do ..Report(pCase,sc)
}

ClassMethod All()
{
    set r = ..#ROOT
    do ..Obs("mode.none",..Mode("none"))
    do ..Case("n1","load "_r_"m43 -dev")
    do ..Case("n2","uninstall m43")
    do ..Obs("mode.direct",..Mode("direct"))
    do ..Case("d1","load "_r_"m43 -dev")
    do ..Case("d2","uninstall m43")
    do ..Obs("mode.secondary",..Mode("secondary"))
    do ..Case("s1","load "_r_"m43 -dev")
    do ..Case("s2","uninstall m43")
    do ..Obs("mode.primary",..Mode("primary"))
    do ..Case("p1","load "_r_"m43 -dev")
    do ..Case("p2","m43 compile -only")
    do ..SaveDoc("p3")
    do ..Case("p4","load "_r_"m43copy -dev")
    do ..Case("p4b","m43 reload -only -DRootDirectory="_r_"m43/")
    set ^IPMCONT43("fail") = "Compile"
    do ..Case("p5","m43 compile -only")
    kill ^IPMCONT43("fail")
    do ..Case("p6","uninstall m43")
    do ..Obs("mode.reset",..Mode("none"))
    write !,"NATIVE-END",!
}

}
