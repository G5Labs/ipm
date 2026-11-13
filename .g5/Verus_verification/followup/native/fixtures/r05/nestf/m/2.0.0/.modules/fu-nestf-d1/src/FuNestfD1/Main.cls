Class FuNestfD1.Main
{

ClassMethod Hello() As %String
{
    quit "fu-nestf-d1 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-nestf-d1")
}

}
