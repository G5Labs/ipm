Class M52.R2
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
