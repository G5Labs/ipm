Class M5nD.Main
{

ClassMethod Hello() As %String
{
    quit "hello from m5n-d"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in m5n-d")
}

}
