/// Package processor whose child resolution fails only while ^G5I4FailResolve is set.
Class Iter4.FailResolve Extends %IPM.ResourceProcessor.Default.Package
{

Method OnResolveChildren(ByRef pResourceArray) As %Status
{
    if $get(^G5I4FailResolve) {
        quit $$$ERROR($$$GeneralError, "fixture resolve failure")
    }
    quit ##super(.pResourceArray)
}

}
