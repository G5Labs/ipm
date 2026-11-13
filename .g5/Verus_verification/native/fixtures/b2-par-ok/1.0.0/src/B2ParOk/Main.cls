Class B2ParOk.Main
{

ClassMethod Hello() As %String
{
    quit "hello from b2-par-ok"
}

/// Invoked in the Activate phase by fixtures that must fail there.
ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in b2-par-ok")
}

/// Invoked in the Activate phase by fixtures that must stall there.
/// Signals ^IRIS.Temp.B2Signal(<package>,"pid") and waits up to 300 s for ...("release").
ClassMethod Wait() As %Status
{
    set tag = $piece($classname(), ".", 1)
    set ^IRIS.Temp.B2Signal(tag, "tlevel") = $tlevel
    set ^IRIS.Temp.B2Signal(tag, "pid") = $job
    for i=1:1:1500 {
        quit:$data(^IRIS.Temp.B2Signal(tag, "release"))
        hang 0.2
    }
    quit $$$OK
}

}
