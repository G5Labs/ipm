Class Iter7.CSP Extends %IPM.ResourceProcessor.CSPApplication
{
Method OnPhaseInternal(pTime As %String, pPhase As %String, ByRef pParams) As %Status [ Private ]
{
 set ^Iter7Directory=..Directory
 return ##super(pTime,pPhase,.pParams)
}
}
