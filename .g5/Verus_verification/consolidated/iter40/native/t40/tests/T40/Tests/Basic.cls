Class T40.Tests.Basic Extends %UnitTest.TestCase
{

Method TestOne()
{
    set ^IPMCONT40("ran") = $get(^IPMCONT40("ran")) + 1
    do $$$AssertEquals(##class(T40.Main).Get(),1,"main class reachable")
}

}
