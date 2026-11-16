/// Test-only source control extension for iteration43. Records IPM hook calls. Not IPM code.
Class IPMCont.TestSC Extends (%Studio.Extension.Base, %IPM.ExtensionBase.SourceControl.Interface)
{

ClassMethod Rec(pKind As %String, pModule, pPhase As %String = "")
{
    set name = $select($isobject(pModule):pModule.Name,1:"")
    set ^IPMCONT43("log",$increment(^IPMCONT43("log"))) = pKind_"|"_name_"|"_pPhase
}

ClassMethod OnBeforePhase(pModule As %IPM.Storage.Module, pPhase As %String, ByRef pParams) As %Status
{
    do ..Rec("B",pModule,pPhase)
    if $get(^IPMCONT43("fail")) = pPhase { quit $$$ERROR($$$GeneralError,"IPMCONT43 refuses "_pPhase) }
    quit $$$OK
}

ClassMethod OnAfterPhase(pModule As %IPM.Storage.Module, pPhase As %String, ByRef pParams) As %Status
{
    do ..Rec("A",pModule,pPhase)
    quit $$$OK
}

ClassMethod OnModifyResources(pModule As %IPM.Storage.Module) As %Status
{
    do ..Rec("M",pModule)
    quit $$$OK
}

ClassMethod OnChangeModuleRoot(pModule As %IPM.Storage.Module) As %Status
{
    do ..Rec("R",pModule)
    quit $$$OK
}

}
