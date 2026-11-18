/// Installer class whose undo hook fails. Lives outside every fixture module.
Class IPMCov.Inst74
{

ClassMethod OnInstallComponent(args...) As %Status
{
    quit $$$OK
}

ClassMethod OnConfigureComponent(args...) As %Status
{
    quit $$$OK
}

ClassMethod OnUnconfigureComponent(args...) As %Status
{
    quit $$$ERROR($$$GeneralError, "h74z unconfigure failure")
}

ClassMethod OnUnInstallComponent(args...) As %Status
{
    quit $$$OK
}

}
