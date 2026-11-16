Class NcDepLf.Hooks
{

ClassMethod Boom() As %Status
{
    quit $$$ERROR($$$GeneralError, "nc-dep-15 Activate failure on purpose")
}

}
