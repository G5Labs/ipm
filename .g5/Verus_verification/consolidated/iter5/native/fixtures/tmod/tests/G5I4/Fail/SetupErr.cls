Class G5I4.Fail.SetupErr Extends %UnitTest.TestCase
{

Method OnBeforeAllTests() As %Status
{
    quit $$$ERROR($$$GeneralError, "setup refused")
}

Method TestNeverRuns()
{
    do $$$AssertTrue(1, "unreached")
}

}
