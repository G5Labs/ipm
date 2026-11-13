Class FuHigherokD.Main
{

ClassMethod Hello() As %String
{
    quit "fu-higherok-d 3.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-higherok-d")
}

}
