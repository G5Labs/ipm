Class N2Upd.Upd.V2 Extends %IPM.General.Update.VersionBase
{

ClassMethod GetOrderedMethods() As %Library.DynamicArray
{
    return ["A", "B"]
}

/// Visible effects (an ordinary global, a $INCREMENT counter, a file line), then a halt when ^N2HaltStep = "A":
/// the process ends after the step's effect and before IPM saves the step record (Storage/Module.cls#L244-L246).
ClassMethod A()
{
    set ^N2UpdX = $get(^N2UpdX) + 1
    if $increment(^N2UpdY)
    set io = $io, f = "/tmp/n2-upd-steps.txt"
    open f:("WAS"):2
    if $test { use f write "A ran tlevel=", $tlevel, ! close f }
    use io
    set ^IRIS.Temp.N2Signal("stepA", $increment(^IRIS.Temp.N2Signal)) = $job_":"_$tlevel
    if $get(^N2HaltStep) = "A" { halt }
}

ClassMethod B()
{
    set ^N2Upd("V2.B") = $get(^N2Upd("V2.B")) + 1
}

}
