Class N52.Main
{

ClassMethod Get() As %Integer
{
    quit 1
}

ClassMethod Bump(pValue As %String)
{
    set ^IPMCONT52("greeting") = pValue
}

}
