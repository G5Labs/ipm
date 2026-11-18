/// Resource processor whose Clean fails. Lives outside every fixture module, so uninstall cannot delete it.
Class IPMCov.FailProc74 Extends %IPM.ResourceProcessor.Abstract
{

Method OnPhase(
	pPhase As %String,
	ByRef pParams,
	Output pResourceHandled As %Boolean = 0) As %Status
{
    if pPhase = "Clean" {
        set pResourceHandled = 1
        do ##class(IPMCov.Reader74).Mid("uninstall", "uninstall", "h74x")
        quit $$$ERROR($$$GeneralError, "h74x clean failure")
    }
    quit $$$OK
}

}
