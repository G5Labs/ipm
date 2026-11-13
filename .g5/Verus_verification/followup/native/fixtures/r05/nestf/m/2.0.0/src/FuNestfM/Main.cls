Class FuNestfM.Main
{

ClassMethod Hello() As %String
{
    quit "fu-nestf-m 2.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-nestf-m")
}

}
