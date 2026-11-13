Class FuDiaS.Main
{

ClassMethod Hello() As %String
{
    quit "fu-dia-s 1.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-dia-s")
}

}
