Class Iter26.Verify.Case Extends %UnitTest.TestCase
{
ClassMethod Marker() As %Integer { quit 3 }
Method TestShouldNotRun() { set ^||Iter26TestCalls=$increment(^||Iter26TestCalls) do ..AssertTrue(1) }
}
