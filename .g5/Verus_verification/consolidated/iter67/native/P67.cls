/// Test-only Studio extension for iteration67. Records calls. Not IPM code.
Class IPMCont.P67 Extends %Studio.Extension.Base
{

ClassMethod Rec(pMethod As %String, pName As %String = "")
{
    set ^IPMCONT67("log",$increment(^IPMCONT67("log"))) = "P|"_pMethod_"|"_pName
}

Method OnBeforeSave(InternalName As %String, Location As %String = "", Object As %RegisteredObject = {$$$NULLOREF}) As %Status
{
    do ..Rec("OnBeforeSave",InternalName)
    quit $$$OK
}

Method OnAfterSave(InternalName As %String, Object As %RegisteredObject = {$$$NULLOREF}) As %Status
{
    do ..Rec("OnAfterSave",InternalName)
    quit $$$OK
}

Method OnBeforeLoad(InternalName As %String) As %Status
{
    do ..Rec("OnBeforeLoad",InternalName)
    quit $$$OK
}

Method OnAfterCompile(InternalName As %String) As %Status
{
    do ..Rec("OnAfterCompile",InternalName)
    quit $$$OK
}

Method OnBeforeDelete(InternalName As %String) As %Status
{
    do ..Rec("OnBeforeDelete",InternalName)
    quit $$$OK
}

Method ItemIconState(InternalName As %String) As %Integer
{
    do ..Rec("ItemIconState",InternalName)
    quit 3
}

Method GetStatus(InternalName As %String, ByRef IsInSourceControl As %Boolean, ByRef Editable As %Boolean, ByRef IsCheckedOut As %Boolean, ByRef UserCheckedOut As %String) As %Status
{
    do ..Rec("GetStatus",InternalName)
    set Editable = +$get(^IPMCONT67("cfg","P","editable"),1), IsInSourceControl = 1
    quit $$$OK
}

Method IsReadOnly(InternalName As %String) As %Boolean
{
    do ..Rec("IsReadOnly",InternalName)
    quit +$get(^IPMCONT67("cfg","P","ro"),0)
}

Method UserAction(Type As %Integer, Name As %String, InternalName As %String, SelectedText As %String, ByRef Action As %String, ByRef Target As %String, ByRef Msg As %String, ByRef Reload As %Boolean) As %Status
{
    do ..Rec("UserAction",Name)
    quit $$$OK
}

Method Custom67() As %Integer
{
    do ..Rec("Custom67")
    quit 67
}

}
