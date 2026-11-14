Class NCQ.Installer
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
    quit $$$OK
}

ClassMethod OnUnInstallComponent(args...) As %Status
{
    set ^NCProbe("q", "uninstall") = $get(^NCProbe("q", "uninstall")) + 1
    quit:$get(^NCProbe("q", "fail")) $$$ERROR($$$GeneralError, "nc-q refuses to uninstall")
    quit $$$OK
}

}
