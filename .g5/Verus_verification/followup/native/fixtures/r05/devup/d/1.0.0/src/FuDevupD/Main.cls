Class FuDevupD.Main
{

ClassMethod Hello() As %String
{
    quit "fu-devup-d 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-devup-d")
}

}
