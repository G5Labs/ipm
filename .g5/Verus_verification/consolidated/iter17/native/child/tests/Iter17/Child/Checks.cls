Class Iter17.Child.Checks Extends %UnitTest.TestCase
{
Method TestPass()
{
 do $$$AssertTrue(1,"child true")
 do $$$AssertEquals("child","child","child equals")
}
}
