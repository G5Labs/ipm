Class XLA64.Good
{

ClassMethod Get() As %Integer
{
    quit 1
}

ClassMethod Fail() As %Status
{
    throw ##class(%Exception.General).%New("IPMCONT64")
}

}
