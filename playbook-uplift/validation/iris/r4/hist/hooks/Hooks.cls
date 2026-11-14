/// Invoke targets for round-4 history probes. Compiled directly into USER (not a module resource).
/// Signals go to ^IRIS.Temp.R4H (IRISTEMP, not rolled back).
Class Uplift.R4.Hooks
{

/// Waits (at most 120 s, or maxloops x 0.25 s) until ^IRIS.Temp.R4H("release", name) is set, then succeeds.
ClassMethod Wait(name As %String) As %Status
{
    set ^IRIS.Temp.R4H("wait", name) = $listbuild($job, $tlevel)
    for i = 1:1:$get(^IRIS.Temp.R4H("maxloops", name), 480) {
        quit:$get(^IRIS.Temp.R4H("release", name))
        hang 0.25
    }
    set ^IRIS.Temp.R4H("released", name) = i
    quit $$$OK
}

}
