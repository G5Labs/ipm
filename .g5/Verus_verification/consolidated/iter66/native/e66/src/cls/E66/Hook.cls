Class E66.Hook
{

ClassMethod Mark(pName As %String) As %Status
{
    set ^IPMCONT66("seq",$increment(^IPMCONT66("n"))) = pName
    quit $$$OK
}

ClassMethod Fail(pName As %String) As %Status
{
    set ^IPMCONT66("seq",$increment(^IPMCONT66("n"))) = pName
    quit $$$ERROR($$$GeneralError,"iter66 hook failure")
}

}
