Class R47.Tests.Basic Extends %UnitTest.TestCase
{

Method TestAPass()
{
    do $$$AssertEquals(1,1,"plain ok")
}

Method TestBPunct()
{
    do $$$AssertEquals(1,2,"has: colon, comma, ""quote"" and #hash")
}

Method TestCLineFeed()
{
    do $$$AssertTrue(0,"first"_$char(10)_"second")
}

Method TestDCrLf()
{
    do $$$AssertTrue(0,"one"_$char(13,10)_"two")
}

Method TestELeading()
{
    do $$$AssertTrue(0,"  leading spaces")
}

Method TestFThrow()
{
    set x = 1/0
}

}
