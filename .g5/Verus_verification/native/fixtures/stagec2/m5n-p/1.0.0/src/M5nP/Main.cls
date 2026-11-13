Class M5nP.Main
{

ClassMethod Hello() As %String
{
    quit "hello from m5n-p"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in m5n-p")
}

}
