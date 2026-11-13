Class FuNestsD1.Main
{

ClassMethod Hello() As %String
{
    quit "fu-nests-d1 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-nests-d1")
}

}
