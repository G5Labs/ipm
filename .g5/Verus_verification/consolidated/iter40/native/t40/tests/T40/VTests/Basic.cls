Class T40.VTests.Basic Extends %UnitTest.TestCase
{

Method TestOne()
{
    set ^IPMCONT40("vran") = $get(^IPMCONT40("vran")) + 1
    do $$$AssertEquals(##class(T40.Main).Get(),1,"main class reachable")
}

}
