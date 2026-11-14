/// Invoke targets for the n2 gap checks. Compiled directly into the namespace (not a module resource).
/// Signals go to ^IRIS.Temp.N2Signal (not rolled back); failure switches are ^N2Fail(<module>) (set outside any load).
Class N2H.Hooks
{

/// Fails when ^N2Fail(name) is set; records that it ran and at which $TLEVEL.
ClassMethod Maybe(name As %String) As %Status
{
    set ^IRIS.Temp.N2Signal("ran", $increment(^IRIS.Temp.N2Signal)) = name_":"_$tlevel
    if $get(^N2Fail(name)) {
        quit $$$ERROR($$$GeneralError, "n2: Activate failure injected for "_name)
    }
    quit $$$OK
}

ClassMethod Boom() As %Status
{
    quit $$$ERROR($$$GeneralError, "n2: phase-level Invoke failure injected")
}

/// Waits (at most 300 s) until ^IRIS.Temp.N2Release(name) is set, then succeeds unless ^N2Fail(name) is set.
ClassMethod Wait(name As %String) As %Status
{
    set ^IRIS.Temp.N2Signal("wait", name) = $listbuild($job, $tlevel)
    for i = 1:1:1200 {
        quit:$get(^IRIS.Temp.N2Release(name))
        hang 0.25
    }
    set ^IRIS.Temp.N2Signal("released", name) = i
    if $get(^N2Fail(name)) {
        quit $$$ERROR($$$GeneralError, "n2: failure after wait for "_name)
    }
    quit $$$OK
}

}
