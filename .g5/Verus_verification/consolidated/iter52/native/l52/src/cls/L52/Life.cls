Class L52.Life Extends %IPM.Lifecycle.Module
{

Method %Activate(ByRef pParams) As %Status
{
    set ^IPMCONT52("life") = $get(^IPMCONT52("life")) + 1
    quit ##super(.pParams)
}

}
