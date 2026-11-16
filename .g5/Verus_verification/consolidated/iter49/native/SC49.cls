/// Test-only source-control class for iteration49. Records calls. Not IPM code.
Class IPMCont.SC49 Extends %Studio.SourceControl.Base
{

Method UserAction(Type As %Integer, Name As %String, InternalName As %String, SelectedText As %String, ByRef Action As %String, ByRef Target As %String, ByRef Msg As %String, ByRef Reload As %Boolean) As %Status
{
    set ^IPMCONT49("UserAction",Type,Name) = $get(^IPMCONT49("UserAction",Type,Name)) + 1
    set ^IPMCONT49("UserActionList") = InternalName
    set Action = 0
    quit $$$OK
}

Method GetStatus(InternalName As %String, ByRef IsInSourceControl As %Boolean, ByRef Editable As %Boolean, ByRef IsCheckedOut As %Boolean, ByRef UserCheckedOut As %String) As %Status
{
    set IsInSourceControl = (InternalName [ "Ctl"), Editable = 1, IsCheckedOut = 1, UserCheckedOut = ""
    quit $$$OK
}

Method OnAfterSave(InternalName As %String, Object As %RegisteredObject = {$$$NULLOREF}) As %Status
{
    set ^IPMCONT49("OnAfterSave",InternalName) = $get(^IPMCONT49("OnAfterSave",InternalName)) + 1
    quit $$$OK
}

Method AddToSourceControl(InternalName As %String, Description As %String = "") As %Status
{
    set ^IPMCONT49("AddToSourceControl",InternalName) = $get(^IPMCONT49("AddToSourceControl",InternalName)) + 1
    quit $$$OK
}

}
