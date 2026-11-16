/// Invoke targets for the uplift scenarios.
Class UpParent.Hooks
{

ClassMethod Boom() As %Status
{
    quit $$$ERROR($$$GeneralError, "uplift: Activate failure injected")
}

ClassMethod Ok() As %Status
{
    quit $$$OK
}

ClassMethod Halt() As %Status
{
    set ^UpliftSignal("halt", $job) = $tlevel
    halt
}

ClassMethod Hang() As %Status
{
    set ^IRIS.Temp.UpliftSignal("hang") = $listbuild($job, $tlevel)
    hang 120
    quit $$$OK
}

}
