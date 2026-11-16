/// Invoke target that always fails, used to fail an install after Activate.
Class Iter5.Fail
{

ClassMethod Go() As %Status
{
    quit $$$ERROR($$$GeneralError, "fixture invoke failure")
}

}
