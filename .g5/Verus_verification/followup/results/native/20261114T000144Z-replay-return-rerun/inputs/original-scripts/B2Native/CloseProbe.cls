/// Registered object whose %OnClose records its tag in IRISTEMP, which rollback does not undo.
Class B2Native.CloseProbe Extends %RegisteredObject
{

Property Tag As %String;

Method %OnNew(tag As %String) As %Status [ Private, ServerOnly = 1 ]
{
    set ..Tag = tag
    quit $$$OK
}

Method %OnClose() As %Status [ Private, ServerOnly = 1 ]
{
    set ^IRIS.Temp.B2Close(..Tag) = $job_" "_$zdatetime($horolog,3)
    quit $$$OK
}

}
