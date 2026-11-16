/// Test-only installer for iteration36: records every callback and its arguments.
Class IPMCont36.Installer
{

ClassMethod Rec(pList As %List)
{
    set n = $increment(^IPMCONT36("n"))
    set ^IPMCONT36(n) = pList
}

ClassMethod OnInstallComponent(pDatabase, pVerbose, pDevMode, pIsUpgrade) As %Status
{
    do ..Rec($listbuild("Install",pDatabase,pVerbose,pDevMode,pIsUpgrade))
    quit:$get(^IPMCONT36("fail")) $$$ERROR($$$GeneralError,"iter36 installer refused")
    quit $$$OK
}

ClassMethod OnConfigureComponent(pNamespace, pVerbose, ByRef pVars) As %Status
{
    do ..Rec($listbuild("Configure",pNamespace,pVerbose,$get(pVars("Mode")),$get(pVars("Greeting")),$data(pVars("InstallDir")),$isobject($get(pVars("PackageManager","Lifecycle")))))
    quit $$$OK
}

ClassMethod OnUnconfigureComponent(pNamespace, pVerbose, pPurge, ByRef pVars) As %Status
{
    do ..Rec($listbuild("Unconfigure",pNamespace,pVerbose,pPurge))
    quit $$$OK
}

ClassMethod OnUnInstallComponent(pDatabase, pVerbose) As %Status
{
    do ..Rec($listbuild("UnInstall",pDatabase,pVerbose))
    quit $$$OK
}

ClassMethod Note(pGreeting)
{
    do ..Rec($listbuild("Note",pGreeting))
}

}
