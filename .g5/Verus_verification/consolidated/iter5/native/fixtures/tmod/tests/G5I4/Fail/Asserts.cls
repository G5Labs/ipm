Class G5I4.Fail.Asserts Extends %UnitTest.TestCase
{

Method TestTwoFailedAsserts()
{
    do $$$AssertEquals(1, 2, "first wrong")
    do $$$AssertTrue(1, "passes")
    do $$$AssertEquals(3, 4, "second wrong")
}

Method TestThrows()
{
    set x = 1 / 0
}

}
