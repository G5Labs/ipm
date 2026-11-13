Class FuPardefD.Main
{

ClassMethod Hello() As %String
{
    quit "fu-pardef-d 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-pardef-d")
}

}
