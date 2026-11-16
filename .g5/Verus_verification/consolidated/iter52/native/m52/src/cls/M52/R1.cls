Class M52.R1
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
