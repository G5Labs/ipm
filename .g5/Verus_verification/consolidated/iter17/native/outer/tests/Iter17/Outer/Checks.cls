Class Iter17.Outer.Checks Extends %UnitTest.TestCase
{
Method TestPass()
{
 do $$$AssertTrue(1,"outer true")
 do $$$AssertEquals("outer","outer","outer equals")
}
Method ExerciseFailure(withChild As %Boolean)
{
 set sc=1
 if withChild {
  set sc=##class(%IPM.Main).Shell("test iter17-child -only -f json -output-file /tmp/iter17/child.json -DUnitTest.Case=Iter17.Child.Checks -DUnitTest.Method=TestPass")
  write !,"OBS|child-shell-ok|",$system.Status.IsOK(sc),!
 }
 do $$$AssertStatusOK(sc,"precondition success")
 do $$$AssertTrue(0,"outer deliberate false")
 do $$$AssertEquals("expected","actual","outer deliberate mismatch")
}
Method TestFail() { do ..ExerciseFailure(0) }
Method TestNestedFail() { do ..ExerciseFailure(1) }
}
