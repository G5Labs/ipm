Include %IPM.Common
Class Iter10.Fault Extends %IPM.ResourceProcessor.Default.Class
{
Method OnResolveChildren(ByRef children) As %Status
{
 set ^Iter10HookCount=$get(^Iter10HookCount)+1
 if $get(^Iter10Fail) { return $$$ERROR($$$GeneralError,"iteration10 returned child hook error") }
 return ##super(.children)
}
}
