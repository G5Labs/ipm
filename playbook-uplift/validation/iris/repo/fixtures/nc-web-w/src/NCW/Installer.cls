Class NCW.Installer
{

ClassMethod OnInstallComponent(args...) As %Status
{
    quit $$$OK
}

ClassMethod OnConfigureComponent(args...) As %Status
{
    quit $$$OK
}

ClassMethod OnUnconfigureComponent(pNamespace, pVerbose, pPurge, ByRef pVars) As %Status
{
    set ^NCProbe("w", "unconfigure") = $get(^NCProbe("w", "unconfigure")) + 1
    quit $$$OK
}

ClassMethod OnUnInstallComponent(args...) As %Status
{
    set ^NCProbe("w", "uninstall") = $get(^NCProbe("w", "uninstall")) + 1
    quit $$$OK
}

}
