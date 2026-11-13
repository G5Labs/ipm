Class M5P.Main
{

ClassMethod Hello() As %String
{
    quit "hello from m5-p"
}

ClassMethod Fail() As %Status
{
    quit $$$ERROR($$$GeneralError, "b2 deliberate failure in m5-p")
}

}
