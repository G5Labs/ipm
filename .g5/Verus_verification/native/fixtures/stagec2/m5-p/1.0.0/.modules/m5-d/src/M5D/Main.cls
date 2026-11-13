Class M5D.Main
{

ClassMethod Hello() As %String
{
    quit "hello from m5-d"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in m5-d")
}

}
