Class NcA2U22.Upd.V2 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["A", "B"]
}

/// Effects: an ordinary global (rolled back by a transaction rollback), a file line (never rolled back) and a
/// run counter in IRISTEMP (not journaled, so not rolled back). When the probe asks for a handshake, the step
/// reports its pid after its work is done and waits until the probe holds the lock on its step record, so that
/// the probe can kill the process after the step method ended and before the step record is saved.
ClassMethod A()
{
    set ^NcA2U22X = $get(^NcA2U22X) + 1
    set io = $io, f = "/tmp/nc-a2-u22-steps.txt"
    open f:("WAS"):2
    if $test { use f write "A ran", ! close f }
    use io
    set ^IRIS.TempNcA2("aRuns") = $get(^IRIS.TempNcA2("aRuns")) + 1
    if $get(^IRIS.TempNcA2("handshake")) {
        set ^IRIS.TempNcA2("pid") = $job
        set ^IRIS.TempNcA2("stepEnded") = 1
        for i = 1:1:200 {
            quit:$get(^IRIS.TempNcA2("lockHeld"))
            hang 0.05
        }
    }
}

ClassMethod B()
{
    set ^NcA2U22("V2.B") = $get(^NcA2U22("V2.B")) + 1
}

}
