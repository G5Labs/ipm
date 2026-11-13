Class FuQ23fM.Main
{

ClassMethod Hello() As %String
{
    quit "fu-q23f-m 2.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-q23f-m")
}

}
