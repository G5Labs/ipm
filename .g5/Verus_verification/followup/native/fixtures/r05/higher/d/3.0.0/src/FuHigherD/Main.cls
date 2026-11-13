Class FuHigherD.Main
{

ClassMethod Hello() As %String
{
    quit "fu-higher-d 3.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-higher-d")
}

}
