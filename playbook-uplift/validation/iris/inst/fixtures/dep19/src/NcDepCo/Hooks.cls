Class NcDepCo.Hooks
{

ClassMethod Boom() As %Status
{
    quit $$$ERROR($$$GeneralError, "nc-dep-19 parent Activate failure on purpose")
}

}
