Class FuQ23sD.Main
{

ClassMethod Hello() As %String
{
    quit "fu-q23s-d 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-q23s-d")
}

}
