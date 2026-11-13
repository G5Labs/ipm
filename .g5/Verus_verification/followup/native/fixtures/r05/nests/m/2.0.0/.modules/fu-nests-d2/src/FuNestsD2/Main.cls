Class FuNestsD2.Main
{

ClassMethod Hello() As %String
{
    quit "fu-nests-d2 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-nests-d2")
}

}
