Class Iter26.Verify.Case Extends %UnitTest.TestCase
{
ClassMethod Marker() As %Integer { quit 2 }
Method TestShouldNotRun() { set ^||Iter26TestCalls=$increment(^||Iter26TestCalls) }
}
