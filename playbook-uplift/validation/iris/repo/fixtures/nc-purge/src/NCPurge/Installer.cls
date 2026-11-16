Class NCPurge.Installer
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
    set ^NCProbe("purge", $increment(^NCProbe("purge"))) = pPurge
    set ^NCProbe("purge", ^NCProbe("purge"), "paramPurge") = $get(pVars("Purge"), "<undef>")
    quit $$$OK
}

ClassMethod OnUnInstallComponent(args...) As %Status
{
    quit $$$OK
}

}
