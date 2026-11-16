Class PhaseCase.Hook
{

ClassMethod Bump(pName As %String)
{
    set ^IPMCONT("hook",pName) = $get(^IPMCONT("hook",pName)) + 1
}

}
