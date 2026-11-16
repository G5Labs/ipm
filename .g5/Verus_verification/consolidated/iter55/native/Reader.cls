/// Test-only observer for iteration55. Not IPM code.
Class IPMCont.NsReader
{

ClassMethod Obs(pKey As %String, pValue As %Integer)
{
    write !,"OBS|"_pKey_"|"_(+pValue),!
}

ClassMethod MakeNamespace(pName As %String) As %Status
{
    new $namespace
    set $namespace = "%SYS"
    set dir = "/usr/irissys/mgr/"_$zconvert(pName,"L")_"/"
    do ##class(%File).CreateDirectoryChain(dir)
    set sc = ##class(SYS.Database).CreateDatabase(dir)
    quit:'sc sc
    kill p set p("Directory") = dir
    set sc = ##class(Config.Databases).Create(pName,.p)
    quit:'sc sc
    kill p set p("Globals") = pName, p("Routines") = pName
    quit ##class(Config.Namespaces).Create(pName,.p)
}

ClassMethod Setup()
{
    do ..Obs("setup.ns",..MakeNamespace("NS55A"))
    kill ^IPMCONT55
}

/// Runs a shell command; records status, namespace after, and output markers. Restores USER.
ClassMethod Case(pCase As %String, pCommand As %String)
{
    do ##class(%IPM.Utils.Module).BeginCaptureOutput(.cookie)
    set sc = ##class(%IPM.Main).Shell(pCommand)
    set after = $namespace
    set $namespace = "USER"
    do ##class(%IPM.Utils.Module).EndCaptureOutput(cookie,.lines)
    set notEnabled = 0, failed = 0, listedA = 0, listedUser = 0, enabledMsg = 0, configuredMsg = 0
    for i=1:1:$get(lines) {
        set line = $zstrip($get(lines(i)),"*C")
        write !,"OUT|",pCase,"|",line
        set:line["IPM is not enabled" notEnabled = 1
        set:(line["NS55A>")&&(line["(IPM not enabled)") listedA = 1
        set:(line["USER>")&&(line'["Enter number") listedUser = 1
        set:line["Enabling package manager extension if present... done." enabledMsg = 1
        set:line["Currently configured to use the following extension classes" configuredMsg = 1
    }
    if 'sc { set text = $system.Status.GetErrorText(sc) write !,"ERR|",pCase,"|",text,! set:text["Failed to switch" failed = 1 }
    do ..Obs(pCase_".ok",$system.Status.IsOK(sc))
    do ..Obs(pCase_".after.user",after="USER")
    do ..Obs(pCase_".after.sys",after="%SYS")
    do ..Obs(pCase_".after.a",after="NS55A")
    do ..Obs(pCase_".notenabled",notEnabled)
    do ..Obs(pCase_".failedswitch",failed)
    do ..Obs(pCase_".listed.a",listedA)
    do ..Obs(pCase_".listed.user",listedUser)
    do ..Obs(pCase_".global",$get(^IPMCONT55))
    do ..Obs(pCase_".msg.enabling",enabledMsg)
    do ..Obs(pCase_".msg.configured",configuredMsg)
    do ..SC(pCase)
}

ClassMethod SC(pCase As %String)
{
    set cls = ##class(%Studio.SourceControl.Interface).SourceControlClassGet()
    do ..Obs(pCase_".sc.empty",cls="")
    do ..Obs(pCase_".sc.composite",cls="%IPM.ExtensionBase.Composite")
    do ..Obs(pCase_".sc.base",cls="%Studio.SourceControl.Base")
    set primary = ##class(%IPM.ExtensionBase.Composite).GetPrimaryExtensionClass()
    do ..Obs(pCase_".sc.storedprimarybase",primary="%Studio.SourceControl.Base")
    do ..Obs(pCase_".sc.activecomposite",(cls="%IPM.ExtensionBase.Composite")&&(primary="%Studio.SourceControl.Base"))
}

ClassMethod SetSC(pClass As %String)
{
    do ##class(%Studio.SourceControl.Interface).SourceControlClassSet(pClass)
}

/// Directly set a dangling class name in the namespace setting (no existence check).
ClassMethod SetDangling(pClass As %String)
{
    set ^oddStudioMenu("00-SourceControlClass") = pClass
}

}
