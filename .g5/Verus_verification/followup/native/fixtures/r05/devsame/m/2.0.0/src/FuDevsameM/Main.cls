Class FuDevsameM.Main
{

ClassMethod Hello() As %String
{
    quit "fu-devsame-m 2.0.0"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "follow-up deliberate failure in fu-devsame-m")
}

}
