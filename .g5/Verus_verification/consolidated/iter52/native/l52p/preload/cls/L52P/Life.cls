Class L52P.Life Extends %IPM.Lifecycle.Module
{

Method %Activate(ByRef pParams) As %Status
{
    set ^IPMCONT52("lifep") = $get(^IPMCONT52("lifep")) + 1
    quit ##super(.pParams)
}

}
