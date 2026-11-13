Class FuDiaD2.Main
{

ClassMethod Hello() As %String
{
    quit "fu-dia-d2 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-dia-d2")
}

}
