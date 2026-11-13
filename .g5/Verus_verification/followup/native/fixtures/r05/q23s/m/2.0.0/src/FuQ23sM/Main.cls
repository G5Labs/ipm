Class FuQ23sM.Main
{

ClassMethod Hello() As %String
{
    quit "fu-q23s-m 2.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-q23s-m")
}

}
